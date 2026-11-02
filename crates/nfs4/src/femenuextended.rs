//! FEMENUEXTENDED.CPP: the front end's NFS4-style menus.
//! - The text box an item is drawn in (`MenuNFS4_DrawTextBox`): its text twice, the second copy
//!   further left, between arrows, in a rounded box.
//! - Items that slide in and fade between enabled and disabled: the NFS4 button and left-right
//!   choice. The options screen's choices: a list's value, or one of two.
//! - The menus holding them: the NFS4 menu with its animated button image; the two-player,
//!   bottom, blank and options menus; their transitions on and off.
//!
//! Written as FEMENU.CPP is (see there): typed views, calls through `crate::call`.

use crate::call::{call, call_addr, call_virtual, with_buffer};
use crate::femenu::{self, ITEM_DISABLED, ITEM_VPTR, ITERATOR_VPTR, KEY_NONE, PLAY_FE_SFX, TEXT_VALUE, TITLE, VALUE};
use crate::fetextrender;
use crate::globals::{FE_APP, FLIP, GAME_TICKS, HELP_POS, PLAYER1_VIEW, SCREEN_WIDTH, SHAPES};
use crate::types::{
    DRAWENV, FEApplication, ListIterator, Menu, MenuBlank, MenuCommand, MenuItem, MenuItemGoToMenuNFS4Button,
    MenuItemLeftRightChoice, MenuItemNFS4LeftRightChoice, MenuItemOptionsTwoItemChoice, MenuNFS4, MenuOptions, RECT,
};
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
/// An item's or a menu's flag: drawn with its reflection below (a menu's: each of its items).
const REFLECTED: u32 = 0x200;

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
pub(crate) const DRAW_SHAPE_EXTENDED: fn(&mut Cpu) = crate::psxfront::draw_shape_extended_entry;
pub(crate) const ROUND_RECTANGLE: fn(&mut Cpu) = nfs4rc::f_8004c490;
pub(crate) const SET_BLITTER: fn(&mut Cpu) = nfs4rc::f_800cc52c;
pub(crate) const RESET_BLITTER: fn(&mut Cpu) = nfs4rc::f_800cc538;
pub(crate) const SET_ABR_MODE: fn(&mut Cpu) = crate::mmeffect::set_abr_mode_entry;
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
/// A front-end shape's entry (32 bytes, PSXFRONT.CPP's): its width and height.
const SHAPE_WIDTH: u32 = 16;
const SHAPE_HEIGHT: u32 = 18;
/// The shapes used here (their entries in the table at `SHAPES`): the box's border, the options
/// choices' arrows (41st).
const BORDER_SHAPE: u32 = 960;
const OPTIONS_SHAPE: u32 = 1312;
/// `DrawShapeExtended`'s shapes and flags used here: the arrows (left, right), the box's border;
/// the arrows' and the button image's flags.
const LEFT_ARROW: u32 = 10;
const RIGHT_ARROW: u32 = 11;
const BORDER: u32 = 30;
const ARROW_FLAGS: u32 = 280;
const BUTTON_IMAGE_FLAGS: u32 = 1040;
/// The text box's and the options' fades: `CalcFadeVal`'s colours from and to.
const BOX_DIM: u32 = 0x00B5_4200;
const BOX_LIT: u32 = 0xBEBE;
/// The button image's tint.
const BUTTON_IMAGE_COLOUR: u32 = 0x00CE_C844;

/// The short in a register's low half, sign-extended.
fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn shape_width(m: &mut Mem, shape: u32) -> i16 {
    m.r16(shape + SHAPE_WIDTH) as i16
}

fn shape_height(m: &mut Mem, shape: u32) -> i16 {
    m.r16(shape + SHAPE_HEIGHT) as i16
}

/// The address of a caller's stack argument `n` (from 0), for the variadic constructors' item
/// lists.
fn stack_arg(c: &Cpu, n: u32) -> u32 {
    c.r[29].wrapping_add(4 * n)
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

/// The drawing clipped to `rect` from here on (a draw area primitive in the ordering table).
fn set_draw_area(c: &mut Cpu, rect: RECT) {
    let prim = add_prim(&mut c.m);
    call_addr(c, SET_DRAW_AREA, &[prim, rect.0]);
}

fn set_rect(m: &mut Mem, rect: RECT, x: i16, y: i16, w: i16, h: i16) {
    rect.set_x(m, x);
    rect.set_y(m, y);
    rect.set_w(m, w);
    rect.set_h(m, h);
}

fn copy_rect(m: &mut Mem, from: RECT, to: RECT) {
    let (x, y, w, h) = (from.x(m), from.y(m), from.w(m), from.h(m));
    set_rect(m, to, x, y, w, h);
}

/// The buffer the drawing functions here pass the library by address: a rectangle,
/// `DrawShapeExtended`'s options (+4 its colour), a choice's on and off colours, a string.
struct Locals(u32);

impl Locals {
    const SIZE: u32 = 136;
    fn rect(&self) -> RECT {
        RECT(self.0)
    }
    fn shape_options(&self) -> u32 {
        self.0 + 8
    }
    fn set_shape_colour(&self, m: &mut Mem, colour: u32) {
        m.w32(self.0 + 12, colour);
    }
    fn shape_colour(&self, m: &mut Mem) -> u32 {
        m.r32(self.0 + 12)
    }
    fn on_colour(&self) -> u32 {
        self.0 + 32
    }
    fn off_colour(&self) -> u32 {
        self.0 + 36
    }
    fn text(&self) -> u32 {
        self.0 + 40
    }
}

/// Text `id`'s width in pixels as the boxes show it: upper case, less a pixel a letter.
fn upper_text_width(c: &mut Cpu, id: u32, buf: u32) -> u32 {
    let word = call(c, TEXT_WORD, &[id]);
    call(c, SPRINTF, &[buf, TEXT_FORMAT, word]);
    call(c, S_UPPER, &[buf]);
    let pixels = call(c, TEXT_PIXELS, &[buf]);
    let letters = call(c, STRLEN, &[buf]);
    pixels.wrapping_sub(letters)
}

/// `MenuNFS4_SetHelpPos` (FEMENUEXTENDED.CPP, 0x8001a128): where the help box goes.
pub fn set_help_pos(m: &mut Mem, rect: u32) {
    copy_rect(m, RECT(rect), RECT(HELP_POS));
}

pub fn abi_set_help_pos(c: &mut Cpu) {
    set_help_pos(&mut c.m, c.r[4]);
}

/// `MenuNFS4_DrawTextBox` (FEMENUEXTENDED.CPP, 0x8001a158): an item's box at `rect`. Its text
/// (when it has one) twice, the second `width` (at least the text's) further left, both `offset`
/// across, in the item's `fade` (`fade_by_y`: faded by the box's height on the screen too, for
/// the reflections), between arrows when `arrows`, clipped to the box; then the box's border and
/// its square.
#[allow(clippy::too_many_arguments)]
pub fn draw_text_box(c: &mut Cpu, text: u32, rect: RECT, width: u32, offset: u32, fade: u32, arrows: u32, fade_by_y: u32) {
    with_buffer(c, Locals::SIZE, |c, at| {
        let l = Locals(at);
        let (view, flip) = (c.m.r32(PLAYER1_VIEW), c.m.r32(FLIP));
        let env = DRAWENV(call(c, GET_DRAWENV, &[view, flip]));
        let colour = call(c, CALC_FADE_VAL, &[BOX_DIM, BOX_LIT, sx(fade)]);
        l.set_shape_colour(&mut c.m, colour);
        if fade_by_y != 0 {
            let down = (224 - rect.y(&mut c.m) as i32).clamp(0, 128);
            let colour = l.shape_colour(&mut c.m);
            let colour = call(c, CALC_FADE_VAL, &[0, colour, down as u32]);
            l.set_shape_colour(&mut c.m, colour);
        }
        call_addr(c, SUBTRACT_RECT_EDGES, &[rect.0]);
        if (text as i32) >= 0 {
            // the whole screen's draw area
            let screen_y = env.clip().y(&mut c.m);
            set_rect(&mut c.m, l.rect(), 0, screen_y, 512, 240);
            set_draw_area(c, l.rect());
            call(c, SET_FONT, &[0]);
            let shown = upper_text_width(c, text, l.text());
            let gap = (shown as i32).max(width as i32).wrapping_add(25) as u32;
            let mut colour = call(c, CALC_TEXT_FADE_SEL_TO_HI, &[1, sx(fade), 0]);
            if fade_by_y != 0 {
                let down = 240u32.wrapping_sub(rect.y(&mut c.m) as i32 as u32);
                colour = call(c, CALC_FADE_VAL, &[0, colour, down]);
            }
            for dx in [0, gap] {
                let word = call(c, TEXT_WORD, &[text]);
                let m = &mut c.m;
                let x = (rect.x(m) as u16 as u32).wrapping_add(offset).wrapping_sub(dx);
                let y = (rect.y(m) as u16 as u32).wrapping_add(4);
                call(c, FULL_TEXT_RGB, &[word, sx(x), sx(y), colour, 0, 0]);
            }
            if arrows != 0 {
                let m = &mut c.m;
                let mut y = (rect.y(m) as i32).wrapping_add((rect.h(m) >> 1) as i32) as u32;
                if fade_by_y == 0 {
                    y = y.wrapping_add(2);
                }
                // each copy's arrows: the left one before it, the right one after
                for at in [0, gap] {
                    for (shape, after) in [(LEFT_ARROW, 0u32.wrapping_sub(10)), (RIGHT_ARROW, shown.wrapping_add(8))] {
                        let x = (rect.x(&mut c.m) as i32 as u32).wrapping_add(sx(offset)).wrapping_sub(at).wrapping_add(after);
                        call(c, DRAW_SHAPE_EXTENDED, &[shape, ARROW_FLAGS, x, y, 0, 0, l.shape_options()]);
                    }
                }
            }
            // the box's own draw area: inside its sides, on the screen being drawn
            let m = &mut c.m;
            copy_rect(m, rect, l.rect());
            let r = l.rect();
            let screen_y = env.clip().y(m);
            r.update_y(m, |y| y.wrapping_add(screen_y));
            r.update_x(m, |x| x.wrapping_add(2));
            r.update_w(m, |w| w.wrapping_sub(4));
            set_draw_area(c, l.rect());
        }
        // the border on the right, and the square under it all
        let m = &mut c.m;
        copy_rect(m, rect, l.rect());
        let r = l.rect();
        let shape = m.r32(SHAPES).wrapping_add(BORDER_SHAPE);
        let y = r.y(m).wrapping_add(1);
        r.set_y(m, y);
        r.update_h(m, |h| h.wrapping_sub(2));
        let w = r.w(m).wrapping_sub(1).wrapping_sub(shape_width(m, shape) >> 1);
        let x = (w as i32).wrapping_add(r.x(m) as i32) as u32;
        r.set_w(m, w);
        call(c, DRAW_SHAPE_EXTENDED, &[BORDER, 8, x, y as i32 as u32, 0, 0, 0]);
        let m = &mut c.m;
        let args = [0, r.x(m) as i32 as u32, r.y(m) as i32 as u32, r.w(m) as i32 as u32, shape_height(m, shape) as i32 as u32];
        call_addr(c, DRAW_SQUARE, &args);
    })
}

pub fn abi_draw_text_box(c: &mut Cpu) {
    let (fade, arrows, fade_by_y) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5), crate::abi::arg(c, 6));
    draw_text_box(c, c.r[4], RECT(c.r[5]), c.r[6], c.r[7], fade, arrows, fade_by_y);
}

// ---- The sliding items (the NFS4 button and left-right choice).

/// The four shorts the sliding items end with: where the box is in its slide, the transition
/// (0 off .. 128 on) and its speed, and the fade between disabled and enabled.
#[derive(Clone, Copy)]
struct Slide(u32);

impl Slide {
    fn of_button(b: MenuItemGoToMenuNFS4Button) -> Self {
        Slide(b.0 + 0x24)
    }
    fn of_choice(c: MenuItemNFS4LeftRightChoice) -> Self {
        Slide(c.0 + 0x20)
    }
    fn offset(self, m: &mut Mem) -> i16 {
        m.r16(self.0) as i16
    }
    fn set_offset(self, m: &mut Mem, v: i16) {
        m.w16(self.0, v as u16)
    }
    fn transition(self, m: &mut Mem) -> i16 {
        m.r16(self.0 + 2) as i16
    }
    fn set_transition(self, m: &mut Mem, v: i16) {
        m.w16(self.0 + 2, v as u16)
    }
    fn speed(self, m: &mut Mem) -> i16 {
        m.r16(self.0 + 4) as i16
    }
    fn set_speed(self, m: &mut Mem, v: i16) {
        m.w16(self.0 + 4, v as u16)
    }
    fn enabled(self, m: &mut Mem) -> i16 {
        m.r16(self.0 + 6) as i16
    }
    fn set_enabled(self, m: &mut Mem, v: i16) {
        m.w16(self.0 + 6, v as u16)
    }
}

/// Where a sliding item's box rests.
const REST: i16 = 14;

/// The fade toward enabled or disabled (12 a frame), within 0..128.
fn step_enabled_fade(m: &mut Mem, item: MenuItem, s: Slide) {
    let step = if item.flags(m) & ITEM_DISABLED == 0 { 12 } else { -12 };
    let v = s.enabled(m).wrapping_add(step).clamp(0, 128);
    s.set_enabled(m, v);
}

/// The text box's width: 115 scaled by the lesser of the transition and the enabled fade.
fn box_width(m: &mut Mem, s: Slide) -> i16 {
    let v = s.transition(m).min(s.enabled(m)) as i32;
    (v * 115 / 128) as i16
}

/// The slide toward the box's place (2 a frame selected, 4 not), round from the text's width
/// past the box (`width`) when it would go past 0.
fn slide(m: &mut Mem, s: Slide, selected: u32, width: u32) {
    let back = if (width as i16) < 140 { 165 } else { width.wrapping_add(25) as i16 };
    let step = if selected != 0 { 2 } else { 4 };
    let v = s.offset(m).wrapping_sub(step);
    s.set_offset(m, if v < 0 { v.wrapping_add(back) } else { v });
}

/// A sliding item's `TransitionOn` (the enabled fade starts again too), `TransitionOff`,
/// `TransitionIsFinished`, `UpdateTransition`.
fn transition_on(m: &mut Mem, s: Slide) {
    s.set_enabled(m, 0);
    s.set_speed(m, 12);
}

fn transition_off(m: &mut Mem, s: Slide) {
    s.set_speed(m, -12);
}

fn transition_is_finished(m: &mut Mem, s: Slide) -> u32 {
    (s.speed(m) == 0) as u32
}

fn update_transition(c: &mut Cpu, item: MenuItem, s: Slide, selected: u32) {
    let m = &mut c.m;
    let v = s.transition(m).wrapping_add(s.speed(m));
    if v < 0 || v > 128 {
        s.set_transition(m, v.clamp(0, 128));
        s.set_speed(m, 0);
    } else {
        s.set_transition(m, v);
    }
    femenu::menu_item_update_transition(c, item, selected);
}

/// A sliding item drawn at (`x`, `y`): its box, and its reflection below (`REFLECTED`).
fn draw_sliding(c: &mut Cpu, item: MenuItem, s: Slide, x: u32, y: u32, arrows: impl Fn(&mut Mem) -> u32) {
    with_buffer(c, 8, |c, at| {
        let rect = RECT(at);
        for reflection in [false, true] {
            if reflection {
                if item.flags(&mut c.m) & REFLECTED == 0 {
                    return;
                }
                call(c, SET_BLITTER, &[MIRROR_BLITTER]);
            }
            let m = &mut c.m;
            let y = if reflection { 280u32.wrapping_sub(y) } else { y };
            set_rect(m, rect, x as i16, y as i16, 115, 11);
            let w = box_width(m, s);
            rect.set_w(m, w);
            let (text, offset, fade) = (item.text_description(m), s.offset(m) as i32 as u32, item.sel_fade(m) as i32 as u32);
            let arrows = arrows(m);
            draw_text_box(c, text, rect, 140, offset, fade, arrows, reflection as u32);
        }
        call(c, RESET_BLITTER, &[]);
    })
}

/// `tMenuItemGoToMenuNFS4Button::tMenuItemGoToMenuNFS4Button` (FEMENUEXTENDED.CPP,
/// 0x8001a60c): a button going to `menu` (its box resting), with a button image of `frames`
/// frames.
pub fn go_to_menu_nfs4_button(c: &mut Cpu, this: MenuItemGoToMenuNFS4Button, text: u32, menu: Menu, on_press: u32, image: u32, frames: u32) -> u32 {
    femenu::go_to_menu_button(c, this.base(), text, menu, on_press);
    let (m, s) = (&mut c.m, Slide::of_button(this));
    this.set_vtable(m, GO_TO_MENU_NFS4_BUTTON_VT);
    s.set_offset(m, REST);
    s.set_transition(m, 0);
    s.set_speed(m, 0);
    s.set_enabled(m, 0);
    this.set_num_frames(m, frames as i32);
    this.set_button_image(m, image as i32);
    this.0
}

pub fn abi_go_to_menu_nfs4_button(c: &mut Cpu) {
    let (image, frames) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5));
    c.r[2] = go_to_menu_nfs4_button(c, MenuItemGoToMenuNFS4Button(c.r[4]), c.r[5], Menu(c.r[6]), c.r[7], image, frames);
}

/// A destructor: the class's vtable put back (when it has one: `(vptr, vtable)`), then the
/// base class's destructor `base` (its game entry) with the caller's `flags`.
pub(crate) fn destroy(c: &mut Cpu, this: u32, flags: u32, vtable: Option<(u32, u32)>, base: fn(&mut Cpu)) {
    if let Some((vptr, vt)) = vtable {
        c.m.w32(this + vptr, vt);
    }
    call(c, base, &[this, flags]);
}

/// `tMenuItemGoToMenuNFS4Button::~tMenuItemGoToMenuNFS4Button` (FEMENUEXTENDED.CPP,
/// 0x8001a674).
pub fn abi_go_to_menu_nfs4_button_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], c.r[5], Some((ITEM_VPTR, GO_TO_MENU_NFS4_BUTTON_VT)), femenu::abi_go_to_menu_button_dtor);
}

/// `tMenuItemGoToMenuNFS4Button::Draw` (FEMENUEXTENDED.CPP, 0x8001a69c): the button's box at
/// (`x`, `y`), sliding to rest unless resting unselected (by its upper-case text's width), fading
/// as it is enabled or not and as it transitions; no arrows.
pub fn go_to_menu_nfs4_button_draw(c: &mut Cpu, this: MenuItemGoToMenuNFS4Button, x: u32, y: u32, selected: u32) {
    let s = Slide::of_button(this);
    if selected == 0 && s.offset(&mut c.m) < REST + 2 {
        s.set_offset(&mut c.m, REST);
    } else {
        call(c, SET_FONT, &[0]);
        let text = this.text_description(&mut c.m);
        let width = with_buffer(c, 88, |c, buf| upper_text_width(c, text, buf));
        slide(&mut c.m, s, selected, width);
    }
    step_enabled_fade(&mut c.m, this.base().base().base(), s);
    draw_sliding(c, this.base().base().base(), s, x, y, |_| 0);
}

pub fn abi_go_to_menu_nfs4_button_draw(c: &mut Cpu) {
    go_to_menu_nfs4_button_draw(c, MenuItemGoToMenuNFS4Button(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

/// `tMenuItemGoToMenuNFS4Button::TransitionOn` (FEMENUEXTENDED.CPP, 0x8001a918).
pub fn abi_button_transition_on(c: &mut Cpu) {
    transition_on(&mut c.m, Slide::of_button(MenuItemGoToMenuNFS4Button(c.r[4])));
}

/// `tMenuItemGoToMenuNFS4Button::TransitionOff` (FEMENUEXTENDED.CPP, 0x8001a928).
pub fn abi_button_transition_off(c: &mut Cpu) {
    transition_off(&mut c.m, Slide::of_button(MenuItemGoToMenuNFS4Button(c.r[4])));
}

/// `tMenuItemGoToMenuNFS4Button::TransitionIsFinished` (FEMENUEXTENDED.CPP, 0x8001a934).
pub fn abi_button_transition_is_finished(c: &mut Cpu) {
    c.r[2] = transition_is_finished(&mut c.m, Slide::of_button(MenuItemGoToMenuNFS4Button(c.r[4])));
}

/// `tMenuItemGoToMenuNFS4Button::UpdateTransition` (FEMENUEXTENDED.CPP, 0x8001a940).
pub fn abi_button_update_transition(c: &mut Cpu) {
    let this = MenuItemGoToMenuNFS4Button(c.r[4]);
    update_transition(c, MenuItem(this.0), Slide::of_button(this), c.r[5]);
}

/// `tMenuItemNFS4LeftRightChoice::tMenuItemNFS4LeftRightChoice` (FEMENUEXTENDED.CPP,
/// 0x8001a99c): a left-right choice that slides in, with a button image of `frames` frames.
pub fn nfs4_left_right_choice(c: &mut Cpu, this: MenuItemNFS4LeftRightChoice, text: u32, data: ListIterator, image: u32, frames: u32) -> u32 {
    femenu::left_right_choice(c, this.base(), text, data);
    let (m, s) = (&mut c.m, Slide::of_choice(this));
    this.set_vtable(m, NFS4_LEFT_RIGHT_CHOICE_VT);
    this.set_button_image(m, image as i32);
    s.set_offset(m, REST);
    s.set_transition(m, 0);
    s.set_speed(m, 0);
    this.set_num_frames(m, frames as i32);
    this.0
}

pub fn abi_nfs4_left_right_choice(c: &mut Cpu) {
    let frames = crate::abi::arg(c, 4);
    c.r[2] = nfs4_left_right_choice(c, MenuItemNFS4LeftRightChoice(c.r[4]), c.r[5], ListIterator(c.r[6]), c.r[7], frames);
}

/// `tMenuItemNFS4LeftRightChoice::~tMenuItemNFS4LeftRightChoice` (FEMENUEXTENDED.CPP,
/// 0x8001aa00).
pub fn abi_nfs4_left_right_choice_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], c.r[5], Some((ITEM_VPTR, NFS4_LEFT_RIGHT_CHOICE_VT)), femenu::abi_left_right_choice_dtor);
}

/// `tMenuItemNFS4LeftRightChoice::Draw` (FEMENUEXTENDED.CPP, 0x8001aa28): as the button's, by
/// its text's width as it is; the arrows unless it rests unselected.
pub fn nfs4_left_right_choice_draw(c: &mut Cpu, this: MenuItemNFS4LeftRightChoice, x: u32, y: u32, selected: u32) {
    let s = Slide::of_choice(this);
    if selected == 0 && s.offset(&mut c.m) < REST + 2 {
        s.set_offset(&mut c.m, REST);
    } else {
        call(c, SET_FONT, &[0]);
        let text = this.text_description(&mut c.m);
        let word = call(c, TEXT_WORD, &[text]);
        let width = call(c, TEXT_PIXELS, &[word]);
        slide(&mut c.m, s, selected, width);
    }
    let item = this.base().base().base();
    step_enabled_fade(&mut c.m, item, s);
    draw_sliding(c, item, s, x, y, |m| (selected != 0 || s.offset(m) != REST) as u32);
}

pub fn abi_nfs4_left_right_choice_draw(c: &mut Cpu) {
    nfs4_left_right_choice_draw(c, MenuItemNFS4LeftRightChoice(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

/// `tMenuItemNFS4LeftRightChoice::TransitionOn` (FEMENUEXTENDED.CPP, 0x8001acbc).
pub fn abi_choice_transition_on(c: &mut Cpu) {
    transition_on(&mut c.m, Slide::of_choice(MenuItemNFS4LeftRightChoice(c.r[4])));
}

/// `tMenuItemNFS4LeftRightChoice::TransitionOff` (FEMENUEXTENDED.CPP, 0x8001accc).
pub fn abi_choice_transition_off(c: &mut Cpu) {
    transition_off(&mut c.m, Slide::of_choice(MenuItemNFS4LeftRightChoice(c.r[4])));
}

/// `tMenuItemNFS4LeftRightChoice::TransitionIsFinished` (FEMENUEXTENDED.CPP, 0x8001acd8).
pub fn abi_choice_transition_is_finished(c: &mut Cpu) {
    c.r[2] = transition_is_finished(&mut c.m, Slide::of_choice(MenuItemNFS4LeftRightChoice(c.r[4])));
}

/// `tMenuItemNFS4LeftRightChoice::UpdateTransition` (FEMENUEXTENDED.CPP, 0x8001ace4).
pub fn abi_choice_update_transition(c: &mut Cpu) {
    let this = MenuItemNFS4LeftRightChoice(c.r[4]);
    update_transition(c, MenuItem(this.0), Slide::of_choice(this), c.r[5]);
}

// ---- The options screen's choices: the item's text, its value between arrows, in a box.

/// The arrows either side of the value and the rounded box round the row (both options
/// choices' `Draw` end so); `shapes` the arrows' shape.
fn options_arrows_and_box(c: &mut Cpu, l: &Locals, item: MenuItem, x: u32, y: u32, selected: u32, shapes: u32) {
    let fade = item.sel_fade(&mut c.m) as i32 as u32;
    let colour = call(c, CALC_FADE_VAL, &[BOX_DIM, BOX_LIT, fade]);
    l.set_shape_colour(&mut c.m, colour);
    let dim = (selected == 0) as u32;
    call(c, DRAW_SHAPE_EXTENDED, &[LEFT_ARROW, ARROW_FLAGS, x.wrapping_add(166), y.wrapping_add(6), 0, dim, l.shape_options()]);
    let right = x.wrapping_sub(shape_width(&mut c.m, shapes) as i32 as u32).wrapping_add(303);
    call(c, DRAW_SHAPE_EXTENDED, &[RIGHT_ARROW, ARROW_FLAGS, right, y.wrapping_add(6), 0, dim, l.shape_options()]);
    let h = shape_height(&mut c.m, shapes);
    set_rect(&mut c.m, l.rect(), x as i16, y as i16, 297, h);
    call(c, ROUND_RECTANGLE, &[u32::MAX, l.rect().0, sx(selected)]);
}

/// `tMenuItemOptionsLeftRightChoice::Draw` (FEMENUEXTENDED.CPP, 0x8001ad40): the item's text
/// and its value's (centred on the value's place) between the arrows, in the rounded box.
pub fn options_left_right_choice_draw(c: &mut Cpu, this: MenuItemLeftRightChoice, x: u32, y: u32, selected: u32) {
    with_buffer(c, Locals::SIZE, |c, at| {
        let l = Locals(at);
        let shapes = c.m.r32(SHAPES).wrapping_add(OPTIONS_SHAPE);
        let fade = this.sel_fade(&mut c.m) as i32 as u32;
        let colour = call(c, CALC_TEXT_FADE_SEL_TO_HI, &[6, fade, 0]);
        let text = this.text_description(&mut c.m);
        let word = call(c, TEXT_WORD, &[text]);
        let ty = sx(y.wrapping_add(3));
        call(c, FULL_TEXT_RGB, &[word, sx(x.wrapping_add(148)), ty, colour, 0, 1]);
        let data = this.data(&mut c.m);
        let value = call_virtual(c, data.0, ITERATOR_VPTR, TEXT_VALUE, &[u32::MAX]);
        let word = call(c, TEXT_WORD, &[sx(value)]);
        let half = (shape_width(&mut c.m, shapes) >> 1) as i32 as u32;
        let vx = sx(half.wrapping_add(x).wrapping_add(217));
        call(c, FULL_TEXT_RGB, &[word, vx, ty, colour, 0, 2]);
        options_arrows_and_box(c, &l, this.base().base(), x, y, selected, shapes);
    })
}

pub fn abi_options_left_right_choice_draw(c: &mut Cpu) {
    options_left_right_choice_draw(c, MenuItemLeftRightChoice(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

/// `tMenuItemOptionsTwoItemChoice::TransitionOn` (FEMENUEXTENDED.CPP, 0x8001af18): the on-off
/// fade at its value's end.
pub fn two_item_choice_transition_on(c: &mut Cpu, this: MenuItemOptionsTwoItemChoice) {
    let data = this.data(&mut c.m);
    let value = call_virtual(c, data.0, ITERATOR_VPTR, VALUE, &[u32::MAX]);
    this.set_on_off_fade(&mut c.m, if value & 0xFF != 0 { 128 } else { 0 });
}

pub fn abi_two_item_choice_transition_on(c: &mut Cpu) {
    two_item_choice_transition_on(c, MenuItemOptionsTwoItemChoice(c.r[4]));
}

/// `tMenuItemOptionsTwoItemChoice::Draw` (FEMENUEXTENDED.CPP, 0x8001af74): the item's text and
/// both of its choices (the list's two texts, in the colours of the on-off fade moving toward
/// the value, 64 a frame), between the arrows, in the rounded box.
pub fn two_item_choice_draw(c: &mut Cpu, this: MenuItemOptionsTwoItemChoice, x: u32, y: u32, selected: u32) {
    with_buffer(c, Locals::SIZE, |c, at| {
        let l = Locals(at);
        let data = this.data(&mut c.m);
        let shapes = c.m.r32(SHAPES).wrapping_add(OPTIONS_SHAPE);
        let value = call_virtual(c, data.0, ITERATOR_VPTR, VALUE, &[u32::MAX]);
        let m = &mut c.m;
        let step = if value & 0xFF != 0 { 64 } else { -64 };
        let v = this.on_off_fade(m).wrapping_add(step).clamp(0, 128);
        this.set_on_off_fade(m, v);
        let fade = this.sel_fade(m) as i32 as u32;
        let colour = call(c, CALC_TEXT_FADE_SEL_TO_HI, &[6, fade, 0]);
        let m = &mut c.m;
        let (on_off, fade) = (this.on_off_fade(m) as i32 as u32, this.sel_fade(m) as i32 as u32);
        call(c, CALC_ON_OFF_FADE, &[6, on_off, fade, 0, l.on_colour(), l.off_colour()]);
        let text = this.text_description(&mut c.m);
        let ty = sx(y.wrapping_add(3));
        let word = call(c, TEXT_WORD, &[text]);
        call(c, FULL_TEXT_RGB, &[word, sx(x.wrapping_add(148)), ty, colour, 0, 1]);
        // the two choices' texts, the first in the off colour, the second in the on one
        let choice = |m: &mut Mem, k: u32| {
            let list = this.data(m).selection_list(m);
            m.r16(list + 2 * k) as i16 as i32 as u32
        };
        let first = choice(&mut c.m, 0);
        let word = call(c, TEXT_WORD, &[first]);
        let off = c.m.r32(l.off_colour());
        call(c, FULL_TEXT_RGB, &[word, sx(x.wrapping_add(176)), ty, off, 0, 0]);
        let second = choice(&mut c.m, 1);
        let word = call(c, TEXT_WORD, &[second]);
        let m = &mut c.m;
        let x2 = sx(x.wrapping_sub(shape_width(m, shapes) as u16 as u32).wrapping_add(294));
        let on = m.r32(l.on_colour());
        call(c, FULL_TEXT_RGB, &[word, x2, ty, on, 0, 1]);
        options_arrows_and_box(c, &l, this.base().base().base(), x, y, selected, shapes);
    })
}

pub fn abi_two_item_choice_draw(c: &mut Cpu) {
    two_item_choice_draw(c, MenuItemOptionsTwoItemChoice(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

/// `tMenuItemOptionsTwoItemChoice::~tMenuItemOptionsTwoItemChoice`,
/// `tMenuItemOptionsLeftRightChoice::~tMenuItemOptionsLeftRightChoice` (FEMENUEXTENDED.CPP,
/// 0x8001c060, 0x8001c080): the left-right choice's (the vtable left as it is).
pub fn abi_two_item_choice_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], c.r[5], None, femenu::abi_left_right_choice_dtor);
}

pub fn abi_options_left_right_choice_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], c.r[5], None, femenu::abi_left_right_choice_dtor);
}

// ---- tMenuNFS4 and the menus derived from it.

/// The menu's `k`th item (read past the 16 of `item_list`, as the original does: see FEMENU).
fn item(m: &mut Mem, this: Menu, k: u32) -> MenuItem {
    MenuItem(m.r32(this.0.wrapping_add(0x10).wrapping_add(k.wrapping_mul(4))))
}

/// Whether item `k` is the current one.
fn is_current(m: &mut Mem, this: Menu, k: u32) -> u32 {
    (k == this.current_item(m) as u32) as u32
}

/// Each item in order (a short counter, from the first to the null).
fn items(c: &mut Cpu, this: Menu, mut each: impl FnMut(&mut Cpu, u32, MenuItem)) {
    let mut k = 0i16;
    loop {
        let it = item(&mut c.m, this, k as i32 as u32);
        if it.0 == 0 {
            return;
        }
        each(c, k as i32 as u32, it);
        k = k.wrapping_add(1);
    }
}

/// A variadic NFS4 menu constructor's arguments: the menu's, then its items (`first`, the rest
/// listed after it on the caller's stack to a null).
struct MenuArgs {
    flags: u32,
    screen: u32,
    next: u32,
    options: u32,
    on_back: u32,
    title: u32,
}

impl MenuArgs {
    /// A constructor's arguments as it was called: this, flags, screen, next menu in
    /// registers, options, on back and title on the stack.
    fn of(c: &mut Cpu) -> Self {
        let (options, on_back, title) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5), crate::abi::arg(c, 6));
        MenuArgs { flags: c.r[5], screen: c.r[6], next: c.r[7], options, on_back, title: sx(title) }
    }
}

/// `tMenuNFS4::tMenuNFS4` (with items: FEMENUEXTENDED.CPP, 0x8001b1f8).
pub fn abi_menu_nfs4(c: &mut Cpu) {
    let (this, a) = (Menu(c.r[4]), MenuArgs::of(c));
    let (first, rest) = (crate::abi::arg(c, 7), stack_arg(c, 8));
    femenu::menu(&mut c.m, this, a.flags, a.screen, a.next, a.options, a.on_back, a.title);
    this.set_vtable(&mut c.m, MENU_NFS4_VT);
    femenu::menu_constructor(&mut c.m, this, first, rest);
    c.r[2] = this.0;
}

/// `tMenuNFS4::tMenuNFS4` (without items: FEMENUEXTENDED.CPP, 0x8001b260).
#[allow(clippy::too_many_arguments)]
pub fn menu_nfs4_bare(c: &mut Cpu, this: u32, flags: u32, screen: u32, next: u32, options: u32, on_back: u32, title: u32) -> u32 {
    femenu::menu(&mut c.m, Menu(this), flags, screen, next, options, on_back, sx(title));
    c.m.w32(this + MENU_VPTR, MENU_NFS4_VT);
    this
}

pub fn abi_menu_nfs4_bare(c: &mut Cpu) {
    let a = MenuArgs::of(c);
    c.r[2] = menu_nfs4_bare(c, c.r[4], a.flags, a.screen, a.next, a.options, a.on_back, a.title);
}

/// A menu derived from `tMenuNFS4`: its bare constructor, then the class's vtable, then (when
/// it has them) the items from `first` and the list `rest`.
fn construct_nfs4(c: &mut Cpu, this: Menu, a: &MenuArgs, vt: u32, items: Option<(u32, u32)>) {
    menu_nfs4_bare(c, this.0, a.flags, a.screen, a.next, a.options, a.on_back, a.title);
    this.set_vtable(&mut c.m, vt);
    if vt == MENU_NFS4_TWO_PLAYER_VT {
        this.set_child_menu(&mut c.m, Menu(0));
    }
    if let Some((first, rest)) = items {
        femenu::menu_constructor(&mut c.m, this, first, rest);
    }
}

/// The constructors with items of the two-player and bottom menus.
fn construct_with_items(c: &mut Cpu, vt: u32) {
    let (this, a) = (Menu(c.r[4]), MenuArgs::of(c));
    let (first, rest) = (crate::abi::arg(c, 7), stack_arg(c, 8));
    construct_nfs4(c, this, &a, vt, Some((first, rest)));
    c.r[2] = this.0;
}

/// `tMenuNFS4::~tMenuNFS4` (FEMENUEXTENDED.CPP, 0x8001b2b8).
pub fn abi_menu_nfs4_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], c.r[5], Some((MENU_VPTR, MENU_NFS4_VT)), femenu::abi_menu_dtor);
}

/// `tMenuNFS4::Initialize` (FEMENUEXTENDED.CPP, 0x8001b2e0): `tMenu`'s; no transitions, the
/// current item noted as the last, its items counted; a reflected menu's items reflected.
pub fn menu_nfs4_initialize(c: &mut Cpu, this: MenuNFS4) {
    let menu = this.base();
    femenu::initialize(&mut c.m, menu);
    let m = &mut c.m;
    let current = menu.current_item(m) as u8;
    let first = item(m, menu, 0);
    this.set_in_item_transition(m, 0);
    this.set_in_menu_transition(m, 0);
    this.set_num_items(m, 0);
    this.set_last_item(m, current as i8);
    if first.0 != 0 {
        loop {
            let n = this.num_items(m).wrapping_add(1);
            this.set_num_items(m, n);
            if item(m, menu, n as u8 as u32).0 == 0 {
                break;
            }
        }
    }
    if menu.flags(m) & REFLECTED != 0 {
        items(c, menu, |c, _, it| it.update_flags(&mut c.m, |f| f | REFLECTED));
    }
}

pub fn abi_menu_nfs4_initialize(c: &mut Cpu) {
    menu_nfs4_initialize(c, MenuNFS4(c.r[4]));
}

/// `tMenuNFS4::ProcessInput` (FEMENUEXTENDED.CPP, 0x8001b390): `tMenu`'s.
pub fn menu_nfs4_process_input(c: &mut Cpu, this: u32, player: u32, key: u32, command: u32) {
    femenu::process_input(c, Menu(this), player, key, MenuCommand(command));
}

pub fn abi_menu_nfs4_process_input(c: &mut Cpu) {
    menu_nfs4_process_input(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tMenuNFS4::TransitionOff` (FEMENUEXTENDED.CPP, 0x8001b3b0): each item's.
pub fn abi_menu_nfs4_transition_off(c: &mut Cpu) {
    items(c, Menu(c.r[4]), |c, _, it| {
        call_virtual(c, it.0, ITEM_VPTR, ITEM_TRANSITION_OFF, &[]);
    });
}

/// `tMenuNFS4::TransitionOn` (FEMENUEXTENDED.CPP, 0x8001b438): each item's.
pub fn abi_menu_nfs4_transition_on(c: &mut Cpu) {
    items(c, Menu(c.r[4]), |c, _, it| {
        call_virtual(c, it.0, ITEM_VPTR, ITEM_TRANSITION_ON, &[]);
    });
}

/// `tMenuNFS4::TransitionIsFinished` (FEMENUEXTENDED.CPP, 0x8001b4c0): whether every item's
/// is (each asked; their results' low bits and'd).
pub fn abi_menu_nfs4_transition_is_finished(c: &mut Cpu) {
    let mut done = 1u32;
    items(c, Menu(c.r[4]), |c, _, it| {
        let v = call_virtual(c, it.0, ITEM_VPTR, ITEM_TRANSITION_IS_FINISHED, &[]);
        done = (done & v != 0) as u32;
    });
    c.r[2] = done;
}

/// `tMenuNFS4::UpdateTransition` (FEMENUEXTENDED.CPP, 0x8001b560): each item's, told whether it
/// is the current one.
pub fn abi_menu_nfs4_update_transition(c: &mut Cpu) {
    let this = Menu(c.r[4]);
    items(c, this, |c, k, it| {
        let selected = is_current(&mut c.m, this, k);
        call_virtual(c, it.0, ITEM_VPTR, ITEM_UPDATE_TRANSITION, &[selected]);
    });
}

/// Item `k` drawn at (`x`, `y`), told whether it is the current one.
fn draw_item_at(c: &mut Cpu, this: Menu, k: u32, x: u32, y: u32) {
    let it = item(&mut c.m, this, k);
    let selected = is_current(&mut c.m, this, k);
    call_virtual(c, it.0, ITEM_VPTR, ITEM_DRAW_AT, &[x, y, selected]);
}

/// `tMenuNFS4::DrawItem` (FEMENUEXTENDED.CPP, 0x8001b5f4): item `k` at (10, 43 + 18k).
pub fn abi_menu_nfs4_draw_item(c: &mut Cpu) {
    let (this, k) = (Menu(c.r[4]), c.r[5]);
    draw_item_at(c, this, k, 10, k.wrapping_mul(18).wrapping_add(43));
}

/// The front end's player whose menus are drawn (the second's half is lower).
fn drawing_player(m: &mut Mem) -> i8 {
    FEApplication(m.r32(FE_APP)).player(m)
}

/// `tMenuNFS4::Draw` (FEMENUEXTENDED.CPP, 0x8001b654): its title, the current item's button
/// image (animated: a frame every 16 ticks; lower for the second player), each item (its
/// virtual `DrawItem`).
pub fn menu_nfs4_draw(c: &mut Cpu, this: Menu) {
    let title = this.title(&mut c.m);
    if title >= 0 {
        call(c, TITLE, &[title as i32 as u32]);
    }
    femenu::initialize(&mut c.m, this);
    let m = &mut c.m;
    let current = this.current_item(m) as u32;
    let it = item(m, this, current);
    let (image, frames) = (it.button_image(m), it.num_frames(m));
    if image >= 0 && frames > 0 {
        let frame = ((m.r32(GAME_TICKS) as i32) >> 4).wrapping_rem(frames);
        let y = if drawing_player(m) != 0 { 121 } else { 16 };
        with_buffer(c, Locals::SIZE, |c, at| {
            let l = Locals(at);
            l.set_shape_colour(&mut c.m, BUTTON_IMAGE_COLOUR);
            call(c, DRAW_SHAPE_EXTENDED, &[image.wrapping_add(frame) as u32, BUTTON_IMAGE_FLAGS, 16, y, 0, 0, l.shape_options()]);
        });
    }
    items(c, this, |c, k, _| {
        call_virtual(c, this.0, MENU_VPTR, MENU_DRAW_ITEM, &[k]);
    });
}

pub fn abi_menu_nfs4_draw(c: &mut Cpu) {
    menu_nfs4_draw(c, Menu(c.r[4]));
}

/// `tMenuNFS4TwoPlayer::tMenuNFS4TwoPlayer` (FEMENUEXTENDED.CPP, 0x8001b79c).
pub fn abi_menu_two_player(c: &mut Cpu) {
    construct_with_items(c, MENU_NFS4_TWO_PLAYER_VT);
}

/// `tMenuNFS4TwoPlayer::~tMenuNFS4TwoPlayer` (FEMENUEXTENDED.CPP, 0x8001b808).
pub fn abi_menu_two_player_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], c.r[5], Some((MENU_VPTR, MENU_NFS4_TWO_PLAYER_VT)), abi_menu_nfs4_dtor);
}

/// `tMenuNFS4TwoPlayer::DrawItem` (FEMENUEXTENDED.CPP, 0x8001b830): item `k` in the drawing
/// player's half of the screen.
pub fn abi_menu_two_player_draw_item(c: &mut Cpu) {
    let (this, k) = (Menu(c.r[4]), c.r[5]);
    let top = if drawing_player(&mut c.m) == 1 { 148 } else { 43 };
    draw_item_at(c, this, k, 10, k.wrapping_mul(18).wrapping_add(top));
}

/// `tMenuNFS4Bottom::tMenuNFS4Bottom` (FEMENUEXTENDED.CPP, 0x8001b8a8).
pub fn abi_menu_bottom(c: &mut Cpu) {
    construct_with_items(c, MENU_NFS4_BOTTOM_VT);
}

/// `tMenuNFS4Bottom::~tMenuNFS4Bottom` (FEMENUEXTENDED.CPP, 0x8001b910).
pub fn abi_menu_bottom_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], c.r[5], Some((MENU_VPTR, MENU_NFS4_BOTTOM_VT)), abi_menu_nfs4_dtor);
}

/// `tMenuNFS4Bottom::Draw` (FEMENUEXTENDED.CPP, 0x8001b938): the help at the bottom, each item
/// (where it puts itself: given no place).
pub fn menu_bottom_draw(c: &mut Cpu, this: Menu) {
    with_buffer(c, 8, |c, at| {
        set_rect(&mut c.m, RECT(at), 57, 198, 114, 11);
        set_help_pos(&mut c.m, at);
    });
    items(c, this, |c, k, it| {
        let selected = is_current(&mut c.m, this, k);
        call_virtual(c, it.0, ITEM_VPTR, ITEM_DRAW_AT, &[0, 0, selected]);
    });
}

pub fn abi_menu_bottom_draw(c: &mut Cpu) {
    menu_bottom_draw(c, Menu(c.r[4]));
}

/// `tMenuBlank::tMenuBlank` (FEMENUEXTENDED.CPP, 0x8001b9f8): a menu of no items, never any
/// enabled.
pub fn abi_menu_blank(c: &mut Cpu) {
    let (this, a) = (Menu(c.r[4]), MenuArgs::of(c));
    construct_nfs4(c, this, &a, MENU_BLANK_VT, None);
    this.set_never_any_enabled(&mut c.m, 1);
    this.set_vert_help(&mut c.m, 0);
    c.r[2] = this.0;
}

/// `tMenuBlank::~tMenuBlank` (FEMENUEXTENDED.CPP, 0x8001ba5c).
pub fn abi_menu_blank_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], c.r[5], Some((MENU_VPTR, MENU_BLANK_VT)), abi_menu_nfs4_dtor);
}

/// `tMenuBlank::Draw` (FEMENUEXTENDED.CPP, 0x8001ba84): in a transition, it steps (its virtual
/// `TransitionIsFinished` asked first).
pub fn menu_blank_draw(c: &mut Cpu, this: MenuBlank) {
    if this.in_menu_transition(&mut c.m) == 0 {
        return;
    }
    call_virtual(c, this.0, MENU_VPTR, MENU_TRANSITION_IS_FINISHED, &[]);
    let m = &mut c.m;
    let step = this.transition_direction(m) as i16;
    this.update_transition_val(m, |v| v.wrapping_add(step));
}

pub fn abi_menu_blank_draw(c: &mut Cpu) {
    menu_blank_draw(c, MenuBlank(c.r[4]));
}

/// `tMenuBlank::ProcessInput` (FEMENUEXTENDED.CPP, 0x8001bae8): up and down are used up; other
/// keys go to `tMenu`'s, the items forgotten first.
pub fn menu_blank_process_input(c: &mut Cpu, this: Menu, player: u32, key: u32, command: u32) {
    let k = c.m.r32(key);
    if k == femenu::KEY_UP || k == femenu::KEY_DOWN {
        c.m.w32(key, KEY_NONE);
        return;
    }
    this.set_item_list(&mut c.m, 0, MenuItem(0));
    femenu::process_input(c, this, player, key, MenuCommand(command));
}

pub fn abi_menu_blank_process_input(c: &mut Cpu) {
    menu_blank_process_input(c, Menu(c.r[4]), c.r[5], c.r[6], c.r[7]);
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
    let (m, this) = (&mut c.m, MenuBlank(c.r[4]));
    this.set_transition_direction(m, 8);
    this.set_in_menu_transition(m, 1);
    this.set_transition_val(m, -112);
}

/// `tMenuBlank::TransitionOn` (FEMENUEXTENDED.CPP, 0x8001bb58): sliding up from 0.
pub fn abi_menu_blank_transition_on(c: &mut Cpu) {
    let (m, this) = (&mut c.m, MenuBlank(c.r[4]));
    this.set_transition_direction(m, -8);
    this.set_in_menu_transition(m, 1);
    this.set_transition_val(m, 0);
}

/// `tMenuBlank::TransitionIsFinished` (FEMENUEXTENDED.CPP, 0x8001bb70): done sliding down at 0,
/// up at -112.
pub fn abi_menu_blank_transition_is_finished(c: &mut Cpu) {
    let (m, this) = (&mut c.m, MenuBlank(c.r[4]));
    let pos = this.transition_val(m);
    let moving = if this.transition_direction(m) > 0 { pos < 0 } else { pos >= -111 };
    this.set_in_menu_transition(m, moving as u32);
    c.r[2] = this.in_menu_transition(m) ^ 1;
}

/// `tMenuOptions::tMenuOptions` (FEMENUEXTENDED.CPP, 0x8001bbac): an options menu for `player`
/// (after the menu's arguments; its items after it).
pub fn abi_menu_options(c: &mut Cpu) {
    let (this, a) = (MenuOptions(c.r[4]), MenuArgs::of(c));
    let (player, first, rest) = (crate::abi::arg(c, 7), crate::abi::arg(c, 8), stack_arg(c, 9));
    construct_nfs4(c, Menu(this.0), &a, MENU_OPTIONS_VT, Some((first, rest)));
    this.set_player(&mut c.m, player as i16);
    c.r[2] = this.0;
}

/// `tMenuOptions::~tMenuOptions` (FEMENUEXTENDED.CPP, 0x8001bc24).
pub fn abi_menu_options_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], c.r[5], Some((MENU_VPTR, MENU_OPTIONS_VT)), abi_menu_nfs4_dtor);
}

/// How long the options box takes to open or close, in ticks.
const OPTIONS_OPENING: u32 = 32;

/// `tMenuOptions::Draw` (FEMENUEXTENDED.CPP, 0x8001bc4c): the options box (320 wide, 18 high a
/// line of the enabled items; growing out of the middle or shrinking into it over 32 ticks in
/// a transition), centred (in the player's half of the screen for one of two players), with
/// its title and the enabled items once open; its back and edges.
pub fn menu_options_draw(c: &mut Cpu, this: MenuOptions) {
    let menu = Menu(this.0);
    let lines = femenu::get_number_enabled_items(&mut c.m, menu);
    call_virtual(c, this.0, MENU_VPTR, MENU_TRANSITION_IS_FINISHED, &[]);
    let m = &mut c.m;
    let closing = this.transition_direction(m) < 0;
    let (mut width, mut height) = (320u32, (lines as i16 as i32 as u32).wrapping_mul(18));
    if this.in_menu_transition(m) != 0 {
        let mut t = m.r32(GAME_TICKS).wrapping_sub(this.menu_enter_ticks(m));
        if t > OPTIONS_OPENING {
            t = OPTIONS_OPENING;
            this.set_in_menu_transition(m, 0);
            if this.transition_direction(m) < 0 {
                return;
            }
        }
        let (grown_w, grown_h) = (t.wrapping_mul(320) >> 5, height.wrapping_mul(t) >> 5);
        if this.transition_direction(m) < 0 {
            width = 320u32.wrapping_sub(grown_w);
            height = height.wrapping_sub(grown_h);
        } else {
            width = grown_w;
            height = grown_h;
        }
    } else if closing {
        return;
    }
    let height = height.wrapping_add(18);
    let spare = 240u32.wrapping_sub(height) as i32;
    let mut top = (spare >> 1) as u32;
    let left = ((m.r32(SCREEN_WIDTH).wrapping_sub(width) as i32) >> 1) as u32;
    match this.player(m) {
        0 => top = top.wrapping_sub((spare >> 2) as u32),
        1 => top = top.wrapping_add((spare >> 2) as u32),
        _ => {}
    }
    if this.in_menu_transition(m) == 0 {
        let title = menu.title(m);
        if title >= 0 {
            let x = sx(left.wrapping_add(((width as i32) >> 1) as u32));
            call(c, MENU_TEXT_POSITIONED_JUSTIFY, &[title as i32 as u32, x, sx(top.wrapping_add(2)), 2, 2, 9]);
        }
        let mut y = top.wrapping_add(18);
        items(c, menu, |c, k, it| {
            if it.flags(&mut c.m) & ITEM_DISABLED == 0 {
                let selected = is_current(&mut c.m, menu, k);
                call_virtual(c, it.0, ITEM_VPTR, ITEM_DRAW_AT, &[left.wrapping_add(10), y, selected]);
                y = y.wrapping_add(18);
            }
        });
    }
    // the title bar, the shade under the box, its sides, its back
    call_addr(c, DRAW_SQUARE, &[0, left, top, width, 12]);
    call_addr(c, DRAW_TRANS_SQUARE, &[0, left, top.wrapping_add(height), width, (-2i32) as u32, 1]);
    let (sides_top, sides_h) = (top.wrapping_add(12), height.wrapping_sub(14));
    call_addr(c, DRAW_TRANS_SQUARE, &[0, left, sides_top, 4, sides_h, 1]);
    call_addr(c, DRAW_TRANS_SQUARE, &[0, left.wrapping_add(width), sides_top, (-4i32) as u32, sides_h, 1]);
    call_addr(c, DRAW_TRANS_SQUARE, &[0, left, top, width, height, 1]);
    call(c, SET_ABR_MODE, &[0]);
}

pub fn abi_menu_options_draw(c: &mut Cpu) {
    menu_options_draw(c, MenuOptions(c.r[4]));
}

/// The options box's sounds: opening, closing.
const SOUND_OPEN: u32 = 15;
const SOUND_CLOSE: u32 = 18;

/// `tMenuOptions::TransitionOff` (FEMENUEXTENDED.CPP, 0x8001bf24): shrinking, from now, with a
/// sound.
pub fn menu_options_transition_off(c: &mut Cpu, this: MenuOptions) {
    let m = &mut c.m;
    this.set_transition_direction(m, -1);
    let now = m.r32(GAME_TICKS);
    this.set_in_menu_transition(m, 1);
    this.set_menu_enter_ticks(m, now);
    call(c, PLAY_FE_SFX, &[SOUND_CLOSE]);
}

pub fn abi_menu_options_transition_off(c: &mut Cpu) {
    menu_options_transition_off(c, MenuOptions(c.r[4]));
}

/// `tMenuOptions::TransitionOn` (FEMENUEXTENDED.CPP, 0x8001bf60): each enabled item's, then
/// growing, from now, with a sound.
pub fn menu_options_transition_on(c: &mut Cpu, this: MenuOptions) {
    let mut k = 0u32;
    loop {
        let it = item(&mut c.m, Menu(this.0), k);
        if it.0 == 0 {
            break;
        }
        if it.flags(&mut c.m) & ITEM_DISABLED == 0 {
            call_virtual(c, it.0, ITEM_VPTR, ITEM_TRANSITION_ON, &[]);
        }
        k += 1;
    }
    let m = &mut c.m;
    this.set_transition_direction(m, 1);
    let now = m.r32(GAME_TICKS);
    this.set_in_menu_transition(m, 1);
    this.set_menu_enter_ticks(m, now);
    call(c, PLAY_FE_SFX, &[SOUND_OPEN]);
}

pub fn abi_menu_options_transition_on(c: &mut Cpu) {
    menu_options_transition_on(c, MenuOptions(c.r[4]));
}

/// `tMenuOptions::TransitionIsFinished` (FEMENUEXTENDED.CPP, 0x8001bff8): 32 ticks after it
/// began.
pub fn abi_menu_options_transition_is_finished(c: &mut Cpu) {
    let (m, this) = (&mut c.m, MenuOptions(c.r[4]));
    let moving = m.r32(GAME_TICKS).wrapping_sub(this.menu_enter_ticks(m)) < OPTIONS_OPENING;
    this.set_in_menu_transition(m, moving as u32);
    c.r[2] = !moving as u32;
}

/// `tMenuOptions::ProcessInput` (FEMENUEXTENDED.CPP, 0x8001c01c): options taken as start; then
/// `tMenuNFS4`'s.
pub fn menu_options_process_input(c: &mut Cpu, this: u32, player: u32, key: u32, command: u32) {
    if c.m.r32(key) == femenu::KEY_OPTIONS {
        c.m.w32(key, femenu::KEY_BACK);
    }
    menu_nfs4_process_input(c, this, player, key, command);
}

pub fn abi_menu_options_process_input(c: &mut Cpu) {
    menu_options_process_input(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tMenuOptions::IsSubMenu` (FEMENUEXTENDED.CPP, 0x8001c050): it is.
pub fn abi_menu_options_is_sub_menu(c: &mut Cpu) {
    c.r[2] = 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, COMMAND, ITEM_SIZE, KEY, MENU2, NEW, NOTHING, pick, player, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};
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

}
