//! FETEXTRENDER.CPP: the front end's text.
//! - `FullTextRGB`: a string in a font, a colour and a justification. The text types give
//!   their fonts, justifications and colours by state (`gTextStyles`), faded or not.
//! - `MenuText...`: a text id at a place, or at its own.
//! - `WordWrap...`: a text word-wrapped into a box (also measuring its height).
//! - The menus' titles, the fonts, the blending.
//!
//! Written as FEMENU.CPP is (see there). The `*_entry` functions are the game's entries, which
//! other modules' ports call through `crate::call::call`.

use crate::call::{call, call_addr, with_buffer};
use crate::globals::{CURRENT_FONT, FE_APP, FONTS, TEXT_ABR, TEXT_COLORS, TEXT_STYLES};
use crate::types::{FEApplication, RECT};
use ps1rt::{Cpu, Mem};

/// The font library: `Font_SwitchFont`, `Font_TextColor`, `Font_TextTint`, `Font_SetABR`;
/// `Font_TextXY` (the host draws the front end's text: called by address).
const SWITCH_FONT: fn(&mut Cpu) = nfs4rc::f_800cc54c;
const TEXT_COLOR: fn(&mut Cpu) = nfs4rc::f_800cbf6c;
const TEXT_TINT: fn(&mut Cpu) = nfs4rc::f_800cbfa4;
const FONT_SET_ABR: fn(&mut Cpu) = nfs4rc::f_800cbfc4;
const TEXT_XY: u32 = 0x800C_C7F0;
/// The C library: `sprintf`, `strlen`, `strcpy`, `blockmove`; `s_lower`, `textpixels`.
const SPRINTF: fn(&mut Cpu) = nfs4rc::f_800e50a8;
const STRLEN: fn(&mut Cpu) = nfs4rc::f_800ea8d0;
const STRCPY: fn(&mut Cpu) = nfs4rc::f_800e6714;
const BLOCKMOVE: fn(&mut Cpu) = nfs4rc::f_800e6ec8;
const S_LOWER: fn(&mut Cpu) = nfs4rc::f_80026970;
const TEXT_PIXELS: fn(&mut Cpu) = nfs4rc::f_800e4a2c;
/// `TextSys_Word`, `TextSys_WordX`, `TextSys_WordY`, `TextSys_WordFlags`: a text id's string,
/// place and text type.
const TEXT_WORD: fn(&mut Cpu) = nfs4rc::f_800b9f18;
const WORD_X: fn(&mut Cpu) = nfs4rc::f_800b9f58;
const WORD_Y: fn(&mut Cpu) = nfs4rc::f_800b9f78;
const WORD_FLAGS: fn(&mut Cpu) = nfs4rc::f_800b9f38;
/// `CalcFadeVal` (the colour and a fade), `FeDraw_SetABRMode`; `PSXDrawTransSquare` (called
/// by address: the host draws it).
const CALC_FADE_VAL: fn(&mut Cpu) = nfs4rc::f_8004bd68;
const SET_ABR_MODE: fn(&mut Cpu) = crate::mmeffect::set_abr_mode_entry;
const DRAW_TRANS_SQUARE: u32 = 0x8004_F3B8;
/// "%s".
const TEXT_FORMAT: u32 = 0x8001_0FBC;

/// The text type whose type comes from the text's flags (`TextSys_WordFlags`).
const TYPE_FROM_TEXT: u32 = 14;
/// The justification that draws a menu text as a title.
const JUSTIFY_TITLE: u32 = 6;
/// The buffers texts are copied into. The original's were 128 bytes for `FullTextRGB` and 80
/// for a title: longer texts overran them into the original's own frame, harmlessly while it
/// was deep in the stack. These are larger, so such texts don't reach the callers' frames.
const TEXT_BUFFER: u32 = 512;
const TITLE_BUFFER: u32 = 256;
const WRAP_LINE: u32 = 128;
const WRAP_TEXT: u32 = 512;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

/// Text type `ty`'s style byte `k` (+0 the font, +1 the justification, +3 on the colours by
/// state).
fn style(m: &mut Mem, ty: u32, k: u32) -> u32 {
    m.r8(TEXT_STYLES.wrapping_add(ty.wrapping_mul(6)).wrapping_add(k)) as u32
}

fn style_font(m: &mut Mem, ty: u32) -> u32 {
    style(m, ty, 0)
}

fn style_justify(m: &mut Mem, ty: u32) -> u32 {
    style(m, ty, 1)
}

/// Text type `ty`'s colour in `state`.
fn style_colour(m: &mut Mem, ty: u32, state: u32) -> u32 {
    let k = style(m, ty, state.wrapping_add(3));
    m.r32(TEXT_COLORS + 4 * k)
}

/// The text colour the blending wants: 15 when it is set, else 1.
fn text_colour(m: &mut Mem) -> u32 {
    if m.r32(TEXT_ABR) != 0 { 15 } else { 1 }
}

/// Whether a justification draws a shadow (3..5).
fn shadowed(justify: u32) -> bool {
    justify.wrapping_sub(3) & 0xFFFF < 3
}

/// `FETextRender_SetFont` (FETEXTRENDER.CPP, 0x80020e38): the font switched to `font` (0 and
/// 3 their own, any other the middle one) when it isn't the current one.
pub fn set_font(c: &mut Cpu, font: u32) {
    let m = &mut c.m;
    if m.r16(CURRENT_FONT) as i16 as i32 as u32 == font {
        return;
    }
    m.w16(CURRENT_FONT, font as u16);
    let at = match font as i16 {
        0 => FONTS,
        3 => FONTS + 8,
        _ => FONTS + 4,
    };
    let f = m.r32(at);
    call(c, SWITCH_FONT, &[f]);
}

pub fn abi_set_font(c: &mut Cpu) {
    set_font(c, c.r[4]);
}

pub fn set_font_entry(c: &mut Cpu) {
    abi_set_font(c);
}

/// `FETextRender_FullTextRGB` (FETEXTRENDER.CPP, 0x80020eac): `text` drawn at `x`, `y` in
/// `colour` and font `font` (font 0's in lower case, copied into a buffer first), justified
/// (1, 4, 6: right; 2, 5: centred) and, for justifications 3..5, shadowed.
pub fn full_text_rgb(c: &mut Cpu, text: u32, x: u32, y: u32, colour: u32, font: u32, justify: u32) {
    with_buffer(c, TEXT_BUFFER, |c, buf| {
        let font = font & 0xFF;
        set_font(c, font);
        let s = if font == 0 {
            call(c, SPRINTF, &[buf, TEXT_FORMAT, text]);
            call(c, S_LOWER, &[buf]);
            buf
        } else {
            text
        };
        let left = match justify as i16 {
            1 | 4 | 6 => x.wrapping_sub(call(c, TEXT_PIXELS, &[s])),
            2 | 5 => x.wrapping_sub(((call(c, TEXT_PIXELS, &[s]) as i32) / 2) as u32),
            _ => x,
        };
        let code = text_colour(&mut c.m);
        call(c, TEXT_COLOR, &[code]);
        call(c, TEXT_TINT, &[colour]);
        let (left, top) = (sx(left), sx(y));
        call_addr(c, TEXT_XY, &[s, left, top]);
        if shadowed(justify) {
            // the shadow, black, down and to the right
            call(c, TEXT_TINT, &[0]);
            call_addr(c, TEXT_XY, &[s, left.wrapping_add(2), top.wrapping_add(1)]);
        }
    });
}

pub fn abi_full_text_rgb(c: &mut Cpu) {
    let (font, justify) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5));
    full_text_rgb(c, c.r[4], c.r[5], c.r[6], c.r[7], font, justify);
}

pub fn full_text_rgb_entry(c: &mut Cpu) {
    abi_full_text_rgb(c);
}

/// `FETextRender_FullText` (FETEXTRENDER.CPP, 0x80021020): `text` in text type `ty`'s font
/// and colour in `state`.
pub fn full_text(c: &mut Cpu, text: u32, x: u32, y: u32, ty: u32, state: u32, justify: u32) {
    let m = &mut c.m;
    let (font, colour) = (style_font(m, ty), style_colour(m, ty, state));
    full_text_rgb(c, text, sx(x), sx(y), colour, font, sx(justify));
}

pub fn abi_full_text(c: &mut Cpu) {
    let (state, justify) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5));
    full_text(c, c.r[4], c.r[5], c.r[6], c.r[7], state, justify);
}

pub fn full_text_entry(c: &mut Cpu) {
    abi_full_text(c);
}

/// `FETextRender_FullTextFade` (FETEXTRENDER.CPP, 0x800210a4): [`full_text`] with its colour
/// faded by `fade`.
#[allow(clippy::too_many_arguments)]
pub fn full_text_fade(c: &mut Cpu, fade: u32, text: u32, x: u32, y: u32, ty: u32, state: u32, justify: u32) {
    let colour = style_colour(&mut c.m, ty, state);
    let faded = call(c, CALC_FADE_VAL, &[colour, fade]);
    let font = style_font(&mut c.m, ty);
    full_text_rgb(c, text, sx(x), sx(y), faded, font, sx(justify));
}

pub fn abi_full_text_fade(c: &mut Cpu) {
    let (ty, state, justify) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5), crate::abi::arg(c, 6));
    full_text_fade(c, c.r[4], c.r[5], c.r[6], c.r[7], ty, state, justify);
}

/// A text type: `ty`, or text `id`'s own for `TYPE_FROM_TEXT`.
fn text_type(c: &mut Cpu, id: u32, ty: u32) -> u32 {
    if ty == TYPE_FROM_TEXT { call(c, WORD_FLAGS, &[sx(id)]) } else { ty }
}

/// `FETextRender_MenuTextPositioned` (FETEXTRENDER.CPP, 0x80021174): text `id` at `x`, `y` in
/// its text type's justification.
pub fn menu_text_positioned(c: &mut Cpu, id: u32, x: u32, y: u32, state: u32, ty: u32) {
    let ty = text_type(c, id, ty);
    let justify = style_justify(&mut c.m, ty);
    menu_text_positioned_justify(c, sx(id), sx(x), sx(y), justify, state, ty);
}

pub fn abi_menu_text_positioned(c: &mut Cpu) {
    let ty = crate::abi::arg(c, 4);
    menu_text_positioned(c, c.r[4], c.r[5], c.r[6], c.r[7], ty);
}

pub fn menu_text_positioned_entry(c: &mut Cpu) {
    abi_menu_text_positioned(c);
}

/// `FETextRender_MenuTextFade` (FETEXTRENDER.CPP, 0x80021214): text `id` at its own place in
/// its text type's justification, faded by `fade`.
pub fn menu_text_fade(c: &mut Cpu, fade: u32, id: u32, state: u32, ty: u32) {
    let id = sx(id);
    let x = call(c, WORD_X, &[id]);
    let y = call(c, WORD_Y, &[id]);
    let ty = text_type(c, id, ty);
    let justify = style_justify(&mut c.m, ty);
    menu_text_positioned_justify_fade(c, fade, id, sx(x), sx(y), justify, state, ty);
}

pub fn abi_menu_text_fade(c: &mut Cpu) {
    menu_text_fade(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `FETextRender_MenuTextPositionedJustify` (FETEXTRENDER.CPP, 0x800212e0): text `id` at `x`,
/// `y`, justified, unfaded.
#[allow(clippy::too_many_arguments)]
pub fn menu_text_positioned_justify(c: &mut Cpu, id: u32, x: u32, y: u32, justify: u32, state: u32, ty: u32) {
    menu_text_positioned_justify_fade(c, 0, sx(id), sx(x), sx(y), sx(justify), state, ty);
}

pub fn abi_menu_text_positioned_justify(c: &mut Cpu) {
    let (state, ty) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5));
    menu_text_positioned_justify(c, c.r[4], c.r[5], c.r[6], c.r[7], state, ty);
}

pub fn menu_text_positioned_justify_entry(c: &mut Cpu) {
    abi_menu_text_positioned_justify(c);
}

/// `FETextRender_MenuTextPositionedJustifyFade` (FETEXTRENDER.CPP, 0x80021334): text `id` at
/// `x`, `y`, justified and faded by `fade`; a title instead for the title's justification or
/// text type 0.
#[allow(clippy::too_many_arguments)]
pub fn menu_text_positioned_justify_fade(c: &mut Cpu, fade: u32, id: u32, x: u32, y: u32, justify: u32, state: u32, ty: u32) {
    // (the justification is a short)
    let justify = sx(justify);
    if justify == JUSTIFY_TITLE || ty == 0 {
        title(c, sx(id));
        return;
    }
    let ty = text_type(c, id, ty);
    let word = call(c, TEXT_WORD, &[sx(id)]);
    full_text_fade(c, fade, word, sx(x), sx(y), ty, state, justify);
}

pub fn abi_menu_text_positioned_justify_fade(c: &mut Cpu) {
    let (justify, state, ty) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5), crate::abi::arg(c, 6));
    menu_text_positioned_justify_fade(c, c.r[4], c.r[5], c.r[6], c.r[7], justify, state, ty);
}

/// A line's height in each font: 8 for font 0, 10 for font 1, 16 for the others.
fn line_height(font: u32) -> u16 {
    match font {
        0 => 8,
        1 => 10,
        _ => 16,
    }
}

/// `FETextRender_WordWrapTextRGBJustify` (FETEXTRENDER.CPP, 0x80021404): `text` word-wrapped
/// into `rect`, its top moved down a line for each line. A line takes about an eighth of the
/// box's width in characters plus 10, backed off a word at a time while wider than the box (a
/// single word wider than it goes on a line of its own). Each line is drawn (unless `measure`)
/// justified (1, 4: right; 2, 5: centred; 3..5 shadowed) in `colour` and font `font` (font 0
/// in lower case). Returns the lines' height.
#[allow(clippy::too_many_arguments)]
pub fn word_wrap_text_rgb_justify(c: &mut Cpu, text: u32, rect: RECT, colour: u32, justify: u32, font: u32, measure: u32) -> u32 {
    let top = rect.y(&mut c.m) as i32 as u32;
    set_font(c, font);
    let height = line_height(font);
    let len = call(c, STRLEN, &[text]);
    let code = text_colour(&mut c.m);
    call(c, TEXT_COLOR, &[code]);
    call(c, TEXT_TINT, &[colour]);
    // the line being drawn at +0, the text's copy (font 0's in lower case) after it
    with_buffer(c, WRAP_LINE + WRAP_TEXT, |c, line| {
        let buf = line + WRAP_LINE;
        call(c, BLOCKMOVE, &[text, buf, len.wrapping_add(1)]);
        if font == 0 {
            call(c, S_LOWER, &[buf]);
        }
        if len as i32 <= 0 {
            return;
        }
        // (the positions in the text are shorts)
        let at = |m: &mut Mem, k: u32| m.r8(buf + (k & 0xFFFF));
        let mut start = 0u32;
        loop {
            // past the spaces, then as much as fits roughly
            while at(&mut c.m, start) == b' ' {
                start = start.wrapping_add(1);
            }
            let step = (rect.w(&mut c.m) >> 3) as i32 as u32;
            let mut end = start.wrapping_add(step).wrapping_add(10);
            if (len as i32) < (end & 0xFFFF) as i32 {
                end = len;
            }
            let first = start & 0xFFFF;
            let n = (end & 0xFFFF).wrapping_sub(first);
            call(c, BLOCKMOVE, &[buf.wrapping_add(first), line, n.wrapping_add(1)]);
            c.m.w8(line.wrapping_add(n), 0);
            let too_wide = |c: &mut Cpu| {
                let w = call(c, TEXT_PIXELS, &[line]);
                (rect.w(&mut c.m) as i32) < w as i16 as i32
            };
            if too_wide(c) {
                // backed off a word at a time while too wide
                loop {
                    let mut to_spaces = true;
                    if at(&mut c.m, end) != b' ' {
                        while first < end & 0xFFFF {
                            end = end.wrapping_sub(1);
                            if at(&mut c.m, end) == b' ' {
                                break;
                            }
                        }
                        to_spaces = at(&mut c.m, end) == b' ';
                    }
                    if to_spaces {
                        while first < end & 0xFFFF {
                            end = end.wrapping_sub(1);
                            if at(&mut c.m, end) != b' ' {
                                break;
                            }
                        }
                    }
                    let last = end & 0xFFFF;
                    c.m.w8(line.wrapping_add(last.wrapping_sub(first)).wrapping_add(1), 0);
                    if !too_wide(c) || first >= last {
                        break;
                    }
                }
            }
            let m = &mut c.m;
            if end & 0xFFFF == start & 0xFFFF {
                // one word wider than the box: all of it
                let next = m.r8(buf.wrapping_add(end & 0xFFFF).wrapping_add(1));
                m.w8(line + 1, next);
                while at(m, end) != 0 && at(m, end) != b' ' {
                    end = end.wrapping_add(1);
                }
                m.w8(line.wrapping_add((end & 0xFFFF).wrapping_sub(start & 0xFFFF)), 0);
            }
            // on to the next word's start
            let mut spaces = false;
            if at(m, end) != 0 {
                loop {
                    if at(m, end) == b' ' {
                        spaces = true;
                        break;
                    }
                    end = end.wrapping_add(1);
                    if at(m, end) == 0 {
                        break;
                    }
                }
            }
            if spaces || at(m, end) == b' ' {
                end = end.wrapping_add(1);
                while at(m, end) == b' ' {
                    end = end.wrapping_add(1);
                }
            }
            m.w8(line.wrapping_add((end & 0xFFFF).wrapping_sub(start & 0xFFFF)), 0);
            let w = call(c, TEXT_PIXELS, &[line]);
            start = end;
            let x = rect.x(&mut c.m) as u16 as u32;
            let x = match justify {
                1 | 4 => x.wrapping_sub(w),
                2 | 5 => x.wrapping_sub((w as i16 >> 1) as i32 as u32),
                _ => x,
            };
            if measure == 0 {
                let x = sx(x);
                let y = rect.y(&mut c.m) as i32 as u32;
                call_addr(c, TEXT_XY, &[line, x, y]);
                if justify.wrapping_sub(3) < 3 {
                    call(c, TEXT_TINT, &[0]);
                    let y = (rect.y(&mut c.m) as i32 as u32).wrapping_add(1);
                    call_addr(c, TEXT_XY, &[line, x.wrapping_add(2), y]);
                    call(c, TEXT_TINT, &[colour]);
                }
            }
            rect.update_y(&mut c.m, |y| y.wrapping_add(height as i16));
            if (start & 0xFFFF) as i32 >= len as i32 {
                break;
            }
        }
    });
    (rect.y(&mut c.m) as i32 as u32).wrapping_sub(top)
}

pub fn abi_word_wrap_text_rgb_justify(c: &mut Cpu) {
    let (font, measure) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5));
    c.r[2] = word_wrap_text_rgb_justify(c, c.r[4], RECT(c.r[5]), c.r[6], c.r[7], font, measure);
}

/// `FETextRender_WordWrapTextRGB` (FETEXTRENDER.CPP, 0x80021878): [`word_wrap_text_rgb_justify`]
/// left-justified in font 0.
pub fn word_wrap_text_rgb(c: &mut Cpu, text: u32, rect: u32, colour: u32) -> u32 {
    word_wrap_text_rgb_justify(c, text, RECT(rect), colour, 0, 0, 0)
}

pub fn abi_word_wrap_text_rgb(c: &mut Cpu) {
    c.r[2] = word_wrap_text_rgb(c, c.r[4], c.r[5], c.r[6]);
}

pub fn word_wrap_text_rgb_entry(c: &mut Cpu) {
    abi_word_wrap_text_rgb(c);
}

/// `FETextRender_WordWrapTextFade` (FETEXTRENDER.CPP, 0x800218a0): `text` word-wrapped into
/// `rect` in text type `ty`'s font, justification and colour in `state`, faded by `fade`.
pub fn word_wrap_text_fade(c: &mut Cpu, fade: u32, text: u32, rect: u32, state: u32, ty: u32) -> u32 {
    let colour = style_colour(&mut c.m, ty, state);
    let faded = call(c, CALC_FADE_VAL, &[colour, fade]);
    let m = &mut c.m;
    let (justify, font) = (style_justify(m, ty), style_font(m, ty));
    word_wrap_text_rgb_justify(c, text, RECT(rect), faded, justify, font, 0)
}

pub fn abi_word_wrap_text_fade(c: &mut Cpu) {
    let ty = crate::abi::arg(c, 4);
    c.r[2] = word_wrap_text_fade(c, c.r[4], c.r[5], c.r[6], c.r[7], ty);
}

/// `FETextRender_WordWrapText` (FETEXTRENDER.CPP, 0x80021948): [`word_wrap_text_fade`]
/// unfaded.
pub fn word_wrap_text(c: &mut Cpu, text: u32, rect: u32, state: u32, ty: u32) -> u32 {
    word_wrap_text_fade(c, 0, text, rect, state, ty)
}

pub fn abi_word_wrap_text(c: &mut Cpu) {
    c.r[2] = word_wrap_text(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `FETextRender_WordWrapFade` (FETEXTRENDER.CPP, 0x80021984): text `id` word-wrapped, faded.
pub fn word_wrap_fade(c: &mut Cpu, fade: u32, id: u32, rect: u32, state: u32, ty: u32) -> u32 {
    let word = call(c, TEXT_WORD, &[sx(id)]);
    word_wrap_text_fade(c, fade, word, rect, state, ty)
}

pub fn abi_word_wrap_fade(c: &mut Cpu) {
    let ty = crate::abi::arg(c, 4);
    c.r[2] = word_wrap_fade(c, c.r[4], c.r[5], c.r[6], c.r[7], ty);
}

/// `FETextRender_WordWrap` (FETEXTRENDER.CPP, 0x800219ec): text `id` word-wrapped, unfaded.
pub fn word_wrap(c: &mut Cpu, id: u32, rect: u32, state: u32, ty: u32) -> u32 {
    word_wrap_fade(c, 0, sx(id), rect, state, ty)
}

pub fn abi_word_wrap(c: &mut Cpu) {
    c.r[2] = word_wrap(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `FETextRender_WordWrapHeight` (FETEXTRENDER.CPP, 0x80021a28): the height `text` takes
/// word-wrapped `width` wide (measured, not drawn).
pub fn word_wrap_height(c: &mut Cpu, width: u32, text: u32) -> u32 {
    with_buffer(c, 8, |c, at| {
        let (m, rect) = (&mut c.m, RECT(at));
        rect.set_x(m, 0);
        rect.set_y(m, 0);
        rect.set_w(m, width as i16);
        rect.set_h(m, 500);
        word_wrap_text_rgb_justify(c, text, rect, 0, 0, 0, 1)
    })
}

pub fn abi_word_wrap_height(c: &mut Cpu) {
    c.r[2] = word_wrap_height(c, c.r[4], c.r[5]);
}

pub fn word_wrap_height_entry(c: &mut Cpu) {
    abi_word_wrap_height(c);
}

/// `FETextRender_Title` (FETEXTRENDER.CPP, 0x80021a74): a menu's title, text `id` (copied into
/// a buffer) in lower case at the top left (lower for the second player), on a translucent bar
/// as long as it.
pub fn title(c: &mut Cpu, id: u32) {
    let app = FEApplication(c.m.r32(FE_APP));
    let top = if app.player(&mut c.m) == 1 { 105 } else { 0 };
    with_buffer(c, TITLE_BUFFER, |c, buf| {
        let word = call(c, TEXT_WORD, &[sx(id)]);
        call(c, STRCPY, &[buf, word]);
        call(c, S_LOWER, &[buf]);
        full_text(c, buf, 48, top | 0x10, 0, 1, 0);
        let w = call(c, TEXT_PIXELS, &[buf]).wrapping_add(10);
        call_addr(c, DRAW_TRANS_SQUARE, &[0, 43, top + 18, sx(w), 12, 2]);
        call(c, SET_ABR_MODE, &[0]);
    });
}

pub fn abi_title(c: &mut Cpu) {
    title(c, c.r[4]);
}

pub fn title_entry(c: &mut Cpu) {
    abi_title(c);
}

/// `FETextRender_SetABR` (FETEXTRENDER.CPP, 0x80021b48): the text's blending (`abr`), and
/// whether it is on (the text colour [`full_text_rgb`] sets).
pub fn set_abr(c: &mut Cpu, abr: u32, on: u32) {
    c.m.w32(TEXT_ABR, on);
    call(c, FONT_SET_ABR, &[abr]);
}

pub fn abi_set_abr(c: &mut Cpu) {
    set_abr(c, c.r[4], c.r[5]);
}

pub fn set_abr_entry(c: &mut Cpu) {
    abi_set_abr(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};
    use std::sync::OnceLock;

    /// The test's structures, in RAM the front end leaves zero: a RECT, a text, a long text to
    /// wrap.
    const RECT: u32 = 0x8014_7000;
    const TEXT: u32 = 0x8014_7100;
    const LONG_TEXT: u32 = 0x8014_7400;

    /// The text ids (and their lengths) short enough for the buffers the functions copy them
    /// into: `FullTextRGB`'s 128 bytes, `Title`'s 80 (a longer text overwrites their saved
    /// registers, which the game's never do).
    fn texts() -> &'static [(u32, u32)] {
        static TEXTS: OnceLock<Vec<(u32, u32)>> = OnceLock::new();
        TEXTS.get_or_init(|| {
            let ram = fe_ram().unwrap_or_default();
            if ram.is_empty() {
                return vec![(0, 0)];
            }
            let mut c = crate::test_util::cpu_with(&ram);
            (0..600).map(|id| (id, base::text_len(&mut c.m, id as i16))).filter(|&(_, n)| n < 80).collect()
        })
    }

    fn text_id(rng: &mut Rng) -> u32 {
        let t = texts();
        t[pick(rng, t.len() as u32) as usize].0
    }

    /// Text `id`'s string.
    fn word(m: &mut Mem, id: u32) -> u32 {
        let mut c = crate::test_util::cpu_with(&m.ram);
        crate::abi::call(&mut c, TEXT_WORD, &[id as i16 as i32 as u32])
    }

    fn copy_str(m: &mut Mem, from: u32, to: u32) -> u32 {
        let mut n = 0;
        loop {
            let b = m.r8(from + n);
            m.w8(to + n, b);
            if b == 0 {
                return n;
            }
            n += 1;
        }
    }

    /// The fonts' state, the blending, the front end's players; a RECT anywhere on the screen
    /// (a box 24 to 320 wide); a text and a long one of a few texts, spaces (some runs of
    /// them) and now and then a word longer than a line.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        m.w16(CURRENT_FONT, (pick(rng, 6) as i32 - 1) as u16);
        m.w32(TEXT_ABR, pick(rng, 2));
        let app = m.r32(FE_APP);
        m.w8(app + 556, pick(rng, 2) as u8);
        for (k, v) in [pick(rng, 320), pick(rng, 240), 24 + pick(rng, 297), pick(rng, 200)].into_iter().enumerate() {
            m.w16(RECT + 2 * k as u32, v as u16);
        }
        let at = word(m, text_id(rng));
        copy_str(m, at, TEXT);
        let mut n = 0;
        for _ in 0..1 + pick(rng, 6) {
            match pick(rng, 8) {
                0 => {
                    for _ in 0..1 + pick(rng, 4) {
                        m.w8(LONG_TEXT + n, b' ');
                        n += 1;
                    }
                }
                1 => {
                    for _ in 0..10 + pick(rng, 50) {
                        m.w8(LONG_TEXT + n, b'A' + pick(rng, 26) as u8);
                        n += 1;
                    }
                }
                _ => {
                    let at = word(m, text_id(rng));
                    n += copy_str(m, at, LONG_TEXT + n);
                }
            }
            if pick(rng, 3) != 0 {
                m.w8(LONG_TEXT + n, b' ');
                n += 1;
            }
        }
        m.w8(LONG_TEXT + n, 0);
    }

    /// A text type (14 now and then: the text's own), a state, a justification, a font.
    fn ty(rng: &mut Rng) -> u32 {
        if pick(rng, 5) == 0 { TYPE_FROM_TEXT } else { pick(rng, 29) }
    }

    fn state(rng: &mut Rng) -> u32 {
        pick(rng, 3)
    }

    fn justify(rng: &mut Rng) -> u32 {
        pick(rng, 8)
    }

    fn font(rng: &mut Rng) -> u32 {
        pick(rng, 5)
    }

    fn coord(rng: &mut Rng) -> u32 {
        pick(rng, 400) as i32 as u32 - 40
    }

    fn cases() -> Vec<Case> {
        vec![
            ("SetFont", 0x8002_0e38, nfs4rc::f_80020e38, |_, r| vec![pick(r, 6).wrapping_sub(1)]),
            ("FullTextRGB", 0x8002_0eac, nfs4rc::f_80020eac, |_, r| vec![TEXT, coord(r), coord(r), r.next() as u32, font(r), justify(r)]),
            ("FullText", 0x8002_1020, nfs4rc::f_80021020, |_, r| vec![TEXT, coord(r), coord(r), ty(r) % 14, state(r), justify(r)]),
            ("FullTextFade", 0x8002_10a4, nfs4rc::f_800210a4, |_, r| vec![pick(r, 140), TEXT, coord(r), coord(r), ty(r) % 14, state(r), justify(r)]),
            ("MenuTextPositioned", 0x8002_1174, nfs4rc::f_80021174, |_, r| vec![text_id(r), coord(r), coord(r), state(r), ty(r)]),
            ("MenuTextFade", 0x8002_1214, nfs4rc::f_80021214, |_, r| vec![pick(r, 140), text_id(r), state(r), ty(r)]),
            ("MenuTextPositionedJustify", 0x8002_12e0, nfs4rc::f_800212e0, |_, r| vec![text_id(r), coord(r), coord(r), justify(r), state(r), ty(r)]),
            ("MenuTextPositionedJustifyFade", 0x8002_1334, nfs4rc::f_80021334, |_, r| {
                vec![pick(r, 140), text_id(r), coord(r), coord(r), justify(r), state(r), ty(r)]
            }),
            ("WordWrapTextRGBJustify", 0x8002_1404, nfs4rc::f_80021404, |_, r| {
                vec![LONG_TEXT, RECT, r.next() as u32, justify(r), font(r), (pick(r, 4) == 0) as u32]
            }),
            ("WordWrapTextRGB", 0x8002_1878, nfs4rc::f_80021878, |_, r| vec![LONG_TEXT, RECT, r.next() as u32]),
            ("WordWrapTextFade", 0x8002_18a0, nfs4rc::f_800218a0, |_, r| vec![pick(r, 140), LONG_TEXT, RECT, state(r), ty(r) % 14]),
            ("WordWrapText", 0x8002_1948, nfs4rc::f_80021948, |_, r| vec![LONG_TEXT, RECT, state(r), ty(r) % 14]),
            ("WordWrapFade", 0x8002_1984, nfs4rc::f_80021984, |_, r| vec![pick(r, 140), text_id(r), RECT, state(r), ty(r) % 14]),
            ("WordWrap", 0x8002_19ec, nfs4rc::f_800219ec, |_, r| vec![text_id(r), RECT, state(r), ty(r) % 14]),
            ("WordWrapHeight", 0x8002_1a28, nfs4rc::f_80021a28, |_, r| vec![24 + pick(r, 297), LONG_TEXT]),
            ("Title", 0x8002_1a74, nfs4rc::f_80021a74, |_, r| vec![text_id(r)]),
            ("SetABR", 0x8002_1b48, nfs4rc::f_80021b48, |_, r| vec![pick(r, 4), pick(r, 2)]),
        ]
    }

    #[test]
    fn fetextrender_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x7E70 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x7E71 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
