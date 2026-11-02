//! FESCREEN.CPP: the front end's screens (`tScreen`). A screen has two shape files: a
//! permanent one and one swapped in and out. Each is read from the CD in the background,
//! uploaded to VRAM and freed. The module also has the loading text shown while a screen comes
//! up, the display put back to 240 lines, the background picture's strips, and the fade a
//! screen comes and goes with.
//!
//! A screen's `GetShapeInfo`, `PreLoad`, `DrawBackground` and `DrawForeground` are virtual.
//! Written as FEMENU.CPP is (see there): typed views, calls through `crate::call`.

use crate::call::{call, call_addr, call_virtual, with_buffer};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SPRINTF, TEXT_PIXELS, TEXT_WORD};
use crate::fetextrender;
use crate::globals::{GAME_TICKS, PATHS, PLAYER1_VIEW, SCREEN_HEIGHT};
use crate::types::{DRAWENV, DrawShapeExtended, RECT, Screen, ShapeInformation, TextureShapeInfo};
use ps1rt::{Cpu, Mem};

/// A screen's vtable (its own) and vptr; its virtual `GetShapeInfo`, `DrawBackground`,
/// `DrawForeground` and `PreLoad`.
const SCREEN_VT: u32 = 0x8001_13C0;
const VPTR: u32 = 96;
const GET_SHAPE_INFO: u32 = 8;
const DRAW_BACKGROUND: u32 = 16;
const DRAW_FOREGROUND: u32 = 24;
const PRE_LOAD: u32 = 40;

/// A shape file's flags: uploaded.
const UPLOADED: u16 = 1;
/// Where a shape file's name is kept (`tShapeInformation::fFilename`, 16 bytes).
const FILENAME: u32 = 20;

/// Set to skip the loading text once; the permanent shapes' table now.
const SKIP_LOADING_TEXT: u32 = 0x8005_1C50;
const PERMANENT_SHAPES: u32 = 0x8005_1C54;
/// The views (`Draw_gView`, `Draw_tView`s of 200 bytes: two draw environments from +8, one
/// for each display buffer) and the display buffers (`gEnviro`, two `dflip`s of 24 bytes, each
/// a `DISPENV` first: its display and screen rectangles, then whether it is interlaced).
const VIEWS: u32 = 0x8011_FF44;
const VIEW_SIZE: u32 = 200;
const VIEW_DRAWENVS: u32 = 8;
const DRAWENV_SIZE: u32 = 92;
const ENVIRO: u32 = 0x8012_0714;
const DFLIP_SIZE: u32 = 24;
const DISPENV_SCREEN: u32 = 8;
const DISPENV_INTERLACED: u32 = 16;
/// A draw environment's bytes past what the view types have: dithering (`dfe`, drawing to the
/// displayed buffer) and clearing the background (`isbg`).
const DRAWENV_DFE: u32 = 23;
const DRAWENV_ISBG: u32 = 24;
/// "LOADING" and the loading bar's text; the swapped file's prefix, a name, the shapes' path,
/// the shapes' memory's name.
const LOADING_TEXT: u32 = 638;
const SWAP_FORMAT: u32 = 0x8001_13A0;
const NAME_FORMAT: u32 = 0x8001_13A4;
const PATH_FORMAT: u32 = 0x8001_13A8;
const SHAPES_NAME: u32 = 0x8001_13B4;
/// Where the shapes' path is (`Paths_Paths + 0x80`).
const SHAPES_PATH: u32 = PATHS + 0x80;

/// The drawing of a frame (`Draw_StartFrameRender`, `Draw_StartRenderingView`,
/// `Draw_StopRenderingView`, the frame shown, 0x800bf0f0); `PSXDrawSquare` (the host captures
/// its primitives); `DrawSync` and `VSync` (the host's: called by address).
const START_FRAME: fn(&mut Cpu) = nfs4rc::f_800bf044;
const START_VIEW: fn(&mut Cpu) = nfs4rc::f_800bedac;
const STOP_VIEW: fn(&mut Cpu) = nfs4rc::f_800bee9c;
const SHOW_FRAME: fn(&mut Cpu) = nfs4rc::f_800bf0f0;
const DRAW_SQUARE: u32 = 0x8004_F12C;
const DRAW_SYNC: u32 = 0x800E_E1D8;
const VSYNC: u32 = 0x800F_2C78;
/// The CD's reads in the background: their state (> 0 done, 0 still reading, -1 failed, -2
/// gone), what they read, cancelling one, starting one into a buffer or into memory reserved
/// for it.
const READ_STATUS: fn(&mut Cpu) = nfs4rc::f_800f240c;
const READ_ADDRESS: fn(&mut Cpu) = nfs4rc::f_800f2374;
const CANCEL_READ: fn(&mut Cpu) = nfs4rc::f_800f2304;
const READ_FILE_AT: fn(&mut Cpu) = nfs4rc::f_800f2170;
const READ_FILE: fn(&mut Cpu) = nfs4rc::f_800f209c;
const READ_FAILED: i32 = -1;
const READ_GONE: i32 = -2;
/// Memory (`reservememadr`, `purgememadr`, `__builtin_delete`); the sound's work while waiting
/// (`FeAudio_systemtask`); a shape's CLUT freed, a shape file's shapes uploaded
/// (`Texture_MenuReleaseClutId`, `FETexture_LoadPmxAtOffset`).
const RESERVE: fn(&mut Cpu) = nfs4rc::f_800e5f28;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const DELETE: fn(&mut Cpu) = nfs4rc::f_800a4910;
const SYSTEM_TASK: fn(&mut Cpu) = crate::feaudio::systemtask_entry;
const RELEASE_CLUT: fn(&mut Cpu) = nfs4rc::f_800e05b0;
const LOAD_PMX: fn(&mut Cpu) = crate::fetexture::load_pmx_at_offset_entry;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

/// The player's view (`Draw_gView[Draw_gPlayer1View]`) and one of its draw environments.
fn view(m: &mut Mem) -> u32 {
    VIEWS.wrapping_add(m.r32(PLAYER1_VIEW).wrapping_mul(VIEW_SIZE))
}

fn view_drawenv(m: &mut Mem, k: u32) -> DRAWENV {
    DRAWENV(view(m) + VIEW_DRAWENVS + k * DRAWENV_SIZE)
}

/// Shape `k` of a table.
fn shape(table: TextureShapeInfo, k: u32) -> TextureShapeInfo {
    table.at(k)
}

/// A shape's CLUT freed, if it has one (its id the low half of the word).
fn release_clut(c: &mut Cpu, shape: TextureShapeInfo) -> bool {
    let id = shape.clut_id(&mut c.m);
    if id == 0 {
        return false;
    }
    call(c, RELEASE_CLUT, &[id as i16 as i32 as u32]);
    true
}

/// What a shape file's read read freed (unless it was read into the file's own buffer), and
/// forgotten.
fn free_file(c: &mut Cpu, info: ShapeInformation) {
    let file = info.file(&mut c.m);
    if file == 0 {
        return;
    }
    if info.dest_file(&mut c.m) == 0 {
        call(c, PURGE, &[file]);
    }
    info.set_file(&mut c.m, 0);
}

/// `tScreen::DisplayLoadingText` (FESCREEN.CPP, 0x80025ad8): "loading" right-justified over a
/// bar, drawn into two frames (the view's background clearing off meanwhile); unless the
/// loading text is to be skipped once.
pub fn display_loading_text(c: &mut Cpu) {
    if c.m.r32(SKIP_LOADING_TEXT) != 0 {
        c.m.w32(SKIP_LOADING_TEXT, 0);
        return;
    }
    let m = &mut c.m;
    let (first, second) = (view_drawenv(m, 0), view_drawenv(m, 1));
    let was = m.r8(first.0 + DRAWENV_ISBG);
    m.w8(second.0 + DRAWENV_ISBG, 0);
    m.w8(first.0 + DRAWENV_ISBG, 0);
    for _ in 0..2 {
        call(c, START_FRAME, &[]);
        let player = c.m.r32(PLAYER1_VIEW);
        call(c, START_VIEW, &[player]);
        fetextrender::menu_text_positioned_justify(c, LOADING_TEXT, 480, 220, 1, 1, 4);
        let word = call(c, TEXT_WORD, &[LOADING_TEXT]);
        let w = call(c, TEXT_PIXELS, &[word]);
        call_addr(c, DRAW_SQUARE, &[0, 480, 220, 0u32.wrapping_sub(w).wrapping_sub(5), 7]);
        let player = c.m.r32(PLAYER1_VIEW);
        call(c, STOP_VIEW, &[player]);
        call(c, SHOW_FRAME, &[]);
    }
    let m = &mut c.m;
    let (first, second) = (view_drawenv(m, 0), view_drawenv(m, 1));
    m.w8(second.0 + DRAWENV_ISBG, was);
    m.w8(first.0 + DRAWENV_ISBG, was);
}

pub fn abi_display_loading_text(c: &mut Cpu) {
    display_loading_text(c);
}

/// `tScreen::GoNonInterlaced` (FESCREEN.CPP, 0x80025c18): the display 256 by 240 again, not
/// interlaced (its buffers from y 256 and 0), the view's draw environments to match; drawing and
/// the frame waited for.
pub fn go_non_interlaced(c: &mut Cpu) {
    let m = &mut c.m;
    m.w32(SCREEN_HEIGHT, 240);
    let h = m.r16(SCREEN_HEIGHT) as i16;
    let buffer = |k: u32| ENVIRO + k * DFLIP_SIZE;
    m.w8(buffer(0) + DISPENV_INTERLACED, 0);
    m.w8(buffer(1) + DISPENV_INTERLACED, 0);
    RECT(buffer(0)).set_y(m, 256);
    RECT(buffer(1)).set_y(m, 0);
    RECT(buffer(0)).set_h(m, h);
    RECT(buffer(0) + DISPENV_SCREEN).set_h(m, h);
    RECT(buffer(1) + DISPENV_SCREEN).set_h(m, h);
    let first = view_drawenv(m, 0);
    m.w8(first.0 + DRAWENV_DFE, 0);
    first.clip().set_y(m, 0);
    first.clip().set_h(m, h);
    first.set_ofs(m, 0, 0);
    first.set_ofs(m, 1, 0);
    let second = view_drawenv(m, 1);
    let h = m.r16(SCREEN_HEIGHT) as i16;
    second.clip().set_y(m, 256);
    second.set_ofs(m, 0, 0);
    second.set_ofs(m, 1, 256);
    m.w8(second.0 + DRAWENV_DFE, 0);
    second.clip().set_h(m, h);
    call_addr(c, DRAW_SYNC, &[0]);
    call_addr(c, VSYNC, &[0]);
}

pub fn abi_go_non_interlaced(c: &mut Cpu) {
    go_non_interlaced(c);
}

/// `tScreen::DrawBackgroundImage` (FESCREEN.CPP, 0x80025ce8): the background's strips from
/// `first`, `n` of them, from the shapes `shapes`, in the screen's fade (each twice, the second
/// further right, when `wide` is above 0; flipped by `wide`).
pub fn draw_background_image(c: &mut Cpu, this: Screen, first: u32, n: u32, shapes: TextureShapeInfo, wide: u32) {
    // (the options' tint left as it is, as the original leaves it)
    with_buffer(c, DrawShapeExtended::SIZE, |c, at| {
        let options = DrawShapeExtended(at);
        options.set_flip_axis(&mut c.m, wide as i16);
        options.set_custom_shapes(&mut c.m, shapes);
        let end = first.wrapping_add(n) as i32;
        let mut k = first;
        while (k as i32) < end {
            let fade = this.screen_fade_val(&mut c.m) as i32 as u32;
            call(c, DRAW_SHAPE_EXTENDED, &[k, 512, 0, 0, fade, 0, at]);
            if wide as i32 > 0 {
                let fade = this.screen_fade_val(&mut c.m) as i32 as u32;
                call(c, DRAW_SHAPE_EXTENDED, &[k, 608, 0, 0, fade, 0, at]);
            }
            k = k.wrapping_add(1);
        }
    })
}

pub fn abi_draw_background_image(c: &mut Cpu) {
    let wide = crate::abi::arg(c, 4);
    draw_background_image(c, Screen(c.r[4]), c.r[5], c.r[6], TextureShapeInfo(c.r[7]), wide);
}

/// `tScreen::AsyncLoadPermanentShapeFile` (FESCREEN.CPP, 0x80025db8): the permanent shapes read
/// from `name` (none: their read cancelled).
pub fn async_load_permanent_shape_file(c: &mut Cpu, this: Screen, name: u32) {
    async_load_shape_file(c, this, name, this.perm_shapes());
}

pub fn abi_async_load_permanent_shape_file(c: &mut Cpu) {
    async_load_permanent_shape_file(c, Screen(c.r[4]), c.r[5]);
}

/// `tScreen::AsyncLoadSwapShapeFile` (FESCREEN.CPP, 0x80025dd8): the swapped shapes read from
/// `name` with a "z" in front (none: their read cancelled).
pub fn async_load_swap_shape_file(c: &mut Cpu, this: Screen, name: u32) {
    with_buffer(c, 64, |c, file| {
        let file = if name != 0 {
            call(c, SPRINTF, &[file, SWAP_FORMAT, name]);
            file
        } else {
            0
        };
        async_load_shape_file(c, this, file, this.swap_shapes());
    })
}

pub fn abi_async_load_swap_shape_file(c: &mut Cpu) {
    async_load_swap_shape_file(c, Screen(c.r[4]), c.r[5]);
}

/// `tScreen::IsShapeFileLoaded` (FESCREEN.CPP, 0x80025e2c): whether a shape file's read is done
/// (what it read kept; freed when it was cancelled meanwhile). A read that failed or is gone is
/// started again; a cancelled one that had finished is started again too.
pub fn is_shape_file_loaded(c: &mut Cpu, this: Screen, info: ShapeInformation) -> bool {
    let handle = info.async_handle(&mut c.m);
    let restart = |c: &mut Cpu| async_load_shape_file(c, this, info.0 + FILENAME, info);
    if handle == 0 {
        if info.load_cancelled(&mut c.m) == 0 {
            return true;
        }
        free_file(c, info);
        info.set_load_cancelled(&mut c.m, 0);
        restart(c);
        is_shape_file_loaded(c, this, info);
        return false;
    }
    let status = call(c, READ_STATUS, &[handle]) as i32;
    if status > 0 {
        let mut file = info.dest_file(&mut c.m);
        if file == 0 {
            let handle = info.async_handle(&mut c.m);
            file = call(c, READ_ADDRESS, &[handle]);
        }
        info.set_file(&mut c.m, file);
        info.set_async_handle(&mut c.m, 0);
        if info.load_cancelled(&mut c.m) != 0 {
            free_file(c, info);
        }
        true
    } else if status == READ_FAILED {
        let handle = info.async_handle(&mut c.m);
        let file = call(c, READ_ADDRESS, &[handle]);
        if file != 0 {
            call(c, PURGE, &[file]);
        }
        restart(c);
        false
    } else {
        if status == READ_GONE {
            restart(c);
        }
        false
    }
}

pub fn abi_is_shape_file_loaded(c: &mut Cpu) {
    c.r[2] = is_shape_file_loaded(c, Screen(c.r[4]), ShapeInformation(c.r[5])) as u32;
}

/// `tScreen::UploadPermanentShapes` (FESCREEN.CPP, 0x80025fa4): the permanent file's first `n`
/// shapes uploaded (all, for 0), their table the front end's now.
pub fn upload_permanent_shapes(c: &mut Cpu, this: Screen, n: u32) {
    let info = this.perm_shapes();
    upload_shapes(c, this, info, 0, 0, sx(n), 0);
    let table = info.shapes(&mut c.m);
    c.m.w32(PERMANENT_SHAPES, table.0);
}

pub fn abi_upload_permanent_shapes(c: &mut Cpu) {
    upload_permanent_shapes(c, Screen(c.r[4]), c.r[5]);
}

/// `tScreen::UploadSwapShapes` (FESCREEN.CPP, 0x80025ff0): the swapped file's first `n` shapes
/// uploaded (all, for 0).
pub fn upload_swap_shapes(c: &mut Cpu, this: Screen, n: u32) {
    upload_shapes(c, this, this.swap_shapes(), 0, 0, sx(n), 0);
}

pub fn abi_upload_swap_shapes(c: &mut Cpu) {
    upload_swap_shapes(c, Screen(c.r[4]), c.r[5]);
}

/// `tScreen::GetShapeInfo` (FESCREEN.CPP, 0x80026028): no shapes, no files.
pub fn get_shape_info(m: &mut Mem, permanent: u32, swap: u32, name: u32, swap_name: u32) {
    m.w16(swap, 0);
    m.w16(permanent, 0);
    m.w32(swap_name, 0);
    m.w32(name, 0);
}

pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = crate::abi::arg(c, 4);
    get_shape_info(&mut c.m, c.r[5], c.r[6], c.r[7], swap_name);
}

/// A screen's shape files as its `GetShapeInfo` gives them: how many shapes in each (the
/// permanent and the swapped file), their names.
struct ShapeFiles {
    permanent: u32,
    swap: u32,
    name: u32,
    swap_name: u32,
}

fn shape_files(c: &mut Cpu, this: Screen) -> ShapeFiles {
    with_buffer(c, 16, |c, at| {
        call_virtual(c, this.0, VPTR, GET_SHAPE_INFO, &[at, at + 2, at + 4, at + 8]);
        let m = &mut c.m;
        ShapeFiles { permanent: m.r16(at) as u32, swap: m.r16(at + 2) as u32, name: m.r32(at + 4), swap_name: m.r32(at + 8) }
    })
}

/// `tScreen::tScreen` (FESCREEN.CPP, 0x80026040): a screen with no shapes, faded out.
pub fn screen(c: &mut Cpu, this: Screen) -> Screen {
    this.set_vtable(&mut c.m, SCREEN_VT);
    this.perm_shapes().set_shapes(&mut c.m, TextureShapeInfo(0));
    initialize_shapes(c, this.perm_shapes(), 0);
    this.swap_shapes().set_shapes(&mut c.m, TextureShapeInfo(0));
    initialize_shapes(c, this.swap_shapes(), 0);
    this.set_screen_fade_val(&mut c.m, 0);
    this
}

pub fn abi_screen(c: &mut Cpu) {
    c.r[2] = screen(c, Screen(c.r[4])).0;
}

/// `tScreen::~tScreen` (FESCREEN.CPP, 0x80026098): deleted when `flags` asks.
pub fn screen_destructor(c: &mut Cpu, this: Screen, flags: u32) {
    this.set_vtable(&mut c.m, SCREEN_VT);
    if flags & 1 != 0 {
        call(c, DELETE, &[this.0]);
    }
}

pub fn abi_screen_destructor(c: &mut Cpu) {
    screen_destructor(c, Screen(c.r[4]), c.r[5]);
}

/// `tScreen::Initialize` (FESCREEN.CPP, 0x800260cc): the loading text; the screen's own
/// preparations; its shape files waited for (the sound kept going), each uploaded as soon as
/// it is read; the shapes' counts kept; the display put back.
pub fn initialize(c: &mut Cpu, this: Screen) {
    display_loading_text(c);
    call_virtual(c, this.0, VPTR, PRE_LOAD, &[]);
    let files = shape_files(c, this);
    let (permanent, swap) = (this.perm_shapes(), this.swap_shapes());
    loop {
        call(c, SYSTEM_TASK, &[0]);
        let loaded = is_shape_file_loaded(c, this, permanent);
        if permanent.file(&mut c.m) != 0 {
            upload_permanent_shapes(c, this, files.permanent);
        }
        let both = loaded && is_shape_file_loaded(c, this, swap);
        if swap.file(&mut c.m) != 0 {
            upload_swap_shapes(c, this, files.swap);
        }
        if both {
            break;
        }
    }
    permanent.set_num_shapes(&mut c.m, files.permanent as u16);
    swap.set_num_shapes(&mut c.m, files.swap as u16);
    go_non_interlaced(c);
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(c, Screen(c.r[4]));
}

/// `tScreen::Cleanup` (FESCREEN.CPP, 0x800261c0): both reads cancelled, both files' shapes
/// freed.
pub fn cleanup(c: &mut Cpu, this: Screen) {
    async_load_permanent_shape_file(c, this, 0);
    async_load_swap_shape_file(c, this, 0);
    free_shapes(c, this, this.perm_shapes());
    free_shapes(c, this, this.swap_shapes());
}

pub fn abi_cleanup(c: &mut Cpu) {
    cleanup(c, Screen(c.r[4]));
}

/// `tScreen::Draw` (FESCREEN.CPP, 0x8002620c): the background, or the foreground. (FEAPP draws
/// the background last: the ordering table draws what is added later under what came before.)
pub fn draw(c: &mut Cpu, this: Screen, background: bool) {
    let slot = if background { DRAW_BACKGROUND } else { DRAW_FOREGROUND };
    call_virtual(c, this.0, VPTR, slot, &[]);
}

pub fn abi_draw(c: &mut Cpu) {
    draw(c, Screen(c.r[4]), c.r[5] != 0);
}

/// `tScreen::AsyncLoadShapeFile` (FESCREEN.CPP, 0x80026260): a shape file's read started (into
/// its buffer, or memory reserved for it), its name kept; the read before cancelled, what was
/// read freed, not uploaded. With no name the read is just cancelled (or, when it is still
/// reading, noted to be freed when it is done).
pub fn async_load_shape_file(c: &mut Cpu, _this: Screen, name: u32, info: ShapeInformation) {
    if name == 0 {
        let handle = info.async_handle(&mut c.m);
        if handle != 0 {
            call(c, CANCEL_READ, &[handle]);
            info.set_async_handle(&mut c.m, 0);
        }
    }
    cancel_async_load(c, info);
    free_file(c, info);
    info.update_flags(&mut c.m, |f| f & !UPLOADED);
    if name == 0 {
        return;
    }
    call(c, SPRINTF, &[info.0 + FILENAME, NAME_FORMAT, name]);
    with_buffer(c, 256, |c, file| {
        let path = c.m.r32(SHAPES_PATH);
        call(c, SPRINTF, &[file, PATH_FORMAT, path, name]);
        if info.load_cancelled(&mut c.m) != 0 {
            return;
        }
        let buffer = info.dest_file(&mut c.m);
        info.set_load_cancelled(&mut c.m, 0);
        let handle = if buffer != 0 { call(c, READ_FILE_AT, &[file, buffer]) } else { call(c, READ_FILE, &[file, 16]) };
        info.set_async_handle(&mut c.m, handle);
    })
}

pub fn abi_async_load_shape_file(c: &mut Cpu) {
    async_load_shape_file(c, Screen(c.r[4]), c.r[5], ShapeInformation(c.r[6]));
}

/// `tScreen::CancelAsyncLoad` (FESCREEN.CPP, 0x80026374): a shape file's read cancelled: one
/// still reading noted to be freed when done; one done freed (unless read into its buffer).
pub fn cancel_async_load(c: &mut Cpu, info: ShapeInformation) {
    let handle = info.async_handle(&mut c.m);
    if handle == 0 {
        return;
    }
    if call(c, READ_STATUS, &[handle]) == 0 {
        info.set_load_cancelled(&mut c.m, 1);
        return;
    }
    if info.dest_file(&mut c.m) == 0 {
        let handle = info.async_handle(&mut c.m);
        let file = call(c, READ_ADDRESS, &[handle]);
        info.set_file(&mut c.m, file);
    }
    free_file(c, info);
    info.set_async_handle(&mut c.m, 0);
}

pub fn abi_cancel_async_load(c: &mut Cpu) {
    cancel_async_load(c, ShapeInformation(c.r[5]));
}

/// `tScreen::InitializeShapes` (FESCREEN.CPP, 0x8002640c): room for `n` shapes (their table
/// reserved, their CLUTs none), the table before freed; no file, nothing read.
pub fn initialize_shapes(c: &mut Cpu, info: ShapeInformation, n: u32) {
    let table = info.shapes(&mut c.m);
    if table.0 != 0 {
        call(c, PURGE, &[table.0]);
        info.set_shapes(&mut c.m, TextureShapeInfo(0));
    }
    let m = &mut c.m;
    info.set_flags(m, 0);
    info.set_num_shapes(m, n as u16);
    info.set_async_handle(m, 0);
    info.set_file(m, 0);
    info.set_dest_file(m, 0);
    info.set_load_cancelled(m, 0);
    if n == 0 {
        return;
    }
    let table = TextureShapeInfo(call(c, RESERVE, &[SHAPES_NAME, n << 5, 0]));
    info.set_shapes(&mut c.m, table);
    for k in 0..n {
        let table = info.shapes(&mut c.m);
        shape(table, k).set_clut_id(&mut c.m, 0);
    }
}

pub fn abi_initialize_shapes(c: &mut Cpu) {
    initialize_shapes(c, ShapeInformation(c.r[5]), c.r[6]);
}

/// `tScreen::FreeShapes` (FESCREEN.CPP, 0x800264b4): a shape file's read cancelled (one still
/// reading waited for, the sound kept going), what it read freed, its shapes' CLUTs and table.
pub fn free_shapes(c: &mut Cpu, _this: Screen, info: ShapeInformation) {
    cancel_async_load(c, info);
    while info.async_handle(&mut c.m) != 0 {
        let handle = info.async_handle(&mut c.m);
        let status = call(c, READ_STATUS, &[handle]) as i32;
        if status > 0 || status == READ_FAILED {
            if info.dest_file(&mut c.m) == 0 {
                let handle = info.async_handle(&mut c.m);
                let file = call(c, READ_ADDRESS, &[handle]);
                info.set_file(&mut c.m, file);
            } else {
                info.set_file(&mut c.m, 0);
            }
            info.set_async_handle(&mut c.m, 0);
        } else if status == READ_GONE {
            info.set_async_handle(&mut c.m, 0);
        }
        call(c, SYSTEM_TASK, &[0]);
    }
    free_file(c, info);
    if info.shapes(&mut c.m).0 == 0 {
        return;
    }
    let mut k = 0u32;
    // (the count read again each time round: the original's loop is a do-while)
    if info.num_shapes(&mut c.m) != 0 {
        loop {
            let table = info.shapes(&mut c.m);
            release_clut(c, shape(table, sx(k)));
            k = k.wrapping_add(1);
            if sx(k) as i32 >= info.num_shapes(&mut c.m) as i32 {
                break;
            }
        }
    }
    let table = info.shapes(&mut c.m);
    call(c, PURGE, &[table.0]);
    info.set_shapes(&mut c.m, TextureShapeInfo(0));
}

pub fn abi_free_shapes(c: &mut Cpu) {
    free_shapes(c, Screen(c.r[4]), ShapeInformation(c.r[5]));
}

/// `tScreen::UploadShapes` (FESCREEN.CPP, 0x80026614): `n` shapes of a file read (all, for 0)
/// uploaded to VRAM into the table's from `first` on (their CLUTs freed first), `dx` across and
/// `dy` down from their places; noted as uploaded, the file freed (unless read into its buffer).
pub fn upload_shapes(c: &mut Cpu, _this: Screen, info: ShapeInformation, dx: u32, dy: u32, n: u32, first: u32) {
    if info.file(&mut c.m) == 0 {
        return;
    }
    let n = if n & 0xFFFF == 0 { info.num_shapes(&mut c.m) as u32 } else { n };
    let count = sx(n) as i32;
    let first = sx(first);
    for k in 0..count.max(0) as u32 {
        let table = info.shapes(&mut c.m);
        if release_clut(c, shape(table, k.wrapping_add(first))) {
            let table = info.shapes(&mut c.m);
            shape(table, k.wrapping_add(first)).set_clut_id(&mut c.m, 0);
        }
    }
    info.update_flags(&mut c.m, |f| f & !UPLOADED);
    for k in 0..count.max(0) as u32 {
        let (file, table) = (info.file(&mut c.m), info.shapes(&mut c.m));
        call(c, LOAD_PMX, &[file, k, shape(table, k.wrapping_add(first)).0, sx(dy), sx(dx)]);
    }
    info.update_flags(&mut c.m, |f| f | UPLOADED);
    if info.dest_file(&mut c.m) == 0 {
        let file = info.file(&mut c.m);
        call(c, PURGE, &[file]);
    }
    info.set_file(&mut c.m, 0);
}

pub fn abi_upload_shapes(c: &mut Cpu) {
    let (n, first) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5));
    upload_shapes(c, Screen(c.r[4]), ShapeInformation(c.r[5]), c.r[6], c.r[7], n, first);
}

/// `tScreen::PreLoad` (FESCREEN.CPP, 0x800267a0): the screen's shape files (its
/// `GetShapeInfo`'s) started reading, room made for their shapes.
pub fn pre_load(c: &mut Cpu, this: Screen) {
    let files = shape_files(c, this);
    initialize_shapes(c, this.perm_shapes(), sx(files.permanent));
    initialize_shapes(c, this.swap_shapes(), sx(files.swap));
    async_load_permanent_shape_file(c, this, files.name);
    async_load_swap_shape_file(c, this, files.swap_name);
}

pub fn abi_pre_load(c: &mut Cpu) {
    pre_load(c, Screen(c.r[4]));
}

/// `tScreen::TransitionOff` (FESCREEN.CPP, 0x80026824): fading out from now.
pub fn transition_off(m: &mut Mem, this: Screen) {
    let ticks = m.r32(GAME_TICKS);
    this.set_screen_fade_val(m, 0);
    this.set_internal_screen_fade_val(m, 0);
    this.set_transition_off(m, 1);
    this.set_transition_ticks(m, ticks as i32);
}

pub fn abi_transition_off(c: &mut Cpu) {
    transition_off(&mut c.m, Screen(c.r[4]));
}

/// `tScreen::TransitionOn` (FESCREEN.CPP, 0x80026844): fading in from now.
pub fn transition_on(m: &mut Mem, this: Screen) {
    this.set_screen_fade_val(m, 128);
    let ticks = m.r32(GAME_TICKS);
    this.set_internal_screen_fade_val(m, 128);
    this.set_transition_off(m, 0);
    this.set_transition_ticks(m, ticks as i32);
}

pub fn abi_transition_on(c: &mut Cpu) {
    transition_on(&mut c.m, Screen(c.r[4]));
}

/// `tScreen::UpdateTransition` (FESCREEN.CPP, 0x80026868): the transition 12 on (going off) or
/// back, within -50..150; the fade it, within 0..128.
pub fn update_transition(m: &mut Mem, this: Screen) {
    let step = if this.transition_off(m) != 0 { 12 } else { -12 };
    let v = this.internal_screen_fade_val(m).wrapping_add(step);
    this.set_internal_screen_fade_val(m, v);
    this.set_internal_screen_fade_val(m, v.clamp(-50, 150));
    let fade = this.internal_screen_fade_val(m) as i16;
    this.set_screen_fade_val(m, fade);
    this.set_screen_fade_val(m, fade.clamp(0, 128));
}

pub fn abi_update_transition(c: &mut Cpu) {
    update_transition(&mut c.m, Screen(c.r[4]));
}

/// `tScreen::TransitionIsFinished` (FESCREEN.CPP, 0x800268e0): once the transition has
/// overshot either way (below -24 or above 142).
pub fn transition_is_finished(m: &mut Mem, this: Screen) -> bool {
    (this.internal_screen_fade_val(m) as u32).wrapping_add(24) >= 167
}

pub fn abi_transition_is_finished(c: &mut Cpu) {
    c.r[2] = transition_is_finished(&mut c.m, Screen(c.r[4])) as u32;
}

/// `tScreen::ProcessInput` (FESCREEN.CPP, 0x800268f8): nothing.
pub fn abi_process_input(_: &mut Cpu) {}

/// `tScreen::DrawForeground` (FESCREEN.H, 0x80026900): nothing.
pub fn abi_draw_foreground(_: &mut Cpu) {}

/// `tScreen::DrawBackground` (FESCREEN.H, 0x80026908): nothing.
pub fn abi_draw_background(_: &mut Cpu) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, COMMAND, Case, KEY, MENU, NEW, pick, player, via_abi};
    use crate::globals::FE_APP;
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// A screen's fields the shuffle sets (`tScreen`, `tShapeInformation`): the swapped shapes,
    /// the transition, going off, the fade; a shape file's table, file, buffer, read, flags and
    /// cancellation.
    const SWAP: u32 = 40;
    const TRANSITION: u32 = 88;
    const GOING_OFF: u32 = 84;
    const FADE: u32 = 92;
    const TABLE: u32 = 0;
    const DATA: u32 = 4;
    const BUFFER: u32 = 8;
    const HANDLE: u32 = 12;
    const COUNT: u32 = 16;
    const UPLOADED: u32 = 18;
    const CANCELLED: u32 = 36;

    /// A copy of the front end's screen (the car select's, its shapes' tables and the swapped
    /// file's buffer, which still holds its last file) as a plain screen; the front end's own
    /// (`gFEApp + 12`, its real vtable).
    const SCREEN: u32 = 0x8014_7000;
    const SCREEN_SIZE: u32 = 100;
    /// Names of shape files, the outputs of `GetShapeInfo`.
    const NAMES: u32 = 0x8014_7200;
    const OUT: u32 = 0x8014_7300;
    /// A `GetShapeInfo` of a few shapes and no files (MIPS, interpreted), and the screen's
    /// vtable copied with it.
    const COUNTS_ONLY: u32 = 0x8014_7400;
    const COUNTS_VT: u32 = 0x8014_7500;
    /// The CD's reads (`numrequests`, `request`: 44 bytes each, +0 the handle, +8 what it read
    /// (0: it failed), +12 an error, +16 where it read to, +20 kept, +24 still reading); two of
    /// them the tests' (their handles).
    const NUM_REQUESTS: u32 = 0x8013_F19C;
    const REQUESTS: u32 = 0x8013_F1A0;
    const READS: [u32; 2] = [28, 29];

    fn read_handle(k: u32) -> u32 {
        0x300 | READS[k as usize]
    }

    fn read_entry(m: &mut Mem, k: u32) -> u32 {
        m.r32(REQUESTS) + 44 * READS[k as usize]
    }

    fn real(m: &mut Mem) -> u32 {
        let app = m.r32(FE_APP);
        m.r32(app + 12)
    }

    fn info(rng: &mut Rng) -> u32 {
        [SCREEN, SCREEN + SWAP][pick(rng, 2) as usize]
    }

    fn name(rng: &mut Rng) -> u32 {
        [0, NAMES, NAMES + 16][pick(rng, 3) as usize]
    }

    /// FEMENU's structures, then the screen copied, its shapes in any state of their reads (none,
    /// one gone missing, done or cancelled; read into the buffer or not; uploaded or not), its
    /// transition and fade about their ranges; the loading text skipped or not.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        let from = real(m);
        for k in 0..SCREEN_SIZE {
            let b = m.r8(from + k);
            m.w8(SCREEN + k, b);
        }
        m.w32(SCREEN + VPTR, SCREEN_VT);
        for (k, s) in [&b"cars\0"[..], b"BNS11\0"].into_iter().enumerate() {
            for (j, &b) in s.iter().enumerate() {
                m.w8(NAMES + 16 * k as u32 + j as u32, b);
            }
        }
        // two reads: done (into memory reserved for it), still reading, failed or in error
        assert!(m.r32(NUM_REQUESTS) > READS[1]);
        let block = crate::abi::call(c, RESERVE, &[SHAPES_NAME, 64, 0]);
        let m = &mut c.m;
        for k in 0..2 {
            let e = read_entry(m, k);
            let state = pick(rng, 4);
            m.w32(e, read_handle(k));
            m.w32(e + 8, if state == 2 { 0 } else { 0x1000 });
            m.w32(e + 12, (state == 3) as u32);
            m.w32(e + 16, [block, 1][pick(rng, 2) as usize]);
            m.w32(e + 20, 1);
            m.w32(e + 24, (state == 1) as u32);
        }
        for info in [SCREEN, SCREEN + SWAP] {
            m.w32(info + HANDLE, [0, 0, 0x1234, read_handle(0), read_handle(1)][pick(rng, 5) as usize]);
            m.w32(info + CANCELLED, (pick(rng, 3) == 0) as u32);
            m.w16(info + UPLOADED, pick(rng, 4) as u16);
        }
        // the swapped file read into its buffer (now and then not read yet)
        let buffer = m.r32(SCREEN + SWAP + BUFFER);
        m.w32(SCREEN + SWAP + DATA, [0, buffer][pick(rng, 2) as usize]);
        // (142 and -24 the last before it is finished)
        let at = [0, 128, -50, 150, 142, 143, -24, -25, pick(rng, 220) as i32 - 60][pick(rng, 9) as usize];
        m.w32(SCREEN + TRANSITION, at as u32);
        m.w32(SCREEN + GOING_OFF, pick(rng, 2));
        m.w16(SCREEN + FADE, [0, 128, pick(rng, 129)][pick(rng, 3) as usize] as u16);
        m.w32(SKIP_LOADING_TEXT, (pick(rng, 3) == 0) as u32);
        // the display's environment and the view as an interlaced screen might leave them
        for k in 0..48 {
            m.w8(ENVIRO + k, pick(rng, 256) as u8);
        }
        let v = view(m);
        for k in [10, 14, 16, 18, 102, 106, 108, 110] {
            m.w16(v + k, pick(rng, 512) as u16);
        }
        for k in [31, 32, 123, 124] {
            m.w8(v + k, pick(rng, 2) as u8);
        }
        m.w32(SCREEN_HEIGHT, [240, 480][pick(rng, 2) as usize]);
        // a few shapes of each kind, no files
        let (permanent, swap) = (pick(rng, 20), pick(rng, 20));
        let code = [0x2402_0000 | permanent, 0xA4A2_0000, 0x2402_0000 | swap, 0xA4C2_0000, 0xACE0_0000, 0x8FA2_0010, 0, 0xAC40_0000, 0x03E0_0008, 0];
        for (k, w) in code.into_iter().enumerate() {
            m.w32(COUNTS_ONLY + 4 * k as u32, w);
        }
        for k in 0..24 {
            let v = m.r32(SCREEN_VT + 4 * k);
            m.w32(COUNTS_VT + 4 * k, v);
        }
        m.w32(COUNTS_VT + GET_SHAPE_INFO + 4, COUNTS_ONLY);
    }

    /// No read left still reading (one would be waited for for ever: the CD never reads).
    fn reads_done(c: &mut Cpu) {
        for k in 0..2 {
            let e = read_entry(&mut c.m, k);
            c.m.w32(e + 24, 0);
        }
    }

    /// The screen with no files but counts, now and then.
    fn counts_only(c: &mut Cpu, r: &mut Rng) {
        if pick(r, 2) == 0 {
            c.m.w32(SCREEN + VPTR, COUNTS_VT);
        }
    }

    /// The swapped shapes' file (11 shapes, in their buffer) given to the permanent shapes too,
    /// for an upload of either.
    fn upload_from_buffer(c: &mut Cpu, info: u32) {
        let m = &mut c.m;
        let buffer = m.r32(SCREEN + SWAP + BUFFER);
        let table = m.r32(SCREEN + SWAP + TABLE);
        m.w32(info + BUFFER, buffer);
        m.w32(info + DATA, buffer);
        m.w32(info + TABLE, table);
        m.w16(info + COUNT, 11);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("DisplayLoadingText", 0x8002_5ad8, nfs4rc::f_80025ad8, |_, _| vec![SCREEN]),
            ("GoNonInterlaced", 0x8002_5c18, nfs4rc::f_80025c18, |_, _| vec![SCREEN]),
            ("DrawBackgroundImage", 0x8002_5ce8, nfs4rc::f_80025ce8, |c, r| {
                // (the strips from the permanent shapes' table, as the screens draw theirs)
                let table = c.m.r32(SCREEN + TABLE);
                vec![SCREEN, pick(r, 130), [0, 1, 2, 5, u32::MAX][pick(r, 5) as usize], table, [0, 1, u32::MAX][pick(r, 3) as usize]]
            }),
            ("AsyncLoadPermanentShapeFile", 0x8002_5db8, nfs4rc::f_80025db8, |_, r| vec![SCREEN, name(r)]),
            ("AsyncLoadSwapShapeFile", 0x8002_5dd8, nfs4rc::f_80025dd8, |_, r| vec![SCREEN, name(r)]),
            ("IsShapeFileLoaded", 0x8002_5e2c, nfs4rc::f_80025e2c, |_, r| vec![SCREEN, info(r)]),
            ("UploadPermanentShapes", 0x8002_5fa4, nfs4rc::f_80025fa4, |c, r| {
                upload_from_buffer(c, SCREEN);
                vec![SCREEN, pick(r, 12)]
            }),
            ("UploadSwapShapes", 0x8002_5ff0, nfs4rc::f_80025ff0, |_, r| vec![SCREEN, pick(r, 12)]),
            ("GetShapeInfo", 0x8002_6028, nfs4rc::f_80026028, |_, _| vec![SCREEN, OUT, OUT + 2, OUT + 4, OUT + 8]),
            ("tScreen", 0x8002_6040, nfs4rc::f_80026040, |c, _| {
                c.m.w32(NEW, 0);
                c.m.w32(NEW + SWAP, 0);
                vec![NEW]
            }),
            ("~tScreen", 0x8002_6098, nfs4rc::f_80026098, |c, r| vec![[SCREEN, real(&mut c.m)][pick(r, 2) as usize], 2 * pick(r, 2)]),
            ("Initialize", 0x8002_60cc, nfs4rc::f_800260cc, |c, r| {
                // (no reads left to wait for: the tours wait for real ones)
                for info in [SCREEN, SCREEN + SWAP] {
                    c.m.w32(info + HANDLE, 0);
                    c.m.w32(info + CANCELLED, 0);
                }
                counts_only(c, r);
                vec![SCREEN]
            }),
            ("Cleanup", 0x8002_61c0, nfs4rc::f_800261c0, |c, _| {
                reads_done(c);
                vec![SCREEN]
            }),
            ("tScreen::Draw", 0x8002_620c, nfs4rc::f_8002620c, |c, r| vec![[SCREEN, real(&mut c.m)][pick(r, 2) as usize], pick(r, 2)]),
            ("AsyncLoadShapeFile", 0x8002_6260, nfs4rc::f_80026260, |_, r| vec![SCREEN, name(r), info(r)]),
            ("CancelAsyncLoad", 0x8002_6374, nfs4rc::f_80026374, |_, r| vec![SCREEN, info(r)]),
            ("InitializeShapes", 0x8002_640c, nfs4rc::f_8002640c, |_, r| vec![SCREEN, info(r), [0, 1, 11, pick(r, 40)][pick(r, 4) as usize]]),
            ("FreeShapes", 0x8002_64b4, nfs4rc::f_800264b4, |c, r| {
                reads_done(c);
                vec![SCREEN, info(r)]
            }),
            ("UploadShapes", 0x8002_6614, nfs4rc::f_80026614, |c, r| {
                let at = info(r);
                upload_from_buffer(c, at);
                let n = pick(r, 12);
                let first = if n == 0 { 0 } else { pick(r, 12 - n) };
                vec![SCREEN, at, pick(r, 64), pick(r, 64), n, first]
            }),
            ("PreLoad", 0x8002_67a0, nfs4rc::f_800267a0, |c, r| {
                counts_only(c, r);
                vec![SCREEN]
            }),
            ("TransitionOff", 0x8002_6824, nfs4rc::f_80026824, |_, r| vec![SCREEN, pick(r, 4), MENU]),
            ("TransitionOn", 0x8002_6844, nfs4rc::f_80026844, |_, r| vec![SCREEN, pick(r, 4), MENU]),
            ("UpdateTransition", 0x8002_6868, nfs4rc::f_80026868, |_, _| vec![SCREEN]),
            ("TransitionIsFinished", 0x8002_68e0, nfs4rc::f_800268e0, |_, _| vec![SCREEN]),
            ("tScreen::ProcessInput", 0x8002_68f8, nfs4rc::f_800268f8, |_, r| vec![SCREEN, player(r), KEY, COMMAND]),
            ("DrawForeground", 0x8002_6900, nfs4rc::f_80026900, |_, _| vec![SCREEN]),
            ("DrawBackground", 0x8002_6908, nfs4rc::f_80026908, |_, _| vec![SCREEN]),
        ]
    }

    #[test]
    fn fescreen_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x5C0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 200, 0x5C1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
