//! TEXTURE.CPP: textures into VRAM. A shape file's images and their palettes are put in VRAM
//! (`Texture_Vramf`: each block of a shape's chain, an image at the texture's place, a palette
//! at its CLUT's), and a pixmap (`Draw_tPixMap`: the four corners' uv, the CLUT, the texture
//! page, the depth cue's palettes, the semi-transparency flag) made for drawing it
//! (`Texture_LoadPmx`). CLUT places come from free lists (16-colour and 256-colour rows of VRAM,
//! the race's and the menus'); identical palettes are shared; car bodies are recoloured.
//!
//! Written readable from the start (step 3 of DECOMP.md's plan): the game's data as the
//! original leaves it, the stack and registers free. All of it runs while the game loads, so it
//! ticks as the original's loops do (see `call::ticks`).

use crate::call::{call_jal, ticks, with_buffer};
use crate::globals::PATHS;
use ps1rt::{Cpu, Mem};

/// The shared palettes: how many 16- and 256-colour ones (`Texture_gNum4bitPal`, `...8bit`), the
/// pixmaps made from them (`Texture_gP4bitPmx`, `...8bit`), the palettes (`Texture_gPalette4bit`,
/// `...8bit`: pointers to their colours).
const NUM_4BIT: u32 = 0x8013_EDF4;
const NUM_8BIT: u32 = 0x8013_EDF8;
const PMX_4BIT: u32 = 0x8013_EDFC;
const PMX_8BIT: u32 = 0x8013_EE00;
const PAL_4BIT: u32 = 0x8013_EE04;
const PAL_8BIT: u32 = 0x8013_EE08;
/// The free CLUT places (`gFreePal4`, `gFreePal8`: ids, `gNbFreePal4`, `...8` of them).
const FREE_4: u32 = 0x8013_EE0C;
const NB_FREE_4: u32 = 0x8013_EE10;
const FREE_8: u32 = 0x8013_EE14;
const NB_FREE_8: u32 = 0x8013_EE18;
/// The car's colour (`Texture_CarColor`: which of a body's palettes), the palettes kept for
/// recolouring (`Texture_palCopy`: 520 bytes each, x and y words then 256 colours;
/// `Texture_palNum` of them).
const CAR_COLOR: u32 = 0x8013_EDE8;
const PAL_COPY: u32 = 0x8013_EDEC;
const PAL_NUM: u32 = 0x8013_EDF0;
const PAL_COPY_SIZE: u32 = 520;
/// The menus' pixmaps (`gMenuPixmap`, 8 pointers, into `gMenuPixmapAlloc`'s 16-byte pixmaps).
const MENU_PIXMAP: u32 = 0x8012_22C0;
const MENU_PIXMAP_ALLOC: u32 = 0x8012_22E0;
const MENU_PIXMAPS: u32 = 8;
const PIXMAP_SIZE: u32 = 16;

/// A shape's blocks: the type byte (& 0xF7: 0x23 a palette, 0x40..0x43 an image; bit 3 set
/// once in VRAM; 0x6B a translucency record), the next block's offset in the top 24 bits of the
/// first word (0 the last), the width (+4) and height (+6), the VRAM place word (+12: x in bits
/// 0..11, y in 16..27, bit 14 the texture flipped), the data (+16).
const TYPE_IN_VRAM: u8 = 8;
const TYPE_PALETTE: u8 = 0x23;
const TYPE_IMAGE_FIRST: u8 = 0x40;
const TYPE_IMAGE_END: u8 = 0x44;
const TYPE_TRANSLUCENCY: u8 = 0x6B;
const PLACE_FLIPPED: u32 = 0x4000;

/// `Texture_LoadPmx`'s flags: the shape's size as it is (else a texel less each way), the
/// semi-transparency checked, a car body (16 colours, recoloured: kept, painted), no palette
/// upload, palettes shared.
const LOAD_FULL_SIZE: u32 = 0x1;
const LOAD_TRANS_CHECK: u32 = 0x2;
const LOAD_CAR: u32 = 0x8;
const LOAD_PAINT: u32 = 0x10;
const LOAD_NO_PALETTE: u32 = 0x20;
const LOAD_SHARE: u32 = 0x40;
/// `Texture_LoadPmx`'s CLUT x: a free place, the depth cue's palettes, the placeholder's (and a
/// shape placed at x -1: 1x1 at (0, 160)).
const CLUT_FREE: i32 = -1;
const CLUT_DEPTH: i32 = -3;
const CLUT_PLACEHOLDER: i32 = -2;

/// Functions called: the library's `reservememadr(name, size, flags)`, `purgememadr`, `LoadImage`
/// (the host's), `shapedepth`, `locateshapez(file, name)`, `GetClut(x, y)`, `GetTPage(depth,
/// mode, x, y)`, `sprintf`, `loadfileadr(name, 0)`; `Platform_GetDCTBuffer(size, name)`, the
/// depth cue's `TextureProcess_DepthColorCluts`, `TransColorCheck`, `Init`.
const RESERVE: u32 = 0x800E_5F28;
const PURGE: u32 = 0x800E_612C;
const LOAD_IMAGE: u32 = 0x800E_E3EC;
const SHAPE_DEPTH: u32 = 0x800F_4D40;
const LOCATE_SHAPE: u32 = 0x800E_BACC;
const GET_CLUT: u32 = 0x800F_4DAC;
const GET_TPAGE: u32 = 0x800F_2930;
const SPRINTF: u32 = 0x800E_50A8;
const LOAD_FILE: u32 = 0x800E_63D4;
const GET_DCT_BUFFER: u32 = 0x800D_D0C0;
const DEPTH_COLOR_CLUTS: u32 = 0x800E_16CC;
const TRANS_COLOR_CHECK: u32 = 0x800E_1490;
const TEXTURE_PROCESS_INIT: u32 = 0x800E_1D40;
/// The blocks' and files' names.
const PSHARE4_NAME: u32 = 0x8005_71C4;
const PSHARE8_NAME: u32 = 0x8005_71D0;
const PAL4_NAME: u32 = 0x8005_71DC;
const PAL8_NAME: u32 = 0x8005_71E8;
const CLUTS4_NAME: u32 = 0x8005_71F4;
const CLUTS8_NAME: u32 = 0x8005_7200;
const MENU_CLUTS4_NAME: u32 = 0x8005_720C;
const MENU_CLUTS8_NAME: u32 = 0x8005_721C;
const SHOW_PSH: u32 = 0x8005_722C;
const SHAPES_PATH: u32 = PATHS + 0x64;
/// The menus' shapes in "show.psh": "show", "shad", "lgt3", "lgta", "lgtb", "lgtc", the
/// menu pixmap each goes to.
const MENU_SHAPES: [(u32, u32); 6] = [(0x8013_EDB8, 0), (0x8013_EDC0, 1), (0x8013_EDC8, 3), (0x8013_EDD0, 4), (0x8013_EDD8, 5), (0x8013_EDE0, 6)];

/// A CLUT id from its place: y in the top ten bits, x / 16 in the low six.
fn clut_id(x16: u32, y: u32) -> u16 {
    ((y << 6) | (x16 & 0x3F)) as u16
}

/// A block's next in the shape's chain (0 the last).
fn next_block(m: &mut Mem, block: u32) -> u32 {
    let w = m.r32(block);
    if w & 0xFFFF_FF00 != 0 { block.wrapping_add(((w as i32) >> 8) as u32) } else { 0 }
}

/// `Texture_InitPaletteSharing` (TEXTURE.CPP, 0x800e01c0): no shared palettes yet, their blocks
/// reserved.
pub fn init_palette_sharing(c: &mut Cpu) {
    c.m.w32(NUM_4BIT, 0);
    c.m.w32(NUM_8BIT, 0);
    for (at, name, size) in [(PMX_4BIT, PSHARE4_NAME, 2048), (PMX_8BIT, PSHARE8_NAME, 256), (PAL_4BIT, PAL4_NAME, 2048), (PAL_8BIT, PAL8_NAME, 256)] {
        let block = call_jal(c, RESERVE, &[name, size, 16]);
        c.m.w32(at, block);
    }
}

pub fn abi_init_palette_sharing(c: &mut Cpu) {
    init_palette_sharing(c);
}

/// `Texture_DeInitPaletteSharing` (TEXTURE.CPP, 0x800e023c): the blocks freed.
pub fn de_init_palette_sharing(c: &mut Cpu) {
    for at in [PMX_4BIT, PMX_8BIT, PAL_4BIT, PAL_8BIT] {
        let block = c.m.r32(at);
        call_jal(c, PURGE, &[block]);
    }
}

pub fn abi_de_init_palette_sharing(c: &mut Cpu) {
    de_init_palette_sharing(c);
}

/// `Texture_ResetPaletteSharing` (TEXTURE.CPP, 0x800e0284): no shared palettes.
pub fn reset_palette_sharing(m: &mut Mem) {
    m.w32(NUM_4BIT, 0);
    m.w32(NUM_8BIT, 0);
}

pub fn abi_reset_palette_sharing(c: &mut Cpu) {
    reset_palette_sharing(&mut c.m);
}

/// `Texture_CheckForSharedPalette` (TEXTURE.CPP, 0x800e0294): with sharing on, whether the
/// palette at `colours` (256 colours if `big`, else 16) is one already shared; then its pixmap
/// copied into `pmx`.
pub fn check_for_shared_palette(c: &mut Cpu, on: bool, colours: u32, pmx: u32, big: bool) -> bool {
    if !on {
        return false;
    }
    let (count, pals, pmxs, words) = if big { (NUM_8BIT, PAL_8BIT, PMX_8BIT, 128) } else { (NUM_4BIT, PAL_4BIT, PMX_4BIT, 8) };
    let n = c.m.r32(count) as i32;
    let table = c.m.r32(pals);
    for k in 0..n.max(0) as u32 {
        let shared = c.m.r32(table.wrapping_add(4 * k));
        let mut same = 0;
        while same < words {
            if c.m.r32(colours.wrapping_add(4 * same)) != c.m.r32(shared.wrapping_add(4 * same)) {
                break;
            }
            same += 1;
        }
        // (the comparison's loop ticks at each match but the last; the palettes' at each miss)
        ticks(c, same.min(words - 1));
        if same == words {
            // (the match branches back to the copy: once for 256 colours, twice for 16)
            ticks(c, if big { 1 } else { 2 });
            let list = c.m.r32(pmxs);
            let from = c.m.r32(list.wrapping_add(4 * k));
            for b in 0..PIXMAP_SIZE {
                let v = c.m.r8(from.wrapping_add(b));
                c.m.w8(pmx.wrapping_add(b), v);
            }
            return true;
        }
        c.tick();
    }
    false
}

pub fn abi_check_for_shared_palette(c: &mut Cpu) {
    c.r[2] = check_for_shared_palette(c, c.r[4] != 0, c.r[5], c.r[6], c.r[7] != 0) as u32;
}

/// `Texture_AddSharedPalette` (TEXTURE.CPP, 0x800e03c8): the palette at `colours` (16 colours
/// for kind 0, 256 for 1) shared, with its pixmap `pmx`.
pub fn add_shared_palette(m: &mut Mem, colours: u32, pmx: u32, kind: u32) {
    let (count, pals, pmxs) = match kind {
        0 => (NUM_4BIT, PAL_4BIT, PMX_4BIT),
        1 => (NUM_8BIT, PAL_8BIT, PMX_8BIT),
        _ => return,
    };
    let n = m.r32(count);
    let (p, x) = (m.r32(pals), m.r32(pmxs));
    m.w32(p.wrapping_add(n << 2), colours);
    m.w32(x.wrapping_add(n << 2), pmx);
    m.w32(count, n.wrapping_add(1));
}

pub fn abi_add_shared_palette(c: &mut Cpu) {
    add_shared_palette(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `count` CLUT places pushed on a free list (`free`'s ids, `nb` of them), rows by rows: each
/// row a column (x / 16) from `x16`, `per_row` places down from `y`.
fn push_cluts(c: &mut Cpu, free: u32, nb: u32, rows: u32, x16: u32, y: u32, per_row: u32) {
    let list = c.m.r32(free);
    for row in 0..rows {
        for k in 0..per_row {
            let n = c.m.r32(nb);
            c.m.w16(list.wrapping_add(2 * n), clut_id(x16 + row, y + k));
            c.m.w32(nb, n.wrapping_add(1));
        }
        ticks(c, per_row - 1);
    }
    ticks(c, rows - 1);
}

/// `Texture_InitClut` (TEXTURE.CPP, 0x800e0440): the race's free CLUT places: 16-colour ones in
/// the 16 columns at x 0..255, rows 0..119, and the 8 columns at x 0..127, rows 128..159;
/// 256-colour ones at x 0, rows 120..127.
pub fn init_clut(c: &mut Cpu) {
    c.m.w32(NB_FREE_4, 0);
    push_cluts(c, FREE_4, NB_FREE_4, 16, 0, 0, 120);
    push_cluts(c, FREE_4, NB_FREE_4, 8, 0, 128, 32);
    c.m.w32(NB_FREE_8, 0);
    push_cluts(c, FREE_8, NB_FREE_8, 1, 0, 120, 8);
}

pub fn abi_init_clut(c: &mut Cpu) {
    init_clut(c);
}

/// `Texture_GetClutId` (TEXTURE.CPP, 0x800e0544): a free CLUT place (256-colour if `big`, else
/// 16) taken from its list's end into `x` and `y` (none left: 0, 0).
pub fn get_clut_id(m: &mut Mem, big: bool, x: u32, y: u32) {
    let (free, nb) = if big { (FREE_8, NB_FREE_8) } else { (FREE_4, NB_FREE_4) };
    let n = m.r32(nb);
    let id = if n == 0 {
        0
    } else {
        let list = m.r32(free);
        m.w32(nb, n.wrapping_sub(1));
        m.r16(list.wrapping_add(2 * n.wrapping_sub(1)))
    };
    m.w32(x, ((id & 0x3F) as u32) << 4);
    m.w32(y, ((id as i16) >> 6) as i32 as u32);
}

pub fn abi_get_clut_id(c: &mut Cpu) {
    get_clut_id(&mut c.m, c.r[4] != 0, c.r[5], c.r[6]);
}

/// `Texture_MenuReleaseClutId` (TEXTURE.CPP, 0x800e05b0): CLUT `id` back on a free list: rows
/// below 488 the 256-colour list, the rest the 16-colour.
pub fn menu_release_clut_id(m: &mut Mem, id: u32) {
    let (free, nb) = if ((id as i16) >> 6) < 488 { (FREE_8, NB_FREE_8) } else { (FREE_4, NB_FREE_4) };
    let n = m.r32(nb);
    let list = m.r32(free);
    m.w16(list.wrapping_add(n << 1), id as u16);
    m.w32(nb, n.wrapping_add(1));
}

pub fn abi_menu_release_clut_id(c: &mut Cpu) {
    menu_release_clut_id(&mut c.m, c.r[4]);
}

/// `Texture_ColorCarPalette` (TEXTURE.CPP, 0x800e0610): `n` colours of the car's colour's
/// palette (the car colour'th 16 from `src`) copied to `dst`.
pub fn color_car_palette(c: &mut Cpu, src: u32, dst: u32, n: u32) {
    let src = src.wrapping_add(c.m.r32(CAR_COLOR) << 5);
    for k in 0..n {
        let v = c.m.r16(src.wrapping_add(2 * k));
        c.m.w16(dst.wrapping_add(2 * k), v);
    }
    ticks(c, n.saturating_sub(1));
}

pub fn abi_color_car_palette(c: &mut Cpu) {
    color_car_palette(c, c.r[4], c.r[5], c.r[6]);
}

/// `Texture_CopyPalette` (TEXTURE.CPP, 0x800e0648): `n` colours at `src` kept for recolouring,
/// with their place (x, y).
pub fn copy_palette(c: &mut Cpu, src: u32, n: i32, x: u32, y: u32) {
    let m = &mut c.m;
    let entry = m.r32(PAL_COPY).wrapping_add(m.r32(PAL_NUM).wrapping_mul(PAL_COPY_SIZE));
    m.w32(entry, x);
    m.w32(entry + 4, y);
    for k in 0..n.max(0) as u32 {
        let v = m.r16(src.wrapping_add(2 * k));
        m.w16(entry + 8 + 2 * k, v);
    }
    ticks(c, (n.max(1) - 1) as u32);
    let k = c.m.r32(PAL_NUM);
    c.m.w32(PAL_NUM, k.wrapping_add(1));
}

pub fn abi_copy_palette(c: &mut Cpu) {
    copy_palette(c, c.r[4], c.r[5] as i32, c.r[6], c.r[7]);
}

/// `Texture_ProcessPaletteCopy` (TEXTURE.CPP, 0x800e06a8): the kept palettes `from` to `to` of
/// `table` recoloured for the car's colour and put back in VRAM.
pub fn process_palette_copy(c: &mut Cpu, table: u32, from: i32, to: i32) {
    if table == 0 {
        return;
    }
    with_buffer(c, 8 + 32, |c, rect| {
        let colours = rect + 8;
        c.m.w16(rect + 4, 16);
        c.m.w16(rect + 6, 1);
        let mut k = from;
        while k < to {
            let entry = table.wrapping_add((k as u32).wrapping_mul(PAL_COPY_SIZE));
            color_car_palette(c, entry + 8, colours, 16);
            let (x, y) = (c.m.r16(entry), c.m.r16(entry + 4));
            c.m.w16(rect, x);
            c.m.w16(rect + 2, y);
            call_jal(c, LOAD_IMAGE, &[rect, colours]);
            k += 1;
            if k < to {
                c.tick();
            }
        }
    });
}

pub fn abi_process_palette_copy(c: &mut Cpu) {
    process_palette_copy(c, c.r[4], c.r[5] as i32, c.r[6] as i32);
}

/// `Texture_LoadImage` (TEXTURE.CPP, 0x800e0754): `data` into VRAM at `rect` (an odd width's
/// height made odd).
pub fn load_image(c: &mut Cpu, rect: u32, data: u32) {
    if c.m.r16(rect.wrapping_add(4)) & 1 != 0 {
        let h = c.m.r16(rect.wrapping_add(6)) | 1;
        c.m.w16(rect.wrapping_add(6), h);
    }
    call_jal(c, LOAD_IMAGE, &[rect, data]);
}

pub fn abi_load_image(c: &mut Cpu) {
    load_image(c, c.r[4], c.r[5]);
}

/// A block's VRAM place word set (x in bits 0..11, y in 16..27) and the block marked in VRAM.
fn place_block(m: &mut Mem, block: u32, x: u32, y16: u32) {
    let w = m.r32(block + 12);
    let t = m.r8(block);
    m.w32(block + 12, (((w & 0xFFFF_F000) | (x & 0xFFF)) & 0xF000_FFFF) | y16);
    m.w8(block, t | TYPE_IN_VRAM);
}

/// `Texture_Vramf` (TEXTURE.CPP, 0x800e0798): each block of `shape`'s chain into VRAM: images at
/// (x, y) (as wide as their depth makes them in 16-bit units), palettes at (clut_x, clut_y) (none
/// when clut_x is negative).
pub fn vramf(c: &mut Cpu, shape: u32, x: i32, y: i32, clut_x: i32, clut_y: i32) {
    let (y16, clut_y16) = (((y as u32) & 0xFFF) << 16, ((clut_y as u32) & 0xFFF) << 16);
    let mut block = shape;
    while block != 0 {
        let kind = c.m.r8(block) & !TYPE_IN_VRAM;
        if kind == TYPE_PALETTE {
            if clut_x >= 0 {
                place_block(&mut c.m, block, clut_x as u32, clut_y16);
                with_buffer(c, 8, |c, rect| {
                    let m = &mut c.m;
                    let w = m.r16(block + 4);
                    m.w16(rect, clut_x as u16);
                    m.w16(rect + 2, clut_y as u16);
                    m.w16(rect + 6, 1);
                    m.w16(rect + 4, w);
                    load_image(c, rect, block + 16);
                });
            }
        } else if (TYPE_IMAGE_FIRST..TYPE_IMAGE_END).contains(&kind) {
            place_block(&mut c.m, block, x as u32, y16);
            with_buffer(c, 8, |c, rect| {
                c.m.w16(rect, x as u16);
                c.m.w16(rect + 2, y as u16);
                let depth = call_jal(c, SHAPE_DEPTH, &[block]) as i32;
                let m = &mut c.m;
                let units = (m.r16(block + 4) as i16 as i32).wrapping_mul(depth).wrapping_add(15) / 16;
                m.w16(rect + 4, units as u16);
                let h = m.r16(block + 6);
                m.w16(rect + 6, h);
                load_image(c, rect, block + 16);
            });
        }
        block = next_block(&mut c.m, block);
        // (the chain's loop: a tick a block but the last)
        if block != 0 {
            c.tick();
        }
    }
}

pub fn abi_vramf(c: &mut Cpu) {
    let clut_y = c.m.r32(c.r[29] + 16);
    vramf(c, c.r[4], c.r[5] as i32, c.r[6] as i32, c.r[7] as i32, clut_y as i32);
}

/// `Texture_Vramcf` (TEXTURE.CPP, 0x800e0944): [`vramf`], but for an image whose rows are an odd
/// number of 16-byte units and of even height: its last row sent first on its own (the
/// first texel two rows up, the rest a row up), then the rest one row shorter.
pub fn vramcf(c: &mut Cpu, shape: u32, x: i32, y: i32, clut_x: i32, clut_y: i32) {
    let depth = call_jal(c, SHAPE_DEPTH, &[shape]) as i32;
    let m = &mut c.m;
    let row = (m.r16(shape + 4) as i16 as i32).wrapping_mul(depth).wrapping_add(15) & !15;
    let row8 = row >> 3;
    let h = m.r16(shape + 6);
    if row8 & 2 == 0 || h & 1 != 0 {
        vramf(c, shape, x, y, clut_x, clut_y);
        return;
    }
    let h = h as i16 as i32;
    let last = shape.wrapping_add(16).wrapping_add((h.wrapping_sub(1)).wrapping_mul(row8) as u32);
    with_buffer(c, 8, |c, rect| {
        let m = &mut c.m;
        m.w16(rect + 2, y.wrapping_add(h).wrapping_sub(2) as u16);
        m.w16(rect, x as u16);
        m.w16(rect + 4, 1);
        m.w16(rect + 6, 2);
        call_jal(c, LOAD_IMAGE, &[rect, last.wrapping_sub(2)]);
        let m = &mut c.m;
        m.w16(rect, x.wrapping_add(1) as u16);
        m.w16(rect + 2, y.wrapping_add(h).wrapping_sub(1) as u16);
        m.w16(rect + 4, ((row >> 4) - 1) as u16);
        m.w16(rect + 6, 1);
        call_jal(c, LOAD_IMAGE, &[rect, last.wrapping_add(2)]);
    });
    let m = &mut c.m;
    let shorter = m.r16(shape + 6).wrapping_sub(1);
    m.w16(shape + 6, shorter);
    vramf(c, shape, x, y, clut_x, clut_y);
    c.m.w16(shape + 6, h as u16);
}

pub fn abi_vramcf(c: &mut Cpu) {
    let clut_y = c.m.r32(c.r[29] + 16);
    vramcf(c, c.r[4], c.r[5] as i32, c.r[6] as i32, c.r[7] as i32, clut_y as i32);
}

/// `Texture_GetTranslucencyMode` (TEXTURE.CPP, 0x800e0aac): the semi-transparency mode of
/// `shape`'s translucency record (bits 5..6 of its +4; mode 3 is taken as 2), or 0 without one.
pub fn get_translucency_mode(c: &mut Cpu, shape: u32) -> u32 {
    let mut block = shape;
    while block != 0 {
        if c.m.r8(block) == TYPE_TRANSLUCENCY {
            let mode = (c.m.r16(block + 4) as u32 >> 5) & 3;
            return if mode == 3 { 2 } else { mode };
        }
        block = next_block(&mut c.m, block);
        c.tick();
    }
    0
}

pub fn abi_get_translucency_mode(c: &mut Cpu) {
    c.r[2] = get_translucency_mode(c, c.r[4]);
}

/// The CLUT place `Texture_LoadPmx` works with (its 6th and 7th arguments, which it changes).
struct Clut {
    x: i32,
    y: i32,
}

/// `Texture_LoadPmx` (TEXTURE.CPP, 0x800e0b24): the shape named `name` in `file` (or `shape`
/// itself without a file) into VRAM at (x + its x offset, y + its y offset) (x -1: a 1x1
/// placeholder at (0, 160)), its palette (unless 16-bit) at `clut_x`, `clut_y` (or a free place,
/// the depth cue's palettes, the placeholder's: -1, -3, -2), shared or not, recoloured for a car;
/// the pixmap `pmx` made: its CLUT, texture page and corners' uv (flipped when the shape says).
/// The shape's size and place word are left as they were.
#[allow(clippy::too_many_arguments)]
pub fn load_pmx(c: &mut Cpu, file: u32, name_or_shape: u32, flags: u32, x: i32, y: i32, clut_x: i32, clut_y: i32, pmx: u32) {
    let shape = if file != 0 { call_jal(c, LOCATE_SHAPE, &[file, name_or_shape]) } else { name_or_shape };
    if shape == 0 {
        return;
    }
    let m = &mut c.m;
    let place = m.r32(shape + 12);
    let (y_off, x_off) = (((place << 4) as i32) >> 20, ((place << 20) as i32) >> 20);
    let first_depth = (m.r8(shape) & 3) as u32;
    let (w0, h0) = (m.r16(shape + 4), m.r16(shape + 6));
    let mut depth = first_depth;
    if flags & LOAD_CAR != 0 && depth != 0 {
        depth = 0;
    }
    let place = m.r32(shape + 12);
    let flipped = place & PLACE_FLIPPED != 0;
    let (tx, ty) = if x == -1 {
        m.w16(shape + 4, 1);
        m.w16(shape + 6, 1);
        (0, 160)
    } else {
        ((((place << 20) as i32) >> 20).wrapping_add(x), (((place << 4) as i32) >> 20).wrapping_add(y))
    };
    let (mut uw, mut uh) = (m.r16(shape + 4) as i16 as i32, m.r16(shape + 6) as i16 as i32);
    if flags & LOAD_FULL_SIZE == 0 {
        uw -= 1;
        uh -= 1;
    }
    let mut clut = Clut { x: clut_x, y: clut_y };
    if depth == 2 {
        vramcf(c, shape, tx, ty, 0, 0);
    } else {
        let palette = shape.wrapping_add(((m.r32(shape) as i32) >> 8) as u32);
        let colours = palette + 16;
        let count = m.r16(palette + 4);
        m.w16(palette + 4, if depth != 0 { 256 } else { 16 });
        let shared = check_for_shared_palette(c, flags & LOAD_SHARE != 0, colours, pmx, depth != 0);
        if !shared {
            match clut.x {
                CLUT_FREE => {
                    with_buffer(c, 8, |c, at| {
                        get_clut_id(&mut c.m, depth != 0, at, at + 4);
                        clut = Clut { x: c.m.r32(at) as i32, y: c.m.r32(at + 4) as i32 };
                    });
                }
                CLUT_DEPTH => {
                    let n = c.m.r16(palette + 4) as i16 as i32;
                    let system = call_jal(c, DEPTH_COLOR_CLUTS, &[colours, n as u32]);
                    c.m.w16(pmx.wrapping_add(10), system as u16);
                }
                CLUT_PLACEHOLDER => {
                    clut = Clut { x: 0, y: 160 };
                    c.m.w16(palette + 4, 1);
                }
                _ => {}
            }
            if flags & LOAD_TRANS_CHECK != 0 {
                let n = c.m.r16(palette + 4) as i16 as i32;
                let semi = call_jal(c, TRANS_COLOR_CHECK, &[colours, n as u32]);
                c.m.w16(pmx.wrapping_add(14), (semi << 1) as u16);
            }
            if flags & LOAD_CAR != 0 && first_depth != 0 {
                if c.m.r32(PAL_COPY) != 0 {
                    copy_palette(c, colours, count as i16 as i32, clut.x as u32, clut.y as u32);
                }
                if flags & LOAD_PAINT != 0 {
                    color_car_palette(c, colours, colours, 16);
                }
            }
            let id = if clut.x >= 0 { call_jal(c, GET_CLUT, &[clut.x as u32, clut.y as u32]) } else { 0xFFFF };
            c.m.w16(pmx.wrapping_add(2), id as u16);
            if flags & LOAD_SHARE != 0 {
                add_shared_palette(&mut c.m, colours, pmx, depth);
            }
        }
        if flags & LOAD_NO_PALETTE != 0 {
            vramcf(c, shape, tx, ty, -1, -1);
        } else {
            vramcf(c, shape, tx, ty, clut.x, clut.y);
        }
        c.m.w16(palette + 4, count);
    }
    let mode = get_translucency_mode(c, shape);
    let page = call_jal(c, GET_TPAGE, &[depth, mode, tx as u32, ty as u32]);
    let m = &mut c.m;
    m.w16(pmx.wrapping_add(6), page as u16);
    // the texture's place in its page: x in 64ths (by depth: 4 or 2 texels a unit), y in 256ths
    let mut u = tx % 64;
    let v = ty % 256;
    if depth == 0 {
        u <<= 2;
    }
    if depth == 1 {
        u <<= 1;
    }
    let corners: [(i32, i32); 4] = if flipped {
        [(u, v + uh), (u, v), (u + uw, v + uh), (u + uw, v)]
    } else {
        [(u, v), (u + uw, v), (u, v + uh), (u + uw, v + uh)]
    };
    for (k, (cu, cv)) in corners.into_iter().enumerate() {
        m.w8(pmx.wrapping_add(4 * k as u32), cu as u8);
        m.w8(pmx.wrapping_add(4 * k as u32 + 1), cv as u8);
    }
    let place = m.r32(shape + 12);
    m.w16(shape + 4, w0);
    m.w16(shape + 6, h0);
    let place = (((place & 0xFFFF_F000) | ((x_off as u32) & 0xFFF)) & 0xF000_FFFF) | (((y_off as u32) & 0xFFF) << 16);
    m.w32(shape + 12, place);
}

pub fn abi_load_pmx(c: &mut Cpu) {
    let sp = c.r[29];
    let a: Vec<u32> = (4..8).map(|k| c.m.r32(sp + 4 * k)).collect();
    load_pmx(c, c.r[4], c.r[5], c.r[6], c.r[7] as i32, a[0] as i32, a[1] as i32, a[2] as i32, a[3]);
}

/// `Texture_CloneUVPmx` (TEXTURE.CPP, 0x800e0f64): `src` copied to `dst` with its corners' uv
/// turned by `turn` (0..4: the five arrangements the track's polygons use).
pub fn clone_uv_pmx(m: &mut Mem, src: u32, turn: u32, dst: u32) {
    for b in 0..PIXMAP_SIZE {
        let v = m.r8(src.wrapping_add(b));
        m.w8(dst.wrapping_add(b), v);
    }
    // (each target corner from a source corner, in the original's order)
    let moves: &[(u32, u32)] = match turn {
        0 => &[(0, 4), (4, 0), (8, 12), (12, 8)],
        1 => &[(12, 4), (4, 12), (8, 0), (0, 8)],
        2 => &[(0, 8), (4, 0), (12, 4), (8, 12)],
        3 => &[(0, 12), (4, 8), (12, 0), (8, 4)],
        4 => &[(0, 4), (4, 12), (12, 8), (8, 0)],
        _ => &[],
    };
    for &(to, from) in moves {
        let v = m.r16(src.wrapping_add(from));
        m.w16(dst.wrapping_add(to), v);
    }
}

pub fn abi_clone_uv_pmx(c: &mut Cpu) {
    clone_uv_pmx(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `Texture_InitTrackTexture` (TEXTURE.CPP, 0x800e10c4): the race's CLUT lists (from the video
/// decoder's buffer), palette sharing, the depth cue, the free places.
pub fn init_track_texture(c: &mut Cpu) {
    let free4 = call_jal(c, GET_DCT_BUFFER, &[5104, CLUTS4_NAME]);
    c.m.w32(FREE_4, free4);
    let free8 = call_jal(c, GET_DCT_BUFFER, &[16, CLUTS8_NAME]);
    c.m.w32(FREE_8, free8);
    init_palette_sharing(c);
    call_jal(c, TEXTURE_PROCESS_INIT, &[]);
    init_clut(c);
}

pub fn abi_init_track_texture(c: &mut Cpu) {
    init_track_texture(c);
}

/// `Texture_InitMenuClut` (TEXTURE.CPP, 0x800e111c): the menus' CLUT lists: 16-colour places in
/// the 16 columns at x 512.., rows 488..511; 256-colour ones at x 512, rows 348..487.
pub fn init_menu_clut(c: &mut Cpu) {
    let free4 = call_jal(c, RESERVE, &[MENU_CLUTS4_NAME, 768, 0]);
    c.m.w32(FREE_4, free4);
    let free8 = call_jal(c, RESERVE, &[MENU_CLUTS8_NAME, 4480, 0]);
    c.m.w32(FREE_8, free8);
    c.m.w32(NB_FREE_4, 0);
    push_cluts(c, FREE_4, NB_FREE_4, 16, 32, 488, 24);
    c.m.w32(NB_FREE_8, 0);
    push_cluts(c, FREE_8, NB_FREE_8, 1, 32, 348, 140);
}

pub fn abi_init_menu_clut(c: &mut Cpu) {
    init_menu_clut(c);
}

/// `Texture_InitMenuTexture` (TEXTURE.CPP, 0x800e121c): no menu pixmaps.
pub fn init_menu_texture(c: &mut Cpu) {
    for k in (0..MENU_PIXMAPS).rev() {
        c.m.w32(MENU_PIXMAP + 4 * k, 0);
    }
    ticks(c, MENU_PIXMAPS - 1);
}

pub fn abi_init_menu_texture(c: &mut Cpu) {
    init_menu_texture(c);
}

/// `Texture_CleanupMenuTexture` (TEXTURE.CPP, 0x800e1244): the menu pixmaps' CLUT places freed,
/// none left.
pub fn cleanup_menu_texture(c: &mut Cpu) {
    for k in 0..MENU_PIXMAPS {
        let pmx = c.m.r32(MENU_PIXMAP + 4 * k);
        if pmx != 0 {
            let id = c.m.r16(pmx.wrapping_add(2)) as i16 as i32 as u32;
            menu_release_clut_id(&mut c.m, id);
        }
        c.m.w32(MENU_PIXMAP + 4 * k, 0);
    }
    ticks(c, MENU_PIXMAPS - 1);
}

pub fn abi_cleanup_menu_texture(c: &mut Cpu) {
    cleanup_menu_texture(c);
}

/// `Texture_LoadMenuTexture` (TEXTURE.CPP, 0x800e12a4): once, the menus' shapes from
/// "show.psh" into VRAM at (896, 384), their pixmaps made.
pub fn load_menu_texture(c: &mut Cpu) {
    if c.m.r32(MENU_PIXMAP) != 0 {
        return;
    }
    let file = with_buffer(c, 256, |c, name| {
        let path = c.m.r32(SHAPES_PATH);
        call_jal(c, SPRINTF, &[name, SHOW_PSH, path]);
        call_jal(c, LOAD_FILE, &[name, 0])
    });
    reset_palette_sharing(&mut c.m);
    for (k, (shape, slot)) in MENU_SHAPES.into_iter().enumerate() {
        let pmx = MENU_PIXMAP_ALLOC + PIXMAP_SIZE * k as u32;
        load_pmx(c, file, shape, 0, 896, 384, -1, -1, pmx);
        c.m.w32(MENU_PIXMAP + 4 * slot, pmx);
    }
    call_jal(c, PURGE, &[file]);
}

pub fn abi_load_menu_texture(c: &mut Cpu) {
    load_menu_texture(c);
}

/// `Texture_KillTrackTexture` (TEXTURE.CPP, 0x800e1458): nothing.
pub fn kill_track_texture(_c: &mut Cpu) {}

pub fn abi_kill_track_texture(c: &mut Cpu) {
    kill_track_texture(c);
}

/// `Texture_KillMenuTexture` (TEXTURE.CPP, 0x800e1460): the menus' CLUT lists freed.
pub fn kill_menu_texture(c: &mut Cpu) {
    for at in [FREE_4, FREE_8] {
        let block = c.m.r32(at);
        call_jal(c, PURGE, &[block]);
    }
}

pub fn abi_kill_menu_texture(c: &mut Cpu) {
    kill_menu_texture(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{COMPARE_TICKS, Rng, against_original};

    /// Free RAM (the race leaves it zero): a shape (an image block, its palette, now and then a
    /// translucency record after), shared palettes and their pixmaps, kept palettes, a pixmap
    /// out, a rectangle, CLUT places.
    const SHAPE: u32 = 0x8014_0000;
    const SHARED: u32 = 0x8014_4000;
    const SHARED_PMX: u32 = 0x8014_5000;
    const KEPT: u32 = 0x8014_5400;
    const PMX: u32 = 0x8014_7000;
    const RECT: u32 = 0x8014_7020;
    const PLACES: u32 = 0x8014_7040;

    /// The shape: its image block's depth (0 4-bit, 1 8-bit, 2 16-bit), size and place word;
    /// where its palette block is (after the image) and how many colours; the translucency
    /// record's mode, if any.
    fn build_shape(m: &mut Mem, rng: &mut Rng) -> (u32, u32) {
        let depth = [0, 0, 1, 2][pick(rng, 4) as usize];
        let w = [16, 17, 32, 6, 1 + pick(rng, 64)][pick(rng, 5) as usize];
        let h = [2, 3, 16, 1 + pick(rng, 40)][pick(rng, 4) as usize];
        let bytes = (w * [4, 8, 16][depth as usize] + 7) / 8 * h;
        let palette = SHAPE + 16 + ((bytes + 3) & !3);
        let kind = TYPE_IMAGE_FIRST as u32 + depth + if pick(rng, 4) == 0 { TYPE_IN_VRAM as u32 } else { 0 };
        m.w32(SHAPE, kind | (palette - SHAPE) << 8);
        m.w16(SHAPE + 4, w as u16);
        m.w16(SHAPE + 6, h as u16);
        let place = (pick(rng, 64) | (pick(rng, 64) << 16)) as u32 | if pick(rng, 3) == 0 { PLACE_FLIPPED } else { 0 };
        m.w32(SHAPE + 8, rng.next() as u32);
        m.w32(SHAPE + 12, place);
        for k in (0..bytes).step_by(4) {
            m.w32(SHAPE + 16 + k, rng.next() as u32);
        }
        let colours = if depth == 1 { 256 } else { 16 };
        let record = palette + 16 + 2 * colours;
        let more = pick(rng, 2) == 0;
        m.w32(palette, TYPE_PALETTE as u32 | if more { (record - palette) << 8 } else { 0 });
        m.w16(palette + 4, [colours, 16, 256, pick(rng, 300)][pick(rng, 4) as usize] as u16);
        m.w16(palette + 6, 1);
        m.w32(palette + 12, rng.next() as u32);
        for k in 0..colours {
            let v = rng.next() as u16;
            m.w16(palette + 16 + 2 * k, [v, v | 0x8000, 0x8000, 0][pick(rng, 4) as usize]);
        }
        if more {
            m.w32(record, TYPE_TRANSLUCENCY as u32);
            m.w16(record + 4, (pick(rng, 4) << 5) as u16);
        }
        (palette, depth)
    }

    /// The race RAM's textures (its CLUT lists, its sharing tables) in any state the loading
    /// can be in: a shape to load, some palettes shared (one now and then the shape's own), the
    /// free lists short or empty, kept palettes for the car or none, any car colour.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        let (palette, depth) = build_shape(m, rng);
        // the shared palettes: random ones, now and then the shape's
        let big = depth == 1;
        let words = if big { 128 } else { 8 };
        let (count, pals, pmxs) = if big { (NUM_8BIT, PAL_8BIT, PMX_8BIT) } else { (NUM_4BIT, PAL_4BIT, PMX_4BIT) };
        let n = pick(rng, 5);
        let (table, pmx_table) = (m.r32(pals), m.r32(pmxs));
        for k in 0..n {
            let at = SHARED + 4 * words * k;
            for j in 0..words {
                let v = if pick(rng, 3) == 0 { m.r32(palette + 16 + 4 * j) } else { rng.next() as u32 };
                m.w32(at + 4 * j, v);
            }
            m.w32(table + 4 * k, at);
            let pmx = SHARED_PMX + 16 * k;
            for j in 0..4 {
                m.w32(pmx + 4 * j, rng.next() as u32);
            }
            m.w32(pmx_table + 4 * k, pmx);
        }
        if n > 0 && pick(rng, 2) == 0 {
            let at = SHARED + 4 * words * pick(rng, n);
            for j in 0..words {
                let v = m.r32(palette + 16 + 4 * j);
                m.w32(at + 4 * j, v);
            }
            // (now and then a near miss: a bit off in one word)
            if pick(rng, 2) == 0 {
                let j = pick(rng, words);
                let v = m.r32(at + 4 * j) ^ (1 << pick(rng, 32));
                m.w32(at + 4 * j, v);
            }
        }
        m.w32(count, n);
        for nb in [NB_FREE_4, NB_FREE_8] {
            let v = m.r32(nb);
            m.w32(nb, [v, 0, 1, pick(rng, 20)][pick(rng, 4) as usize]);
        }
        let free8 = m.r32(FREE_8);
        for k in 0..20 {
            m.w16(free8 + 2 * k, clut_id(32, 348 + k) as u16);
        }
        m.w32(PAL_COPY, [0, KEPT][pick(rng, 2) as usize]);
        m.w32(PAL_NUM, pick(rng, 3));
        for k in 0..(3 * PAL_COPY_SIZE / 4) {
            m.w32(KEPT + 4 * k, rng.next() as u32);
        }
        for e in 0..3 {
            m.w32(KEPT + PAL_COPY_SIZE * e, pick(rng, 1024));
            m.w32(KEPT + PAL_COPY_SIZE * e + 4, pick(rng, 512));
        }
        m.w32(CAR_COLOR, pick(rng, 8));
        for k in 0..4 {
            m.w32(PMX + 4 * k, rng.next() as u32);
        }
        m.w16(RECT, pick(rng, 1024) as u16);
        m.w16(RECT + 2, pick(rng, 512) as u16);
        m.w16(RECT + 4, [16, 17, pick(rng, 64)][pick(rng, 3) as usize] as u16);
        m.w16(RECT + 6, pick(rng, 4) as u16);
        for k in 0..8 {
            m.w32(MENU_PIXMAP + 4 * k, if pick(rng, 2) == 0 { MENU_PIXMAP_ALLOC + 16 * k } else { 0 });
            m.w16(MENU_PIXMAP_ALLOC + 16 * k + 2, clut_id(32 + pick(rng, 16), [348 + pick(rng, 140), 488 + pick(rng, 24)][pick(rng, 2) as usize]) as u16);
        }
    }

    fn place(r: &mut Rng) -> u32 {
        [-1i32, 0, 384, pick(r, 960) as i32][pick(r, 4) as usize] as u32
    }

    fn clut_x(r: &mut Rng) -> u32 {
        [-1i32, -1, -2, -3, 0, pick(r, 1024) as i32][pick(r, 6) as usize] as u32
    }

    fn flags(r: &mut Rng) -> u32 {
        [0, 0x41, 0x43, 0x08, 0x18, 0x09, 0x20, 0x40, 0x1B, pick(r, 0x80)][pick(r, 10) as usize]
    }

    fn cases() -> Vec<Case> {
        vec![
            ("Texture_InitPaletteSharing", 0x800e_01c0, nfs4rc::f_800e01c0, |_, _| vec![]),
            ("Texture_DeInitPaletteSharing", 0x800e_023c, nfs4rc::f_800e023c, |_, _| vec![]),
            ("Texture_ResetPaletteSharing", 0x800e_0284, nfs4rc::f_800e0284, |_, _| vec![]),
            ("Texture_CheckForSharedPalette", 0x800e_0294, nfs4rc::f_800e0294, |c, r| {
                let palette = SHAPE + ((c.m.r32(SHAPE) as i32) >> 8) as u32;
                let big = c.m.r8(SHAPE) & 3 == 1;
                vec![pick(r, 3).min(1), palette + 16, PMX, [big as u32, pick(r, 2)][pick(r, 2) as usize]]
            }),
            ("Texture_AddSharedPalette", 0x800e_03c8, nfs4rc::f_800e03c8, |_, r| vec![SHARED, PMX, pick(r, 3)]),
            ("Texture_InitClut", 0x800e_0440, nfs4rc::f_800e0440, |_, _| vec![]),
            ("Texture_GetClutId", 0x800e_0544, nfs4rc::f_800e0544, |_, r| vec![pick(r, 2), PLACES, PLACES + 4]),
            ("Texture_MenuReleaseClutId", 0x800e_05b0, nfs4rc::f_800e05b0, |_, r| {
                vec![clut_id(32, [348 + pick(r, 140), 487, 488, 488 + pick(r, 24)][pick(r, 4) as usize]) as u32 | [0, 0xFFFF_0000][pick(r, 2) as usize]]
            }),
            ("Texture_ColorCarPalette", 0x800e_0610, nfs4rc::f_800e0610, |_, r| vec![KEPT + 8, PLACES, [16, 1, pick(r, 40)][pick(r, 3) as usize]]),
            ("Texture_CopyPalette", 0x800e_0648, nfs4rc::f_800e0648, |c, r| {
                c.m.w32(PAL_COPY, KEPT);
                vec![SHARED, [16, 256, 0, pick(r, 300)][pick(r, 4) as usize], pick(r, 1024), pick(r, 512)]
            }),
            ("Texture_ProcessPaletteCopy", 0x800e_06a8, nfs4rc::f_800e06a8, |_, r| vec![[KEPT, 0][(pick(r, 5) == 0) as usize], pick(r, 2), pick(r, 4)]),
            ("Texture_LoadImage", 0x800e_0754, nfs4rc::f_800e0754, |_, _| vec![RECT, SHARED]),
            ("Texture_Vramf", 0x800e_0798, nfs4rc::f_800e0798, |_, r| vec![[SHAPE, 0][(pick(r, 8) == 0) as usize], pick(r, 960), pick(r, 512), clut_x(r), pick(r, 512)]),
            ("Texture_Vramcf", 0x800e_0944, nfs4rc::f_800e0944, |_, r| vec![SHAPE, pick(r, 960), pick(r, 512), clut_x(r), pick(r, 512)]),
            ("Texture_GetTranslucencyMode", 0x800e_0aac, nfs4rc::f_800e0aac, |_, r| vec![[SHAPE, 0][(pick(r, 8) == 0) as usize]]),
            ("Texture_LoadPmx", 0x800e_0b24, nfs4rc::f_800e0b24, |_, r| vec![0, SHAPE, flags(r), place(r), pick(r, 512), clut_x(r), pick(r, 512), PMX]),
            ("Texture_CloneUVPmx", 0x800e_0f64, nfs4rc::f_800e0f64, |_, r| vec![PMX, pick(r, 6), [SHARED_PMX, PMX][pick(r, 2) as usize]]),
            ("Texture_InitTrackTexture", 0x800e_10c4, nfs4rc::f_800e10c4, |_, _| vec![]),
            ("Texture_InitMenuClut", 0x800e_111c, nfs4rc::f_800e111c, |_, _| vec![]),
            ("Texture_InitMenuTexture", 0x800e_121c, nfs4rc::f_800e121c, |_, _| vec![]),
            ("Texture_CleanupMenuTexture", 0x800e_1244, nfs4rc::f_800e1244, |_, _| vec![]),
            ("Texture_LoadMenuTexture", 0x800e_12a4, nfs4rc::f_800e12a4, |c, r| {
                if pick(r, 2) == 0 {
                    c.m.w32(MENU_PIXMAP, 0);
                }
                vec![]
            }),
            ("Texture_KillTrackTexture", 0x800e_1458, nfs4rc::f_800e1458, |_, _| vec![]),
            ("Texture_KillMenuTexture", 0x800e_1460, nfs4rc::f_800e1460, |c, _| {
                // (only blocks the heap holds can be freed)
                for at in [FREE_4, FREE_8] {
                    let block = crate::abi::call(c, nfs4rc::f_800e5f28, &[MENU_CLUTS4_NAME, 64, 0]);
                    c.m.w32(at, block);
                }
                vec![]
            }),
        ]
    }

    #[test]
    fn texture_fuzz() {
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            // (it all runs while the game loads: the ticks too)
            COMPARE_TICKS.set(true);
            let args_rng = std::cell::RefCell::new(Rng(0x1010 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original(name, 400, 0x1011 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }
}
