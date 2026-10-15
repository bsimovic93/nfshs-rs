//! Races: the HUD's helpers and each player view's set-up (Render_RenderPlayerView), the
//! world itself left to the engine.

use super::*;

impl GameHost {
    /// Races: the HUD's helpers and each player view's set-up (Render_RenderPlayerView), the world left to the engine. `None`: not one of these (or the original's own code runs).
    pub(super) fn race_call(&mut self, c: &mut Cpu, addr: u32) -> Option<u32> {
        let (a0, a1, a2, a3) = (c.r[4], c.r[5], c.r[6], c.r[7]);
        Some(match addr {
            // the HUD's helpers: in races the original builds its primitives (recorded from
            // the HUD views' ordering tables)
            0x800C_C7F0 | 0x800D_4E9C | 0x800D_8690 | 0x800D_9ABC | 0x800D_3558 if self.mode == Mode::Race && !Self::paused(c) => return None,
            0x800C_C7F0 if self.mode == Mode::FrontEnd => {
                self.font_quads(c, a0, a1 as i32, a2 as i32);
                0
            }
            0x800C_C7F0 => {
                // Font_TextXY(text, x, y)
                let mut text = Vec::new();
                while text.len() < 256 {
                    let b = c.m.r8(a0 + text.len() as u32);
                    if b == 0 {
                        break;
                    }
                    text.push(b);
                }
                let colour = ((c.m.r16(FONT_CURRENT_CLUT) as u32 & 0x3F) as i32 - (c.m.r16(FONT_CLUT_X) as i32 >> 4)).clamp(0, 15) as u8;
                self.overlay.push(OverlayCmd::Text { x: a1 as i16, y: a2 as i16, colour, space: c.m.r8(FONT_SPACE), text });
                0
            }
            0x800D_4E9C => {
                // Hud_BuildString(text, x, y, colour, pad, measure only): the HUD's small
                // letters and digits and the pad button symbols; returns the width
                let sp = c.r[29];
                let measure = c.m.r32(sp + 20) != 0;
                let record = !measure && !native_hud(c.r[31]);
                let width = |c: &mut Cpu, s: u32| c.m.r16(crate::ps1::race::HUD_SHAPES + 20 * s + 16) as i16 as i32;
                let (x0, y) = (a1 as i32, a2 as i32);
                let mut x = x0;
                for i in 0..64 {
                    let ch = c.m.r8(a0 + i) as u32;
                    if ch == 0 {
                        break;
                    }
                    // (shape, y offset); a standard pad's symbols
                    let (shape, dy) = match ch {
                        0x20 => {
                            x += 3;
                            continue;
                        }
                        0x2A => {
                            // '*': the X button, two pixels in
                            x += 2;
                            (170, 0)
                        }
                        0x5E => (170, 0),
                        0x28 => (169, 0),
                        0x29 => (168, 0),
                        0x26 => (174, 0),
                        0x30..=0x39 => (ch + 110, 0),
                        0x2D => (72, 0),
                        0x3A => (73, 0),
                        0xC0..=0xDC => (ch - 118, -1),
                        0xE5 => (103, -1),
                        _ => (ch + 67, 0),
                    };
                    let shape = shape & 0xFF;
                    if record {
                        self.overlay.push(OverlayCmd::Shape { shape: shape as u8, x: x as i16, y: (y + dy) as i16, colour: a3 & 0xFF_FFFF });
                    }
                    x += width(c, shape) + if ch == 0x2A { 3 } else { 1 };
                }
                (x - x0) as u32
            }
            0x800D_8690 => {
                if !native_hud(c.r[31]) {
                    self.overlay.push(OverlayCmd::ThinBox { x: a0 as i16, y: a1 as i16, w: a2 as i16, h: a3 as i16 });
                }
                0
            }
            0x800D_9ABC => {
                self.overlay.push(OverlayCmd::PauseBox { x: a0 as i16, y: a1 as i16, w: a2 as i16, h: a3 as i16 });
                0
            }
            0x800D_3558 if native_hud(c.r[31]) => 0,
            0x800D_3558 => {
                let sp = c.r[29];
                if std::env::var_os("PS1_OVLOG").is_some() {
                    eprintln!("F4 ({a1},{a2}) {a3}x{} from {:#x}", c.m.r32(sp + 16) as i32, c.r[31]);
                }
                self.overlay.push(OverlayCmd::Rect {
                    semi: a0 != 0,
                    x: a1 as i16,
                    y: a2 as i16,
                    w: a3 as i16,
                    h: c.m.r32(sp + 16) as i16,
                    colour: c.m.r32(sp + 20) & 0xFF_FFFF,
                    dx0: c.m.r32(sp + 24) as i8,
                    dx1: c.m.r32(sp + 28) as i8,
                });
                0
            }
            0x800B_4024 => {
                // Render_RenderPlayerView(view, player): the world itself is drawn by the host
                // renderer. Run the original view set-up (Render_StartRenderingWorldView:
                // camera, fog keys, environment colours) and keep what it programs into the
                // GTE, but skip Render_RenderWorld.
                c.m.w32(RENDER_VIEW, a0);
                c.m.w32(RENDER_VIEW + 4, a1);
                c.call_nested(RENDER_START_WORLD_VIEW, &[a0]);
                // the HUD's values (speed in display units, gear, times, position): game logic
                // the original runs while rendering the HUD
                let g = |r: usize| c.gte_read(32 + r) as i32;
                self.view_env = [g(21), g(22), g(23), g(27), g(28), g(26), g(13), g(14), g(15)];
                if std::env::var_os("NFSHS_ENVLOG").is_some() && self.frame.frames_run.is_multiple_of(120) {
                    eprintln!("view {a0} env (far rgb, dqa, dqb, h, back rgb) {:?}, ofx {} ofy {}", self.view_env, c.gte_read(32 + 24) as i32 as f32 / 65536.0, c.gte_read(32 + 25) as i32 as f32 / 65536.0);
                }
                // which track chunks to draw, and how (detail level)
                c.call_nested(SETUP_BUILD_MATRICES, &[RENDER_VIEW, 0x1F80_0000]);
                c.call_nested(DRAWW_WORLD_SETUP_MATRIX, &[G_WORLD_MAT, 0x1F80_0014]);
                let n = c.call_nested(SETUP_CHUNK_BUILD_LIST, &[RENDER_VIEW]);
                c.m.w32(CHUNK_BUILD_COUNT, n);
                // the track lights' glares as the original builds them for the listed chunks
                // (BWorld_BuildGlareEffects, as BWorld_OnyxBuildFacets calls it), recorded
                let start = c.m.r32(0x1F80_0004);
                GLARE_MARKS.with_borrow_mut(Vec::clear);
                c.inner_hook = Some(glare_marker);
                for i in 0..n.min(64) {
                    let chunk = c.m.r16(0x8011_02B0 + 4 * i) as i16;
                    if chunk < 0 {
                        continue;
                    }
                    let list = c.m.r32(TRACK_CHUNK_LIST);
                    let group = c.m.r32(list + 112 * chunk as u32 + 84);
                    if group != 0 {
                        c.call_nested(BWORLD_BUILD_GLARE_EFFECTS, &[RENDER_VIEW, 0x1F80_0000, group]);
                    }
                }
                c.inner_hook = None;
                let end = c.m.r32(0x1F80_0004);
                self.track_glows.clear();
                if end > start && end - start < 0x4_0000 {
                    let marks = GLARE_MARKS.with_borrow(Clone::clone);
                    let saved = std::mem::take(&mut self.overlay);
                    self.prim_offset = [0, 0];
                    for (k, &(from, anchor)) in marks.iter().enumerate() {
                        let to = marks.get(k + 1).map_or(end, |m| m.0);
                        if to > from {
                            self.record_packets(c, from, to);
                            let cmds = std::mem::take(&mut self.overlay);
                            if !cmds.is_empty() {
                                self.track_glows.push(Glow { anchor, car: None, cmds, smoke: false });
                            }
                        }
                    }
                    self.overlay = saved;
                }
                // the tyre smoke and dust (Souffle_InsertFacet), each puff recorded with its
                // position
                let start = c.m.r32(0x1F80_0004);
                GLARE_MARKS.with_borrow_mut(Vec::clear);
                c.inner_hook = Some(smoke_marker);
                c.call_nested(SOUFFLE_INSERT_FACET, &[RENDER_VIEW]);
                c.inner_hook = None;
                let end = c.m.r32(0x1F80_0004);
                if std::env::var_os("NFSHS_SMOKELOG").is_some() {
                    let n = c.m.r32(GP + 3828);
                    let arr = c.m.r32(GP + 3824);
                    let live = (0..n.min(64)).filter(|&i| c.m.r8(arr + 68 * i) != 0).count();
                    eprintln!("smoke: {n} souffles ({live} live), {} built, {} bytes", GLARE_MARKS.with_borrow(Vec::len), end.wrapping_sub(start));
                }
                if end > start && end - start < 0x4_0000 {
                    let marks = GLARE_MARKS.with_borrow(Clone::clone);
                    let saved = std::mem::take(&mut self.overlay);
                    self.prim_offset = [0, 0];
                    for (k, &(from, anchor)) in marks.iter().enumerate() {
                        let to = marks.get(k + 1).map_or(end, |m| m.0);
                        if to > from {
                            self.record_packets(c, from, to);
                            let cmds = std::mem::take(&mut self.overlay);
                            if std::env::var_os("NFSHS_SMOKELOG").is_some() {
                                eprintln!("puff at {:?}: {}", anchor.map(|v| v >> 16), format!("{cmds:?}").chars().take(300).collect::<String>());
                            }
                            if !cmds.is_empty() {
                                self.track_glows.push(Glow { anchor, car: None, cmds, smoke: true });
                            }
                        }
                    }
                    self.overlay = saved;
                }
                // the weather (rain or snow, and the drops on the lens) as Render_RenderWorld
                // builds it after the world, recorded
                let start = c.m.r32(0x1F80_0004);
                c.call_nested(WEATHER_RENDER, &[RENDER_VIEW]);
                let end = c.m.r32(0x1F80_0004);
                if std::env::var_os("NFSHS_WEATHERLOG").is_some() {
                    eprintln!("weather: prims {start:#x}..{end:#x}");
                }
                if end > start && end - start < 0x4_0000 {
                    let saved = std::mem::take(&mut self.overlay);
                    self.prim_offset = [0, 0];
                    self.record_packets(c, start, end);
                    self.weather = std::mem::replace(&mut self.overlay, saved);
                }
                // the sun's flare (Hrz_BuildSky: Flare_Sun when the sky record's flags have
                // 4): the horizon's rotation (the view's matrix, no translation) on the GTE,
                // its primitives recorded as the sky layer
                let mut view_sky = Vec::new();
                let sky = c.m.r32(GP + 4920);
                if sky != 0 && c.m.r32(0x8012_45C8) & 4 != 0 {
                    let (rot, tr) = (std::array::from_fn::<u32, 8, _>(|i| c.gte_read(32 + i)), [c.gte_read(37), c.gte_read(38), c.gte_read(39)]);
                    c.call_nested(HRZ_SET_PSX_MATRIX, &[RENDER_VIEW + 68]);
                    for (i, v) in [37, 38, 39].into_iter().enumerate() {
                        let _ = i;
                        c.gte_write(v, 0);
                    }
                    c.r[4] = 0x8013_F0F8;
                    c.r[5] = 0x1F80_0000;
                    let saved = std::mem::take(&mut self.overlay);
                    self.prim_offset = [0, 0];
                    self.capture_prims(c, FLARE_SUN);
                    let sun = std::mem::replace(&mut self.overlay, saved);
                    if std::env::var_os("NFSHS_SUNLOG").is_some() {
                        for q in &sun {
                            eprintln!("sun {}", format!("{q:?}").chars().take(220).collect::<String>());
                        }
                    }
                    view_sky = sun.clone();
                    self.sky_overlay.extend(sun);
                    for (i, v) in rot.into_iter().enumerate() {
                        c.gte_write(32 + i, v);
                    }
                    for (i, v) in tr.into_iter().enumerate() {
                        c.gte_write(37 + i, v);
                    }
                }
                c.call_nested(DASHHUD_CHECK_WRONG_WAY, &[a1]);
                c.call_nested(DASHHUD_HUD_CALC, &[a1]);
                // the cars: the original decides what is drawn of each (the host draws them).
                // Its calls cannot reach the host from here: R3DCar_InsertCarFacetII is
                // replaced by an inner hook that records the decision instead of drawing.
                c.capture.clear();
                HALO_RANGES.with_borrow_mut(Vec::clear);
                c.inner_hook = Some(record_car_facet);
                c.call_nested(R3DCAR_INSERT_ALL_LIST_FACET, &[RENDER_VIEW]);
                c.inner_hook = None;
                self.car_facets = CarFacet::parse(&c.capture);
                // the cars' light halos, as the original built them (screen space)
                let ranges = HALO_RANGES.with_borrow(Clone::clone);
                let saved = std::mem::take(&mut self.overlay);
                self.prim_offset = [0, 0];
                self.car_glows.clear();
                for (start, end, car) in ranges {
                    self.record_packets(c, start, end);
                    let cmds = std::mem::take(&mut self.overlay);
                    let anchor = [c.m.r32(car + 0xA0) as i32, c.m.r32(car + 0xA4) as i32, c.m.r32(car + 0xA8) as i32];
                    self.car_glows.push(Glow { anchor, car: Some(car), cmds, smoke: false });
                }
                self.overlay = saved;
                // this view as set up (Draw_gViews[view]: its drawing environment's clip,
                // relative to the first view's, and size)
                let env = |c: &mut Cpu, v: u32, k: u32| c.m.r16(DRAW_VIEWS + 200 * v + 8 + 2 * k) as i16;
                let rect = [env(c, a0, 0) - env(c, 0, 0), env(c, a0, 1) - env(c, 0, 1), c.m.r16(DRAW_VIEWS + 200 * a0 + 12) as i16, c.m.r16(DRAW_VIEWS + 200 * a0 + 14) as i16];
                self.views.push(ViewCapture {
                    player: a1,
                    rect,
                    camera_pos: std::array::from_fn(|i| c.m.r32(RENDER_VIEW + 8 + 4 * i as u32) as i32),
                    camera_rot: std::array::from_fn(|i| c.m.r32(RENDER_VIEW + 0x14 + 4 * i as u32) as i32),
                    projection: c.m.r32(crate::ps1::race::addr::CAMERA_ZOOM),
                    view_env: self.view_env,
                    chunks: (0..n.min(64)).map(|i| (c.m.r16(0x8011_02B0 + 4 * i) as i16, c.m.r16(0x8011_02B2 + 4 * i))).collect(),
                    car_facets: self.car_facets.clone(),
                    glows: self.track_glows.iter().chain(&self.car_glows).cloned().collect(),
                    weather: self.weather.clone(),
                    sky_overlay: view_sky,
                    night_beam: crate::ps1::race::NightBeam::read(c, a1),
                });
                0
            }
            _ => return None,
        })
    }
}
