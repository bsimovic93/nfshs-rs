//! FESCREEN.CPP: the front end's screens (`tScreen`): their shape files, a permanent one and
//! one swapped in and out (each loaded from the CD in the background, uploaded to VRAM, freed),
//! the loading text shown while a screen comes up, the display put back to 240 lines, the
//! background picture's strips, and the fade a screen comes and goes with.
//!
//! A screen's `GetShapeInfo`, `PreLoad`, `DrawBackground` and `DrawForeground` are virtual,
//! called with the registers the original has at its `jalr`.

use crate::abi::{self, Saved};
use crate::femenu::method;
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SPRINTF, TEXT_PIXELS, TEXT_WORD};
use crate::fetextrender;
use crate::globals::{GAME_TICKS, PATHS, PLAYER1_VIEW, SCREEN_HEIGHT};
use ps1rt::{Cpu, Mem};

/// A screen's vtable (its own) and vptr; its virtual `GetShapeInfo`, `DrawBackground`,
/// `DrawForeground` and `PreLoad`.
const SCREEN_VT: u32 = 0x8001_13C0;
const VPTR: u32 = 96;
const GET_SHAPE_INFO: u32 = 8;
const DRAW_BACKGROUND: u32 = 16;
const DRAW_FOREGROUND: u32 = 24;
const PRE_LOAD: u32 = 40;

// A screen: +0 its permanent shapes, +40 its swapped ones (each a `tShapeInformation`), +80 the
// tick its transition started, +84 going off, +88 the transition (128 on, 0 off, overshooting
// to -50 and 150), +92 its fade (0..128).
const SWAP: u32 = 40;
const TRANSITION_AT: u32 = 80;
const GOING_OFF: u32 = 84;
const TRANSITION: u32 = 88;
const FADE: u32 = 92;

// A screen's shapes (`tShapeInformation`): +0 the shapes' table (32 bytes a shape, +4 its
// CLUT id), +4 the file read, +8 the buffer it is read into (none: memory reserved for it),
// +12 its read's handle, +16 how many shapes, +18 uploaded (bit 0), +20 the file's name, +36
// cancelled while it was still reading.
const TABLE: u32 = 0;
const DATA: u32 = 4;
const BUFFER: u32 = 8;
const HANDLE: u32 = 12;
const COUNT: u32 = 16;
const UPLOADED: u32 = 18;
const NAME: u32 = 20;
const CANCELLED: u32 = 36;

/// Set to skip the loading text once; the permanent shapes' table now.
const SKIP_LOADING_TEXT: u32 = 0x8005_1C50;
const PERMANENT_SHAPES: u32 = 0x8005_1C54;
/// The views (200 bytes each) and the display's environment (`Draw_gView`, `gEnviro`).
const VIEWS: u32 = 0x8011_FF44;
const VIEW_SIZE: u32 = 200;
const ENVIRO: u32 = 0x8012_0714;
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
/// The CD's reads in the background: their state, what they read, cancelling one, starting
/// one into a buffer or into memory reserved for it.
const READ_STATUS: fn(&mut Cpu) = nfs4rc::f_800f240c;
const READ_ADDRESS: fn(&mut Cpu) = nfs4rc::f_800f2374;
const CANCEL_READ: fn(&mut Cpu) = nfs4rc::f_800f2304;
const READ_FILE_AT: fn(&mut Cpu) = nfs4rc::f_800f2170;
const READ_FILE: fn(&mut Cpu) = nfs4rc::f_800f209c;
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

fn lh(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as i16 as i32 as u32
}

/// The player's view (`Draw_gView[Draw_gPlayer1View]`).
fn view(m: &mut Mem) -> u32 {
    VIEWS.wrapping_add(m.r32(PLAYER1_VIEW).wrapping_mul(VIEW_SIZE))
}

/// `tScreen::DisplayLoadingText` (FESCREEN.CPP, 0x80025ad8): "loading" right-justified over
/// a bar, drawn into two frames (its view's two flags off meanwhile); unless the loading text
/// is to be skipped once.
pub fn abi_display_loading_text(c: &mut Cpu) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        if c.m.r32(SKIP_LOADING_TEXT) != 0 {
            c.m.w32(SKIP_LOADING_TEXT, 0);
            return;
        }
        r.0[0] = 0;
        let v = view(&mut c.m);
        let was = c.m.r8(v + 32);
        r.0[2] = was as u32;
        r.0[1] = 0x8014_0000;
        c.m.w8(v + 124, 0);
        c.m.w8(v + 32, 0);
        while (sx(r.0[0]) as i32) < 2 {
            r.call_leaving(c, START_FRAME, &[], 0x8002_5b44);
            let player = c.m.r32(PLAYER1_VIEW);
            r.call_leaving(c, START_VIEW, &[player], 0x8002_5b50);
            r.call_leaving(c, fetextrender::menu_text_positioned_justify_entry, &[LOADING_TEXT, 480, 220, 1, 1, 4], 0x8002_5b74);
            let word = r.call_leaving(c, TEXT_WORD, &[LOADING_TEXT], 0x8002_5b7c);
            let w = r.call_leaving(c, TEXT_PIXELS, &[word], 0x8002_5b84);
            r.call_ptr_leaving(c, DRAW_SQUARE, &[0, 480, 220, 0u32.wrapping_sub(w).wrapping_sub(5), 7], 0x8002_5ba4);
            let player = c.m.r32(PLAYER1_VIEW);
            r.call_leaving(c, STOP_VIEW, &[player], 0x8002_5bb0);
            r.call_leaving(c, SHOW_FRAME, &[], 0x8002_5bb8);
            r.0[0] = r.0[0].wrapping_add(1);
        }
        let v = view(&mut c.m);
        c.m.w8(v + 124, was);
        c.m.w8(v + 32, was);
    })
}

/// `tScreen::GoNonInterlaced` (FESCREEN.CPP, 0x80025c18): the display 256 by 240 again, not
/// interlaced, the view's too; drawing and the frame waited for.
pub fn abi_go_non_interlaced(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        m.w32(SCREEN_HEIGHT, 240);
        let h = m.r16(SCREEN_HEIGHT);
        m.w8(ENVIRO + 16, 0);
        m.w8(ENVIRO + 40, 0);
        m.w16(ENVIRO + 2, 256);
        m.w16(ENVIRO + 26, 0);
        m.w16(ENVIRO + 6, h);
        m.w16(ENVIRO + 14, h);
        m.w16(ENVIRO + 38, h);
        let v = view(m);
        m.w8(v + 31, 0);
        m.w16(v + 10, 0);
        m.w16(v + 14, h);
        m.w16(v + 16, 0);
        m.w16(v + 18, 0);
        let v = view(m);
        let h = m.r16(SCREEN_HEIGHT);
        m.w16(v + 102, 256);
        m.w16(v + 108, 0);
        m.w16(v + 110, 256);
        m.w8(v + 123, 0);
        m.w16(v + 106, h);
        // (a1..a3 as the stores above left them: the height, the views, 256; the host's calls
        // leave them for the caller)
        c.r[5..8].copy_from_slice(&[h as u32, VIEWS, 256]);
        let r = Saved::of(c);
        r.call_ptr_leaving(c, DRAW_SYNC, &[0], 0x8002_5cd0);
        r.call_ptr_leaving(c, VSYNC, &[0], 0x8002_5cd8);
    })
}

/// `tScreen::DrawBackgroundImage` (FESCREEN.CPP, 0x80025ce8): the background's strips `first`
/// on, `n` of them, in the screen's fade (each twice, the second further right, when `wide`).
pub fn abi_draw_background_image(c: &mut Cpu) {
    let (this, first, n, info) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let wide = abi::arg(c, 4);
    abi::frame(c, 88, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        r.0[3] = first;
        r.0[4] = n;
        r.0[0] = first;
        r.0[5] = wide;
        c.m.w32(sp + 52, info);
        c.m.w16(sp + 32, wide as u16);
        if (first as i32) >= (first.wrapping_add(n) as i32) {
            return;
        }
        let place = sp + 32;
        r.0[1] = place;
        loop {
            let k = r.0[0];
            let fade = lh(&mut c.m, this + FADE);
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[k, 512, 0, 0, fade, 0, place], 0x8002_5d58);
            if wide as i32 > 0 {
                let fade = lh(&mut c.m, this + FADE);
                r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[k, 608, 0, 0, fade, 0, place], 0x8002_5d80);
            }
            r.0[0] = k.wrapping_add(1);
            if (r.0[0] as i32) >= (first.wrapping_add(n) as i32) {
                return;
            }
        }
    })
}

/// `tScreen::AsyncLoadPermanentShapeFile` (FESCREEN.CPP, 0x80025db8): the permanent shapes read
/// from `name` (none: their read cancelled).
pub fn abi_async_load_permanent_shape_file(c: &mut Cpu) {
    let (this, name) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        Saved::of(c).enter_leaving(c, 0x8002_6260, 0x8002_5dc8, |c| async_load_shape_file(c, this, name, this));
    })
}

/// `tScreen::AsyncLoadSwapShapeFile` (FESCREEN.CPP, 0x80025dd8): the swapped shapes read from
/// `name` with a "z" in front (none: their read cancelled).
pub fn abi_async_load_swap_shape_file(c: &mut Cpu) {
    let (this, name) = (c.r[4], c.r[5]);
    abi::frame(c, 56, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let file = if name != 0 {
            let file = sp + 16;
            r.call_leaving(c, SPRINTF, &[file, SWAP_FORMAT, name], 0x8002_5e00);
            file
        } else {
            0
        };
        r.enter_leaving(c, 0x8002_6260, 0x8002_5e1c, |c| async_load_shape_file(c, this, file, this + SWAP));
    })
}

/// `tScreen::IsShapeFileLoaded` (FESCREEN.CPP, 0x80025e2c): whether a shape file's read is done
/// (what it read kept; freed when it was cancelled meanwhile); a read that failed or went
/// missing started again; a cancelled one that had finished started again too.
pub fn is_shape_file_loaded(c: &mut Cpu, this: u32, info: u32) -> u32 {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        r.0[0] = info;
        r.0[1] = 1;
        let handle = c.m.r32(info + HANDLE);
        if handle == 0 {
            if c.m.r32(info + CANCELLED) == 0 {
                return 1;
            }
            let data = c.m.r32(info + DATA);
            if data != 0 {
                if c.m.r32(info + BUFFER) == 0 {
                    r.call_leaving(c, PURGE, &[data], 0x8002_5f60);
                }
                c.m.w32(info + DATA, 0);
            }
            c.m.w32(info + CANCELLED, 0);
            r.enter_leaving(c, 0x8002_6260, 0x8002_5f78, |c| async_load_shape_file(c, this, info + NAME, info));
            r.enter_leaving(c, 0x8002_5e2c, 0x8002_5f84, |c| is_shape_file_loaded(c, this, info));
            return 0;
        }
        let status = r.call_leaving(c, READ_STATUS, &[handle], 0x8002_5e60) as i32;
        if status > 0 {
            let mut data = c.m.r32(info + BUFFER);
            if data == 0 {
                let handle = c.m.r32(info + HANDLE);
                data = r.call_leaving(c, READ_ADDRESS, &[handle], 0x8002_5e88);
            }
            c.m.w32(info + DATA, data);
            let data = c.m.r32(info + DATA);
            c.m.w32(info + HANDLE, 0);
            if data != 0 && c.m.r32(info + CANCELLED) != 0 {
                if c.m.r32(info + BUFFER) == 0 {
                    r.call_leaving(c, PURGE, &[data], 0x8002_5ec4);
                }
                c.m.w32(info + DATA, 0);
            }
            1
        } else if status == -1 {
            let handle = c.m.r32(info + HANDLE);
            let data = r.call_leaving(c, READ_ADDRESS, &[handle], 0x8002_5ee0);
            if data != 0 {
                r.call_leaving(c, PURGE, &[data], 0x8002_5ef0);
            }
            r.enter_leaving(c, 0x8002_6260, 0x8002_5f00, |c| async_load_shape_file(c, this, info + NAME, info));
            0
        } else {
            r.0[1] = 0;
            if status == -2 {
                r.enter_leaving(c, 0x8002_6260, 0x8002_5f20, |c| async_load_shape_file(c, this, info + NAME, info));
            }
            0
        }
    })
}

pub fn abi_is_shape_file_loaded(c: &mut Cpu) {
    c.r[2] = is_shape_file_loaded(c, c.r[4], c.r[5]);
}

/// `tScreen::UploadPermanentShapes` (FESCREEN.CPP, 0x80025fa4): the permanent file's first `n`
/// shapes uploaded (all, for 0), their table the front end's now.
pub fn abi_upload_permanent_shapes(c: &mut Cpu) {
    let (this, n) = (c.r[4], c.r[5]);
    abi::frame(c, 32, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        c.m.w32(sp + 16, sx(n));
        c.m.w32(sp + 20, 0);
        r.enter_leaving(c, 0x8002_6614, 0x8002_5fd4, |c| upload_shapes(c, this, this, 0, 0, sx(n), 0));
        let table = c.m.r32(this + TABLE);
        c.m.w32(PERMANENT_SHAPES, table);
    })
}

/// `tScreen::UploadSwapShapes` (FESCREEN.CPP, 0x80025ff0): the swapped file's first `n` shapes
/// uploaded (all, for 0).
pub fn abi_upload_swap_shapes(c: &mut Cpu) {
    let (this, n) = (c.r[4], c.r[5]);
    abi::frame(c, 32, |c, sp| {
        c.m.w32(sp + 16, sx(n));
        c.m.w32(sp + 20, 0);
        Saved::of(c).enter_leaving(c, 0x8002_6614, 0x8002_6018, |c| upload_shapes(c, this, this + SWAP, 0, 0, sx(n), 0));
    })
}

/// `tScreen::GetShapeInfo` (FESCREEN.CPP, 0x80026028): no shapes, no files.
pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = abi::arg(c, 4);
    let (permanent, swap, name) = (c.r[5], c.r[6], c.r[7]);
    let m = &mut c.m;
    m.w16(swap, 0);
    m.w16(permanent, 0);
    m.w32(swap_name, 0);
    m.w32(name, 0);
}

/// `tScreen::tScreen` (FESCREEN.CPP, 0x80026040): a screen with no shapes, faded out.
pub fn screen(c: &mut Cpu, this: u32) -> u32 {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        c.m.w32(this + VPTR, SCREEN_VT);
        c.m.w32(this + TABLE, 0);
        r.enter_leaving(c, 0x8002_640c, 0x8002_606c, |c| initialize_shapes(c, this, 0));
        c.m.w32(this + SWAP + TABLE, 0);
        r.enter_leaving(c, 0x8002_640c, 0x8002_6080, |c| initialize_shapes(c, this + SWAP, 0));
        c.m.w16(this + FADE, 0);
        this
    })
}

pub fn abi_screen(c: &mut Cpu) {
    c.r[2] = screen(c, c.r[4]);
}

/// `tScreen::~tScreen` (FESCREEN.CPP, 0x80026098): deleted when asked.
pub fn abi_screen_destructor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        c.m.w32(this + VPTR, SCREEN_VT);
        if flags & 1 != 0 {
            Saved::of(c).call_leaving(c, DELETE, &[this, 1], 0x8002_60bc);
        }
    })
}

/// `tScreen::Initialize` (FESCREEN.CPP, 0x800260cc): the loading text; the screen's own
/// preparations; its shape files waited for (the sound kept going), each uploaded as soon as
/// it is read; the CLUTs' places kept; the display put back.
pub fn abi_initialize(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 56, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        r.enter_leaving(c, 0x8002_5ad8, 0x8002_60e4, abi_display_loading_text);
        let (obj, f) = method(&mut c.m, this, VPTR, PRE_LOAD);
        r.call_ptr_leaving(c, f, &[obj], 0x8002_6100);
        let (obj, f) = method(&mut c.m, this, VPTR, GET_SHAPE_INFO);
        r.call_ptr_leaving(c, f, &[obj, sp + 24, sp + 26, sp + 28, sp + 32], 0x8002_612c);
        loop {
            r.call_leaving(c, SYSTEM_TASK, &[0], 0x8002_6134);
            let loaded = r.enter_leaving(c, 0x8002_5e2c, 0x8002_6140, |c| is_shape_file_loaded(c, this, this));
            r.0[0] = loaded;
            if c.m.r32(this + DATA) != 0 {
                let n = c.m.r16(sp + 24) as u32;
                r.enter_leaving(c, 0x8002_5fa4, 0x8002_615c, |c| {
                    c.r[4] = this;
                    c.r[5] = sx(n);
                    abi_upload_permanent_shapes(c)
                });
            }
            let mut both = 0;
            if r.0[0] != 0 {
                both = (r.enter_leaving(c, 0x8002_5e2c, 0x8002_6170, |c| is_shape_file_loaded(c, this, this + SWAP)) != 0) as u32;
            }
            r.0[0] = both;
            if c.m.r32(this + SWAP + DATA) != 0 {
                let n = c.m.r16(sp + 26) as u32;
                r.enter_leaving(c, 0x8002_5ff0, 0x8002_6190, |c| {
                    c.r[4] = this;
                    c.r[5] = sx(n);
                    abi_upload_swap_shapes(c)
                });
            }
            if both != 0 {
                break;
            }
        }
        let m = &mut c.m;
        let (permanent, swap) = (m.r16(sp + 24), m.r16(sp + 26));
        m.w16(this + COUNT, permanent);
        m.w16(this + SWAP + COUNT, swap);
        r.enter_leaving(c, 0x8002_5c18, 0x8002_61ac, abi_go_non_interlaced);
    })
}

/// `tScreen::Cleanup` (FESCREEN.CPP, 0x800261c0): both reads cancelled, both files' shapes
/// freed.
pub fn abi_cleanup(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.enter_leaving(c, 0x8002_5db8, 0x8002_61d8, |c| {
            c.r[4] = this;
            c.r[5] = 0;
            abi_async_load_permanent_shape_file(c)
        });
        r.enter_leaving(c, 0x8002_5dd8, 0x8002_61e4, |c| {
            c.r[4] = this;
            c.r[5] = 0;
            abi_async_load_swap_shape_file(c)
        });
        r.enter_leaving(c, 0x8002_64b4, 0x8002_61f0, |c| free_shapes(c, this, this));
        r.enter_leaving(c, 0x8002_64b4, 0x8002_61fc, |c| free_shapes(c, this, this + SWAP));
    })
}

/// `tScreen::Draw` (FESCREEN.CPP, 0x8002620c): the background, or the foreground.
pub fn abi_draw(c: &mut Cpu) {
    let (this, background) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        let slot = if background != 0 { DRAW_BACKGROUND } else { DRAW_FOREGROUND };
        let (obj, f) = method(&mut c.m, this, VPTR, slot);
        Saved::of(c).call_ptr_leaving(c, f, &[obj], 0x8002_6250);
    })
}

/// `tScreen::AsyncLoadShapeFile` (FESCREEN.CPP, 0x80026260): a shape file's read started (into
/// its buffer, or memory reserved for it), its name kept; the read before cancelled, what was
/// read freed, not uploaded; with no name, the read just cancelled (or, when it is still
/// reading, noted to be freed when it is done).
pub fn async_load_shape_file(c: &mut Cpu, this: u32, name: u32, info: u32) {
    abi::frame(c, 160, |c, sp| {
        let mut r = Saved::of(c);
        r.0[..3].copy_from_slice(&[info, name, this]);
        if name == 0 {
            let handle = c.m.r32(info + HANDLE);
            if handle != 0 {
                r.call_leaving(c, CANCEL_READ, &[handle], 0x8002_629c);
                c.m.w32(info + HANDLE, 0);
            }
        }
        r.enter_leaving(c, 0x8002_6374, 0x8002_62ac, |c| cancel_async_load(c, info));
        let data = c.m.r32(info + DATA);
        if data != 0 {
            if c.m.r32(info + BUFFER) == 0 {
                r.call_leaving(c, PURGE, &[data], 0x8002_62d4);
            }
            c.m.w32(info + DATA, 0);
        }
        let flags = c.m.r16(info + UPLOADED) & 0xFFFE;
        c.m.w16(info + UPLOADED, flags);
        if name == 0 {
            return;
        }
        r.call_leaving(c, SPRINTF, &[info + NAME, NAME_FORMAT, name], 0x8002_6300);
        let path = c.m.r32(SHAPES_PATH);
        let file = sp + 16;
        r.call_leaving(c, SPRINTF, &[file, PATH_FORMAT, path, name], 0x8002_631c);
        if c.m.r32(info + CANCELLED) != 0 {
            return;
        }
        let buffer = c.m.r32(info + BUFFER);
        c.m.w32(info + CANCELLED, 0);
        let handle = if buffer != 0 {
            r.call_leaving(c, READ_FILE_AT, &[file, buffer], 0x8002_6344)
        } else {
            r.call_leaving(c, READ_FILE, &[file, 16], 0x8002_6358)
        };
        c.m.w32(info + HANDLE, handle);
    })
}

pub fn abi_async_load_shape_file(c: &mut Cpu) {
    async_load_shape_file(c, c.r[4], c.r[5], c.r[6]);
}

/// `tScreen::CancelAsyncLoad` (FESCREEN.CPP, 0x80026374): a shape file's read cancelled: one
/// still reading noted to be freed when done; one done freed (unless read into its buffer).
pub fn cancel_async_load(c: &mut Cpu, info: u32) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = info;
        let handle = c.m.r32(info + HANDLE);
        if handle == 0 {
            return;
        }
        if r.call_leaving(c, READ_STATUS, &[handle], 0x8002_639c) == 0 {
            c.m.w32(info + CANCELLED, 1);
            return;
        }
        if c.m.r32(info + BUFFER) == 0 {
            let handle = c.m.r32(info + HANDLE);
            let data = r.call_leaving(c, READ_ADDRESS, &[handle], 0x8002_63c8);
            c.m.w32(info + DATA, data);
        }
        let data = c.m.r32(info + DATA);
        if data != 0 {
            if c.m.r32(info + BUFFER) == 0 {
                r.call_leaving(c, PURGE, &[data], 0x8002_63f4);
            }
            c.m.w32(info + DATA, 0);
        }
        c.m.w32(info + HANDLE, 0);
    })
}

pub fn abi_cancel_async_load(c: &mut Cpu) {
    cancel_async_load(c, c.r[5]);
}

/// `tScreen::InitializeShapes` (FESCREEN.CPP, 0x8002640c): room for `n` shapes (their table
/// reserved, their CLUTs none), the table before freed; no file, nothing read.
pub fn initialize_shapes(c: &mut Cpu, info: u32, n: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = info;
        r.0[1] = n;
        let table = c.m.r32(info + TABLE);
        if table != 0 {
            r.call_leaving(c, PURGE, &[table], 0x8002_6438);
            c.m.w32(info + TABLE, 0);
        }
        let m = &mut c.m;
        m.w16(info + UPLOADED, 0);
        m.w16(info + COUNT, n as u16);
        for f in [HANDLE, DATA, BUFFER, CANCELLED] {
            m.w32(info + f, 0);
        }
        if n == 0 {
            return;
        }
        let table = r.call_leaving(c, RESERVE, &[SHAPES_NAME, n << 5, 0], 0x8002_646c);
        c.m.w32(info + TABLE, table);
        let mut k = 0u32;
        loop {
            let at = c.m.r32(info + TABLE).wrapping_add((k & 0xFFFF) << 5);
            c.m.w32(at + 4, 0);
            k = k.wrapping_add(1);
            if (k & 0xFFFF) >= n {
                break;
            }
        }
    })
}

pub fn abi_initialize_shapes(c: &mut Cpu) {
    initialize_shapes(c, c.r[5], c.r[6]);
}

/// `tScreen::FreeShapes` (FESCREEN.CPP, 0x800264b4): a shape file's read cancelled (one still
/// reading waited for, the sound kept going), what it read freed, its shapes' CLUTs and table.
pub fn free_shapes(c: &mut Cpu, this: u32, info: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = info;
        r.enter_leaving(c, 0x8002_6374, 0x8002_64d0, |c| {
            c.r[4] = this;
            cancel_async_load(c, info)
        });
        if c.m.r32(info + HANDLE) != 0 {
            r.0[2] = u32::MAX;
            r.0[1] = (-2i32) as u32;
            loop {
                let handle = c.m.r32(info + HANDLE);
                let status = r.call_leaving(c, READ_STATUS, &[handle], 0x8002_64f4) as i32;
                if status > 0 || status == -1 {
                    if c.m.r32(info + BUFFER) == 0 {
                        let handle = c.m.r32(info + HANDLE);
                        let data = r.call_leaving(c, READ_ADDRESS, &[handle], 0x8002_6520);
                        c.m.w32(info + DATA, data);
                    } else {
                        c.m.w32(info + DATA, 0);
                    }
                    c.m.w32(info + HANDLE, 0);
                } else if status == -2 {
                    c.m.w32(info + HANDLE, 0);
                }
                r.call_leaving(c, SYSTEM_TASK, &[0], 0x8002_6544);
                if c.m.r32(info + HANDLE) == 0 {
                    break;
                }
            }
        }
        let data = c.m.r32(info + DATA);
        if data != 0 {
            if c.m.r32(info + BUFFER) == 0 {
                r.call_leaving(c, PURGE, &[data], 0x8002_657c);
            }
            c.m.w32(info + DATA, 0);
        }
        if c.m.r32(info + TABLE) == 0 {
            return;
        }
        r.0[1] = 0;
        if c.m.r16(info + COUNT) != 0 {
            loop {
                let at = c.m.r32(info + TABLE).wrapping_add(sx(r.0[1]) << 5);
                if c.m.r32(at + 4) != 0 {
                    let clut = lh(&mut c.m, at + 4);
                    r.call_leaving(c, RELEASE_CLUT, &[clut], 0x8002_65cc);
                }
                r.0[1] = r.0[1].wrapping_add(1);
                if (sx(r.0[1]) as i32) >= c.m.r16(info + COUNT) as i32 {
                    break;
                }
            }
        }
        let table = c.m.r32(info + TABLE);
        r.call_leaving(c, PURGE, &[table], 0x8002_65f8);
        c.m.w32(info + TABLE, 0);
    })
}

pub fn abi_free_shapes(c: &mut Cpu) {
    free_shapes(c, c.r[4], c.r[5]);
}

/// `tScreen::UploadShapes` (FESCREEN.CPP, 0x80026614): `n` shapes of a file read (all, for 0)
/// uploaded to VRAM, the table's from `first` on (their CLUTs freed first), at the loader's
/// offsets `b`, `a` (`FETexture_LoadPmxAtOffset`'s); noted as uploaded, the file freed (unless
/// read into its buffer).
pub fn upload_shapes(c: &mut Cpu, _this: u32, info: u32, a: u32, b: u32, n: u32, first: u32) {
    abi::frame(c, 64, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = info;
        r.0[8] = a;
        r.0[7] = b;
        r.0[3] = n;
        r.0[6] = first;
        if c.m.r32(info + DATA) == 0 {
            return;
        }
        if n << 16 == 0 {
            r.0[3] = c.m.r16(info + COUNT) as u32;
        }
        let count = sx(r.0[3]);
        r.0[0] = 0;
        if count as i32 > 0 {
            r.0[5] = sx(first);
            r.0[4] = count;
            while (r.0[0] as i32) < (count as i32) {
                let at = r.0[0].wrapping_add(r.0[5]) << 5;
                r.0[1] = at;
                let shape = c.m.r32(info + TABLE).wrapping_add(at);
                if c.m.r32(shape + 4) != 0 {
                    let clut = lh(&mut c.m, shape + 4);
                    r.call_leaving(c, RELEASE_CLUT, &[clut], 0x8002_66c0);
                    let shape = c.m.r32(info + TABLE).wrapping_add(at);
                    c.m.w32(shape + 4, 0);
                }
                r.0[0] = r.0[0].wrapping_add(1);
            }
        }
        let flags = c.m.r16(info + UPLOADED) & 0xFFFE;
        c.m.w16(info + UPLOADED, flags);
        let count = sx(r.0[3]);
        r.0[0] = 0;
        if count as i32 > 0 {
            r.0[5] = sx(first);
            r.0[4] = b << 16;
            r.0[3] = sx(a);
            r.0[1] = count;
            loop {
                let k = r.0[0];
                let at = k.wrapping_add(r.0[5]) << 5;
                r.0[0] = k.wrapping_add(1);
                let (data, table) = (c.m.r32(info + DATA), c.m.r32(info + TABLE));
                let args = [data, k, table.wrapping_add(at), ((r.0[4] as i32) >> 16) as u32, r.0[3]];
                r.call_leaving(c, LOAD_PMX, &args, 0x8002_6740);
                if (r.0[0] as i32) >= (count as i32) {
                    break;
                }
            }
        }
        let flags = c.m.r16(info + UPLOADED) | 1;
        c.m.w16(info + UPLOADED, flags);
        if c.m.r32(info + BUFFER) == 0 {
            let data = c.m.r32(info + DATA);
            r.call_leaving(c, PURGE, &[data], 0x8002_676c);
        }
        c.m.w32(info + DATA, 0);
    })
}

pub fn abi_upload_shapes(c: &mut Cpu) {
    let (n, first) = (abi::arg(c, 4), abi::arg(c, 5));
    upload_shapes(c, c.r[4], c.r[5], c.r[6], c.r[7], n, first);
}

/// `tScreen::PreLoad` (FESCREEN.CPP, 0x800267a0): the screen's shape files (its
/// `GetShapeInfo`'s) started reading, room made for their shapes.
pub fn abi_pre_load(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let (obj, f) = method(&mut c.m, this, VPTR, GET_SHAPE_INFO);
        r.call_ptr_leaving(c, f, &[obj, sp + 24, sp + 26, sp + 28, sp + 32], 0x8002_67dc);
        let n = lh(&mut c.m, sp + 24);
        r.enter_leaving(c, 0x8002_640c, 0x8002_67ec, |c| initialize_shapes(c, this, n));
        let n = lh(&mut c.m, sp + 26);
        r.enter_leaving(c, 0x8002_640c, 0x8002_67fc, |c| initialize_shapes(c, this + SWAP, n));
        let name = c.m.r32(sp + 28);
        r.enter_leaving(c, 0x8002_5db8, 0x8002_6808, |c| {
            c.r[4] = this;
            c.r[5] = name;
            abi_async_load_permanent_shape_file(c)
        });
        let name = c.m.r32(sp + 32);
        r.enter_leaving(c, 0x8002_5dd8, 0x8002_6814, |c| {
            c.r[4] = this;
            c.r[5] = name;
            abi_async_load_swap_shape_file(c)
        });
    })
}

/// `tScreen::TransitionOff` (FESCREEN.CPP, 0x80026824): fading out from now.
pub fn abi_transition_off(c: &mut Cpu) {
    let (m, this) = (&mut c.m, c.r[4]);
    let ticks = m.r32(GAME_TICKS);
    m.w16(this + FADE, 0);
    m.w32(this + TRANSITION, 0);
    m.w32(this + GOING_OFF, 1);
    m.w32(this + TRANSITION_AT, ticks);
}

/// `tScreen::TransitionOn` (FESCREEN.CPP, 0x80026844): fading in from now.
pub fn abi_transition_on(c: &mut Cpu) {
    let (m, this) = (&mut c.m, c.r[4]);
    m.w16(this + FADE, 128);
    let ticks = m.r32(GAME_TICKS);
    m.w32(this + TRANSITION, 128);
    m.w32(this + GOING_OFF, 0);
    m.w32(this + TRANSITION_AT, ticks);
}

/// `tScreen::UpdateTransition` (FESCREEN.CPP, 0x80026868): the transition 12 on (going off) or
/// back, within -50..150; the fade it, within 0..128.
pub fn abi_update_transition(c: &mut Cpu) {
    let (m, this) = (&mut c.m, c.r[4]);
    let step = if m.r32(this + GOING_OFF) != 0 { 12 } else { -12 };
    let v = (m.r32(this + TRANSITION) as i32).wrapping_add(step);
    m.w32(this + TRANSITION, v as u32);
    let v = v.clamp(-50, 150);
    m.w32(this + TRANSITION, v as u32);
    let fade = m.r16(this + TRANSITION);
    m.w16(this + FADE, fade);
    m.w16(this + FADE, (fade as i16).clamp(0, 128) as u16);
}

/// `tScreen::TransitionIsFinished` (FESCREEN.CPP, 0x800268e0): once the transition has
/// overshot either way.
pub fn abi_transition_is_finished(c: &mut Cpu) {
    let v = c.m.r32(c.r[4] + TRANSITION).wrapping_add(24);
    c.r[2] = (v >= 167) as u32;
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
    use crate::femenu::tests::{self as base, ADDR, COMMAND, Case, KEY, MENU, NEW, TOP, pick, player, via_abi};
    use crate::globals::FE_APP;
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

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

    #[test]
    fn fescreen_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 100, 0x5D1 + k as u64, shuffle, &args);
        }
    }
}
