//! The track's objects as the original draws them (DrawW_DoObjects run for the view with its
//! drawing recorded): the chunks' objects (minus those knocked down, which their animations
//! draw where they fly), the track-wide ones the view's slice shows, and the sim objects.

use super::*;

/// One track object drawn this frame: the model, its rotation and scale, and where.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectDraw {
    /// The track model (the GRP's 0x21/0x08 models, `0x8013E7BC`'s table).
    pub model: u16,
    /// Rotation and scale in PS1 space (`matrixtdef`, rows, 16.16), applied as M * v.
    pub matrix: [i32; 9],
    /// Position in PS1 space (16.16).
    pub pos: [i32; 3],
    /// Turned to face the camera (record byte 5 bit 0): the engine turns it to its own.
    pub billboard: bool,
}

thread_local! {
    /// The objects recorded during [`GameHost::record_objects`], and the model table (object
    /// definition addresses by model) it looks models up in.
    static OBJECT_DRAWS: std::cell::RefCell<Vec<ObjectDraw>> = const { std::cell::RefCell::new(Vec::new()) };
    static MODEL_TABLE: std::cell::RefCell<Vec<u32>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// DrawW_DoObjects(view, build list): every object of the listed chunks (DrawW_BuildChunkObjectFacets),
/// the track-wide groups (DrawW_BuildObjectFacets) and the sim objects (DrawW_BuildCustomObjectFacets).
const DRAWW_DO_OBJECTS: u32 = 0x800C_9A24;
/// BWorld_gChunkBuildList (4 bytes per chunk: index, resolution, flags).
const CHUNK_BUILD_LIST: u32 = 0x8011_02B0;
/// The track's object definitions by model.
const OBJECT_DEFS: u32 = 0x8013_E7BC;
/// DrawObjectTransform(view, dcache, matrix, objdef, pos, light, arg) and
/// DrawObjectSimple(view, dcache, objdef, pos, light): an object's primitives.
const DRAW_OBJECT_TRANSFORM: u32 = 0x800C_8C1C;
const DRAW_OBJECT_SIMPLE: u32 = 0x800C_8F74;
/// ObjectClipped(view, model, pos, dcache): outside a cone of |x| <= z (the PS1's 4:3 view).
const OBJECT_CLIPPED: u32 = 0x800C_9930;
/// Where DrawW_BuildChunkObjectFacets' DrawObjectTransform returns to (its record in s4,
/// the position at record + 8).
const CHUNK_OBJECT_DRAW_RA: u32 = 0x800C_989C;

/// Inner hook while DrawW_DoObjects runs: each object's drawing recorded instead of built,
/// nothing clipped by the original's narrower view, and no glares (recorded elsewhere).
fn record_object(c: &mut Cpu, addr: u32) -> bool {
    let read3 = |c: &mut Cpu, p: u32| std::array::from_fn(|i| c.m.r32(p + 4 * i as u32) as i32);
    let model = |objdef: u32| MODEL_TABLE.with_borrow(|t| t.iter().position(|&d| d == objdef)).map(|m| m as u16);
    match addr {
        OBJECT_CLIPPED | FLARE_HALO | FLARE_HALO2 => {}
        DRAW_OBJECT_TRANSFORM => {
            let (matrix, objdef, pos) = (c.r[6], c.r[7], c.m.r32(c.r[29] + 16));
            let billboard = c.r[31] == CHUNK_OBJECT_DRAW_RA && c.m.r8(pos - 8 + 5) & 1 != 0;
            if let Some(model) = model(objdef) {
                let d = ObjectDraw { model, matrix: std::array::from_fn(|i| c.m.r32(matrix + 4 * i as u32) as i32), pos: read3(c, pos), billboard };
                OBJECT_DRAWS.with_borrow_mut(|o| o.push(d));
            }
        }
        DRAW_OBJECT_SIMPLE => {
            let (objdef, pos) = (c.r[6], c.r[7]);
            if let Some(model) = model(objdef) {
                let one = 0x1_0000;
                let d = ObjectDraw { model, matrix: [one, 0, 0, 0, one, 0, 0, 0, one], pos: read3(c, pos), billboard: false };
                OBJECT_DRAWS.with_borrow_mut(|o| o.push(d));
            }
        }
        _ => return false,
    }
    c.r[2] = 0;
    true
}

impl GameHost {
    /// The objects the original draws in the view set up for this frame (its build list
    /// filled): DrawW_DoObjects as Render_RenderWorld runs it, recorded. It also runs the
    /// knocked-down objects' animations, which move on as they are drawn.
    pub(super) fn record_objects(&mut self, c: &mut Cpu) -> Vec<ObjectDraw> {
        let table = c.m.r32(OBJECT_DEFS);
        MODEL_TABLE.with_borrow_mut(|t| {
            t.clear();
            if table != 0 {
                t.extend((0..1024).map(|i| c.m.r32(table + 4 * i)));
            }
        });
        OBJECT_DRAWS.with_borrow_mut(Vec::clear);
        c.inner_hook = Some(record_object);
        c.call_nested(DRAWW_DO_OBJECTS, &[RENDER_VIEW, CHUNK_BUILD_LIST]);
        c.inner_hook = None;
        OBJECT_DRAWS.take()
    }
}
