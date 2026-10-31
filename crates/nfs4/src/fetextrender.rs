//! FETEXTRENDER.CPP: the front end's text: a string drawn in a font, a colour and a
//! justification (`FullTextRGB`; the text types' fonts, justifications and colours by state
//! from `gTextStyles`, faded or not), a text id at a place or at its own (`MenuText...`),
//! word-wrapped into a box (`WordWrap...`, also measuring a text's height), the menus' titles,
//! the fonts and the blending.
//!
//! The functions call each other down to `FullTextRGB` and `WordWrapTextRGBJustify`, as the
//! original's callers `jal` them (the `*_entry` functions: their prologues left, arguments
//! from the fifth on at sp+16), the registers each call gets as the original has them.

use crate::abi::{self, Saved};
use crate::globals::{CURRENT_FONT, FE_APP, FONTS, TEXT_ABR, TEXT_COLORS, TEXT_STYLES};
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
/// by address, as FEMENUEXTENDED's callers do).
const CALC_FADE_VAL: fn(&mut Cpu) = nfs4rc::f_8004bd68;
const SET_ABR_MODE: fn(&mut Cpu) = nfs4rc::f_8004dbc0;
const DRAW_TRANS_SQUARE: u32 = 0x8004_F3B8;
/// "%s".
const TEXT_FORMAT: u32 = 0x8001_0FBC;

/// The text type whose type comes from the text's flags (`TextSys_WordFlags`).
const TYPE_FROM_TEXT: u32 = 14;
/// The justification that draws a menu text as a title.
const JUSTIFY_TITLE: u32 = 6;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as i16 as i32 as u32
}

/// Text type `ty`'s style byte `k` (+0 the font, +1 the justification, +3 on the colours by
/// state).
fn style(m: &mut Mem, ty: u32, k: u32) -> u32 {
    m.r8(TEXT_STYLES.wrapping_add(ty.wrapping_mul(6)).wrapping_add(k)) as u32
}

/// Text type `ty`'s colour in `state`.
fn style_color(m: &mut Mem, ty: u32, state: u32) -> u32 {
    let k = style(m, ty, state.wrapping_add(3));
    m.r32(TEXT_COLORS + 4 * k)
}

/// The text colour the blending wants: 15 when it is set, else 1.
fn text_color(m: &mut Mem) -> u32 {
    if m.r32(TEXT_ABR) != 0 { 15 } else { 1 }
}

/// `FETextRender_SetFont` (FETEXTRENDER.CPP, 0x80020e38): the font switched to `font` (0 and
/// 3 their own, any other the middle one) when it isn't the current one.
pub fn set_font(c: &mut Cpu, font: u32) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        if lh(m, CURRENT_FONT) == font {
            return;
        }
        m.w16(CURRENT_FONT, font as u16);
        let at = match sx(font) {
            0 => FONTS,
            3 => FONTS + 8,
            _ => FONTS + 4,
        };
        let f = m.r32(at);
        Saved::of(c).call_leaving(c, SWITCH_FONT, &[f], 0x8002_0e9c);
    })
}

pub fn abi_set_font(c: &mut Cpu) {
    set_font(c, c.r[4]);
}

/// `FETextRender_SetFont` entered as the original's callers `jal` it.
pub fn set_font_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8002_0e38, None, &[]);
    abi_set_font(c);
}

/// `FETextRender_FullTextRGB` (FETEXTRENDER.CPP, 0x80020eac): `text` drawn at `x`, `y` in
/// `color` and font `font` (font 0's lower case, copied into a 128-byte buffer first),
/// justified (1, 4, 6: right; 2, 5: centred) and, for justifications 3..5, shadowed.
pub fn full_text_rgb(c: &mut Cpu, text: u32, x: u32, y: u32, color: u32, font: u32, justify: u32) {
    abi::frame(c, 184, |c, sp| {
        let mut r = Saved::of(c);
        let font = font & 0xFF;
        r.0[..8].copy_from_slice(&[font, text, x, justify, x, y, justify, color]);
        r.enter_leaving(c, 0x8002_0e38, 0x8002_0efc, |c| set_font(c, font));
        let mut s = text;
        if font == 0 {
            let buf = sp + 16;
            r.call_leaving(c, SPRINTF, &[buf, TEXT_FORMAT, text], 0x8002_0f18);
            r.call_leaving(c, S_LOWER, &[buf], 0x8002_0f20);
            s = buf;
            r.0[1] = s;
        }
        let mut left = x;
        match sx(justify) as i32 {
            1 | 4 | 6 => {
                let w = r.call_leaving(c, TEXT_PIXELS, &[s], 0x8002_0f50);
                left = x.wrapping_sub(w);
            }
            2 | 5 => {
                let w = r.call_leaving(c, TEXT_PIXELS, &[s], 0x8002_0f70);
                left = x.wrapping_sub(((w as i32) / 2) as u32);
            }
            _ => {}
        }
        r.0[2] = left;
        let color_code = text_color(&mut c.m);
        r.call_leaving(c, TEXT_COLOR, &[color_code], 0x8002_0fa0);
        r.call_leaving(c, TEXT_TINT, &[color], 0x8002_0fa8);
        let (left, top) = (sx(left), sx(y));
        r.0[2] = left;
        r.0[0] = top;
        r.call_ptr_leaving(c, TEXT_XY, &[s, left, top], 0x8002_0fc8);
        if justify.wrapping_sub(3) & 0xFFFF < 3 {
            // the shadow, black, down and to the right
            r.call_leaving(c, TEXT_TINT, &[0], 0x8002_0fe4);
            r.call_ptr_leaving(c, TEXT_XY, &[s, left.wrapping_add(2), top.wrapping_add(1)], 0x8002_0ff4);
        }
    })
}

pub fn abi_full_text_rgb(c: &mut Cpu) {
    let (font, justify) = (abi::arg(c, 4), abi::arg(c, 5));
    full_text_rgb(c, c.r[4], c.r[5], c.r[6], c.r[7], font, justify);
}

/// `FETextRender_FullTextRGB` entered as the original's callers `jal` it.
pub fn full_text_rgb_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8002_0eac, None, &[]);
    abi_full_text_rgb(c);
}

/// `FETextRender_FullText` (FETEXTRENDER.CPP, 0x80021020): `text` in text type `ty`'s font
/// and colour in `state`.
pub fn full_text(c: &mut Cpu, text: u32, x: u32, y: u32, ty: u32, state: u32, justify: u32) {
    abi::frame(c, 32, |c, _| {
        let m = &mut c.m;
        let (font, color) = (style(m, ty, 0), style_color(m, ty, state));
        let args = [text, sx(x), sx(y), color, font, sx(justify)];
        Saved::of(c).call_leaving(c, full_text_rgb_entry, &args, 0x8002_1094);
    })
}

pub fn abi_full_text(c: &mut Cpu) {
    let (state, justify) = (abi::arg(c, 4), abi::arg(c, 5));
    full_text(c, c.r[4], c.r[5], c.r[6], c.r[7], state, justify);
}

pub fn full_text_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8002_1020, None, &[]);
    abi_full_text(c);
}

/// `FETextRender_FullTextFade` (FETEXTRENDER.CPP, 0x800210a4): [`full_text`] with its colour
/// faded by `fade`.
#[allow(clippy::too_many_arguments)]
pub fn full_text_fade(c: &mut Cpu, fade: u32, text: u32, x: u32, y: u32, ty: u32, state: u32, justify: u32) {
    abi::frame(c, 56, |c, _| {
        let mut r = Saved::of(c);
        let row = ty.wrapping_mul(6);
        r.0[..6].copy_from_slice(&[row, justify, TEXT_STYLES, x, y, text]);
        let color = style_color(&mut c.m, ty, state);
        let faded = r.call_leaving(c, CALC_FADE_VAL, &[color, fade], 0x8002_111c);
        r.0[0] = row.wrapping_add(TEXT_STYLES);
        r.0[1] = sx(justify);
        r.0[3] = x << 16;
        r.0[4] = y << 16;
        let font = style(&mut c.m, ty, 0);
        r.call_leaving(c, full_text_rgb_entry, &[text, sx(x), sx(y), faded, font, sx(justify)], 0x8002_1150);
    })
}

pub fn abi_full_text_fade(c: &mut Cpu) {
    let (ty, state, justify) = (abi::arg(c, 4), abi::arg(c, 5), abi::arg(c, 6));
    full_text_fade(c, c.r[4], c.r[5], c.r[6], c.r[7], ty, state, justify);
}

pub(crate) fn full_text_fade_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8002_10a4, None, &[]);
    abi_full_text_fade(c);
}

/// `FETextRender_MenuTextPositioned` (FETEXTRENDER.CPP, 0x80021174): text `id` at `x`, `y` in
/// its text type's justification (the type from the text for type 14).
pub fn menu_text_positioned(c: &mut Cpu, id: u32, x: u32, y: u32, state: u32, ty: u32) {
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        r.0[..4].copy_from_slice(&[id, x, y, state]);
        let mut ty = ty;
        if ty == TYPE_FROM_TEXT {
            ty = r.call_leaving(c, WORD_FLAGS, &[sx(id)], 0x8002_11b4);
        }
        let justify = style(&mut c.m, ty, 1);
        r.call_leaving(c, menu_text_positioned_justify_entry, &[sx(id), sx(x), sx(y), justify, state, ty], 0x8002_11f8);
    })
}

pub fn abi_menu_text_positioned(c: &mut Cpu) {
    let ty = abi::arg(c, 4);
    menu_text_positioned(c, c.r[4], c.r[5], c.r[6], c.r[7], ty);
}

pub fn menu_text_positioned_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8002_1174, None, &[]);
    abi_menu_text_positioned(c);
}

/// `FETextRender_MenuTextFade` (FETEXTRENDER.CPP, 0x80021214): text `id` at its own place in
/// its text type's justification, faded by `fade`.
pub fn menu_text_fade(c: &mut Cpu, fade: u32, id: u32, state: u32, ty: u32) {
    abi::frame(c, 64, |c, _| {
        let mut r = Saved::of(c);
        let id = sx(id);
        r.0[0] = ty;
        r.0[1] = id;
        r.0[4] = state;
        r.0[5] = fade;
        let x = r.call_leaving(c, WORD_X, &[id], 0x8002_1250);
        r.0[3] = x;
        let y = r.call_leaving(c, WORD_Y, &[id], 0x8002_125c);
        r.0[2] = y;
        let mut ty = ty;
        if ty == TYPE_FROM_TEXT {
            ty = r.call_leaving(c, WORD_FLAGS, &[id], 0x8002_1274);
            r.0[0] = ty;
        }
        let justify = style(&mut c.m, ty, 1);
        r.call_leaving(c, menu_text_positioned_justify_fade_entry, &[fade, id, sx(x), sx(y), justify, state, ty], 0x8002_12bc);
    })
}

pub fn abi_menu_text_fade(c: &mut Cpu) {
    menu_text_fade(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `FETextRender_MenuTextPositionedJustify` (FETEXTRENDER.CPP, 0x800212e0): text `id` at `x`,
/// `y`, justified, unfaded.
#[allow(clippy::too_many_arguments)]
pub fn menu_text_positioned_justify(c: &mut Cpu, id: u32, x: u32, y: u32, justify: u32, state: u32, ty: u32) {
    abi::frame(c, 40, |c, _| {
        let args = [0, sx(id), sx(x), sx(y), sx(justify), state, ty];
        Saved::of(c).call_leaving(c, menu_text_positioned_justify_fade_entry, &args, 0x8002_1324);
    })
}

pub fn abi_menu_text_positioned_justify(c: &mut Cpu) {
    let (state, ty) = (abi::arg(c, 4), abi::arg(c, 5));
    menu_text_positioned_justify(c, c.r[4], c.r[5], c.r[6], c.r[7], state, ty);
}

pub fn menu_text_positioned_justify_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8002_12e0, None, &[]);
    abi_menu_text_positioned_justify(c);
}

/// `FETextRender_MenuTextPositionedJustifyFade` (FETEXTRENDER.CPP, 0x80021334): text `id` at
/// `x`, `y`, justified and faded by `fade`; a title instead for the title's justification or
/// text type 0.
#[allow(clippy::too_many_arguments)]
pub fn menu_text_positioned_justify_fade(c: &mut Cpu, fade: u32, id: u32, x: u32, y: u32, justify: u32, state: u32, ty: u32) {
    abi::frame(c, 64, |c, _| {
        let mut r = Saved::of(c);
        // (the justification read as a short from the argument's word)
        let justify = sx(justify);
        r.0[..6].copy_from_slice(&[ty, id, justify, x, y, fade]);
        if justify == JUSTIFY_TITLE || ty == 0 {
            r.enter_leaving(c, 0x8002_1a74, 0x8002_1388, |c| title(c, sx(id)));
            return;
        }
        let mut ty = ty;
        if ty == TYPE_FROM_TEXT {
            ty = r.call_leaving(c, WORD_FLAGS, &[sx(id)], 0x8002_13a4);
            r.0[0] = ty;
        }
        let word = r.call_leaving(c, TEXT_WORD, &[sx(id)], 0x8002_13b4);
        r.call_leaving(c, full_text_fade_entry, &[fade, word, sx(x), sx(y), ty, state, justify], 0x8002_13e0);
    })
}

pub fn abi_menu_text_positioned_justify_fade(c: &mut Cpu) {
    let (justify, state, ty) = (abi::arg(c, 4), abi::arg(c, 5), abi::arg(c, 6));
    menu_text_positioned_justify_fade(c, c.r[4], c.r[5], c.r[6], c.r[7], justify, state, ty);
}

pub(crate) fn menu_text_positioned_justify_fade_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8002_1334, None, &[]);
    abi_menu_text_positioned_justify_fade(c);
}

/// `FETextRender_WordWrapTextRGBJustify` (FETEXTRENDER.CPP, 0x80021404): `text` word-wrapped
/// into `rect` (a line about an eighth of its width in characters plus 10, backed off to
/// the last space while wider than the box), each line drawn (unless `measure`) justified
/// (1, 4: right; 2, 5: centred; 3..5 shadowed) in `color` and font `font`, the RECT's top
/// moved down a line each (8 pixels for font 0, 10 for font 1, else 16). Returns the height
/// of the lines.
#[allow(clippy::too_many_arguments)]
pub fn word_wrap_text_rgb_justify(c: &mut Cpu, text: u32, rect: u32, color: u32, justify: u32, font: u32, measure: u32) -> u32 {
    abi::frame(c, 712, |c, sp| {
        let mut r = Saved::of(c);
        // (the register arguments homed in the caller's frame; the top and the line's height)
        let m = &mut c.m;
        m.w32(sp + 720, color);
        m.w32(sp + 724, justify);
        let top = lh(m, rect + 2);
        m.w32(sp + 656, top);
        r.0[0] = text;
        r.0[1] = font;
        r.0[7] = rect;
        r.enter_leaving(c, 0x8002_0e38, 0x8002_1454, |c| set_font(c, font));
        let height = match font {
            1 => 10,
            0 => 8,
            _ => 16,
        };
        c.m.w16(sp + 664, height);
        r.0[4] = 0;
        let len = r.call_leaving(c, STRLEN, &[text], 0x8002_148c);
        r.0[8] = len;
        let color_code = text_color(&mut c.m);
        r.call_leaving(c, TEXT_COLOR, &[color_code], 0x8002_14b0);
        let tint = c.m.r32(sp + 720);
        r.call_leaving(c, TEXT_TINT, &[tint], 0x8002_14bc);
        // the text copied (font 0's in lower case), and each line from it at sp+16
        let (buf, line) = (sp + 144, sp + 16);
        r.0[0] = buf;
        r.call_leaving(c, BLOCKMOVE, &[text, buf, len.wrapping_add(1)], 0x8002_14d0);
        if font == 0 {
            r.call_leaving(c, S_LOWER, &[buf], 0x8002_14e0);
        }
        r.0[3] = buf;
        if len as i32 > 0 {
            r.0[5] = line;
            r.0[6] = b' ' as u32;
            let at = |m: &mut Mem, k: u32| m.r8(buf + (k & 0xFFFF));
            let mut start = 0u32;
            loop {
                // past the spaces, then as much as fits roughly
                if at(&mut c.m, start) == b' ' {
                    start = start.wrapping_add(1);
                    while at(&mut c.m, start) == b' ' {
                        start = start.wrapping_add(1);
                    }
                }
                let step = (c.m.r16(rect + 4) as i16 >> 3) as i32 as u32;
                let mut end = start.wrapping_add(step).wrapping_add(10);
                if (len as i32) < (end & 0xFFFF) as i32 {
                    end = len;
                }
                let first = start & 0xFFFF;
                let n = (end & 0xFFFF).wrapping_sub(first);
                r.0[0] = n;
                r.0[1] = end;
                r.0[2] = first;
                r.0[4] = start;
                r.call_leaving(c, BLOCKMOVE, &[buf.wrapping_add(first), line, n.wrapping_add(1)], 0x8002_156c);
                r.0[0] = line.wrapping_add(n);
                c.m.w8(line.wrapping_add(n), 0);
                let w = r.call_leaving(c, TEXT_PIXELS, &[line], 0x8002_157c);
                if (lh(&mut c.m, rect + 4) as i32) < w as i16 as i32 {
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
                        r.0[0] = last;
                        r.0[1] = end;
                        c.m.w8(line.wrapping_add(last.wrapping_sub(first)).wrapping_add(1), 0);
                        let w = r.call_leaving(c, TEXT_PIXELS, &[line], 0x8002_1634);
                        if (lh(&mut c.m, rect + 4) as i32) >= w as i16 as i32 || first >= last {
                            break;
                        }
                    }
                }
                let m = &mut c.m;
                if end & 0xFFFF == start & 0xFFFF {
                    // one word longer than the box: all of it
                    let next = m.r8(buf.wrapping_add(end & 0xFFFF).wrapping_add(1));
                    m.w8(sp + 17, next);
                    if at(m, end) != 0 {
                        while at(m, end) != b' ' {
                            end = end.wrapping_add(1);
                            if at(m, end) == 0 {
                                break;
                            }
                        }
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
                r.0[1] = end;
                let w = r.call_leaving(c, TEXT_PIXELS, &[line], 0x8002_176c);
                start = end;
                r.0[4] = start;
                let m = &mut c.m;
                let justify = m.r32(sp + 724);
                let mut x = m.r16(rect) as u32;
                match justify {
                    1 | 4 => x = x.wrapping_sub(w),
                    2 | 5 => x = x.wrapping_sub((w as i16 >> 1) as i32 as u32),
                    _ => {}
                }
                if measure == 0 {
                    let x = sx(x);
                    r.0[0] = x;
                    let y = lh(m, rect + 2);
                    r.call_ptr_leaving(c, TEXT_XY, &[line, x, y], 0x8002_17d8);
                    let justify = c.m.r32(sp + 724);
                    if justify.wrapping_sub(3) < 3 {
                        r.call_leaving(c, TEXT_TINT, &[0], 0x8002_17f8);
                        let y = lh(&mut c.m, rect + 2).wrapping_add(1);
                        r.call_ptr_leaving(c, TEXT_XY, &[line, x.wrapping_add(2), y], 0x8002_180c);
                        let tint = c.m.r32(sp + 720);
                        r.call_leaving(c, TEXT_TINT, &[tint], 0x8002_1818);
                    }
                }
                let m = &mut c.m;
                let y = m.r16(rect + 2).wrapping_add(m.r16(sp + 664));
                m.w16(rect + 2, y);
                if (start & 0xFFFF) as i32 >= len as i32 {
                    break;
                }
            }
        }
        let m = &mut c.m;
        lh(m, rect + 2).wrapping_sub(m.r32(sp + 656))
    })
}

pub fn abi_word_wrap_text_rgb_justify(c: &mut Cpu) {
    let (font, measure) = (abi::arg(c, 4), abi::arg(c, 5));
    c.r[2] = word_wrap_text_rgb_justify(c, c.r[4], c.r[5], c.r[6], c.r[7], font, measure);
}

pub(crate) fn word_wrap_text_rgb_justify_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8002_1404, None, &[]);
    abi_word_wrap_text_rgb_justify(c);
}

/// `FETextRender_WordWrapTextRGB` (FETEXTRENDER.CPP, 0x80021878): [`word_wrap_text_rgb_justify`]
/// left-justified in font 0.
pub fn word_wrap_text_rgb(c: &mut Cpu, text: u32, rect: u32, color: u32) -> u32 {
    abi::frame(c, 32, |c, _| Saved::of(c).call_leaving(c, word_wrap_text_rgb_justify_entry, &[text, rect, color, 0, 0, 0], 0x8002_1890))
}

pub fn abi_word_wrap_text_rgb(c: &mut Cpu) {
    c.r[2] = word_wrap_text_rgb(c, c.r[4], c.r[5], c.r[6]);
}

pub fn word_wrap_text_rgb_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8002_1878, None, &[]);
    abi_word_wrap_text_rgb(c);
}

/// `FETextRender_WordWrapTextFade` (FETEXTRENDER.CPP, 0x800218a0): `text` word-wrapped into
/// `rect` in text type `ty`'s font, justification and colour in `state`, faded by `fade`.
pub fn word_wrap_text_fade(c: &mut Cpu, fade: u32, text: u32, rect: u32, state: u32, ty: u32) -> u32 {
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        let row = ty.wrapping_mul(6);
        r.0[..4].copy_from_slice(&[row, TEXT_STYLES, text, rect]);
        let color = style_color(&mut c.m, ty, state);
        let faded = r.call_leaving(c, CALC_FADE_VAL, &[color, fade], 0x8002_1908);
        r.0[0] = row.wrapping_add(TEXT_STYLES);
        let m = &mut c.m;
        let (justify, font) = (style(m, ty, 1), style(m, ty, 0));
        r.call_leaving(c, word_wrap_text_rgb_justify_entry, &[text, rect, faded, justify, font, 0], 0x8002_192c)
    })
}

pub fn abi_word_wrap_text_fade(c: &mut Cpu) {
    let ty = abi::arg(c, 4);
    c.r[2] = word_wrap_text_fade(c, c.r[4], c.r[5], c.r[6], c.r[7], ty);
}

pub(crate) fn word_wrap_text_fade_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8002_18a0, None, &[]);
    abi_word_wrap_text_fade(c);
}

/// `FETextRender_WordWrapText` (FETEXTRENDER.CPP, 0x80021948): [`word_wrap_text_fade`]
/// unfaded.
pub fn word_wrap_text(c: &mut Cpu, text: u32, rect: u32, state: u32, ty: u32) -> u32 {
    abi::frame(c, 32, |c, _| Saved::of(c).call_leaving(c, word_wrap_text_fade_entry, &[0, text, rect, state, ty], 0x8002_1974))
}

pub fn abi_word_wrap_text(c: &mut Cpu) {
    c.r[2] = word_wrap_text(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `FETextRender_WordWrapFade` (FETEXTRENDER.CPP, 0x80021984): text `id` word-wrapped, faded.
pub fn word_wrap_fade(c: &mut Cpu, fade: u32, id: u32, rect: u32, state: u32, ty: u32) -> u32 {
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        r.0[..4].copy_from_slice(&[ty, fade, rect, state]);
        let word = r.call_leaving(c, TEXT_WORD, &[sx(id)], 0x8002_19b8);
        r.call_leaving(c, word_wrap_text_fade_entry, &[fade, word, rect, state, ty], 0x8002_19d0)
    })
}

pub fn abi_word_wrap_fade(c: &mut Cpu) {
    let ty = abi::arg(c, 4);
    c.r[2] = word_wrap_fade(c, c.r[4], c.r[5], c.r[6], c.r[7], ty);
}

fn word_wrap_fade_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8002_1984, None, &[]);
    abi_word_wrap_fade(c);
}

/// `FETextRender_WordWrap` (FETEXTRENDER.CPP, 0x800219ec): text `id` word-wrapped, unfaded.
pub fn word_wrap(c: &mut Cpu, id: u32, rect: u32, state: u32, ty: u32) -> u32 {
    abi::frame(c, 32, |c, _| Saved::of(c).call_leaving(c, word_wrap_fade_entry, &[0, sx(id), rect, state, ty], 0x8002_1a18))
}

pub fn abi_word_wrap(c: &mut Cpu) {
    c.r[2] = word_wrap(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `FETextRender_WordWrapHeight` (FETEXTRENDER.CPP, 0x80021a28): the height `text` takes
/// word-wrapped `width` wide (measured, not drawn).
pub fn word_wrap_height(c: &mut Cpu, width: u32, text: u32) -> u32 {
    abi::frame(c, 40, |c, sp| {
        let m = &mut c.m;
        m.w16(sp + 24, 0);
        m.w16(sp + 26, 0);
        m.w16(sp + 28, width as u16);
        m.w16(sp + 30, 500);
        Saved::of(c).call_leaving(c, word_wrap_text_rgb_justify_entry, &[text, sp + 24, 0, 0, 0, 1], 0x8002_1a64)
    })
}

pub fn abi_word_wrap_height(c: &mut Cpu) {
    c.r[2] = word_wrap_height(c, c.r[4], c.r[5]);
}

pub fn word_wrap_height_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8002_1a28, None, &[]);
    abi_word_wrap_height(c);
}

/// `FETextRender_Title` (FETEXTRENDER.CPP, 0x80021a74): a menu's title, text `id` (copied into
/// an 80-byte buffer) in lower case at the top left (lower in a two-player front end), on a
/// translucent bar as long as it.
pub fn title(c: &mut Cpu, id: u32) {
    abi::frame(c, 128, |c, sp| {
        let mut r = Saved::of(c);
        let app = c.m.r32(FE_APP);
        let top = if c.m.r8(app + 556) == 1 { 105 } else { 0 };
        r.0[0] = top;
        r.0[1] = 1;
        let buf = sp + 24;
        let word = r.call_leaving(c, TEXT_WORD, &[sx(id)], 0x8002_1aac);
        r.call_leaving(c, STRCPY, &[buf, word], 0x8002_1ab8);
        r.call_leaving(c, S_LOWER, &[buf], 0x8002_1ac0);
        r.call_leaving(c, full_text_entry, &[buf, 48, top | 0x10, 0, 1, 0], 0x8002_1adc);
        let m = &mut c.m;
        m.w16(sp + 104, 43);
        m.w16(sp + 106, (top + 18) as u16);
        let w = r.call_leaving(c, TEXT_PIXELS, &[buf], 0x8002_1af4).wrapping_add(10);
        let m = &mut c.m;
        m.w16(sp + 108, w as u16);
        m.w16(sp + 110, 12);
        let (x, y) = (lh(m, sp + 104), lh(m, sp + 106));
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0, x, y, sx(w), 12, 2], 0x8002_1b2c);
        r.call_leaving(c, SET_ABR_MODE, &[0], 0x8002_1b34);
    })
}

pub fn abi_title(c: &mut Cpu) {
    title(c, c.r[4]);
}

pub fn title_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8002_1a74, None, &[]);
    abi_title(c);
}

/// `FETextRender_SetABR` (FETEXTRENDER.CPP, 0x80021b48): the text's blending (`abr`), and
/// whether it is on (the text colour [`full_text_rgb`] sets).
pub fn set_abr(c: &mut Cpu, abr: u32, on: u32) {
    abi::frame(c, 24, |c, _| {
        c.m.w32(TEXT_ABR, on);
        Saved::of(c).call_leaving(c, FONT_SET_ABR, &[abr], 0x8002_1b5c);
    })
}

pub fn abi_set_abr(c: &mut Cpu) {
    set_abr(c, c.r[4], c.r[5]);
}

pub fn set_abr_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8002_1b48, None, &[]);
    abi_set_abr(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, TOP, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};
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
        abi::call(&mut c, TEXT_WORD, &[id as i16 as i32 as u32])
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
            ("SetFont", 0x8002_0e38, nfs4rc::f_80020e38, |_, r| vec![pick(r, 6) as i32 as u32 - 1]),
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

    #[test]
    fn fetextrender_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0x7E91 + k as u64, shuffle, &args);
        }
    }
}
