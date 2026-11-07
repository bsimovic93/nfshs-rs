//! TRACK.CPP: the track's loading. `Track_Init` loads the track file (its serialized groups)
//! into `Track_mem`, a `SimpleMem` block sized from the file: the header, the chunks' centres, the
//! light table, each chunk (its geometry instanced, the chunks it sees: up to 36), then the
//! persistent groups (the materials, the world's slices, the objects and their definitions); the
//! objects' bounding spheres, the midground objects' precision halved twice, the breakable
//! objects that stay, the objects the kill file removes, the surface saves.
//!
//! The textures come first (`TexturesLoadInitial`): the track's shapes for its time and weather
//! ("0.psh", "N0.psh" at night, "W0.psh" in rain, "S0.psh" both) made pixmaps in VRAM, the spike
//! belt's after them, the horizon's, then the reflection maps ("r.psh"). Shape names carry
//! extra pictures: "!" a palette of another's (`gTempMultiPalInfo`), "ZR"/"ZZ" a mip map's
//! levels (`gTempMipMapInfo`, with mip maps on). Each of the file's materials then gets its
//! pixmap (`Track_LinkMaterials`), a new one where its uv flags flip or turn the picture; the
//! animated ones get a controller (`Track_gMatController`) that `Track_AnimateTextures` runs each
//! frame: a palette cycling through pictures (type 4), or a scroll of its picture's v (0x80).
//!
//! All but the animation and the surface saves runs while the race loads and ticks as the
//! original's loops do (see `call::ticks`). Written readable from the start (step 3 of
//! DECOMP.md's plan): the game's data as the original leaves it, the stack and CPU registers
//! free.

use crate::call::{call_jal, ticks, with_buffer};
use crate::fixed::{div, rem};
use crate::globals::{GAME_SETUP, SIM_GLOBAL};
use ps1rt::{Cpu, Mem};

/// The track's number (`gtrackNumber`, 0..15), the paths (`Paths_Paths`: +0x18 the tracks', +0x20
/// their data's), the names made (`strspc`), their format ("%sTr%02d%s"), "%s".
const TRACK_NUMBER: u32 = 0x8013_E7C4;
const TRACK_PATH: u32 = 0x8011_7738;
const DATA_PATH: u32 = 0x8011_7740;
const PATH_NAME: u32 = 0x8013_F608;
const DATA_PATH_NAME: u32 = 0x8013_F648;
const PATH_FORMAT: u32 = 0x8005_6DF8;
const STRING_FORMAT: u32 = 0x8013_E774;

/// An art resource (`gInitialArt`, the track's): its id (+0, -1), its shapes while loading (+4),
/// how many pixmaps it made (+8), the pixmaps (+0xC, 16 bytes each), the next free one (+0x10;
/// +0x14 the first after the shapes').
const INITIAL_ART: u32 = 0x8011_F448;
const ART_ID: u32 = 0;
const ART_SHAPES: u32 = 4;
const ART_COUNT: u32 = 8;
const ART_PMX: u32 = 0xC;
const ART_NEXT: u32 = 0x10;
const ART_FIRST_FREE: u32 = 0x14;
const PMX_SIZE: u32 = 16;
/// How many pixmaps the track's art has room for, the materials' block (`Track_materials`: 850
/// of 4 bytes), its second word (`Track_materials + 4`, never set: the objects' buffer's extra),
/// the reflection maps' pixmaps (`Track_gReflectionMaps`), the spike belt's (`gSpikeBeltPixmap`).
const ART_PIXMAPS: u32 = 850;
const MATERIALS: u32 = 0x8013_E7B4;
const MATERIALS_SECOND: u32 = 0x8013_E7B8;
const MATERIALS_SIZE: u32 = 3400;
const REFLECTION_MAPS: u32 = 0x8011_F460;
const SPIKE_BELT_PIXMAP: u32 = 0x8013_E4B8;

/// The shapes' extra pictures while loading: mip maps (`gTempMipMapInfo`: 32 of the first level's
/// two name bytes (shorts), its shape (+4) and the last level's (+8)) and palettes
/// (`gTempMultiPalInfo`: 128 of two name bytes, the palette's number (+2), the shape with the
/// picture (+4), this shape (+6)).
const MIP_INFO: u32 = 0x8013_F0DC;
const MULTI_PAL_INFO: u32 = 0x8013_F0E0;
const MIPS: u32 = 32;
const MIP_SIZE: u32 = 12;
const MULTI_PALS: u32 = 128;
const MULTI_PAL_SIZE: u32 = 8;
/// The pixmaps' loading flags (`Texture_LoadPmx`'s): the track's (2), the reflection maps' (0x42);
/// the palette place for any (-1), the depth cue's (-3, with mip maps); the first 12 shapes (the
/// horizon's) never mip mapped.
const LOAD_TRACK: u32 = 2;
const LOAD_ENV_MAP: u32 = 0x42;
const ANY_CLUT: u32 = -1i32 as u32;
const DEPTH_CUE_CLUT: u32 = -3i32 as u32;
const HORIZON_SHAPES: u32 = 12;
/// The track spec's mip maps switch (`TrackSpec_gSpec`'s first half).
const MIP_MAPS_ON: u32 = 0x8012_456C;

/// The materials' controllers (`Track_gMatController`, `Track_gControllerCount`), 16 bytes each:
/// the kind (+0: the material's flags, 4 a palette cycle, 0x80 a scroll), the scroll's range (+2)
/// and its corners' first v (+4..+7), the frames (+8), the ticks a frame (+9), the first picture
/// (+0xA), the material (+0xC).
const CONTROLLERS: u32 = 0x8013_E7D0;
const CONTROLLER_COUNT: u32 = 0x8013_E7CC;
const CONTROLLER_SIZE: u32 = 16;
const CYCLE: u32 = 4;
const SCROLL: u32 = 0x80;
/// A material record in the file (10 bytes, after a 16-byte header): its shape (+0), flags (+2:
/// 2 a palette of another, 4 a cycle, 0x80 a scroll), uv flags (+3: 0x10, 0x40, 2/4/8 the five
/// turns; 0x5E any), pictures (+7), palette or speed (+8). A material (4 bytes): flags (+0: 8 mip
/// mapped), mip levels (+1), pixmap (+2).
const RECORD_SIZE: u32 = 10;
const RECORDS_AT: u32 = 16;
const UV_FLAGS: u8 = 0x5E;

/// The objects: definitions (`Track_gObjDefs`: pointers into `gObjDefOffsetsGroup`, each a
/// short id (-1 none), a vertex count (+2) and vertices of 8 bytes from +4), the persistent
/// groups (`gPersistObjDef`, `gPersistObjInst`, `gPersistMidgroundObjInst`: a count, then records
/// with their size (+0), a flags byte (+2: 0x80 killed, 0 not to collide) and definition (+6)),
/// the definitions' bounding spheres (`gPersistObjDefBoundingSpheres`: a count, then x, y, z, r
/// shorts).
const OBJ_DEFS: u32 = 0x8013_E7BC;
const OBJ_DEF_OFFSETS: u32 = 0x8013_E7C8;
const PERSIST_OBJ_DEF: u32 = 0x8013_E7A0;
const PERSIST_OBJ_INST: u32 = 0x8013_E7A4;
const PERSIST_MIDGROUND: u32 = 0x8013_E7A8;
const BOUNDING_SPHERES: u32 = 0x8013_E7AC;
const KILLED: u8 = 0x80;

/// The track (`Track_mem`: a `SimpleMem` of its block (+0), next (+4) and size (+8); 12 bytes),
/// its header (`Track_header`: the chunks +0x1C), the chunks (`Track_chunkList`: 112 bytes each,
/// their objects +0x40, lights +0x50), what each sees (`Track_gInViewList`: 36 shorts at 64-byte
/// steps, 1023 none; `Track_gInViewCount`: a byte each), the chunks' centres
/// (`Chunk_chunkCenters`), the light table (`Chunk_lightTable`: 1024 bytes, `Chunk_numLight`),
/// the surface saves (`Track_gSaveSurface`).
const TRACK_MEM: u32 = 0x8013_E7C0;
const TRACK_HEADER: u32 = 0x8013_E798;
const HEADER_CHUNKS: u32 = 0x1C;
const CHUNK_LIST: u32 = 0x8013_E79C;
const CHUNK_SIZE: u32 = 112;
const CHUNK_OBJECTS: u32 = 0x40;
const CHUNK_LIGHTS: u32 = 0x50;
const IN_VIEW_LIST: u32 = 0x8013_E790;
const IN_VIEW_COUNT: u32 = 0x8013_E794;
const IN_VIEW_STEP: u32 = 64;
const IN_VIEW_MAX: u32 = 36;
const IN_VIEW_NONE: u16 = 1023;
const CHUNK_CENTERS: u32 = 0x8013_DB10;
const LIGHT_TABLE: u32 = 0x8013_DB0C;
const LIGHT_TABLE_SIZE: u32 = 1028;
const NUM_LIGHT: u32 = 0x8013_E7D4;
const SAVE_SURFACE: u32 = 0x8013_E7B0;
const SAVE_SURFACES: u32 = 48;
/// The room `Track_mem` keeps beyond the file (the file loads at its end).
const TRACK_MEM_EXTRA: u32 = 0x9080;

/// The serialized groups' types: the materials (2), the objects (7), their definitions (8), the
/// world's slices (15), the midground objects (36), the definitions' offsets (38); the header
/// (31), the chunks' centres (32), a chunk (29) and the chunks it sees (4), the persistent groups
/// (33), the lights (35).
const GROUP_MATERIALS: u32 = 2;
const GROUP_OBJECTS: u32 = 7;
const GROUP_OBJ_DEFS: u32 = 8;
const GROUP_SLICES: u32 = 15;
const GROUP_MIDGROUND: u32 = 36;
const GROUP_OBJ_DEF_OFFSETS: u32 = 38;
const GROUP_HEADER: u32 = 31;
const GROUP_CENTERS: u32 = 32;
const GROUP_CHUNK: u32 = 29;
const GROUP_IN_VIEW: u32 = 4;
const GROUP_PERSISTENT: u32 = 33;
const GROUP_LIGHTS: u32 = 35;

/// The places whose quads `Track_FlagListedQuads` flags on the first track (`langFileName +
/// 0x18`: x, z shorts in whole units, to (-1, -1)).
const LISTED_QUADS: u32 = 0x8011_F410;
const QUAD_FLAGGED: u8 = 0x80;

/// Functions called: the library's `sprintf`, `reservememadr(name, size, flags)`,
/// `purgememadr`, `DrawSync`, `shapecount`, `shapepointer`, `shapename(file, k, out)`,
/// `loadshapeadr(name, 0)`, `atoi`, `fixedsqrt`, `filesize`, `loadfileatadr(name, at)`,
/// `loadfileadrz(name, 0)`; `BWAllocMem`, `Texture_LoadPmx`, `Texture_ResetPaletteSharing`,
/// `Texture_CloneUVPmx`, `TextureProcess_Init`, the serialized groups' `LocateGroupNum`,
/// `LocateGroupType(g, type, 0)`, `LocateNextGroupType`, `LocateCreateGroupType(g, type, mem, 0)`,
/// `CreateLiteGroup(g, g, mem)`, `BWorldSm_Init`/`DeInit`, `BWorldSm_SetSlice`,
/// `BWorldSm_FindClosestQuadRez(pos, situation, 1)`, `SimpleMem`'s `Alloc(mem, size, 0)`,
/// `FeignAlloc`, `ResizeToFit`, `Chunk_Init`, `Chunk_DeInit` (the art's release),
/// `Chunk::InstanceGroup(chunk, group, mem)`, `__builtin_new`, `__builtin_delete`, the objects'
/// buffer (0x800a7788: 48 bytes an object) and its release (0x800a77c0), `Math_DistXZ`.
const SPRINTF: u32 = 0x800E_50A8;
const RESERVE: u32 = 0x800E_5F28;
const PURGE: u32 = 0x800E_612C;
const DRAW_SYNC: u32 = 0x800E_E1D8;
const SHAPE_COUNT: u32 = 0x800F_17E0;
const SHAPE_POINTER: u32 = 0x800F_17EC;
const SHAPE_NAME: u32 = 0x800F_1814;
const LOAD_SHAPE: u32 = 0x800F_2970;
const ATOI: u32 = 0x800E_BA5C;
const FIXED_SQRT: u32 = 0x800E_BA08;
const FILE_SIZE: u32 = 0x800E_6258;
const LOAD_FILE_AT: u32 = 0x800E_64DC;
const LOAD_FILE_Z: u32 = 0x800E_6394;
const BW_ALLOC: u32 = 0x8007_EDF4;
const LOAD_PMX: u32 = 0x800E_0B24;
const RESET_PALETTE_SHARING: u32 = 0x800E_0284;
const CLONE_UV_PMX: u32 = 0x800E_0F64;
const TEXTURE_PROCESS_INIT: u32 = 0x800E_1D40;
const LOCATE_GROUP_NUM: u32 = 0x8009_CC84;
const LOCATE_GROUP_TYPE: u32 = 0x8009_CC14;
const LOCATE_NEXT_GROUP_TYPE: u32 = 0x8009_CBEC;
const LOCATE_CREATE_GROUP_TYPE: u32 = 0x8009_CCC8;
const CREATE_LITE_GROUP: u32 = 0x8009_CD40;
const BWORLDSM_INIT: u32 = 0x8007_F334;
const BWORLDSM_DEINIT: u32 = 0x8007_F38C;
const SET_SLICE: u32 = 0x8007_F788;
const FIND_CLOSEST_QUAD: u32 = 0x8008_04E8;
const MEM_ALLOC: u32 = 0x800B_7F24;
const MEM_FEIGN_ALLOC: u32 = 0x800B_7F70;
const MEM_RESIZE_TO_FIT: u32 = 0x800B_7F90;
const CHUNK_INIT: u32 = 0x8008_C418;
const CHUNK_DEINIT: u32 = 0x8008_C420;
const CHUNK_INSTANCE_GROUP: u32 = 0x8008_BE88;
const NEW: u32 = 0x800A_48C0;
const DELETE: u32 = 0x800A_4910;
const OBJECT_BUFFER: u32 = 0x800A_7788;
const OBJECT_BUFFER_RELEASE: u32 = 0x800A_77C0;
const DIST_XZ: u32 = 0x8009_DEA8;
/// The names: "mipmapinf", "mpalinfo", "lighttbl", "Track_mem", "Surface Save", "bsphere";
/// the files' ends: "S0.psh", "N0.psh", "W0.psh", "0.psh", "r.psh", ".grp", ".kil".
const MIP_INFO_NAME: u32 = 0x8005_6E04;
const MULTI_PAL_NAME: u32 = 0x8005_6E10;
const LIGHT_TABLE_NAME: u32 = 0x8005_6E1C;
const TRACK_MEM_NAME: u32 = 0x8005_6E28;
const SURFACE_SAVE_NAME: u32 = 0x8005_6E34;
const BSPHERE_NAME: u32 = 0x8013_E780;
const STORM_SHAPES: u32 = 0x8013_E74C;
const NIGHT_SHAPES: u32 = 0x8013_E754;
const WET_SHAPES: u32 = 0x8013_E75C;
const DAY_SHAPES: u32 = 0x8013_E764;
const REFLECTION_SHAPES: u32 = 0x8013_E76C;
const GRP_END: u32 = 0x8013_E778;
const KILL_END: u32 = 0x8013_E788;
/// The objects' buffer's room beyond the track's (`Track_materials + 4` more).
const OBJECT_BUFFER_EXTRA: u32 = 42;
/// A kill file's lights within this of a killed object go out (16 in their +0x13).
const KILL_LIGHT_NEAR: i32 = 6553;
const LIGHT_SIZE: u32 = 20;
const LIGHT_OFF: u8 = 16;

fn sh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

/// The track's chunks (its header's count).
fn chunk_count(m: &mut Mem) -> u32 {
    let header = m.r32(TRACK_HEADER);
    m.r32(header + HEADER_CHUNKS)
}

/// `n` bytes copied (the original's unaligned word copies).
fn copy_bytes(m: &mut Mem, to: u32, from: u32, n: u32) {
    for k in 0..n {
        let b = m.r8(from.wrapping_add(k));
        m.w8(to.wrapping_add(k), b);
    }
}

/// `Track_SetTrackNumber` (TRACK.CPP, 0x800b9fc0).
pub fn set_track_number(m: &mut Mem, n: u32) {
    m.w32(TRACK_NUMBER, n & 0xF);
}

pub fn abi_set_track_number(c: &mut Cpu) {
    set_track_number(&mut c.m, c.r[4]);
}

fn path_name(c: &mut Cpu, out: u32, path: u32, end: u32) -> u32 {
    let (dir, n) = (c.m.r32(path), c.m.r32(TRACK_NUMBER));
    call_jal(c, SPRINTF, &[out, PATH_FORMAT, dir, n, end]);
    out
}

/// `Track_MakeTrackPathName` (TRACK.CPP, 0x800b9fd0): the track's file ending `end` ("TrNN"
/// after the tracks' path).
pub fn make_track_path_name(c: &mut Cpu, end: u32) -> u32 {
    path_name(c, PATH_NAME, TRACK_PATH, end)
}

pub fn abi_make_track_path_name(c: &mut Cpu) {
    c.r[2] = make_track_path_name(c, c.r[4]);
}

/// `Track_MakeTrackDataPathName` (TRACK.CPP, 0x800ba018): the same in the data's path.
pub fn make_track_data_path_name(c: &mut Cpu, end: u32) -> u32 {
    path_name(c, DATA_PATH_NAME, DATA_PATH, end)
}

pub fn abi_make_track_data_path_name(c: &mut Cpu) {
    c.r[2] = make_track_data_path_name(c, c.r[4]);
}

/// `AllocArtResource` (TRACK.CPP, 0x800ba060): `art` with room for `pixmaps`, none made.
pub fn alloc_art_resource(c: &mut Cpu, art: u32, pixmaps: u32) {
    c.m.w32(art + ART_ID, -1i32 as u32);
    c.m.w32(art + ART_FIRST_FREE, 0);
    let block = call_jal(c, BW_ALLOC, &[pixmaps << 4]);
    c.m.w32(art + ART_PMX, block);
    c.m.w32(art + ART_COUNT, 0);
    c.m.w32(art + ART_SHAPES, 0);
}

pub fn abi_alloc_art_resource(c: &mut Cpu) {
    alloc_art_resource(c, c.r[4], c.r[5]);
}

/// `InitArtResources` (TRACK.CPP, 0x800ba0a0): the track's art and the materials' block.
pub fn init_art_resources(c: &mut Cpu) {
    alloc_art_resource(c, INITIAL_ART, ART_PIXMAPS);
    let block = call_jal(c, BW_ALLOC, &[MATERIALS_SIZE]);
    c.m.w32(MATERIALS, block);
}

pub fn abi_init_art_resources(c: &mut Cpu) {
    init_art_resources(c);
}

/// `DeInitArtResources` (TRACK.CPP, 0x800ba0d0): the track's pixmaps forgotten, the chunks' (and
/// art's) memory released.
pub fn de_init_art_resources(c: &mut Cpu) {
    c.m.w32(INITIAL_ART + ART_PMX, 0);
    call_jal(c, CHUNK_DEINIT, &[]);
}

pub fn abi_de_init_art_resources(c: &mut Cpu) {
    de_init_art_resources(c);
}

/// `LoadShapesAndMakePmx` (TRACK.CPP, 0x800ba0f4): each of `shapes`' shapes made a pixmap (in
/// `pmx` on, those there are) at (`x`, `y`) with `flags`: mip mapped when mip maps are on (the
/// depth cue's 16-colour palettes) but for the horizon's, the 256-colour and the "#" shapes; the "!"
/// shapes' palettes and the mip maps' levels noted for the materials.
pub fn load_shapes_and_make_pmx(c: &mut Cpu, shapes: u32, pmx: u32, flags: u32, x: u32, y: u32) {
    let mip_maps = sh(&mut c.m, MIP_MAPS_ON) != 0;
    c.m.w32(MIP_INFO, 0);
    let clut = if mip_maps { DEPTH_CUE_CLUT } else { ANY_CLUT };
    if mip_maps {
        let info = call_jal(c, RESERVE, &[MIP_INFO_NAME, MIPS * MIP_SIZE, 16]);
        c.m.w32(MIP_INFO, info);
        for k in 0..MIPS {
            let at = c.m.r32(MIP_INFO).wrapping_add(MIP_SIZE * k);
            c.m.w32(at, -1i32 as u32);
        }
        ticks(c, MIPS - 1);
    }
    let info = call_jal(c, RESERVE, &[MULTI_PAL_NAME, MULTI_PALS * MULTI_PAL_SIZE, 16]);
    c.m.w32(MULTI_PAL_INFO, info);
    for k in 0..MULTI_PALS {
        c.m.w16(info.wrapping_add(MULTI_PAL_SIZE * k + 2), -1i16 as u16);
    }
    ticks(c, MULTI_PALS - 1);
    call_jal(c, DRAW_SYNC, &[0]);
    let (mut pmx, mut mips, mut pal_entry) = (pmx, 0u32, 0u32);
    let mut k = 0;
    while (k as i32) < call_jal(c, SHAPE_COUNT, &[shapes]) as i32 {
        let shape = call_jal(c, SHAPE_POINTER, &[shapes, k]);
        if shape != 0 {
            with_buffer(c, 8, |c, name_at| {
                // (`shapename` writes the four bytes alone; the original's `atoi` reads on into
                // its stack, here a 0)
                c.m.w8(name_at + 4, 0);
                call_jal(c, SHAPE_NAME, &[shapes, k, name_at]);
                let name: [u8; 4] = std::array::from_fn(|i| c.m.r8(name_at + i as u32));
                let big = c.m.r8(shape) & 3 == 1;
                let clut = if k < HORIZON_SHAPES || big || name[0] == b'#' { ANY_CLUT } else { clut };
                call_jal(c, LOAD_PMX, &[0, shape, flags | LOAD_TRACK, x, y, clut, clut, pmx]);
                if name[0] == b'!' {
                    let entry = c.m.r32(MULTI_PAL_INFO).wrapping_add(pal_entry);
                    note_multi_palette(c, entry, name_at, k);
                    pal_entry += MULTI_PAL_SIZE;
                }
                if sh(&mut c.m, MIP_MAPS_ON) != 0 && name[0] == b'Z' {
                    note_mip_level(c, shape, &name, &mut mips, k);
                }
            });
            pmx = pmx.wrapping_add(PMX_SIZE);
        }
        k += 1;
        c.tick();
    }
}

pub fn abi_load_shapes_and_make_pmx(c: &mut Cpu) {
    let y = c.m.r32(c.r[29] + 16);
    load_shapes_and_make_pmx(c, c.r[4], c.r[5], c.r[6], c.r[7], y);
}

/// A "!" shape's palette noted (its name at `name_at`): its two bytes, its number (the name's
/// rest), this shape; a number 0 is the picture itself, else the picture's shape is the earlier
/// entry's of number 0 with the same bytes.
fn note_multi_palette(c: &mut Cpu, entry: u32, name_at: u32, k: u32) {
    let (a, b) = (c.m.r8(name_at + 1), c.m.r8(name_at + 2));
    c.m.w8(entry, a);
    c.m.w8(entry + 1, b);
    let number = call_jal(c, ATOI, &[name_at + 3]);
    c.m.w16(entry + 2, number as u16);
    c.m.w16(entry + 6, k as u16);
    if number == 0 {
        c.m.w16(entry + 4, k as u16);
        return;
    }
    let info = c.m.r32(MULTI_PAL_INFO);
    let found = (0..MULTI_PALS).position(|j| {
        let e = info.wrapping_add(MULTI_PAL_SIZE * j);
        c.m.r8(e) == a && c.m.r8(e + 1) == b && c.m.r16(e + 2) == 0
    });
    match found {
        Some(j) => {
            ticks(c, j as u32);
            let picture = c.m.r16(info.wrapping_add(MULTI_PAL_SIZE * j as u32 + 4));
            c.m.w16(entry + 4, picture);
        }
        None => ticks(c, MULTI_PALS - 1),
    }
}

/// A mip map's level noted: "ZR" and two bytes its first level (256-colour shapes only), "ZZ"
/// its last (the first's entry found by the two bytes).
fn note_mip_level(c: &mut Cpu, shape: u32, name: &[u8; 4], mips: &mut u32, k: u32) {
    let (a, b) = (name[2] as u16, name[3] as u16);
    if name[1] == b'R' && c.m.r8(shape) & 3 == 1 {
        let entry = c.m.r32(MIP_INFO).wrapping_add(MIP_SIZE * *mips);
        c.m.w16(entry, a);
        c.m.w16(entry + 2, b);
        c.m.w32(entry + 4, k);
        c.m.w32(entry + 8, k);
        *mips += 1;
    } else if name[1] == b'Z' {
        let info = c.m.r32(MIP_INFO);
        let found = (0..MIPS).position(|j| {
            let e = info.wrapping_add(MIP_SIZE * j);
            c.m.r16(e) == a && c.m.r16(e + 2) == b
        });
        match found {
            Some(j) => {
                // (the match branches back to its store: a tick more)
                ticks(c, j as u32 + 1);
                c.m.w32(info.wrapping_add(MIP_SIZE * j as u32 + 8), k);
            }
            None => ticks(c, MIPS - 1),
        }
    }
}

/// `LoadShapesAndMakePmx_EnvMap` (TRACK.CPP, 0x800ba440): each of `shapes`' shapes made a
/// reflection map's pixmap (in `pmx` on) at (`x`, `y`), its palette anywhere.
pub fn load_shapes_and_make_pmx_env_map(c: &mut Cpu, shapes: u32, pmx: u32, x: u32, y: u32) {
    let mut pmx = pmx;
    let mut k = 0;
    while (k as i32) < call_jal(c, SHAPE_COUNT, &[shapes]) as i32 {
        let shape = call_jal(c, SHAPE_POINTER, &[shapes, k]);
        if shape != 0 {
            call_jal(c, LOAD_PMX, &[0, shape, LOAD_ENV_MAP, x, y, ANY_CLUT, ANY_CLUT, pmx]);
            pmx = pmx.wrapping_add(PMX_SIZE);
        }
        k += 1;
        c.tick();
    }
}

pub fn abi_load_shapes_and_make_pmx_env_map(c: &mut Cpu) {
    load_shapes_and_make_pmx_env_map(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `TexturesLoadInitial` (TRACK.CPP, 0x800ba4f8): the track's shapes for its time and weather
/// made the art's pixmaps, the spike belt's after them (the first free after), the horizon's
/// pictures, the reflection maps when the track has them. Without the shapes' file the game
/// stops here.
pub fn textures_load_initial(c: &mut Cpu) {
    let m = &mut c.m;
    let (night, wet) = (GAME_SETUP.time(m) != 0, GAME_SETUP.weather(m) != 0);
    let file = match (night, wet) {
        (true, true) => STORM_SHAPES,
        (true, false) => NIGHT_SHAPES,
        (false, true) => WET_SHAPES,
        (false, false) => DAY_SHAPES,
    };
    let name = make_track_path_name(c, file);
    let shapes = call_jal(c, LOAD_SHAPE, &[name, 0]);
    c.m.w32(INITIAL_ART + ART_SHAPES, shapes);
    if shapes == 0 {
        // (the original's `for (;;)`)
        loop {
            c.tick();
        }
    }
    call_jal(c, RESET_PALETTE_SHARING, &[]);
    let count = call_jal(c, SHAPE_COUNT, &[shapes]);
    c.m.w32(INITIAL_ART + ART_COUNT, count);
    let pmx = c.m.r32(INITIAL_ART + ART_PMX);
    load_shapes_and_make_pmx(c, shapes, pmx, 64, 256, 0);
    let m = &mut c.m;
    let at = m.r32(INITIAL_ART + ART_PMX).wrapping_add(m.r32(INITIAL_ART + ART_COUNT) << 4);
    let belt = m.r32(SPIKE_BELT_PIXMAP);
    copy_bytes(m, at, belt, PMX_SIZE);
    let count = m.r32(INITIAL_ART + ART_COUNT).wrapping_add(1);
    m.w32(INITIAL_ART + ART_COUNT, count);
    m.w32(INITIAL_ART + ART_NEXT, count);
    m.w32(INITIAL_ART + ART_FIRST_FREE, count);
    call_jal(c, PURGE, &[shapes]);
    let pmx = c.m.r32(INITIAL_ART + ART_PMX);
    crate::hrzsku::get_horizon_pix_map(c, pmx);
    let name = make_track_path_name(c, REFLECTION_SHAPES);
    let maps = call_jal(c, LOAD_SHAPE, &[name, 0]);
    if maps != 0 {
        call_jal(c, RESET_PALETTE_SHARING, &[]);
        load_shapes_and_make_pmx_env_map(c, maps, REFLECTION_MAPS, 992, 0);
        call_jal(c, PURGE, &[maps]);
    }
}

pub fn abi_textures_load_initial(c: &mut Cpu) {
    textures_load_initial(c);
}

/// `Track_AnimateTextures` (TRACK.CPP, 0x800ba6a4): each controller's frame now (the game's
/// ticks over its ticks a frame, round its frames): a scroll moves its picture's corners' v on
/// from their first, a cycle shows its first picture's frame-th after.
pub fn animate_textures(m: &mut Mem) {
    let now = SIM_GLOBAL.game_ticks(m);
    let mut ctrl = m.r32(CONTROLLERS);
    for _ in 0..m.r32(CONTROLLER_COUNT) {
        let kind = sh(m, ctrl) as u32;
        let frame = |m: &mut Mem, frames: u32| rem(div(now, m.r8(ctrl + 9) as i32), m.r8(ctrl + frames) as i32);
        if kind == SCROLL {
            let frame = frame(m, 2) as u8;
            let material = m.r32(ctrl + 12);
            let pmx = m.r32(INITIAL_ART + ART_PMX).wrapping_add((sh(m, material + 2) << 4) as u32);
            for k in 0..4 {
                let v = m.r8(ctrl + 4 + k).wrapping_add(frame);
                m.w8(pmx.wrapping_add(4 * k + 1), v);
            }
        } else if kind == CYCLE {
            let frame = frame(m, 8) as u16;
            let material = m.r32(ctrl + 12);
            let picture = m.r16(ctrl + 10).wrapping_add(frame);
            m.w16(material.wrapping_add(2), picture);
        }
        ctrl = ctrl.wrapping_add(CONTROLLER_SIZE);
    }
}

pub fn abi_animate_textures(c: &mut Cpu) {
    animate_textures(&mut c.m);
}

/// `Track_GetProperMultiPalShapeIndex` (TRACK.CPP, 0x800ba83c): the shape with `shape`'s picture
/// in palette `palette` (a "!" shape), or `shape`.
pub fn get_proper_multi_pal_shape_index(m: &mut Mem, shape: i32, palette: i32) -> i32 {
    let info = m.r32(MULTI_PAL_INFO);
    for k in 0..MULTI_PALS {
        let e = info.wrapping_add(MULTI_PAL_SIZE * k);
        if sh(m, e + 4) == shape && sh(m, e + 2) == palette {
            return sh(m, e + 6);
        }
    }
    shape
}

/// [`get_proper_multi_pal_shape_index`] ticking as the original's search does (one a miss).
fn multi_pal_shape(c: &mut Cpu, shape: i32, palette: i32) -> i32 {
    let info = c.m.r32(MULTI_PAL_INFO);
    let found = (0..MULTI_PALS).position(|k| {
        let e = info.wrapping_add(MULTI_PAL_SIZE * k);
        sh(&mut c.m, e + 4) == shape && sh(&mut c.m, e + 2) == palette
    });
    ticks(c, found.map_or(MULTI_PALS - 1, |k| k as u32));
    get_proper_multi_pal_shape_index(&mut c.m, shape, palette)
}

pub fn abi_get_proper_multi_pal_shape_index(c: &mut Cpu) {
    c.r[2] = multi_pal_shape(c, c.r[4] as i32, c.r[5] as i32) as u32;
}

/// `Track_ProcessFlipAndUVFlags` (TRACK.CPP, 0x800ba88c): the pixmap `from` copied to `to` with
/// the turns its uv flags ask for, in order: 0x10 (turn 0), 0x40 (turn 1), then one of 2, 4, 8
/// (turns 2, 3, 4).
pub fn process_flip_and_uv_flags(c: &mut Cpu, flags: u32, from: u32, to: u32) {
    with_buffer(c, 4 * PMX_SIZE, |c, buf| {
        copy_bytes(&mut c.m, buf, from, PMX_SIZE);
        let mut n = 0;
        let mut turn = |c: &mut Cpu, how: u32| {
            call_jal(c, CLONE_UV_PMX, &[buf + PMX_SIZE * n, how, buf + PMX_SIZE * (n + 1)]);
            n += 1;
        };
        if flags & 0x10 != 0 {
            turn(c, 0);
        }
        if flags & 0x40 != 0 {
            turn(c, 1);
        }
        if flags & 0xE != 0 {
            turn(c, if flags & 2 != 0 { 2 } else if flags & 4 != 0 { 3 } else { 4 });
        }
        copy_bytes(&mut c.m, to, buf + PMX_SIZE * n, PMX_SIZE);
    });
}

pub fn abi_process_flip_and_uv_flags(c: &mut Cpu) {
    process_flip_and_uv_flags(c, c.r[4], c.r[5], c.r[6]);
}

/// `Track_AssociateSingleMaterial` (TRACK.CPP, 0x800ba9f4): the material `mat` for the file's
/// record `record`: its flags, its pixmap (its shape's, in its palette when it has one; for each
/// of its pictures a new one of `art`'s when its uv flags turn it, the first the material's).
pub fn associate_single_material(c: &mut Cpu, record: u32, mat: u32, art: u32) {
    let flags = c.m.r8(record + 2);
    c.m.w8(mat, flags);
    let mut k = 0;
    while k <= 0 || k < c.m.r8(record + 7) as i32 {
        let mut shape = sh(&mut c.m, record);
        if c.m.r8(record + 2) & 2 != 0 {
            let palette = c.m.r8(record + 8) as i32;
            shape = multi_pal_shape(c, shape, palette);
        }
        if c.m.r8(record + 3) & UV_FLAGS != 0 {
            let m = &mut c.m;
            let pmx = m.r32(art + ART_PMX);
            let from = pmx.wrapping_add((shape.wrapping_add(k) << 4) as u32);
            let next = m.r32(art + ART_NEXT);
            let uv = m.r8(record + 3) as u32;
            process_flip_and_uv_flags(c, uv, from, pmx.wrapping_add(next << 4));
            if k == 0 {
                let next = c.m.r32(art + ART_NEXT);
                c.m.w16(mat + 2, next as u16);
            }
            let next = c.m.r32(art + ART_NEXT).wrapping_add(1);
            c.m.w32(art + ART_NEXT, next);
        } else if k == 0 {
            c.m.w16(mat + 2, shape as u16);
        }
        k += 1;
        c.tick();
    }
}

pub fn abi_associate_single_material(c: &mut Cpu) {
    associate_single_material(c, c.r[4], c.r[5], c.r[6]);
}

/// `Track_LinkMaterials` (TRACK.CPP, 0x800bab60): the file's material records (`data`, `size`
/// bytes of them) made the materials at `mats`: their pixmaps, the animated ones' controllers
/// (a scroll's range from its picture's v, its speed at least 1), the mip mapped ones' levels (a
/// new pixmap for their last level when their uv flags turn it); the loading's notes given back,
/// the art's next free pixmap the first after the shapes'.
pub fn link_materials(c: &mut Cpu, data: u32, size: u32, mats: u32) {
    if data == 0 {
        return;
    }
    let n = size / RECORD_SIZE;
    let records = data.wrapping_add(RECORDS_AT);
    c.m.w32(CONTROLLER_COUNT, 0);
    let first_free = c.m.r32(INITIAL_ART + ART_FIRST_FREE);
    c.m.w32(INITIAL_ART + ART_NEXT, first_free);
    for k in 0..n {
        if c.m.r16(records.wrapping_add(RECORD_SIZE * k) + 2) & (CYCLE | SCROLL) as u16 != 0 {
            let count = c.m.r32(CONTROLLER_COUNT).wrapping_add(1);
            c.m.w32(CONTROLLER_COUNT, count);
        }
    }
    ticks(c, n.saturating_sub(1));
    let count = c.m.r32(CONTROLLER_COUNT);
    if count != 0 {
        let block = call_jal(c, BW_ALLOC, &[count << 4]);
        c.m.w32(CONTROLLERS, block);
    }
    let mut controllers = 0;
    for k in 0..n {
        let (record, mat) = (records.wrapping_add(RECORD_SIZE * k), mats.wrapping_add(4 * k));
        associate_single_material(c, record, mat, INITIAL_ART);
        let m = &mut c.m;
        let flags = m.r8(record + 2);
        if flags & CYCLE as u8 != 0 || flags & SCROLL as u8 != 0 {
            let ctrl = m.r32(CONTROLLERS).wrapping_add(controllers << 4);
            m.w16(ctrl, flags as u16);
            if flags & CYCLE as u8 == 0 && m.r8(record + 8) == 0 {
                m.w8(record + 8, 1);
            }
            let (speed, frames, picture) = (m.r8(record + 8), m.r8(record + 7), m.r16(mat + 2));
            m.w8(ctrl + 9, speed);
            m.w8(ctrl + 8, frames);
            m.w16(ctrl + 10, picture);
            m.w32(ctrl + 12, mat);
            if flags & CYCLE as u8 == 0 {
                // a scroll: its corners' first v, its range (the picture's height)
                let pmx = m.r32(INITIAL_ART + ART_PMX).wrapping_add((sh(m, mat + 2) << 4) as u32);
                let v: [u8; 4] = std::array::from_fn(|i| m.r8(pmx.wrapping_add(4 * i as u32 + 1)));
                for (i, v) in v.into_iter().enumerate() {
                    m.w8(ctrl + 4 + i as u32, v);
                }
                m.w8(ctrl + 2, ((v[3] as i32 - v[0] as i32).abs() + 1) as u8);
            }
            controllers += 1;
        }
        if sh(&mut c.m, MIP_MAPS_ON) != 0 {
            link_mip_levels(c, record, mat);
            ticks(c, MIPS);
        }
        c.tick();
    }
    let info = c.m.r32(MIP_INFO);
    if info != 0 {
        call_jal(c, PURGE, &[info]);
    }
    let info = c.m.r32(MULTI_PAL_INFO);
    call_jal(c, PURGE, &[info]);
    let count = c.m.r32(INITIAL_ART + ART_COUNT);
    c.m.w32(INITIAL_ART + ART_SHAPES, 0);
    c.m.w32(INITIAL_ART + ART_NEXT, count);
}

pub fn abi_link_materials(c: &mut Cpu) {
    link_materials(c, c.r[4], c.r[5], c.r[6]);
}

/// A material's mip map levels: each noted mip map of its record's shape marks it mip mapped,
/// its levels (the last's shape past the first's), its last level's pixmap (turned by its uv
/// flags, a new one) unless its pixmap is its shape's own.
fn link_mip_levels(c: &mut Cpu, record: u32, mat: u32) {
    for j in 0..MIPS {
        let m = &mut c.m;
        let e = m.r32(MIP_INFO).wrapping_add(MIP_SIZE * j);
        let (first, last) = (m.r32(e + 4), m.r32(e + 8));
        if m.r32(e) == -1i32 as u32 || first as i32 != sh(m, record) {
            continue;
        }
        let flags = m.r8(mat) | 8;
        m.w8(mat, flags);
        let levels = last.wrapping_sub(first);
        let shape = sh(m, record);
        if shape == sh(m, mat + 2) {
            m.w8(mat + 1, levels as u8);
        } else {
            let pmx = m.r32(INITIAL_ART + ART_PMX);
            let from = pmx.wrapping_add((shape.wrapping_add(levels as i32) << 4) as u32);
            let next = m.r32(INITIAL_ART + ART_NEXT);
            let uv = m.r8(record + 3) as u32;
            process_flip_and_uv_flags(c, uv, from, pmx.wrapping_add(next << 4));
            let next = c.m.r32(INITIAL_ART + ART_NEXT).wrapping_add(1);
            c.m.w32(INITIAL_ART + ART_NEXT, next);
            c.m.w8(mat + 1, 1);
        }
    }
}

/// The definition of an object record (its +6).
fn object_def(m: &mut Mem, record: u32) -> u32 {
    let k = sh(m, record + 6) as u32;
    let defs = m.r32(OBJ_DEFS);
    m.r32(defs.wrapping_add(k << 2))
}

/// The records of a persistent object group (a count, then records of their own sizes).
fn object_records(m: &mut Mem, group: u32) -> Vec<u32> {
    let mut at = group.wrapping_add(4);
    (0..m.r32(group) as i32)
        .map(|_| {
            let record = at;
            at = at.wrapping_add(sh(m, at) as u32);
            record
        })
        .collect()
}

/// `ReduceObjectPrecision` (TRACK.CPP, 0x800baf24): each of `objects`' definitions' vertices
/// shifted down by `shift` (unless `defs` is none; a definition shared is shifted again).
pub fn reduce_object_precision(c: &mut Cpu, objects: u32, defs: u32, shift: u32) {
    if objects == 0 {
        return;
    }
    for record in object_records(&mut c.m, objects) {
        if defs != 0 {
            let m = &mut c.m;
            let def = object_def(m, record);
            let n = m.r8(def + 2) as u32;
            for k in 0..n {
                for axis in 0..3 {
                    let at = def + 4 + 8 * k + 2 * axis;
                    let v = sh(m, at) >> (shift & 31);
                    m.w16(at, v as u16);
                }
            }
            ticks(c, n);
        }
        c.tick();
    }
}

pub fn abi_reduce_object_precision(c: &mut Cpu) {
    reduce_object_precision(c, c.r[4], c.r[5], c.r[6]);
}

/// `InvalidatePersistentCollideBoomObjects` (TRACK.CPP, 0x800bafc4): `objects` whose
/// definitions have an id not collided with (their flags 0).
pub fn invalidate_persistent_collide_boom_objects(c: &mut Cpu, objects: u32, defs: u32) {
    if objects == 0 || defs == 0 {
        return;
    }
    let records = object_records(&mut c.m, objects);
    for &record in &records {
        let m = &mut c.m;
        let def = object_def(m, record);
        if sh(m, def) != -1 {
            m.w8(record + 2, 0);
        }
    }
    ticks(c, (records.len() as u32).saturating_sub(1));
}

pub fn abi_invalidate_persistent_collide_boom_objects(c: &mut Cpu) {
    invalidate_persistent_collide_boom_objects(c, c.r[4], c.r[5]);
}

/// `CalcObjectBoundingSphere` (TRACK.CPP, 0x800bb030): for each of the `defs` group's
/// definitions its vertices' centre and the farthest vertex's distance from it (in 64ths,
/// squared, then the root) appended to `spheres`.
pub fn calc_object_bounding_sphere(c: &mut Cpu, defs: u32, spheres: u32) {
    let n = c.m.r32(defs) as i32;
    for k in 0..n.max(0) as u32 {
        let m = &mut c.m;
        let defs_at = m.r32(OBJ_DEFS);
        let def = m.r32(defs_at.wrapping_add(4 * k));
        let count = m.r8(def + 2) as u32;
        let vertex = |m: &mut Mem, j: u32, axis: u32| sh(m, def + 4 + 8 * j + 2 * axis);
        let mut sum = [0i32; 3];
        for j in 0..count {
            for (axis, s) in sum.iter_mut().enumerate() {
                *s = s.wrapping_add(vertex(m, j, axis as u32));
            }
        }
        let centre = sum.map(|s| div(s, count as i32));
        let mut farthest = 0i32;
        for j in 0..count {
            let d: i32 = (0..3).map(|axis| {
                let d = centre[axis].wrapping_sub(vertex(m, j, axis as u32)) >> 6;
                d.wrapping_mul(d)
            }).fold(0i32, |a, b| a.wrapping_add(b));
            farthest = farthest.max(d);
        }
        ticks(c, count.saturating_sub(1) + count);
        let entry = spheres.wrapping_add(4 + 8 * k);
        for (axis, &v) in centre.iter().enumerate() {
            c.m.w16(entry + 2 * axis as u32, v as u16);
        }
        let root = call_jal(c, FIXED_SQRT, &[(farthest << 10) as u32]);
        c.m.w16(entry + 6, (root >> 7) as u16);
        let n = c.m.r32(spheres).wrapping_add(1);
        c.m.w32(spheres, n);
        c.tick();
    }
}

pub fn abi_calc_object_bounding_sphere(c: &mut Cpu) {
    calc_object_bounding_sphere(c, c.r[4], c.r[5]);
}

/// `CalcObjDefPtrs` (TRACK.CPP, 0x800bb290): the definitions' offsets group's words made the
/// definitions' places (`Track_gObjDefs`): the first after the persistent definitions' count,
/// each next its offset past the last.
pub fn calc_obj_def_ptrs(c: &mut Cpu) {
    let m = &mut c.m;
    let (defs, group) = (m.r32(PERSIST_OBJ_DEF), m.r32(OBJ_DEF_OFFSETS));
    m.w32(group + 4, defs + 4);
    let mut k = 1;
    while k < m.r32(defs) as i32 {
        let at = group.wrapping_add(4 * k as u32);
        let v = m.r32(at).wrapping_add(m.r32(at + 4));
        m.w32(at + 4, v);
        k += 1;
    }
    m.w32(OBJ_DEFS, group + 4);
    ticks(c, (k - 1) as u32);
}

pub fn abi_calc_obj_def_ptrs(c: &mut Cpu) {
    calc_obj_def_ptrs(c);
}

/// `Track_FlagListedQuads` (TRACK.CPP, 0x800bb2f4; retail only, no symbol): on the first track
/// (`track` 0), the road's quads at the listed places get flag 0x80.
pub fn flag_listed_quads(c: &mut Cpu, track: u32) {
    if track != 0 {
        return;
    }
    with_buffer(c, 136 + 16, |c, situation| {
        let pos = situation + 136;
        call_jal(c, SET_SLICE, &[0, situation]);
        let end = |m: &mut Mem, k: u32| sh(m, LISTED_QUADS + 4 * k) == -1 && sh(m, LISTED_QUADS + 4 * k + 2) == -1;
        let mut k = 0;
        while !end(&mut c.m, k) {
            if k > 0 {
                c.tick();
            }
            let (x, z) = (sh(&mut c.m, LISTED_QUADS + 4 * k), sh(&mut c.m, LISTED_QUADS + 4 * k + 2));
            c.m.w32(pos, (x << 16) as u32);
            c.m.w32(pos + 4, 0);
            c.m.w32(pos + 8, (z << 16) as u32);
            call_jal(c, FIND_CLOSEST_QUAD, &[pos, situation, 1]);
            let quad = c.m.r32(situation + 120);
            if quad != 0 {
                let flags = c.m.r8(quad) | QUAD_FLAGGED;
                c.m.w8(quad, flags);
            }
            k += 1;
        }
    });
}

pub fn abi_flag_listed_quads(c: &mut Cpu) {
    flag_listed_quads(c, c.r[4]);
}

/// `Track_InitPersistentData` (TRACK.CPP, 0x800bb3d8): the persistent groups (`groups`' each):
/// the materials linked, the world's slices set up, the objects', midground objects',
/// definitions' and offsets' groups made lite in the track's memory; the definitions' places.
pub fn init_persistent_data(c: &mut Cpu, groups: u32) {
    c.m.w32(OBJ_DEF_OFFSETS, 0);
    let n = c.m.r32(groups + 12) as i32;
    let found: Vec<u32> = (0..n.max(0) as u32).map(|k| call_jal(c, LOCATE_GROUP_NUM, &[groups, k])).collect();
    ticks(c, found.len().saturating_sub(1) as u32);
    // (its definitions' group looked for, and not used)
    call_jal(c, LOCATE_GROUP_TYPE, &[groups, GROUP_OBJ_DEFS, 0]);
    for g in found {
        let lite = |c: &mut Cpu| {
            let mem = c.m.r32(TRACK_MEM);
            call_jal(c, CREATE_LITE_GROUP, &[g, g, mem])
        };
        match c.m.r32(g) {
            GROUP_MATERIALS => {
                let (size, mats) = (c.m.r32(g + 4).wrapping_sub(16), c.m.r32(MATERIALS));
                link_materials(c, g, size, mats);
            }
            GROUP_SLICES => {
                let v = lite(c);
                call_jal(c, BWORLDSM_INIT, &[v]);
            }
            GROUP_MIDGROUND => {
                let v = lite(c);
                c.m.w32(PERSIST_MIDGROUND, v);
            }
            GROUP_OBJECTS => {
                let v = lite(c);
                c.m.w32(PERSIST_OBJ_INST, v);
            }
            GROUP_OBJ_DEFS => {
                let v = lite(c);
                c.m.w32(PERSIST_OBJ_DEF, v);
            }
            GROUP_OBJ_DEF_OFFSETS => {
                let v = lite(c);
                c.m.w32(OBJ_DEF_OFFSETS, v);
            }
            _ => {}
        }
        c.tick();
    }
    if c.m.r32(OBJ_DEF_OFFSETS) != 0 {
        calc_obj_def_ptrs(c);
    }
}

pub fn abi_init_persistent_data(c: &mut Cpu) {
    init_persistent_data(c, c.r[4]);
}

/// A chunk's view list: the chunks it sees (up to 36, those of the track), the rest none.
fn init_in_view(c: &mut Cpu, chunk_group: u32, k: u32) {
    let g = call_jal(c, LOCATE_GROUP_TYPE, &[chunk_group, GROUP_IN_VIEW, 0]);
    let m = &mut c.m;
    let seen = (m.r32(g + 12) as i32).min(IN_VIEW_MAX as i32);
    let list = m.r32(IN_VIEW_LIST).wrapping_add(IN_VIEW_STEP * k);
    let mut kept = 0u32;
    for j in 0..seen.max(0) as u32 {
        let id = m.r16(g + 16 + 2 * j);
        let chunks = chunk_count(m) as i32;
        if ((id & 0x3FF) as i32) < chunks {
            let at = m.r32(IN_VIEW_LIST).wrapping_add(IN_VIEW_STEP * k + 2 * kept);
            m.w16(at, id);
            kept += 1;
        }
    }
    let at = m.r32(IN_VIEW_COUNT).wrapping_add(k);
    m.w8(at, kept as u8);
    for j in kept..IN_VIEW_MAX {
        m.w16(list.wrapping_add(2 * j), IN_VIEW_NONE);
    }
    ticks(c, (seen.max(0) as u32).saturating_sub(1) + (IN_VIEW_MAX - kept).saturating_sub(1));
}

/// `Track_Init` (TRACK.CPP, 0x800bb5b4): the track `name` loaded (see the module's words).
pub fn init(c: &mut Cpu, name: u32) {
    with_buffer(c, 128, |c, path| {
        c.m.w32(SAVE_SURFACE, 0);
        c.m.w32(OBJ_DEFS, 0);
        c.m.w32(MATERIALS_SECOND, 0);
        call_jal(c, SPRINTF, &[path, STRING_FORMAT, name]);
        let size = call_jal(c, FILE_SIZE, &[name]);
        let table = call_jal(c, RESERVE, &[LIGHT_TABLE_NAME, LIGHT_TABLE_SIZE, 0]);
        c.m.w32(LIGHT_TABLE, table);
        call_jal(c, TEXTURE_PROCESS_INIT, &[]);
        init_art_resources(c);
        textures_load_initial(c);
        let mem = call_jal(c, NEW, &[12]);
        let room = size.wrapping_add(TRACK_MEM_EXTRA);
        let block = call_jal(c, RESERVE, &[TRACK_MEM_NAME, room, 0]);
        c.m.w32(mem, block);
        c.m.w32(mem + 4, block);
        c.m.w32(mem + 8, if block == 0 { 0 } else { room });
        c.m.w32(TRACK_MEM, mem);
        let at = call_jal(c, MEM_FEIGN_ALLOC, &[mem, size]);
        let data = call_jal(c, LOAD_FILE_AT, &[path, at.wrapping_add(TRACK_MEM_EXTRA)]);
        let mem = c.m.r32(TRACK_MEM);
        let header = call_jal(c, LOCATE_CREATE_GROUP_TYPE, &[data, GROUP_HEADER, mem, 0]);
        c.m.w32(TRACK_HEADER, header.wrapping_add(4));
        let mem = c.m.r32(TRACK_MEM);
        let centers = call_jal(c, LOCATE_CREATE_GROUP_TYPE, &[data, GROUP_CENTERS, mem, 0]);
        c.m.w32(CHUNK_CENTERS, centers.wrapping_add(4));
        let lights = call_jal(c, LOCATE_GROUP_TYPE, &[data, GROUP_LIGHTS, 0]);
        let table = c.m.r32(LIGHT_TABLE);
        copy_bytes(&mut c.m, table, lights.wrapping_add(16), 1024);
        ticks(c, 1024 / 16 - 1);
        let lights = call_jal(c, LOCATE_GROUP_TYPE, &[data, GROUP_LIGHTS, 0]);
        let m = &mut c.m;
        let n = m.r32(lights + 4).wrapping_sub(16) >> 2;
        m.w32(NUM_LIGHT, n);
        let chunks = chunk_count;
        for (at, size) in [(IN_VIEW_LIST, 72), (IN_VIEW_COUNT, 1), (CHUNK_LIST, CHUNK_SIZE)] {
            let (mem, n) = (c.m.r32(TRACK_MEM), chunks(&mut c.m));
            let block = call_jal(c, MEM_ALLOC, &[mem, n.wrapping_mul(size), 0]);
            c.m.w32(at, block);
        }
        call_jal(c, CHUNK_INIT, &[]);
        let persistent = call_jal(c, LOCATE_GROUP_TYPE, &[data, GROUP_PERSISTENT, 0]);
        let mut group = call_jal(c, LOCATE_GROUP_TYPE, &[data, GROUP_CHUNK, 0]);
        let mut k = 0u32;
        while (k as i32) < chunks(&mut c.m) as i32 {
            let chunk = c.m.r32(CHUNK_LIST).wrapping_add(CHUNK_SIZE * k);
            let next = if (k as i32) < chunks(&mut c.m) as i32 - 1 { call_jal(c, LOCATE_NEXT_GROUP_TYPE, &[group, GROUP_CHUNK]) } else { 0 };
            let mem = c.m.r32(TRACK_MEM);
            call_jal(c, CHUNK_INSTANCE_GROUP, &[chunk, group, mem]);
            init_in_view(c, group, k);
            group = next;
            k += 1;
            c.tick();
        }
        init_persistent_data(c, persistent);
        let mem = c.m.r32(TRACK_MEM);
        call_jal(c, MEM_RESIZE_TO_FIT, &[mem]);
        make_track_path_name(c, GRP_END);
        let defs = c.m.r32(PERSIST_OBJ_DEF);
        let size = (c.m.r32(defs) << 3) | 4;
        let spheres = call_jal(c, RESERVE, &[BSPHERE_NAME, size, 0]);
        c.m.w32(BOUNDING_SPHERES, spheres);
        let defs = c.m.r32(PERSIST_OBJ_DEF);
        calc_object_bounding_sphere(c, defs, spheres);
        let (mid, defs) = (c.m.r32(PERSIST_MIDGROUND), c.m.r32(PERSIST_OBJ_DEF));
        reduce_object_precision(c, mid, defs, 2);
        let (objects, defs) = (c.m.r32(PERSIST_OBJ_INST), c.m.r32(PERSIST_OBJ_DEF));
        invalidate_persistent_collide_boom_objects(c, objects, defs);
        let saves = call_jal(c, NEW, &[8]);
        let saves = save_surface_new(c, saves, SAVE_SURFACES);
        c.m.w32(SAVE_SURFACE, saves);
        load_object_kill_data(c);
        let extra = c.m.r32(MATERIALS_SECOND).wrapping_add(OBJECT_BUFFER_EXTRA);
        call_jal(c, OBJECT_BUFFER, &[extra]);
        let track = GAME_SETUP.track(&mut c.m) as u32;
        flag_listed_quads(c, track);
    });
}

pub fn abi_init(c: &mut Cpu) {
    init(c, c.r[4]);
}

/// `Track_DeInit` (TRACK.CPP, 0x800bba40): the track's memory, surface saves, shapes, slices,
/// objects' buffer, art, light table and bounding spheres given back.
pub fn de_init(c: &mut Cpu) {
    let mem = c.m.r32(TRACK_MEM);
    if mem != 0 {
        let block = c.m.r32(mem);
        call_jal(c, PURGE, &[block]);
        c.m.w32(mem + 4, 0);
        call_jal(c, DELETE, &[mem]);
    }
    let saves = c.m.r32(SAVE_SURFACE);
    if saves != 0 {
        save_surface_delete(c, saves, 3);
        c.m.w32(SAVE_SURFACE, 0);
    }
    if c.m.r32(INITIAL_ART + ART_SHAPES) != 0 {
        c.m.w32(INITIAL_ART + ART_SHAPES, 0);
    }
    call_jal(c, BWORLDSM_DEINIT, &[]);
    call_jal(c, OBJECT_BUFFER_RELEASE, &[]);
    de_init_art_resources(c);
    let table = c.m.r32(LIGHT_TABLE);
    if table != 0 {
        call_jal(c, PURGE, &[table]);
        c.m.w32(LIGHT_TABLE, 0);
    }
    let spheres = c.m.r32(BOUNDING_SPHERES);
    if spheres != 0 {
        call_jal(c, PURGE, &[spheres]);
    }
    for at in [BOUNDING_SPHERES, PERSIST_MIDGROUND, PERSIST_OBJ_INST, PERSIST_OBJ_DEF] {
        c.m.w32(at, 0);
    }
}

pub fn abi_de_init(c: &mut Cpu) {
    de_init(c);
}

/// `Track_AnimateTrackLighting` (TRACK.CPP, 0x800bbb1c): nothing.
pub fn abi_animate_track_lighting(_c: &mut Cpu) {}

/// `KillFile_OpenRead` (TRACK.CPP, 0x800bbb24): the track's kill file loaded (or 0).
pub fn kill_file_open_read(c: &mut Cpu) -> u32 {
    let name = make_track_path_name(c, KILL_END);
    with_buffer(c, 128, |c, path| {
        call_jal(c, SPRINTF, &[path, STRING_FORMAT, name]);
        call_jal(c, LOAD_FILE_Z, &[path, 0])
    })
}

pub fn abi_kill_file_open_read(c: &mut Cpu) {
    c.r[2] = kill_file_open_read(c);
}

/// `KillFile_ReadEntry` (TRACK.CPP, 0x800bbb68): the kill file's entry `k`: its chunk and object.
pub fn kill_file_read_entry(m: &mut Mem, data: u32, k: u32) -> (u32, u32) {
    let e = data.wrapping_add(k << 3);
    (m.r32(e + 4), m.r32(e + 8))
}

pub fn abi_kill_file_read_entry(c: &mut Cpu) {
    let (chunk, object) = kill_file_read_entry(&mut c.m, c.r[4], c.r[5]);
    c.m.w32(c.r[6], chunk);
    c.m.w32(c.r[7], object);
}

/// `Track_LoadObjectKillData` (TRACK.CPP, 0x800bbb88): the kill file's objects killed.
pub fn load_object_kill_data(c: &mut Cpu) {
    let data = kill_file_open_read(c);
    if data != 0 {
        kill_objects(c, data);
    }
}

/// The kill file `data`'s objects (by chunk and number among the chunk's) killed, its chunk's
/// lights near each put out; the file given back.
pub fn kill_objects(c: &mut Cpu, data: u32) {
    let mut k = 0u32;
    while (k as i32) < c.m.r32(data) as i32 {
        let (chunk, object) = kill_file_read_entry(&mut c.m, data, k);
        let m = &mut c.m;
        let chunk = m.r32(CHUNK_LIST).wrapping_add(chunk.wrapping_mul(CHUNK_SIZE));
        let objects = m.r32(chunk + CHUNK_OBJECTS);
        if objects != 0 && (object as i32) < m.r32(objects) as i32 {
            let mut record = objects + 4;
            for _ in 0..(object as i32).max(0) {
                record = record.wrapping_add(sh(m, record) as u32);
            }
            ticks(c, (object as i32).max(1) as u32 - 1);
            let lights = c.m.r32(chunk + CHUNK_LIGHTS);
            if lights != 0 {
                let n = c.m.r32(lights) as i32;
                for j in 0..n.max(0) as u32 {
                    let light = lights + 4 + LIGHT_SIZE * j;
                    let d = call_jal(c, DIST_XZ, &[light, record.wrapping_add(8)]) as i32;
                    if d < KILL_LIGHT_NEAR {
                        c.m.w8(light + 19, LIGHT_OFF);
                    }
                }
                ticks(c, (n.max(0) as u32).saturating_sub(1));
            }
            let flags = c.m.r8(record + 2) | KILLED;
            c.m.w8(record + 2, flags);
        }
        k += 1;
        c.tick();
    }
    call_jal(c, PURGE, &[data]);
}

pub fn abi_load_object_kill_data(c: &mut Cpu) {
    load_object_kill_data(c);
}

/// The surface saves (`SaveSurface`, 8 bytes): how many (+0) of how many it holds (+2), the
/// saves (+4: 8 bytes each, a quad and its flags byte).
const SAVES_COUNT: u32 = 0;
const SAVES_ROOM: u32 = 2;
const SAVES: u32 = 4;

/// `SaveSurface::Save` (TRACK.CPP, 0x800bbce0): `quad`'s flags byte kept (while there's room,
/// one more than it holds as the original lets it).
pub fn save_surface_save(m: &mut Mem, saves: u32, quad: u32) {
    let count = sh(m, saves + SAVES_COUNT);
    if sh(m, saves + SAVES_ROOM) < count || quad == 0 {
        return;
    }
    let at = m.r32(saves + SAVES).wrapping_add((count << 3) as u32);
    m.w32(at, quad);
    let flags = m.r8(quad);
    let at = m.r32(saves + SAVES).wrapping_add((sh(m, saves + SAVES_COUNT) << 3) as u32);
    m.w8(at + 4, flags);
    let count = m.r16(saves + SAVES_COUNT).wrapping_add(1);
    m.w16(saves + SAVES_COUNT, count);
}

pub fn abi_save_surface_save(c: &mut Cpu) {
    save_surface_save(&mut c.m, c.r[4], c.r[5]);
}

/// `SaveSurface::SaveSurface` (TRACK.CPP, 0x800bbd44): room for `n` saves, none kept.
pub fn save_surface_new(c: &mut Cpu, saves: u32, n: u32) -> u32 {
    c.m.w16(saves + SAVES_ROOM, n as u16);
    c.m.w16(saves + SAVES_COUNT, 0);
    let block = call_jal(c, RESERVE, &[SURFACE_SAVE_NAME, n << 3, 0]);
    c.m.w32(saves + SAVES, block);
    saves
}

pub fn abi_save_surface_new(c: &mut Cpu) {
    c.r[2] = save_surface_new(c, c.r[4], c.r[5]);
}

/// `SaveSurface::~SaveSurface` (TRACK.CPP, 0x800bbd88): the saves given back (and the object
/// when `how`'s bit 0 says).
pub fn save_surface_delete(c: &mut Cpu, saves: u32, how: u32) {
    let block = c.m.r32(saves + SAVES);
    call_jal(c, PURGE, &[block]);
    if how & 1 != 0 {
        call_jal(c, DELETE, &[saves]);
    }
}

pub fn abi_save_surface_delete(c: &mut Cpu) {
    save_surface_delete(c, c.r[4], c.r[5]);
}

/// `SaveSurface::RestoreAll` (TRACK.CPP, 0x800bbdd0): each kept quad's flags byte put back, the
/// saves emptied.
pub fn save_surface_restore_all(c: &mut Cpu, saves: u32) {
    let m = &mut c.m;
    let n = sh(m, saves + SAVES_COUNT);
    for k in 0..n.max(0) as u32 {
        let at = m.r32(saves + SAVES).wrapping_add(k << 3);
        let (quad, flags) = (m.r32(at), m.r8(at + 4));
        m.w8(quad, flags);
        let at = m.r32(saves + SAVES).wrapping_add(k << 3);
        m.w32(at, 0);
        let at = m.r32(saves + SAVES).wrapping_add(k << 3);
        m.w8(at + 4, 0);
    }
    m.w16(saves + SAVES_COUNT, 0);
    ticks(c, (n.max(0) as u32).saturating_sub(1));
}

pub fn abi_save_surface_restore_all(c: &mut Cpu) {
    save_surface_restore_all(c, c.r[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{COMPARE_TICKS, Rng, against_original};

    /// `random`'s state (six words).
    const RANDOM_STATE: u32 = 0x8012_478C;

    /// Free RAM (the race leaves it zero): a name, the art's pixmaps, the loading's notes, the
    /// controllers, materials and their records, pixmaps to turn, the objects (definitions and
    /// their table, groups, spheres), the surface saves and quads, a kill file and its chunks, a
    /// shape file.
    const NAME: u32 = 0x8014_0000;
    const PATHS_AT: u32 = 0x8014_0040;
    const ART_BLOCK: u32 = 0x8014_0100;
    const ART_BLOCK_PIXMAPS: u32 = 96;
    const MPAL_BLOCK: u32 = 0x8014_0800;
    const MIP_BLOCK: u32 = 0x8014_0C00;
    const CTRL_BLOCK: u32 = 0x8014_0D80;
    const MATS: u32 = 0x8014_0E00;
    const RECORDS: u32 = 0x8014_0F00;
    const PIXMAP: u32 = 0x8014_1000;
    const PIXMAP_OUT: u32 = 0x8014_1010;
    const DEF_TABLE: u32 = 0x8014_1100;
    const DEFS_AT: u32 = 0x8014_1200;
    const DEF_SIZE: u32 = 0x80;
    const DEFS: u32 = 12;
    const OBJECTS: u32 = 0x8014_1C00;
    const DEF_GROUP: u32 = 0x8014_1E00;
    const OFFSETS: u32 = 0x8014_1E80;
    const SPHERES: u32 = 0x8014_1F00;
    const SAVES_OBJ: u32 = 0x8014_2000;
    const SAVES_BLOCK: u32 = 0x8014_2010;
    const QUADS: u32 = 0x8014_2200;
    const KILL_FILE: u32 = 0x8014_2300;
    const CHUNKS: u32 = 0x8014_2400;
    const CHUNK_OBJECTS_AT: u32 = 0x8014_2800;
    const CHUNK_LIGHTS_AT: u32 = 0x8014_2A00;
    const SHAPE_FILE: u32 = 0x8014_4000;
    const SHAPES_AT: u32 = 0x8014_4400;
    const SHAPE_STEP: u32 = 0x400;
    const PMX_OUT: u32 = 0x8014_C000;
    const RECORD_COUNT: u32 = 12;

    fn pick_i(rng: &mut Rng, lo: i32, hi: i32) -> i32 {
        lo + pick(rng, (hi - lo) as u32) as i32
    }

    /// Two name bytes from a few (so the loading's notes match now and then).
    fn tag(rng: &mut Rng) -> u8 {
        b"ABC"[pick(rng, 3) as usize]
    }

    /// A shape file of `n` shapes: 16- and 256-colour images (16 by 2, 8 by 2) with their
    /// palettes, named plainly, "#", "!" (a palette number 0..2) or "Z" ("ZR" or "ZZ", their
    /// bytes from a few).
    fn shape_file(m: &mut Mem, rng: &mut Rng) {
        let n = 1 + pick(rng, 18);
        m.w32(SHAPE_FILE, u32::from_le_bytes(*b"SHPP"));
        m.w32(SHAPE_FILE + 8, n);
        for k in 0..n {
            let name: [u8; 4] = match pick(rng, 5) {
                0 => [b'a' + pick(rng, 26) as u8, b'b', b'c', b'd'],
                1 => [b'#', b'x', b'y', b'z'],
                2 => [b'!', tag(rng), tag(rng), b'0' + pick(rng, 3) as u8],
                _ => [b'Z', [b'R', b'Z', b'Q'][pick(rng, 3) as usize], tag(rng), tag(rng)],
            };
            m.w32(SHAPE_FILE + 16 + 8 * k, u32::from_le_bytes(name));
            let shape = SHAPES_AT + SHAPE_STEP * k;
            m.w32(SHAPE_FILE + 20 + 8 * k, shape - SHAPE_FILE);
            let big = pick(rng, 3) == 0;
            let (w, bytes) = if big { (8, 16) } else { (16, 16) };
            let palette = shape + 16 + bytes;
            m.w32(shape, if big { 0x41 } else { 0x40 } | (palette - shape) << 8);
            m.w16(shape + 4, w);
            m.w16(shape + 6, 2);
            m.w32(shape + 8, 0);
            m.w32(shape + 12, (pick(rng, 64) | pick(rng, 64) << 16) as u32);
            for j in (0..bytes).step_by(4) {
                m.w32(shape + 16 + j, rng.next() as u32);
            }
            let colours = if big { 256 } else { 16 };
            m.w32(palette, 0x23);
            m.w16(palette + 4, colours as u16);
            m.w16(palette + 6, 1);
            for j in 0..colours {
                m.w16(palette + 16 + 2 * j, rng.next() as u16);
            }
        }
    }

    /// Object definitions (an id, now and then -1; up to 12 vertices) and a group of records
    /// using them (their own sizes).
    fn objects(m: &mut Mem, rng: &mut Rng) {
        m.w32(OBJ_DEFS, DEF_TABLE);
        for k in 0..DEFS {
            let def = DEFS_AT + DEF_SIZE * k;
            m.w32(DEF_TABLE + 4 * k, def);
            m.w16(def, [-1i32, pick_i(rng, 0, 100), -2, -0x8000][pick(rng, 4) as usize] as u16);
            m.w8(def + 2, 1 + pick(rng, 12) as u8);
            for j in 0..12 {
                for axis in 0..3 {
                    m.w16(def + 4 + 8 * j + 2 * axis, pick_i(rng, -30000, 30000) as u16);
                }
            }
        }
        let n = pick(rng, 8);
        m.w32(OBJECTS, n);
        let mut at = OBJECTS + 4;
        for _ in 0..n {
            let size = [8, 12, 20][pick(rng, 3) as usize];
            m.w16(at, size);
            m.w8(at + 2, rng.next() as u8);
            m.w16(at + 6, pick(rng, DEFS) as u16);
            at += size as u32;
        }
        m.w32(DEF_GROUP, 1 + pick(rng, DEFS));
        m.w32(SPHERES, pick(rng, 3));
        for k in 0..DEFS + 2 {
            m.w32(OFFSETS + 4 * k, pick(rng, 0x100));
        }
    }

    /// The race RAM made any track's loading: the track's number, the art (its pixmaps any, how
    /// many made and free), the loading's notes (palettes and mip maps, some alike), mip maps on
    /// or off, the controllers and materials, their records (shapes, flags, uv flags, pictures,
    /// palettes or speeds), objects, surface saves, a kill file's chunks, a shape file.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        m.w32(TRACK_NUMBER, pick(rng, 16));
        for (k, b) in b"Tr.psh\0".iter().enumerate() {
            m.w8(NAME + k as u32, *b);
        }
        // (the tracks' and their data's paths told apart)
        for (path, at, text) in [(TRACK_PATH, PATHS_AT, &b"cd:trk/\0"[..]), (DATA_PATH, PATHS_AT + 16, &b"cd:dat/\0"[..])] {
            for (k, b) in text.iter().enumerate() {
                m.w8(at + k as u32, *b);
            }
            m.w32(path, at);
        }
        // (what the functions fill in, any first)
        for k in 0..16 {
            m.w32(PIXMAP_OUT + 4 * k, rng.next() as u32);
        }
        m.w32(INITIAL_ART + ART_PMX, ART_BLOCK);
        for k in 0..ART_BLOCK_PIXMAPS * 4 {
            m.w32(ART_BLOCK + 4 * k, rng.next() as u32);
        }
        m.w32(INITIAL_ART + ART_COUNT, 30 + pick(rng, 10));
        m.w32(INITIAL_ART + ART_NEXT, 30 + pick(rng, 20));
        m.w32(INITIAL_ART + ART_FIRST_FREE, 40 + pick(rng, 20));
        m.w32(MULTI_PAL_INFO, MPAL_BLOCK);
        for k in 0..MULTI_PALS {
            let e = MPAL_BLOCK + MULTI_PAL_SIZE * k;
            m.w8(e, tag(rng));
            m.w8(e + 1, tag(rng));
            m.w16(e + 2, [-1i32, 0, 1, 2][pick(rng, 4) as usize] as u16);
            m.w16(e + 4, pick(rng, 24) as u16);
            m.w16(e + 6, pick(rng, 40) as u16);
        }
        m.w32(MIP_INFO, [0, MIP_BLOCK][pick(rng, 2) as usize]);
        for k in 0..MIPS {
            let e = MIP_BLOCK + MIP_SIZE * k;
            m.w16(e, if pick(rng, 3) == 0 { 0xFFFF } else { tag(rng) as u16 });
            m.w16(e + 2, if pick(rng, 3) == 0 { 0xFFFF } else { tag(rng) as u16 });
            m.w32(e + 4, pick(rng, 24));
            m.w32(e + 8, pick(rng, 30));
        }
        m.w16(MIP_MAPS_ON, pick(rng, 2) as u16);
        // the controllers and materials
        let n = pick(rng, 8);
        m.w32(CONTROLLERS, CTRL_BLOCK);
        m.w32(CONTROLLER_COUNT, n);
        for k in 0..8 {
            let ctrl = CTRL_BLOCK + CONTROLLER_SIZE * k;
            m.w16(ctrl, [CYCLE, SCROLL, CYCLE | SCROLL, 0][pick(rng, 4) as usize] as u16);
            m.w8(ctrl + 2, 1 + pick(rng, 16) as u8);
            m.w32(ctrl + 4, rng.next() as u32);
            m.w8(ctrl + 8, 1 + pick(rng, 8) as u8);
            m.w8(ctrl + 9, 1 + pick(rng, 8) as u8);
            m.w16(ctrl + 10, pick(rng, 40) as u16);
            m.w32(ctrl + 12, MATS + 4 * pick(rng, RECORD_COUNT));
        }
        for k in 0..RECORD_COUNT {
            m.w32(MATS + 4 * k, (rng.next() as u32) & 0x003F_FFFF);
            let r = RECORDS + RECORDS_AT + RECORD_SIZE * k;
            m.w16(r, pick(rng, 24) as u16);
            m.w8(r + 2, [0, 2, CYCLE as u8, SCROLL as u8, 2 | CYCLE as u8, rng.next() as u8][pick(rng, 6) as usize]);
            m.w8(r + 3, [0, 0x10, 0x40, 2, 4, 8, 0x52, rng.next() as u8][pick(rng, 8) as usize]);
            m.w8(r + 7, pick(rng, 4) as u8);
            m.w8(r + 8, pick(rng, 3) as u8);
        }
        for k in 0..4 {
            m.w32(PIXMAP + 4 * k, rng.next() as u32);
        }
        objects(m, rng);
        // the surface saves: a few kept of some room, the quads
        m.w16(SAVES_OBJ + SAVES_COUNT, pick(rng, 5) as u16);
        m.w16(SAVES_OBJ + SAVES_ROOM, pick(rng, 5) as u16);
        m.w32(SAVES_OBJ + SAVES, SAVES_BLOCK);
        for k in 0..6 {
            m.w32(SAVES_BLOCK + 8 * k, QUADS + 4 * pick(rng, 8));
            m.w8(SAVES_BLOCK + 8 * k + 4, rng.next() as u8);
            m.w32(QUADS + 4 * k, rng.next() as u32);
        }
        // a kill file: entries of chunk and object; the chunks' objects and lights
        let kills = pick(rng, 6);
        m.w32(KILL_FILE, kills);
        for k in 0..kills {
            m.w32(KILL_FILE + 4 + 8 * k, pick(rng, 4));
            m.w32(KILL_FILE + 8 + 8 * k, pick_i(rng, -1, 6) as u32);
        }
        m.w32(CHUNK_LIST, CHUNKS);
        for k in 0..4 {
            let chunk = CHUNKS + CHUNK_SIZE * k;
            let objects = CHUNK_OBJECTS_AT + 0x80 * k;
            m.w32(chunk + CHUNK_OBJECTS, if pick(rng, 4) == 0 { 0 } else { objects });
            m.w32(objects, pick(rng, 6));
            for j in 0..6 {
                let at = objects + 4 + 16 * j;
                m.w16(at, 16);
                m.w8(at + 2, rng.next() as u8);
                for axis in 0..3 {
                    m.w32(at + 8 + 4 * axis, pick_i(rng, -0x8000, 0x8000) as u32);
                }
            }
            let lights = CHUNK_LIGHTS_AT + 0x80 * k;
            m.w32(chunk + CHUNK_LIGHTS, if pick(rng, 3) == 0 { 0 } else { lights });
            m.w32(lights, pick(rng, 5));
            for j in 0..5 {
                let at = lights + 4 + LIGHT_SIZE * j;
                for axis in 0..3 {
                    m.w32(at + 4 * axis, pick_i(rng, -0x8000, 0x8000) as u32);
                }
                m.w8(at + 19, rng.next() as u8);
            }
        }
        shape_file(m, rng);
        // (the library's `random` state)
        for k in 0..6 {
            m.w32(RANDOM_STATE + 4 * k, rng.next() as u32);
        }
    }

    /// A heap block (reserved as the game would) or none.
    fn block(c: &mut Cpu, r: &mut Rng, size: u32) -> u32 {
        if pick(r, 4) == 0 { 0 } else { crate::abi::call(c, nfs4rc::f_800e5f28, &[MIP_INFO_NAME, size, 16]) }
    }

    /// The loading's notes as heap blocks (the function gives them back), copied from the free
    /// RAM ones.
    fn heap_notes(c: &mut Cpu, r: &mut Rng) {
        let mpal = crate::abi::call(c, nfs4rc::f_800e5f28, &[MULTI_PAL_NAME, MULTI_PALS * MULTI_PAL_SIZE, 16]);
        copy_bytes(&mut c.m, mpal, MPAL_BLOCK, MULTI_PALS * MULTI_PAL_SIZE);
        c.m.w32(MULTI_PAL_INFO, mpal);
        let mip = block(c, r, MIPS * MIP_SIZE);
        if mip != 0 {
            copy_bytes(&mut c.m, mip, MIP_BLOCK, MIPS * MIP_SIZE);
        }
        c.m.w32(MIP_INFO, mip);
    }

    /// `Track_LoadObjectKillData` from its kill file loaded (the test CPUs find no file): the
    /// original entered after `KillFile_OpenRead`, its frame and v0 as it has them there.
    fn kill_original(c: &mut Cpu) {
        let sp = c.r[29].wrapping_sub(56);
        c.r[29] = sp;
        c.m.w32(sp + 52, c.r[31]);
        for (k, slot) in (16..23).zip([24, 28, 32, 36, 40, 44, 48]) {
            c.m.w32(sp + slot, c.r[k]);
        }
        c.r[2] = c.r[4];
        nfs4rc::f_800bbb88_at(c, 0x800b_bbb0);
    }

    fn kill_port(c: &mut Cpu, a: &[u32]) -> Option<u32> {
        kill_objects(c, a[0]);
        None
    }

    fn cases() -> Vec<Case> {
        vec![
            ("Track_SetTrackNumber", 0x800b_9fc0, nfs4rc::f_800b9fc0, |_, r| vec![r.next() as u32]),
            ("Track_MakeTrackPathName", 0x800b_9fd0, nfs4rc::f_800b9fd0, |_, _| vec![NAME]),
            ("Track_MakeTrackDataPathName", 0x800b_a018, nfs4rc::f_800ba018, |_, _| vec![NAME]),
            ("AllocArtResource", 0x800b_a060, nfs4rc::f_800ba060, |_, r| vec![PIXMAP_OUT, pick(r, 300)]),
            ("InitArtResources", 0x800b_a0a0, nfs4rc::f_800ba0a0, |_, _| vec![]),
            ("DeInitArtResources", 0x800b_a0d0, nfs4rc::f_800ba0d0, |_, _| vec![]),
            ("LoadShapesAndMakePmx", 0x800b_a0f4, nfs4rc::f_800ba0f4, |_, r| vec![SHAPE_FILE, PMX_OUT, [64, 0, 0x40][pick(r, 3) as usize], [256, 0][pick(r, 2) as usize], 0]),
            ("LoadShapesAndMakePmx_EnvMap", 0x800b_a440, nfs4rc::f_800ba440, |_, _| vec![SHAPE_FILE, PMX_OUT, 992, 0]),
            ("Track_AnimateTextures", 0x800b_a6a4, nfs4rc::f_800ba6a4, |c, r| {
                // (now and then far on in the race)
                if pick(r, 2) == 0 {
                    let t = SIM_GLOBAL.game_ticks(&mut c.m).wrapping_add(pick(r, 100_000) as i32);
                    SIM_GLOBAL.set_game_ticks(&mut c.m, t);
                }
                vec![]
            }),
            ("Track_GetProperMultiPalShapeIndex", 0x800b_a83c, nfs4rc::f_800ba83c, |_, r| vec![pick(r, 24), pick_i(r, -1, 3) as u32]),
            ("Track_ProcessFlipAndUVFlags", 0x800b_a88c, nfs4rc::f_800ba88c, |_, r| vec![[0, 0x10, 0x40, 2, 4, 8, 0x12, 0x5E, r.next() as u32 & 0xFF][pick(r, 9) as usize], PIXMAP, PIXMAP_OUT]),
            ("Track_AssociateSingleMaterial", 0x800b_a9f4, nfs4rc::f_800ba9f4, |_, r| {
                let k = pick(r, RECORD_COUNT);
                vec![RECORDS + RECORDS_AT + RECORD_SIZE * k, MATS + 4 * k, INITIAL_ART]
            }),
            ("Track_LinkMaterials", 0x800b_ab60, nfs4rc::f_800bab60, |c, r| {
                heap_notes(c, r);
                vec![[RECORDS, 0][(pick(r, 8) == 0) as usize], RECORD_SIZE * pick(r, RECORD_COUNT + 1) + pick(r, RECORD_SIZE), MATS]
            }),
            ("ReduceObjectPrecision", 0x800b_af24, nfs4rc::f_800baf24, |_, r| vec![[OBJECTS, 0][(pick(r, 8) == 0) as usize], [DEF_GROUP, 0][(pick(r, 4) == 0) as usize], pick(r, 4)]),
            ("InvalidatePersistentCollideBoomObjects", 0x800b_afc4, nfs4rc::f_800bafc4, |_, r| vec![[OBJECTS, 0][(pick(r, 8) == 0) as usize], [DEF_GROUP, 0][(pick(r, 8) == 0) as usize]]),
            ("CalcObjectBoundingSphere", 0x800b_b030, nfs4rc::f_800bb030, |_, _| vec![DEF_GROUP, SPHERES]),
            ("CalcObjDefPtrs", 0x800b_b290, nfs4rc::f_800bb290, |c, _| {
                c.m.w32(PERSIST_OBJ_DEF, DEF_GROUP);
                c.m.w32(OBJ_DEF_OFFSETS, OFFSETS);
                vec![]
            }),
            ("Track_FlagListedQuads", 0x800b_b2f4, nfs4rc::f_800bb2f4, |_, r| vec![[0, 0, 1, 5][pick(r, 4) as usize]]),
            ("Track_DeInit", 0x800b_ba40, nfs4rc::f_800bba40, |c, r| {
                for at in [LIGHT_TABLE, BOUNDING_SPHERES] {
                    let b = block(c, r, 64);
                    c.m.w32(at, b);
                }
                if pick(r, 3) == 0 {
                    c.m.w32(SAVE_SURFACE, 0);
                }
                vec![]
            }),
            ("Track_AnimateTrackLighting", 0x800b_bb1c, nfs4rc::f_800bbb1c, |_, _| vec![]),
            ("KillFile_ReadEntry", 0x800b_bb68, nfs4rc::f_800bbb68, |_, r| vec![KILL_FILE, pick(r, 6), PIXMAP_OUT, PIXMAP_OUT + 4]),
            ("SaveSurface::Save", 0x800b_bce0, nfs4rc::f_800bbce0, |_, r| vec![SAVES_OBJ, [0, QUADS + 4 * pick(r, 8)][pick(r, 4).min(1) as usize]]),
            ("SaveSurface::SaveSurface", 0x800b_bd44, nfs4rc::f_800bbd44, |_, r| vec![PIXMAP_OUT, pick(r, 64)]),
            ("SaveSurface::~SaveSurface", 0x800b_bd88, nfs4rc::f_800bbd88, |c, r| {
                let b = block(c, r, 64);
                c.m.w32(SAVES_OBJ + SAVES, b);
                vec![SAVES_OBJ, pick(r, 4) & 2]
            }),
            ("SaveSurface::RestoreAll", 0x800b_bdd0, nfs4rc::f_800bbdd0, |_, _| vec![SAVES_OBJ]),
        ]
    }

    #[test]
    fn track_fuzz() {
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            // (the ticks too: it runs while the race loads; but the closest quad's search, a
            // bit-exact port that doesn't tick, and the animation, which runs in the race)
            COMPARE_TICKS.set(!matches!(name, "Track_FlagListedQuads" | "Track_AnimateTextures"));
            let args_rng = std::cell::RefCell::new(Rng(0x7A0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original(name, 300, 0x7A1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

    #[test]
    fn track_kill_file_fuzz() {
        COMPARE_TICKS.set(true);
        against_original("Track_LoadObjectKillData (loaded)", 300, 0x7B1, shuffle, kill_original, &|c| {
            // (the file a heap block: it's given back)
            let n = c.m.r32(KILL_FILE);
            let data = crate::abi::call(c, nfs4rc::f_800e5f28, &[MIP_INFO_NAME, 4 + 8 * n + 8, 0]);
            copy_bytes(&mut c.m, data, KILL_FILE, 4 + 8 * n + 8);
            vec![data]
        }, kill_port);
    }
}
