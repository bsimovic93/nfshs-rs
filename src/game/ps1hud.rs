//! The in-race HUD as the original draws it (Hud_RenderHudView, Hud_BuildNumbers0,
//! Hud_BuildNumbers), rebuilt: the layout Hud_Init prepared (sprite slots, panel quads,
//! shape table) comes from the simulation, the per-frame values from DashHUD_gInfo, and the
//! drawing uses the original ZHUD.PSH shapes and ZFONT.PFN glyphs. Coordinates are the PS1's
//! 320x240 screen.

use crate::ps1::host::OverlayCmd;
use crate::assets::{Atlas, GameData, Rect, Vertex};
use crate::formats::psh::Psh;
use crate::ps1::race::{HudSetup, MapView};
use anyhow::{Context, Result};
use glam::Vec2;
use std::sync::Arc;

/// HudPmx_InitTextures' load order; `HudPmx_gShapes[i]` is `SHAPE_NAMES[i]`.
const SHAPE_NAMES: &[&str] = &[
    "cmra", "cam0", "cam1", "cam2", "cam3", "cam4", "cam5", "cam6", "cam7", "cam8", "cam9", "ca10", "ca11", "ca12", "crn1", "crn2", "crn3", "crn4",
    "arrl", "arrr", "mwt1", "mwt2", "mwb1", "mwb2", "msid", "msi2", "kmhh", "mphh", "gea1", "gea2", "gea3", "gea4", "gea5", "gea6", "smln", "smlr",
    "pau0", "pau1", "pau2", "pau3", "rwid", "rwi2", "rsid", "rsi2", "big0", "big1", "big2", "big3", "big4", "big5", "big6", "big7", "big8",
    "big9", "sml1", "sml2", "sml3", "sml4", "sml5", "sml6", "321d", "321l", "mpbt", "smlx", "sml+", "sml-", "sml:", "sml\"", "sml'", "sml,",
    "sml.", "sml/", "ahyp", "acol", "a192", "a193", "a194", "wrng", "a196", "a197", "a199", "a200", "a201", "a203", "a204", "a205", "a206",
    "a207", "a209", "a210", "a211", "a212", "a214", "a217", "a218", "a219", "a220", "A229", "time", "timb", "laps", "ejct", "play", "paus",
    "resg", "rad0", "rad1", "s1/4", "s1/2", "spd2", "spd1", "plus", "minu", "plr1", "oppn", "barl", "barr", "barb", "baXl", "baXr", "tacc",
    "<dial>", "tpno",
    // the HUD's small font (Hud_BuildString) and the pad button symbols
    "alpA", "alpB", "alpC", "alpD", "alpE", "alpF", "alpG", "alpH", "alpI", "alpJ", "alpK", "alpL", "alpM", "alpN", "alpO", "alpP", "alpQ",
    "alpR", "alpS", "alpT", "alpU", "alpV", "alpW", "alpX", "alpY", "alpZ", "alp0", "alp1", "alp2", "alp3", "alp4", "alp5", "alp6", "alp7",
    "alp8", "alp9", "alTR", "alCI", "alSQ", "negA", "negB", "neg2", "alUP",
];
/// The tachometer dial per car (HudPmx_InitTextures' day and night tables; 29 and up: "bon").
const DIALS_DAY: [&str; 29] = [
    "tslk", "tbz3", "thsv", "tfor", "tz28", "ttra", "tdb7", "txkr", "ttm5", "tvet", "t550", "t911", "tf50", "tdsv", "tclk", "tmcf", "t911", "thsv",
    "tvet", "tbon", "tbon", "tbon", "tcap", "thsv", "ttm5", "tvet", "t911", "tdsv", "tbon",
];
const DIALS_NIGHT: [&str; 29] = [
    "nslk", "nbz3", "nhsv", "nfor", "nz28", "ntra", "ndb7", "nxkr", "nnm5", "nvet", "n550", "n911", "nf50", "tdsv", "tclk", "nmcf", "n911", "nhsv",
    "nvet", "nbon", "nbon", "nbon", "ncap", "nhsv", "nnm5", "nvet", "n911", "tdsv", "nbon",
];
/// Flare_gOct: the halo's octagon (x, y per 256 of its size).
const FLARE_OCT: [(i32, i32); 8] = [(0, -256), (180, -180), (256, 0), (180, 180), (0, 256), (-180, 180), (-256, 0), (-180, -180)];
/// Flare_gType: (centre colour, spike colour) per flare type, 0xBBGGRR.
const FLARE_TYPES: [(u32, u32); 12] = [
    (0x808080, 0x808080),
    (0x303030, 0x808080),
    (0x080868, 0x060628),
    (0x808080, 0x303030),
    (0x406FC0, 0x0808C0),
    (0x406FC0, 0x0808C0),
    (0x707070, 0x101018),
    (0x080880, 0x080828),
    (0x080880, 0x080828),
    (0x088080, 0x082828),
    (0x800808, 0x280808),
    (0x303030, 0x808080),
];
/// Hud_Character: the countdown digits on a 5x5 grid of lights (bit 5 * row + column).
const COUNTDOWN_DIGITS: [u32; 5] = [0x4255C4, 0x423108, 0xF0BA1C, 0x743A1C, 0];

/// What the HUD shows this frame (from the simulation's state).
pub struct HudFrame<'a> {
    /// DashHUD_gInfo
    pub dash: &'a [u8],
    /// The layout Hud_Init chose (`*(gp + 6352)`).
    pub layout: &'a [u8],
    pub map: &'a MapView,
    /// Phase of the blinking elements.
    pub blink: bool,
    /// The start countdown's stage and the ticks since it began.
    pub countdown: u8,
    pub countdown_ticks: i32,
    /// The pause menu and other 2D drawing the game recorded this frame.
    pub overlay: &'a [OverlayCmd],
}

/// A ZFONT.PFN glyph: character code, size, place in the font picture, advance and offset.
#[derive(Clone, Copy)]
struct Glyph {
    code: u16,
    w: u16,
    h: u16,
    u: u16,
    v: u16,
    advance: i8,
    dx: i8,
    dy: i8,
}

const BIG0: usize = 44;
const SML0: usize = 53; // sml1 = 54

/// A texture area in VRAM terms (u, v, clut) and where its picture is in the atlas.
#[derive(Clone, Copy)]
struct Tex {
    u: u16,
    v: u16,
    clut: u16,
    w: u16,
    h: u16,
    rect: Rect,
}

pub struct Ps1Hud {
    setup: Arc<HudSetup>,
    textures: Vec<Tex>,
    /// `HudPmx_gShapes`: (u0, v0, clut, w, h).
    shapes: Vec<(u16, u16, u16, u16, u16)>,
    /// 1x1 white with the semi-transparency bit (alpha 128).
    white_semi: Rect,
    glyphs: Vec<Glyph>,
    /// The font picture in each of the font's 16 colours (Font_TextColor).
    font_pages: Vec<Rect>,
    atlas_size: f32,
}

fn u16le(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}
fn i16le(d: &[u8], o: usize) -> i16 {
    i16::from_le_bytes([d[o], d[o + 1]])
}
fn i32le(d: &[u8], o: usize) -> i32 {
    i32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

/// One 2D primitive in PS1 screen space.
struct Quad {
    pts: [Vec2; 4],
    uv: [Vec2; 4],
    rect: Rect,
    colors: [[u8; 3]; 4],
    textured: bool,
    semi: bool,
    /// Semi-transparency mode 1 (background + foreground) instead of mode 0 (halves).
    additive: bool,
    /// Drawing area (x0, y0, x1, y1) the primitive is clipped to, for the small views.
    clip: Option<[f32; 4]>,
}

/// rmult: 16.16 fixed-point multiply, rounded.
fn rmult(a: i32, b: i32) -> i32 {
    ((a as i64 * b as i64 + 0x8000) >> 16) as i32
}

/// fixedsincos: sine and cosine (16.16) of an angle in 16.16 turns.
fn fixed_sin_cos(a: i32) -> (i32, i32) {
    let t = a as f64 / 65536.0 * std::f64::consts::TAU;
    ((t.sin() * 65536.0).round() as i32, (t.cos() * 65536.0).round() as i32)
}

/// A polygon corner: position, colour, texture coordinate.
type Corner = (Vec2, [f32; 3], Vec2);

/// Clip a convex polygon to a rectangle (Sutherland-Hodgman).
fn clip_polygon(pts: Vec<Corner>, r: [f32; 4]) -> Vec<Corner> {
    let mut poly = pts;
    let edges: [(usize, f32, bool); 4] = [(0, r[0], true), (0, r[2], false), (1, r[1], true), (1, r[3], false)];
    for (axis, at, keep_above) in edges {
        let inside = |p: Vec2| if keep_above { p[axis] >= at } else { p[axis] <= at };
        let mut out = Vec::new();
        for i in 0..poly.len() {
            let (a, ca, ta) = poly[i];
            let (b, cb, tb) = poly[(i + 1) % poly.len()];
            if inside(a) {
                out.push((a, ca, ta));
            }
            if inside(a) != inside(b) {
                let t = (at - a[axis]) / (b[axis] - a[axis]);
                let c = [ca[0] + (cb[0] - ca[0]) * t, ca[1] + (cb[1] - ca[1]) * t, ca[2] + (cb[2] - ca[2]) * t];
                out.push((a + (b - a) * t, c, ta + (tb - ta) * t));
            }
        }
        poly = out;
        if poly.is_empty() {
            break;
        }
    }
    poly
}

impl Ps1Hud {
    pub fn new(data: &GameData, atlas: &mut Atlas, setup: Arc<HudSetup>, track: u32) -> Result<Self> {
        let psh = Psh::parse(&data.read("ZHUD.PSH")?)?;
        let shapes: Vec<(u16, u16, u16, u16, u16)> = setup
            .shapes
            .as_chunks::<20>().0.iter()
            .map(|e| (e[0] as u16, e[1] as u16, u16le(e, 2), u16le(e, 16), u16le(e, 18)))
            .collect();
        // HudPmx_gShapes follows the load order in SHAPE_NAMES, except that shapes missing from
        // the file are replaced by "wrng" (36x36) and the track map comes late: align by size
        let car = i32le(&setup.car_info, 0);
        let dial = match (setup.night, usize::try_from(car).ok().filter(|&c| c < 29)) {
            (false, Some(c)) => DIALS_DAY[c],
            (true, Some(c)) => DIALS_NIGHT[c],
            (false, None) => "tbon",
            (true, None) => "nbon",
        };
        let shape_names: Vec<&str> = SHAPE_NAMES.iter().map(|&n| if n == "<dial>" { dial } else { n }).collect();
        let size = |n: &str| psh.shapes.iter().find(|s| s.name == n).map(|s| (s.width, s.height));
        let mut names: Vec<Option<String>> = Vec::new();
        let mut j = 0;
        for &(_, _, _, w, h) in &shapes {
            if w == 0 {
                names.push(None);
                continue;
            }
            match (j..(j + 16).min(shape_names.len())).find(|&k| size(shape_names[k]) == Some((w, h))) {
                Some(k) => {
                    names.push(Some(shape_names[k].to_string()));
                    j = k + 1;
                }
                None if (w, h) == (36, 36) => names.push(Some("wrng".into())),
                None if (w, h) == (48, 48) => names.push(Some(format!("mp{track:02}"))),
                None => names.push(None),
            }
        }
        let mut textures = Vec::new();
        for (&(u, v, clut, w, h), name) in shapes.iter().zip(&names) {
            let Some(name) = name else { continue };
            let Some(sh) = psh.shapes.iter().find(|s| s.name.eq_ignore_ascii_case(name)) else { continue };
            if sh.width != w || sh.height != h {
                continue;
            }
            let rect = atlas.add(w as u32, h as u32, &sh.rgba);
            textures.push(Tex { u, v, clut, w, h, rect });
        }
        // the font (ZFONT.PFN): glyph table at 32, one 4-bit picture at 0x690; it sits 128
        // lines down in its texture page, with the digits' palette
        let pfn = data.read("ZFONT.PFN")?;
        let count = u16le(&pfn, 10) as usize;
        let img_at = i32le(&pfn, 28) as usize;
        // a 4-bit picture after a 16-byte shape header; its palette is the one the game built
        let (fw, fh) = (u16le(&pfn, img_at + 4) as usize, u16le(&pfn, img_at + 6) as usize);
        let pixels = pfn.get(img_at + 16..img_at + 16 + fw * fh / 2).context("font picture")?;
        let rgba: Vec<u8> = (0..fw * fh)
            .flat_map(|i| {
                let b = pixels[i / 2];
                let idx = if i % 2 == 0 { b >> 4 } else { b & 15 };
                crate::formats::psh::ps1_color(setup.font_clut[idx as usize])
            })
            .collect();
        let font_rect = atlas.add(fw as u32, fh as u32, &rgba);
        let font_pages = setup
            .font_palettes
            .iter()
            .map(|pal| {
                let rgba: Vec<u8> = (0..fw * fh)
                    .flat_map(|i| {
                        let b = pixels[i / 2];
                        let idx = if i % 2 == 0 { b >> 4 } else { b & 15 };
                        crate::formats::psh::ps1_color(pal[idx as usize])
                    })
                    .collect();
                atlas.add(fw as u32, fh as u32, &rgba)
            })
            .collect();
        let font_clut = (setup.number_uv[0] >> 16) as u16;
        let mut glyphs = Vec::with_capacity(count);
        for i in 0..count {
            let g = &pfn[32 + 11 * i..32 + 11 * i + 11];
            glyphs.push(Glyph { code: u16le(g, 0), w: g[2] as u16, h: g[3] as u16, u: u16le(g, 4), v: u16le(g, 6), advance: g[8] as i8, dx: g[9] as i8, dy: g[10] as i8 });
            let (w, h, u, v) = (g[2] as u16, g[3] as u16, u16le(g, 4), u16le(g, 6));
            if w == 0 || h == 0 {
                continue;
            }
            let rect = Rect { x: font_rect.x + u as u32, y: font_rect.y + v as u32, w: w as u32, h: h as u32 };
            textures.push(Tex { u, v: v + 128, clut: font_clut, w, h, rect });
        }
        let white_semi = atlas.add(1, 1, &[255, 255, 255, 128]);
        Ok(Self { setup, textures, shapes, white_semi, glyphs, font_pages, atlas_size: atlas.size as f32 })
    }

    fn find(&self, u: u16, v: u16, clut: u16) -> Option<(Tex, u16, u16)> {
        self.textures.iter().find(|t| t.clut == clut && u >= t.u && u < t.u + t.w && v >= t.v && v < t.v + t.h).map(|t| (*t, u - t.u, v - t.v))
    }

    fn slot(&self, i: usize) -> &[u8] {
        &self.setup.slots[i * 20..i * 20 + 20]
    }

    /// A SPRT slot as Hud_Init (and the per-frame code) left it; `uv_clut` overrides its texture.
    fn sprite(&self, out: &mut Vec<Quad>, i: usize, uv_clut: Option<u32>) {
        let s = self.slot(i);
        let code = s[7];
        let (x, y) = (i16le(s, 8) as f32, i16le(s, 10) as f32);
        let w = u16le(s, 16);
        let h = u16le(s, 18);
        let (u, v, clut) = match uv_clut {
            Some(t) => ((t & 0xFF) as u16, ((t >> 8) & 0xFF) as u16, (t >> 16) as u16),
            None => (s[12] as u16, s[13] as u16, u16le(s, 14)),
        };
        self.textured(out, x, y, w, h, u, v, clut, [[s[4], s[5], s[6]]; 4], code & 2 != 0);
    }

    #[allow(clippy::too_many_arguments)]
    fn textured(&self, out: &mut Vec<Quad>, x: f32, y: f32, w: u16, h: u16, u: u16, v: u16, clut: u16, colors: [[u8; 3]; 4], semi: bool) {
        self.textured_clipped(out, x, y, w, h, u, v, clut, colors, semi, None);
    }

    #[allow(clippy::too_many_arguments)]
    fn textured_clipped(
        &self,
        out: &mut Vec<Quad>,
        x: f32,
        y: f32,
        w: u16,
        h: u16,
        u: u16,
        v: u16,
        clut: u16,
        colors: [[u8; 3]; 4],
        semi: bool,
        clip: Option<[f32; 4]>,
    ) {
        let Some((t, du, dv)) = self.find(u, v, clut) else {
            if std::env::var_os("NFSHS_HUD_DEBUG").is_some() {
                println!("hud: no texture at uv ({u},{v}) clut {clut:04x} ({w}x{h} at {x},{y})");
            }
            return;
        };
        let (w, h) = (w.min(t.w - du) as f32, h.min(t.h - dv) as f32);
        let (u0, v0) = (du as f32, dv as f32);
        out.push(Quad {
            pts: [Vec2::new(x, y), Vec2::new(x + w, y), Vec2::new(x + w, y + h), Vec2::new(x, y + h)],
            uv: [Vec2::new(u0, v0), Vec2::new(u0 + w, v0), Vec2::new(u0 + w, v0 + h), Vec2::new(u0, v0 + h)],
            rect: t.rect,
            colors,
            textured: true,
            semi,
            additive: false,
            clip,
        });
    }

    /// Shape `i` of HudPmx_gShapes at (x, y), as a gouraud quad (Hud_BuildGT4).
    fn shape(&self, out: &mut Vec<Quad>, i: usize, x: f32, y: f32, colors: [[u8; 3]; 4]) {
        let Some(&(u, v, clut, w, h)) = self.shapes.get(i) else { return };
        self.textured(out, x, y, w, h, u, v, clut, colors, false);
    }

    fn panel(&self, out: &mut Vec<Quad>, k: usize) {
        // POLY_G4: tag, (rgb code, xy) x 4
        let p = &self.setup.panels[k * 36..k * 36 + 36];
        let pt = |j: usize| Vec2::new(i16le(p, 8 + 8 * j) as f32, i16le(p, 10 + 8 * j) as f32);
        let col = |j: usize| [p[4 + 8 * j], p[5 + 8 * j], p[6 + 8 * j]];
        let pts = [pt(0), pt(1), pt(3), pt(2)];
        out.push(Quad { pts, uv: [Vec2::ZERO; 4], rect: self.white_semi, colors: [col(0), col(1), col(3), col(2)], textured: false, semi: p[7] & 2 != 0, additive: false, clip: None });
    }

    fn cap(&self, out: &mut Vec<Quad>, k: usize) {
        // POLY_F4: tag, rgb code, xy x 4
        let p = &self.setup.caps[k * 24..k * 24 + 24];
        let pt = |j: usize| Vec2::new(i16le(p, 8 + 4 * j) as f32, i16le(p, 10 + 4 * j) as f32);
        let c = [p[4], p[5], p[6]];
        out.push(Quad { pts: [pt(0), pt(1), pt(3), pt(2)], uv: [Vec2::ZERO; 4], rect: self.white_semi, colors: [c; 4], textured: false, semi: p[7] & 2 != 0, additive: false, clip: None });
    }

    /// A flat polygon (POLY_F3 as a quad with its last point repeated, or a LINE_F2 as a thin quad).
    fn flat(&self, out: &mut Vec<Quad>, pts: [Vec2; 4], rgb: u32, semi: bool, clip: Option<[f32; 4]>) {
        let c = [rgb as u8, (rgb >> 8) as u8, (rgb >> 16) as u8];
        out.push(Quad { pts, uv: [Vec2::ZERO; 4], rect: self.white_semi, colors: [c; 4], textured: false, semi, additive: false, clip });
    }

    /// LINE_F2: the pixels from `a` to `b` inclusive, as a one pixel wide quad.
    fn line(&self, out: &mut Vec<Quad>, a: Vec2, b: Vec2, rgb: u32, semi: bool, clip: Option<[f32; 4]>) {
        let d = (b - a).normalize_or(Vec2::X) * 0.5;
        let n = Vec2::new(-d.y, d.x);
        self.flat(out, [a - d - n, b + d - n, b + d + n, a - d + n], rgb, semi, clip);
    }

    /// Hud_RenderTacView / Hud_BuildTach: the tachometer is a 28x28 view of its own (at
    /// (184, 21), Hud_CreateHudViews) showing the piece of the car's 86x85 dial around the
    /// needle's tip, so the dial turns behind a fixed needle.
    fn tach(&self, out: &mut Vec<Quad>, rpm: i32) {
        let (vx, vy) = (184.0, 21.0);
        let clip = Some([vx, vy, vx + 28.0, vy + 28.0]);
        let a = (rpm.wrapping_shl(16) / 10800 + 22937).clamp(22937, 0x13334);
        let (sn, cs) = fixed_sin_cos(a);
        let s = self.slot(2);
        let u = (29 + rmult(cs, 29)) as u16;
        let v = (157 + rmult(sn, 29)) as u16;
        let (x, y) = (vx + i16le(s, 8) as f32, vy + i16le(s, 10) as f32);
        self.textured(out, x, y, u16le(s, 16), u16le(s, 18), u, v, u16le(s, 14), [[s[4], s[5], s[6]]; 4], s[7] & 2 != 0);
        // the needle: a thin triangle from the dial's centre through the window, its shadow
        // two lines down, and a semi-transparent line along it
        let p = |x: i32, y: i32| Vec2::new(vx + x as f32, vy + y as f32);
        let base = p(14 + rmult(cs, 10), 14 + rmult(sn, 10));
        let tip = |da: i32| {
            let (s, c) = fixed_sin_cos(a + da);
            p(14 - rmult(c, 32), 14 - rmult(s, 32))
        };
        let (t0, t1) = (tip(-512), tip(512));
        let down = Vec2::new(0.0, 2.0);
        self.flat(out, [base + down, t0 + down, t1 + down, t1 + down], 0, false, clip);
        let car = i32le(&self.setup.car_info, 0).clamp(0, 29) as usize;
        let color = self.setup.needle_colors[car];
        self.line(out, p(14 - rmult(cs, 29), 14 - rmult(sn, 29)), base, color.wrapping_add(0x48_4848) & 0xFF_FFFF, true, clip);
        self.flat(out, [base, t0, t1, t1], color, false, clip);
    }

    /// Hud_RenderMapView: the track map, a 45x48 view at (255, 172) (Hud_CreateHudViews),
    /// with a marker per car.
    fn map(&self, out: &mut Vec<Quad>, map: &MapView) {
        let (vx, vy) = (255.0, 172.0);
        let clip = Some([vx, vy, vx + 45.0, vy + 48.0]);
        // the map: POLY_FT4 (tag, rgb code, then xy + uv (+ clut / tpage) per corner)
        if let [_, _, _, _, r, g, b, code, ..] = map.prim[..] {
            let p = &map.prim;
            let pt = |k: usize| Vec2::new(vx + i16le(p, 8 + 8 * k) as f32, vy + i16le(p, 10 + 8 * k) as f32);
            let uv = |k: usize| (p[12 + 8 * k] as u16, p[13 + 8 * k] as u16);
            let clut = u16le(p, 14);
            let (u0, v0) = (0..4).map(uv).fold((u16::MAX, u16::MAX), |(a, b), (u, v)| (a.min(u), b.min(v)));
            if code & 0xFC == 0x2C {
                if let Some((t, du, dv)) = self.find(u0, v0, clut) {
                    let tc = |k: usize| {
                        let (u, v) = uv(k);
                        Vec2::new((du + u - u0) as f32, (dv + v - v0) as f32)
                    };
                    out.push(Quad {
                        pts: [pt(0), pt(1), pt(3), pt(2)],
                        uv: [tc(0), tc(1), tc(3), tc(2)],
                        rect: t.rect,
                        colors: [[r, g, b]; 4],
                        textured: true,
                        semi: code & 2 != 0,
                        additive: false,
                        clip,
                    });
                }
            }
        }
        // the markers, last linked drawn first
        for m in map.markers.iter().rev() {
            let Some(&(u, v, clut, w, h)) = self.shapes.get(m.shape as usize) else { continue };
            let c = [m.color as u8, (m.color >> 8) as u8, (m.color >> 16) as u8];
            self.textured_clipped(out, vx + m.x as f32, vy + m.y as f32, w, h, u, v, clut, [c; 4], false, clip);
        }
    }

    /// Flare_2DHalo: a glow of the given flare type around (x, y), additive: a gouraud
    /// octagon from the centre colour to black and a spike to each corner.
    fn halo(&self, out: &mut Vec<Quad>, x: i32, y: i32, sx: i32, sy: i32, ty: usize) {
        let (c0, c1) = FLARE_TYPES[ty % FLARE_TYPES.len()];
        let rgb = |c: u32| [c as u8, (c >> 8) as u8, (c >> 16) as u8];
        let centre = Vec2::new(x as f32, y as f32);
        // (the product rounded towards zero, as the original)
        let corner = |k: usize| Vec2::new((x + FLARE_OCT[k].0 * sx / 65536) as f32, (y + FLARE_OCT[k].1 * sy / 65536) as f32);
        for k in 0..8 {
            let (a, b) = (corner(k), corner((k + 1) % 8));
            out.push(Quad {
                pts: [b, centre, a, a],
                uv: [Vec2::ZERO; 4],
                rect: self.white_semi,
                colors: [[0; 3], rgb(c0), [0; 3], [0; 3]],
                textured: false,
                semi: true,
                additive: true,
                clip: None,
            });
            let d = (a - centre).normalize_or(Vec2::X) * 0.5;
            let n = Vec2::new(-d.y, d.x);
            out.push(Quad {
                pts: [centre - n, a + d - n, a + d + n, centre + n],
                uv: [Vec2::ZERO; 4],
                rect: self.white_semi,
                colors: [rgb(c1), [0; 3], [0; 3], rgb(c1)],
                textured: false,
                semi: true,
                additive: true,
                clip: None,
            });
        }
    }

    /// Hud_BlackThinBox: a black frame 2 pixels wide with rounded corners (crn1..4).
    fn black_thin_box(&self, out: &mut Vec<Quad>, x: f32, y: f32, w: f32, h: f32) {
        let (cw, ch) = self.shapes.get(17).map(|s| (s.3 as f32, s.4 as f32)).unwrap_or((7.0, 7.0));
        let rect = |out: &mut Vec<Quad>, x: f32, y: f32, w: f32, h: f32| {
            self.flat(out, [Vec2::new(x, y), Vec2::new(x + w, y), Vec2::new(x + w, y + h), Vec2::new(x, y + h)], 0, false, None);
        };
        rect(out, x + cw - 2.0, y - 2.0, w - 2.0 * cw + 2.0, 2.0);
        rect(out, x - 2.0, y + ch - 2.0, 2.0, h - 2.0 * ch + 4.0);
        rect(out, x + w - 2.0, y + ch - 2.0, 2.0, h - 2.0 * ch + 4.0);
        rect(out, x + cw - 2.0, y + h, w - 2.0 * cw + 2.0, 2.0);
        let black = [[0; 3]; 4];
        self.shape(out, 14, x - 2.0, y - 2.0, black);
        self.shape(out, 15, x + w - cw, y - 2.0, black);
        self.shape(out, 16, x - 2.0, y + h - ch + 2.0, black);
        self.shape(out, 17, x + w - cw, y + h - ch + 2.0, black);
    }

    /// Font_TextXY: each character's glyph at (x + its x offset, y + its y offset), then
    /// advance; characters below '!' advance by the space width.
    fn text(&self, out: &mut Vec<Quad>, x: i16, y: i16, colour: u8, space: u8, text: &[u8]) {
        let Some(&page) = self.font_pages.get(colour as usize) else { return };
        let mut x = x as i32;
        for &ch in text {
            if ch < 33 {
                x += space as i32;
                continue;
            }
            let code = ch as u16;
            let direct = self.glyphs.get(ch as usize - 32).filter(|g| g.code == code);
            let Some(g) = direct.or_else(|| self.glyphs.iter().find(|g| g.code == code)) else { continue };
            if g.w != 0 && g.h != 0 {
                let (px, py) = ((x + g.dx as i32) as f32, (y as i32 + g.dy as i32) as f32);
                let (w, h) = (g.w as f32, g.h as f32);
                let (u, v) = (g.u as f32, g.v as f32);
                out.push(Quad {
                    pts: [Vec2::new(px, py), Vec2::new(px + w, py), Vec2::new(px + w, py + h), Vec2::new(px, py + h)],
                    uv: [Vec2::new(u, v), Vec2::new(u + w, v), Vec2::new(u + w, v + h), Vec2::new(u, v + h)],
                    rect: page,
                    colors: [[0x80; 3]; 4],
                    textured: true,
                    semi: false,
                    additive: false,
                    clip: None,
                });
            }
            x += g.advance as i32;
        }
    }

    /// Hud_RenderPauseBox: a dark red semi-transparent box under a black title band, framed
    /// by Hud_BlackThinBox, with black lower corners and bottom edge.
    fn pause_box(&self, out: &mut Vec<Quad>, x: f32, y: f32, w: f32, h: f32) {
        let (cw, ch) = self.shapes.get(17).map(|s| (s.3 as f32, s.4 as f32)).unwrap_or((7.0, 7.0));
        let black = [[0; 3]; 4];
        let rect = |out: &mut Vec<Quad>, x: f32, y: f32, w: f32, h: f32, rgb: u32, semi: bool| {
            self.flat(out, [Vec2::new(x, y), Vec2::new(x + w, y), Vec2::new(x + w, y + h), Vec2::new(x, y + h)], rgb, semi, None);
        };
        // in the reverse of the order the game adds them to the ordering table
        rect(out, x, y + 12.0, w, h - 12.0, 0x46_1414, true);
        rect(out, x, y, w, 12.0, 0, false);
        self.black_thin_box(out, x, y, w, h);
        rect(out, x, y + h - 2.0, w, 2.0, 0, false);
        self.shape(out, 17, x + w - cw, y + h - ch, black);
        self.shape(out, 16, x - 2.0, y + h - ch, black);
        self.shape(out, 15, x + w - cw, y + 10.0, black);
        self.shape(out, 14, x - 2.0, y + 10.0, black);
    }

    /// The recorded overlay. The game adds each primitive at the head of its ordering table
    /// slot, so what it asked for last is drawn first.
    fn overlay(&self, out: &mut Vec<Quad>, cmds: &[OverlayCmd]) {
        for c in cmds.iter().rev() {
            match c {
                &OverlayCmd::PauseBox { x, y, w, h } => self.pause_box(out, x as f32, y as f32, w as f32, h as f32),
                &OverlayCmd::Rect { semi, x, y, w, h, colour, dx0, dx1 } => {
                    let (x, y, w, h) = (x as f32, y as f32, w as f32, h as f32);
                    let pts = [Vec2::new(x + dx0 as f32, y), Vec2::new(x + w + dx1 as f32, y), Vec2::new(x + w, y + h), Vec2::new(x, y + h)];
                    self.flat(out, pts, colour, semi, None);
                }
                OverlayCmd::Text { x, y, colour, space, text } => self.text(out, *x, *y, *colour, *space, text),
                &OverlayCmd::Shape { shape, x, y, colour } => {
                    if let Some(&(u, v, clut, w, h)) = self.shapes.get(shape as usize) {
                        let rgb = [colour as u8, (colour >> 8) as u8, (colour >> 16) as u8];
                        self.textured(out, x as f32, y as f32, w, h, u, v, clut, [rgb; 4], true);
                    }
                }
                &OverlayCmd::ThinBox { x, y, w, h } => self.black_thin_box(out, x as f32, y as f32, w as f32, h as f32),
                OverlayCmd::Quad { .. } | OverlayCmd::Clip { .. } | OverlayCmd::Movie { .. } | OverlayCmd::ShowroomCar { .. } | OverlayCmd::ShowroomFloor(_) | OverlayCmd::Bitmap { .. } => {}
            }
        }
    }

    /// Hud_Render321Go / Hud_Draw321Num: the start lights, three panels of 5x5 lamps
    /// showing the count, the lit lamps glowing brightly when the number changes.
    fn countdown(&self, out: &mut Vec<Quad>, stage: u8, ticks: i32, y: f32) {
        if !(1..=3).contains(&stage) {
            return;
        }
        let glow = if (0..100).contains(&ticks) { 6000 - 60 * ticks } else { 0 };
        let bits = COUNTDOWN_DIGITS[(4 - stage) as usize];
        for x in [72.0, 135.0, 198.0] {
            let corners = [Vec2::new(x - 3.0, y - 2.0), Vec2::new(x + 53.0, y - 2.0), Vec2::new(x + 53.0, y + 47.0), Vec2::new(x - 3.0, y + 47.0)];
            self.flat(out, corners, 0, false, None);
            for k in 0..25 {
                let (c, r) = ((k % 5) as f32, (k / 5) as f32);
                let lit = bits >> k & 1 != 0;
                self.shape(out, if lit { 61 } else { 60 }, x + 10.0 * c + 1.0, y + 9.0 * r + 1.0, [[0x80; 3]; 4]);
            }
            self.black_thin_box(out, x - 3.0, y - 2.0, 56.0, 49.0);
            if glow != 0 {
                for k in 0..25 {
                    if bits >> k & 1 != 0 {
                        let (c, r) = (k % 5, k / 5);
                        self.halo(out, x as i32 + 10 * c + 4, y as i32 + 9 * r + 4, glow, glow, 6);
                    }
                }
            }
        }
    }

    /// Hud_BuildETimeString ("MM:SS.hh", 8 slots) / Hud_BuildTimeString ("M:SS.hh", 7 slots):
    /// only the digits' glyphs change; the time is in 1/64 s.
    fn time(&self, out: &mut Vec<Quad>, first: usize, t: i32, long: bool) {
        let t = t.max(0);
        let secs = t >> 6;
        let hund = ((t - (secs << 6)) * 100) >> 6;
        let (m, s) = (secs / 60, secs % 60);
        let digits: Vec<Option<i32>> = if long {
            vec![Some(m / 10 % 10), Some(m % 10), None, Some(s / 10), Some(s % 10), None, Some(hund / 10), Some(hund % 10)]
        } else {
            vec![Some(m % 10), None, Some(s / 10), Some(s % 10), None, Some(hund / 10), Some(hund % 10)]
        };
        for (k, d) in digits.iter().enumerate() {
            let uv = d.map(|d| self.setup.number_uv[d as usize]);
            self.sprite(out, first + k, uv);
        }
    }

    /// The HUD for player 1 from this frame's DashHUD_gInfo.
    fn build(&self, f: &HudFrame) -> Vec<Quad> {
        let (dash, layout, map, blink) = (f.dash, f.layout, f.map, f.blink);
        let mut out = Vec::new();
        if dash.len() < 0x60 || layout.len() < 0x48 {
            return out;
        }
        let d = |o: usize| i32le(dash, o);
        let l = |o: usize| i16le(layout, o) as f32;
        let ci = |o: usize| i32le(&self.setup.car_info, o);
        if std::env::var_os("NFSHS_HUD_DEBUG").is_some() {
            println!("hud: lap {}/{} position {}/{} best {} rpm {} countdown {} +{}", d(56), d(60), d(84), d(88), d(92), d(64), f.countdown, f.countdown_ticks);
        }
        // Hud_BuildNumbers0: race timer
        if ci(0x84) != 0 {
            self.panel(&mut out, 0);
            self.cap(&mut out, 0);
            for k in 4..6 {
                self.sprite(&mut out, k, None);
            }
            if d(20) == 0 || !blink {
                self.time(&mut out, 12, d(52), true);
            }
        }
        // Hud_BuildNumbers: lap counter (only for races of more than one lap)
        if ci(0x88) != 0 && d(60) != 1 {
            self.panel(&mut out, 1);
            self.cap(&mut out, 1);
            for k in 6..8 {
                self.sprite(&mut out, k, None);
            }
            self.sprite(&mut out, 20, Some(self.setup.number_uv[d(56).clamp(0, 9) as usize]));
            self.sprite(&mut out, 21, None);
            self.sprite(&mut out, 22, Some(self.setup.number_uv[d(60).clamp(0, 9) as usize]));
        }
        // best lap
        let best = d(92);
        if ci(0x84) != 0 && best != 0 && best <= 0x95FF && d(60) != 1 {
            self.panel(&mut out, 3);
            self.cap(&mut out, 2);
            for k in 8..10 {
                self.sprite(&mut out, k, None);
            }
            self.time(&mut out, 23, best, false);
        }
        // race position "p/n"
        let (pos, cars) = (d(84), d(88));
        if ci(0x8C) != 0 && cars >= 2 {
            let w = |i: usize| self.shapes.get(i).map(|s| s.3 as f32).unwrap_or(0.0);
            let big = BIG0 + pos.clamp(0, 9) as usize;
            let x = l(56) + l(40) + w(BIG0) - 2.0 - w(big);
            let s = self.slot(37);
            if let Some(&(u, v, clut, sw, sh)) = self.shapes.get(big) {
                self.textured(&mut out, x, i16le(s, 10) as f32, sw, sh, u, v, clut, [[s[4], s[5], s[6]]; 4], s[7] & 2 != 0);
            }
            self.sprite(&mut out, 38, None);
            let small = SML0 + cars.clamp(1, 6) as usize;
            let s = self.slot(39);
            if let Some(&(u, v, clut, sw, sh)) = self.shapes.get(small) {
                self.textured(&mut out, i16le(s, 8) as f32, i16le(s, 10) as f32, sw, sh, u, v, clut, [[s[4], s[5], s[6]]; 4], s[7] & 2 != 0);
            }
        }
        // speedometer: units, gear and the big digits
        if ci(0x78) != 0 {
            // the frame of speedometer and tachometer ("tacc")
            self.sprite(&mut out, 1, None);
            self.sprite(&mut out, if ci(0x70) == 0 { 50 } else { 49 }, None);
            let gear = d(72);
            let slot = match gear {
                0 => 48,
                1 => 47,
                g => (39 + g).clamp(41, 46) as usize,
            };
            self.sprite(&mut out, slot, None);
            // speed in display units (rmult of the unit factor and the 16.16 speed)
            let speed = rmult(ci(0x74), d(76));
            let n = (speed.max(0) >> 16).min(999);
            let (h, t, o) = (n / 100, n / 10 % 10, n % 10);
            let w = |i: usize| self.shapes.get(i).map(|s| s.3 as f32).unwrap_or(0.0);
            let step = w(BIG0) + 1.0;
            let mut x = l(4) + l(48) + 4.0 + 2.0 * step;
            let y = l(6) + l(50);
            let grad = [[0xC8; 3], [0xC8; 3], [0x50; 3], [0x50; 3]];
            self.shape(&mut out, BIG0 + o as usize, x, y, grad);
            x -= match t {
                1 => ((step + w(BIG0 + 1)) as i32 >> 1) as f32,
                7 => ((step + w(BIG0 + 7)) as i32 >> 1) as f32,
                _ => step,
            };
            if h != 0 || t != 0 {
                self.shape(&mut out, BIG0 + t as usize, x, y, grad);
            }
            if t == 1 {
                x -= step - ((step + w(BIG0 + 1)) as i32 >> 1) as f32;
            }
            x = x - 1.0 - w(BIG0 + h as usize);
            if h != 0 {
                self.shape(&mut out, BIG0 + h as usize, x, y, grad);
            }
            // Hud_RenderTacView: when the player's tachometer is on (DashHUD_gInfo +28)
            if d(28) != 0 {
                self.tach(&mut out, d(64));
            }
        }
        // the track map and its frame (slots 63..70)
        if ci(0x7C) != 0 {
            if d(28) != 0 {
                self.map(&mut out, map);
            }
            for k in (63..71).rev() {
                self.sprite(&mut out, k, None);
            }
        }
        self.countdown(&mut out, f.countdown, f.countdown_ticks, l(70));
        self.overlay(&mut out, f.overlay);
        out
    }

    /// Vertices for the HUD, mapping the PS1 screen (320x240) into a `w` x `h` window.
    pub fn vertices(&self, frame: &HudFrame, w: f32, h: f32) -> Vec<Vertex> {
        let atlas_uv = |r: Rect, t: Vec2| [(r.x as f32 + t.x) / self.atlas_size, (r.y as f32 + t.y) / self.atlas_size];
        let quads = self.build(frame);
        // 4:3 area centred in the window
        let scale = (h / 240.0).min(w / 320.0);
        let ox = (w - 320.0 * scale) / 2.0;
        let oy = (h - 240.0 * scale) / 2.0;
        let mut v = Vec::with_capacity(quads.len() * 6);
        for q in quads {
            // textured: colour / 128 modulates the texel (x2 in the shader); flat: the colour itself
            let div = if q.textured { 256.0 } else { 510.0 };
            let alpha = match (q.semi, q.additive) {
                (false, _) => 1.0,
                (true, false) => 0.5,
                (true, true) => 0.0,
            };
            // the PS1 fills a rectangle's pixels [x0, x1): no offset at any scale
            let shift = 0.0;
            let mut vert = |p: Vec2, c: [f32; 3], t: Vec2| {
                let p = p + Vec2::splat(shift);
                v.push(Vertex {
                    pos: [ox + p.x * scale, oy + p.y * scale, 0.0],
                    uv: atlas_uv(q.rect, if q.textured { t } else { Vec2::new(0.5, 0.5) }),
                    color: [c[0] / div, c[1] / div, c[2] / div, alpha],
                });
            };
            let color = |k: usize| q.colors[k].map(|c| c as f32);
            match q.clip {
                Some(r) => {
                    let poly = clip_polygon((0..4).map(|k| (q.pts[k], color(k), q.uv[k])).collect(), r);
                    for k in 1..poly.len().saturating_sub(1) {
                        for (p, c, t) in [poly[0], poly[k], poly[k + 1]] {
                            vert(p, c, t);
                        }
                    }
                }
                None => {
                    for k in [0, 1, 2, 0, 2, 3] {
                        vert(q.pts[k], color(k), q.uv[k]);
                    }
                }
            }
        }
        v
    }
}
