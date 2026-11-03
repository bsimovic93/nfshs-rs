//! RPAUSE.CPP: the pause menu's frames. While paused only the menu's view is drawn: the race's
//! last picture is copied into the buffer being drawn, the menu drawn over it.
//!
//! Written readable from the start (step 3 of DECOMP.md's plan): the game's data as the
//! original leaves it, the stack and registers free.

use crate::call::{call_addr, with_buffer};
use crate::globals::{FLIP, PLAYER1_VIEW};
use ps1rt::Cpu;

/// The menu's rectangle (`gPauseMenuRect`: x, y, w, h) and view (`Render_gPauseMenuView`), the
/// views (`Draw_gView`, 200 bytes each: +0 the ordering table's length, +192 its two buffers'
/// tables), the display's set-up (a `DISPENV`).
const PAUSE_MENU_RECT: u32 = 0x8013_E6B8;
const PAUSE_MENU_VIEW: u32 = 0x8013_E6CC;
const VIEWS: u32 = 0x8011_FF44;
const VIEW_SIZE: u32 = 200;
const VIEW_OTS: u32 = 192;
const DISP_ENV: u32 = 0x8012_072C;

/// `Draw_GetDRAWENV(view, back)`, `Draw_StartFrameRender`; the GPU library's (the host's)
/// `MoveImage`, `PutDispEnv`, `DrawSync`, `VSync`, `DrawOTag`; `systemtask` (hooked).
const GET_DRAWENV: u32 = 0x800B_EB84;
const START_FRAME_RENDER: u32 = 0x800B_F044;
const MOVE_IMAGE: u32 = 0x800E_E4AC;
const PUT_DISP_ENV: u32 = 0x800E_E740;
const DRAW_SYNC: u32 = 0x800E_E1D8;
const VSYNC: u32 = 0x800F_2C78;
const DRAW_OTAG: u32 = 0x800E_E610;
const SYSTEM_TASK: u32 = 0x800E_77F0;

/// `RPause_CopyBackToFrontBuffer` (RPAUSE.CPP, 0x800ddad0): the menu's rectangle of the first
/// player's picture copied from the buffer shown to the one drawn.
pub fn copy_back_to_front_buffer(c: &mut Cpu) {
    let view = c.m.r32(PLAYER1_VIEW);
    let shown = call_addr(c, GET_DRAWENV, &[view, 1]);
    with_buffer(c, 8, |c, rect| {
        let m = &mut c.m;
        // its clipping rectangle (the environment's first 8 bytes), moved by the menu's place
        for k in 0..8 {
            let b = m.r8(shown.wrapping_add(k));
            m.w8(rect + k, b);
        }
        let x = m.r16(rect).wrapping_add(m.r16(PAUSE_MENU_RECT));
        let view = m.r32(PLAYER1_VIEW);
        let (w, h) = (m.r16(PAUSE_MENU_RECT + 4), m.r16(PAUSE_MENU_RECT + 6));
        m.w16(rect, x);
        let y = m.r16(rect + 2).wrapping_add(m.r16(PAUSE_MENU_RECT + 2));
        m.w16(rect + 4, w);
        m.w16(rect + 6, h);
        m.w16(rect + 2, y);
        let drawn = call_addr(c, GET_DRAWENV, &[view, 0]);
        let m = &mut c.m;
        let x = (m.r16(drawn) as i16 as i32).wrapping_add(m.r16(PAUSE_MENU_RECT) as i16 as i32);
        let y = (m.r16(drawn + 2) as i16 as i32).wrapping_add(m.r16(PAUSE_MENU_RECT + 2) as i16 as i32);
        call_addr(c, MOVE_IMAGE, &[rect, x as u32, y as u32]);
    });
}

pub fn abi_copy_back_to_front_buffer(c: &mut Cpu) {
    copy_back_to_front_buffer(c);
}

/// `RPause_StartPauseMenu` (RPAUSE.CPP, 0x800ddb90): a frame begun.
pub fn start_pause_menu(c: &mut Cpu) {
    call_addr(c, START_FRAME_RENDER, &[]);
}

pub fn abi_start_pause_menu(c: &mut Cpu) {
    start_pause_menu(c);
}

/// `RPause_StopPauseMenu` (RPAUSE.CPP, 0x800ddbb0): the frame shown (the display set up, the
/// drawing finished, the vertical blank waited for), the race's picture copied under the menu,
/// the menu's view's ordering table drawn from its last entry, the buffers flipped.
pub fn stop_pause_menu(c: &mut Cpu) {
    call_addr(c, PUT_DISP_ENV, &[DISP_ENV]);
    call_addr(c, DRAW_SYNC, &[0]);
    call_addr(c, VSYNC, &[0]);
    copy_back_to_front_buffer(c);
    let m = &mut c.m;
    let (flip, view) = (m.r32(FLIP), m.r32(PAUSE_MENU_VIEW));
    let view = VIEWS.wrapping_add(view.wrapping_mul(VIEW_SIZE));
    let length = m.r32(view);
    let ot = m.r32(view.wrapping_add(flip.wrapping_mul(4)).wrapping_add(VIEW_OTS));
    call_addr(c, DRAW_OTAG, &[ot.wrapping_add(length.wrapping_mul(4)).wrapping_sub(4)]);
    call_addr(c, SYSTEM_TASK, &[0]);
    let flip = c.m.r32(FLIP);
    c.m.w32(FLIP, 1u32.wrapping_sub(flip));
}

pub fn abi_stop_pause_menu(c: &mut Cpu) {
    stop_pause_menu(c);
}
