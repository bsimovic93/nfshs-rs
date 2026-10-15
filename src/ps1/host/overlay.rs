//! The 2D the original draws (front end, HUD, glows), recorded as overlay commands: its
//! ordering tables' GPU packets and the drawing calls the host takes over.

use super::*;

/// 2D drawing the game asked for this frame outside the HUD (pause menu and the like):
/// recorded from its calls, drawn by the host.
#[derive(Clone, Debug)]
pub enum OverlayCmd {
    /// Font_TextXY: text at (x, y) in font colour `colour` (Font_TextColor index); `space`
    /// is the font's space width.
    Text { x: i16, y: i16, colour: u8, space: u8, text: Vec<u8> },
    /// Hud_RenderPauseBox(x, y, w, h).
    PauseBox { x: i16, y: i16, w: i16, h: i16 },
    /// Hud_FBuildF4(semi, x, y, w, h, colour 0xBBGGRR, top-left / top-right x offsets).
    Rect { semi: bool, x: i16, y: i16, w: i16, h: i16, colour: u32, dx0: i8, dx1: i8 },
    /// Hud_FBuildSprite: HudPmx_gShapes[shape] at (x, y) modulated by colour (0xBBGGRR).
    Shape { shape: u8, x: i16, y: i16, colour: u32 },
    /// Hud_BlackThinBox(x, y, w, h).
    ThinBox { x: i16, y: i16, w: i16, h: i16 },
    /// A screen-space quad (front end): corners in order top-left, top-right, bottom-left,
    /// bottom-right with their colours (0xBBGGRR, 0x80 = 1 on textured quads), optionally
    /// textured from VRAM; `abr` is the semi-transparency mode (0 half, 1 add, 2 subtract,
    /// 3 add a quarter).
    Quad { pts: [[i16; 2]; 4], colours: [u32; 4], tex: Option<QuadTex>, semi: bool, abr: u8 },
    /// SetDrawArea in the front end: drawing is clipped to this screen rectangle.
    Clip { x: i16, y: i16, w: i16, h: i16 },
    /// Movie_Play: frame `frame` of the movie file `file` (under MOVIES) fills the screen.
    Movie { file: String, frame: u32, fps: f32 },
    /// The showroom's car (R3DCar_InsertCarFacetMenuII): the car, the camera (the view's
    /// position and rotation rows, 16.16 / 4096 scaled as in races) and the GTE projection
    /// (distance H, screen centre OFX/OFY 16.16); `mirrored` is its reflection in the floor.
    ShowroomCar { car: Box<crate::ps1::race::CarState>, parts: Vec<u8>, cam_pos: [i32; 3], cam_rot: [i32; 9], h: u32, ofx: i32, ofy: i32, mirrored: bool, level: u32, env: Option<CarEnv> },
    /// The showroom's turntable and spot (R3DCar_Showroom): primitives in the first slot of
    /// the car cache's ordering table, drawn before the cars (in this list's order, as the
    /// overlay: last first).
    ShowroomFloor(Vec<OverlayCmd>),
    /// Draw_DrawDirectScreen: a picture put straight onto the screen at (x, y), opaque.
    Bitmap { x: i16, y: i16, w: u16, h: u16, rgba: std::sync::Arc<Vec<u8>> },
}

/// A quad's texture in VRAM terms: texture page and CLUT ids, texel coordinates per corner.
#[derive(Clone, Copy, Debug)]
pub struct QuadTex {
    pub tpage: u16,
    pub clut: u16,
    /// 0..=256: a sprite's right or bottom edge can be the page's end (its texels wrap at 256,
    /// see `fe::texel`).
    pub uv: [[u16; 2]; 4],
}

impl OverlayCmd {
    /// The command moved by (dx, dy) on the screen.
    pub fn offset(&self, dx: i16, dy: i16) -> OverlayCmd {
        let mut c = self.clone();
        if dx == 0 && dy == 0 {
            return c;
        }
        match &mut c {
            OverlayCmd::Text { x, y, .. }
            | OverlayCmd::PauseBox { x, y, .. }
            | OverlayCmd::Rect { x, y, .. }
            | OverlayCmd::Shape { x, y, .. }
            | OverlayCmd::ThinBox { x, y, .. }
            | OverlayCmd::Clip { x, y, .. }
            | OverlayCmd::Bitmap { x, y, .. } => {
                *x += dx;
                *y += dy;
            }
            OverlayCmd::Quad { pts, .. } => {
                for p in pts {
                    p[0] += dx;
                    p[1] += dy;
                }
            }
            OverlayCmd::ShowroomFloor(v) => {
                for c in v {
                    *c = c.offset(dx, dy);
                }
            }
            OverlayCmd::Movie { .. } | OverlayCmd::ShowroomCar { .. } => {}
        }
        c
    }
}

/// An axis-aligned rectangle's corners in quad order (top-left, top-right, bottom-left,
/// bottom-right).
pub(super) fn rect_pts(x: i32, y: i32, w: i32, h: i32) -> [[i16; 2]; 4] {
    [[x as i16, y as i16], [(x + w) as i16, y as i16], [x as i16, (y + h) as i16], [(x + w) as i16, (y + h) as i16]]
}

impl GameHost {
    /// PS1_MISSLOG: the primitives in the front end's ordering table (from `start`) that no
    /// recording took, by code and first vertex (drawing the original made that is not shown).
    pub(super) fn report_unrecorded(&mut self, c: &mut Cpu, start: u32) {
        let mut missed = std::collections::BTreeMap::<(u8, i16, i16), u32>::new();
        let mut builders = std::collections::BTreeSet::<String>::new();
        let (mut a, mut n) = (start, 0);
        while n < 20_000 {
            let w = c.m.r32(a);
            let len = w >> 24;
            if len > 0 && !self.recorded_packets.contains(&a) {
                let code = (c.m.r32(a + 4) >> 24) as u8;
                if !(0xE1..=0xE6).contains(&code) && code != 0 {
                    // (the first vertex: after the colour word)
                    let v = c.m.r32(a + 8);
                    *missed.entry((code, v as i16, (v >> 16) as i16)).or_default() += 1;
                    if let Some(m) = self.prim_marks.iter().find(|m| m.0 > a) {
                        builders.insert(m.1.iter().map(|f| format!("{f:#x}")).collect::<Vec<_>>().join("<"));
                    }
                }
            }
            n += 1;
            if w & 0xFF_FFFF == 0xFF_FFFF {
                break;
            }
            a = 0x8000_0000 | (w & 0xFF_FFFF);
        }
        if std::env::var("PS1_MISSLOG").as_deref() == Ok("2") {
            eprintln!("[{:>8}] ordering table {start:#x}: {n} entries, {} recorded packets", self.clock, self.recorded_packets.len());
        }
        if !missed.is_empty() {
            let list: Vec<String> = missed.iter().take(24).map(|((code, x, y), k)| format!("{code:#04x}@{x},{y}x{k}")).collect();
            eprintln!("[{:>8}] unrecorded front-end primitives: {} kinds: {}", self.clock, missed.len(), list.join(" "));
            eprintln!("[{:>8}]   built in (innermost first): {}", self.clock, builders.into_iter().collect::<Vec<_>>().join(" | "));
        }
        self.recorded_packets.clear();
        self.prim_marks.clear();
    }

    /// Packets in an ordering table.
    pub(super) fn ot_len(c: &mut Cpu, start: u32) -> u32 {
        let (mut a, mut n) = (start, 0);
        while n < 20_000 {
            let next = c.m.r32(a) & 0xFF_FFFF;
            n += 1;
            if next == 0xFF_FFFF {
                break;
            }
            a = 0x8000_0000 | next;
        }
        n
    }

    /// Walk an ordering table from `start` and record its primitives (a race's HUD view).
    pub(super) fn record_ot(&mut self, c: &mut Cpu, start: u32) {
        let before = self.overlay.len();
        let mut a = start;
        self.prim_offset = [0, 0];
        // each list starts unclipped (its own drawing environment, if any, clips it)
        self.overlay.push(OverlayCmd::Clip { x: 0, y: 0, w: 1024, h: 512 });
        for _ in 0..20_000 {
            let tag = c.m.r32(a);
            let (len, next) = (tag >> 24, tag & 0xFF_FFFF);
            if len > 0 {
                let code = c.m.r8(a + 7);
                // a view's drawing environment (SetDrawEnv's packet): the offset from its E5
                // word, the clip SetDrawEnv was given for that offset (the packet's own area
                // words are not used by the game)
                let e5 = (code >= 0xE0).then(|| (0..len).map(|k| c.m.r32(a + 4 + 4 * k)).find(|w| w >> 24 == 0xE5)).flatten();
                if let Some(e5) = e5 {
                    let sx = |v: u32| ((v as i32) << 21) >> 21;
                    let ofs = [sx(e5 & 0x7FF), sx((e5 >> 11) & 0x7FF)];
                    let [ox, oy] = self.draw_origin;
                    let clip = self.draw_envs.values().find(|(_, o)| o[0] as i32 == ofs[0] && o[1] as i32 == ofs[1]).map(|e| e.0);
                    match clip {
                        Some(cl) => self.overlay.push(OverlayCmd::Clip { x: (cl[0] as i32 - ox) as i16, y: (cl[1] as i32 - oy) as i16, w: cl[2], h: cl[3] }),
                        None => self.overlay.push(OverlayCmd::Clip { x: 0, y: 0, w: 1024, h: 512 }),
                    }
                    self.prim_offset = [(ofs[0] - ox) as i16, (ofs[1] - oy) as i16];
                    for k in 0..len {
                        let w = c.m.r32(a + 4 + 4 * k);
                        if w >> 24 == 0xE1 {
                            self.draw_mode = w as u16 & 0x1FF;
                        }
                    }
                } else if code >= 0xE0 {
                    // environment commands, several to a packet
                    for k in 0..len {
                        let w = c.m.r32(a + 4 + 4 * k);
                        self.environment(w);
                    }
                } else {
                    self.parse_prim(c, a + 4, len);
                }
            }
            if next == 0xFF_FFFF {
                break;
            }
            a = 0x8000_0000 | next;
        }
        if std::env::var_os("NFSHS_HUDLOG").is_some() {
            eprintln!("hud view {start:#x}: {} commands, display {:?}", self.overlay.len() - before, self.display);
            if std::env::var("NFSHS_HUDLOG").as_deref() == Ok("2") {
                for o in &self.overlay[before..] {
                    eprintln!("    {}", format!("{o:?}").chars().take(160).collect::<String>());
                }
            }
        }
    }

    /// GP0 E1..E6 in a recorded view: draw mode, drawing area (a clip), drawing offset.
    pub(super) fn environment(&mut self, w: u32) {
        let [ox, oy] = self.draw_origin;
        match w >> 24 {
            0xE1 => self.draw_mode = w as u16 & 0x1FF,
            0xE3 => self.area_tl = [(w & 0x3FF) as i32 - ox, ((w >> 10) & 0x3FF) as i32 - oy],
            0xE4 => {
                let (x1, y1) = ((w & 0x3FF) as i32 - ox, ((w >> 10) & 0x3FF) as i32 - oy);
                let [x0, y0] = self.area_tl;
                self.overlay.push(OverlayCmd::Clip { x: x0 as i16, y: y0 as i16, w: (x1 - x0 + 1) as i16, h: (y1 - y0 + 1) as i16 });
            }
            0xE5 => {
                let sx = |v: u32| ((v as i32) << 21) >> 21;
                self.prim_offset = [(sx(w & 0x7FF) - ox) as i16, (sx((w >> 11) & 0x7FF) - oy) as i16];
            }
            _ => {}
        }
    }

    /// The front end's shape emitter (0x8004E74C) as quads: the shape is cut into strips that
    /// stay inside one texture page, each a gouraud-textured quad.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn shape_quads(&mut self, c: &mut Cpu, info: u32, flags: u32, x: i32, y: i32, colours: [u32; 4], abr: u32) {
        let clut = c.m.r32(info + 4) as u16;
        let bpp = (c.m.r8(info + 8) as i32).max(1);
        let mode = (c.m.r8(info + 9) & 3) as u16;
        let w = c.m.r16(info + 16) as i16 as i32;
        let h0 = c.m.r16(info + 18) as i16 as i32;
        let vx = c.m.r16(info + 24) as i32;
        let w26 = c.m.r16(info + 26);
        let (mut top, mut h) = (y, h0);
        let mut v0 = (w26 & 0xFF) as i32;
        if flags & 2 != 0 {
            top += h0;
            h = -h0;
            v0 -= 1;
        }
        let bottom = top + h;
        let mut s1 = 0;
        while s1 < w {
            let first = (vx << 4) / bpp;
            let col = vx + ((s1 * bpp) >> 4);
            let page = ((col & !63) << 4) / bpp;
            let u = s1 + first - page;
            let mut width = (255 - u).min(w - s1);
            let tpage = (mode << 7) | (((abr & 3) as u16) << 5) | ((w26 & 0x100) >> 4) | (((col & 0x3C0) >> 6) as u16) | ((w26 & 0x200) << 2);
            let (mut u0, mut extra) = (u, 0);
            if flags & 4 != 0 && h0 < 255 {
                u0 = u - 1;
                extra = 1;
            }
            if width <= 0 {
                width = 1;
            }
            let (xa, xb) = if flags & 4 != 0 { (w + x - s1 + extra - 1, w + x - (s1 + width) + extra - 1) } else { (s1 + x, s1 + x + width) };
            let (va, ua) = ((v0 & 0xFF) as u16, (u0 & 0xFF) as u16);
            let (vb, ub) = ((va as i32 + h0).max(0) as u16, ua + width as u16);
            self.overlay.push(OverlayCmd::Quad {
                pts: [[xa as i16, top as i16], [xb as i16, top as i16], [xa as i16, bottom as i16], [xb as i16, bottom as i16]],
                colours,
                tex: Some(QuadTex { tpage, clut, uv: [[ua, va], [ub, va], [ua, vb], [ub, vb]] }),
                semi: flags & 1 != 0,
                abr: ((tpage >> 5) & 3) as u8,
            });
            s1 += width;
        }
    }

    /// Font_TextXY in the front end: the blitter's glyphs (of the current font's 4-bit
    /// picture) as quads. Font_Blit: sprites in CLUT gp+4880 and colour gp+4872, blended;
    /// FontUpsideDownBlit (the menu's reflection in the showroom floor): flipped opaque quads
    /// in its own CLUT and colour.
    pub(super) fn font_quads(&mut self, c: &mut Cpu, text: u32, x: i32, y: i32) {
        let upside_down = match c.m.r32(FONT_BLITTER) {
            FONT_UPSIDE_DOWN_BLIT => true,
            FONT_BLIT => false,
            b => {
                log::warn!("front-end text with an unknown blitter {b:#x}");
                false
            }
        };
        let table = c.m.r32(0x8013_6F14);
        let count = c.m.r32(0x8013_6F04);
        let shape = c.m.r32(0x8013_6F18);
        if table == 0 || shape == 0 {
            return;
        }
        let pos = c.m.r32(shape + 12);
        let (fx, fy) = ((pos & 0xFFF) as u16, ((pos >> 16) & 0xFFF) as u16);
        let tpage = (fx / 64) | ((fy / 256) << 4);
        let (clut, colour) = if upside_down {
            (c.m.r16(0x8013_EB4C), c.m.r32(0x8013_EB44) & 0xFF_FFFF)
        } else {
            (c.m.r16(FONT_CURRENT_CLUT), c.m.r32(0x8013_D83C + 4872) & 0xFF_FFFF)
        };
        let space = c.m.r8(FONT_SPACE) as i32;
        let code = |c: &mut Cpu, a: u32| c.m.r8(a) as u16 | (c.m.r8(a + 1) as u16) << 8;
        let mut x = x;
        for i in 0..256 {
            let ch = c.m.r8(text + i) as u16;
            if ch == 0 {
                break;
            }
            if ch < 33 {
                x += space;
                continue;
            }
            let k = ch as u32 - 32;
            let direct = table + 11 * k;
            let g = if k < count && code(c, direct) == ch { Some(direct) } else { (0..count).map(|k| table + 11 * k).find(|&a| code(c, a) == ch) };
            let Some(g) = g else { continue };
            let (w, h) = (c.m.r8(g + 2) as i32, c.m.r8(g + 3) as i32);
            let (u, v) = (code(c, g + 4) as i32, code(c, g + 6) as i32);
            let (adv, dx, dy) = (c.m.r8(g + 8) as i8 as i32, c.m.r8(g + 9) as i8 as i32, c.m.r8(g + 10) as i8 as i32);
            if w > 0 && h > 0 && upside_down {
                // the glyph's top row (from one texel above it) at the bottom, around y + 5
                let (u0, v0) = ((u & 0xFF) as u16, ((((fy as i32 + v) & 0xFF) - 1) & 0xFF) as u16);
                let (u1, v1) = (u0 + w as u16, v0 + h as u16);
                let (top, bottom) = (y + 5 - h - dy, y + 5 - dy);
                let (l, r) = (x + dx, x + dx + w);
                self.overlay.push(OverlayCmd::Quad {
                    pts: [[l, bottom], [r, bottom], [l, top], [r, top]].map(|[a, b]| [a as i16, b as i16]),
                    colours: [colour; 4],
                    tex: Some(QuadTex { tpage, clut, uv: [[u0, v0], [u1, v0], [u0, v1], [u1, v1]] }),
                    semi: false,
                    abr: 0,
                });
            } else if w > 0 && h > 0 {
                let (u0, v0) = ((u & 0xFF) as u16, ((fy as i32 + v) & 0xFF) as u16);
                let (u1, v1) = (u0 + w as u16, v0 + h as u16);
                self.overlay.push(OverlayCmd::Quad {
                    pts: rect_pts(x + dx, y + dy, w, h),
                    colours: [colour; 4],
                    tex: Some(QuadTex { tpage, clut, uv: [[u0, v0], [u1, v0], [u0, v1], [u1, v1]] }),
                    semi: true,
                    abr: 0,
                });
            }
            x += adv;
        }
    }

    /// Run a front-end drawing routine that builds its own primitives (DrawTV and friends) and
    /// record what it appended to the primitive buffer (scratchpad 0x1F800004) as quads, in the
    /// order it built them.
    pub(super) fn capture_prims(&mut self, c: &mut Cpu, addr: u32) -> u32 {
        let start = c.m.r32(0x1F80_0004);
        let args = [c.r[4], c.r[5], c.r[6], c.r[7]];
        // arguments past the fourth are on the caller's stack: copy them to where the nested
        // call's frame (see Cpu::call_nested) has them
        let (sp, nested_sp) = (c.r[29], (c.r[29] - 0x100) & !7);
        for i in 0..16 {
            let w = c.m.r32(sp + 16 + 4 * i);
            c.m.w32(nested_sp + 16 + 4 * i, w);
        }
        self.capturing = true;
        let ret = c.call_nested(addr, &args);
        self.capturing = false;
        let end = c.m.r32(0x1F80_0004);
        self.record_packets(c, start, end);
        ret
    }

    /// Record the GPU packets built in the primitive buffer from `start` to `end`.
    pub(super) fn record_packets(&mut self, c: &mut Cpu, start: u32, end: u32) {
        let mut packets = Vec::new();
        let mut p = start;
        while p < end && end - p < 0x10_0000 {
            let len = c.m.r32(p) >> 24;
            if len == 0 {
                p += 4;
                continue;
            }
            packets.push((p, len));
            self.recorded_packets.insert(p);
            p += 4 + 4 * len;
        }
        // the packets go into the ordering table head first, so they are drawn in reverse:
        // a draw mode (DR_MODE, E1) applies to the packets added before it. Resolve each
        // packet's mode in drawing order, then record in the order added.
        let entry_mode = self.draw_mode;
        let mut modes = vec![entry_mode; packets.len()];
        let mut mode = entry_mode;
        for (i, &(p, _)) in packets.iter().enumerate().rev() {
            let w0 = c.m.r32(p + 4);
            if w0 >> 24 == 0xE1 {
                mode = w0 as u16 & 0x1FF;
            }
            modes[i] = mode;
        }
        for (i, &(p, len)) in packets.iter().enumerate() {
            self.draw_mode = modes[i];
            if c.m.r32(p + 4) >> 24 != 0xE1 {
                self.parse_prim(c, p + 4, len);
            }
        }
        self.draw_mode = entry_mode;
    }

    /// Points moved by the recorded view's drawing offset.
    pub(super) fn offset_pts(&self, pts: [[i16; 2]; 4]) -> [[i16; 2]; 4] {
        pts.map(|[x, y]| [x + self.prim_offset[0], y + self.prim_offset[1]])
    }

    /// One GPU primitive (its command words at `a`) as a quad, as far as the front end uses them.
    pub(super) fn parse_prim(&mut self, c: &mut Cpu, a: u32, len: u32) {
        let w = |c: &mut Cpu, k: u32| c.m.r32(a + 4 * k);
        let w0 = w(c, 0);
        let code = (w0 >> 24) as u8;
        let semi = code & 2 != 0;
        let raw = code & 1 != 0;
        let xy = |v: u32| [v as u16 as i16, (v >> 16) as u16 as i16];
        match code {
            0xE1 => self.draw_mode = w0 as u16 & 0x1FF,
            0x20..=0x3F => {
                let textured = code & 4 != 0;
                let gouraud = code & 0x10 != 0;
                let n = if code & 8 != 0 { 4 } else { 3 };
                // words per vertex: colour (gouraud, after the first), xy, uv (textured)
                let per = 1 + u32::from(textured) + u32::from(gouraud);
                let mut pts = [[0i16; 2]; 4];
                let mut colours = [w0 & 0xFF_FFFF; 4];
                let mut uv = [[0u16; 2]; 4];
                let (mut clut, mut tpage) = (0u16, self.draw_mode);
                for k in 0..n {
                    let base = 1 + per * k as u32;
                    if gouraud && k > 0 {
                        colours[k] = w(c, base - 1) & 0xFF_FFFF;
                    }
                    pts[k] = xy(w(c, base));
                    if textured {
                        let t = w(c, base + 1);
                        uv[k] = [t as u8 as u16, (t >> 8) as u8 as u16];
                        match k {
                            0 => clut = (t >> 16) as u16,
                            1 => tpage = (t >> 16) as u16,
                            _ => {}
                        }
                    }
                }
                if len < per * n as u32 {
                    return;
                }
                if n == 3 {
                    pts[3] = pts[2];
                    colours[3] = colours[2];
                    uv[3] = uv[2];
                }
                if textured {
                    self.draw_mode = tpage & 0x1FF;
                }
                if textured && raw {
                    colours = [0x80_8080; 4];
                }
                let abr = (if textured { tpage } else { self.draw_mode } >> 5) as u8 & 3;
                let tex = textured.then_some(QuadTex { tpage, clut, uv });
                let pts = self.offset_pts(pts);
                self.overlay.push(OverlayCmd::Quad { pts, colours, tex, semi, abr });
            }
            0x60..=0x7F => {
                // TILE / SPRT (variable or 1, 8, 16 pixels): the draw mode's texture page
                let textured = code & 4 != 0;
                let [x, y] = xy(w(c, 1));
                let (tw, th): (i32, i32);
                let (mut uv0, mut clut) = ([0u16; 2], 0u16);
                let mut k = 2;
                if textured {
                    let t = w(c, 2);
                    uv0 = [t as u8 as u16, (t >> 8) as u8 as u16];
                    clut = (t >> 16) as u16;
                    k = 3;
                }
                match (code >> 3) & 3 {
                    0 => {
                        let s = w(c, k);
                        tw = (s & 0xFFFF) as i32;
                        th = (s >> 16) as i32;
                    }
                    1 => (tw, th) = (1, 1),
                    2 => (tw, th) = (8, 8),
                    _ => (tw, th) = (16, 16),
                }
                let (x, y) = (x as i32, y as i32);
                let colours = if textured && raw { [0x80_8080; 4] } else { [w0 & 0xFF_FFFF; 4] };
                let tex = textured.then(|| {
                    let (u0, v0) = (uv0[0], uv0[1]);
                    // (its last texels can be the page's last: the edge is then 256)
                    let (u1, v1) = (u0 + tw as u16, v0 + th as u16);
                    QuadTex { tpage: self.draw_mode, clut, uv: [[u0, v0], [u1, v0], [u0, v1], [u1, v1]] }
                });
                let abr = (self.draw_mode >> 5) as u8 & 3;
                let pts = self.offset_pts(rect_pts(x, y, tw, th));
                self.overlay.push(OverlayCmd::Quad { pts, colours, tex, semi, abr });
            }
            0x40..=0x5F if code & 8 == 0 => {
                // a line (one segment): a pixel-wide quad
                let gouraud = code & 0x10 != 0;
                let (c0, p0) = (w0 & 0xFF_FFFF, xy(w(c, 1)));
                let (c1, p1) = if gouraud { (w(c, 2) & 0xFF_FFFF, xy(w(c, 3))) } else { (c0, xy(w(c, 2))) };
                let (dx, dy) = ((p1[0] - p0[0]) as f32, (p1[1] - p0[1]) as f32);
                let n = (dx * dx + dy * dy).sqrt().max(1.0);
                let (nx, ny) = ((-dy / n * 0.5).round() as i16, (dx / n * 0.5).round() as i16);
                let (nx, ny) = if nx == 0 && ny == 0 { (0, 1) } else { (nx, ny) };
                let pts = [[p0[0], p0[1]], [p1[0], p1[1]], [p0[0] + nx, p0[1] + ny], [p1[0] + nx, p1[1] + ny]];
                let abr = (self.draw_mode >> 5) as u8 & 3;
                let pts = self.offset_pts(pts);
                self.overlay.push(OverlayCmd::Quad { pts, colours: [c0, c1, c0, c1], tex: None, semi, abr });
            }
            _ => {
                if std::env::var_os("PS1_PRIMLOG").is_some() {
                    eprintln!("prim {code:#04x} len {len} not drawn");
                }
            }
        }
    }
}
