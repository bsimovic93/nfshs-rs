//! The game's own drawing and mode changes the host takes over or watches: the front end's
//! shapes and fonts, the showroom car, the loading screen, the switch into a race.

use super::*;

impl GameHost {
    /// The game's own drawing and mode changes the host takes over or watches (front end, showroom, loading screen). `None`: not one of these (or the original's own code runs).
    pub(super) fn game_call(&mut self, c: &mut Cpu, addr: u32) -> Option<u32> {
        let (a0, a1, a2, a3) = (c.r[4], c.r[5], c.r[6], c.r[7]);
        Some(match addr {
            0x8004_E74C => {
                // the front end's shape emitter (DrawShapeExtended and friends): shape info,
                // flags (1 semi-transparent, 2 flip vertically, 4 flip horizontally), position,
                // four corner colours, blend mode
                let sp = c.r[29];
                let (colours_at, abr) = (c.m.r32(sp + 16), c.m.r32(sp + 20));
                let colours: [u32; 4] = std::array::from_fn(|k| c.m.r32(colours_at + 4 * k as u32) & 0xFF_FFFF);
                self.shape_quads(c, a0, a1, a2 as i32, a3 as i32, colours, abr);
                0
            }
            0x8004_F12C | 0x8004_F3B8 => {
                // PSXDrawSquare(colour, x, y, w, h) / PSXDrawTransSquare(.., count): flat quads,
                // the transparent one drawn `count` times over itself
                let sp = c.r[29];
                let h = c.m.r32(sp + 16) as i32;
                let (x, y, w) = (a1 as i32, a2 as i32, a3 as i32);
                let semi = addr == 0x8004_F3B8;
                let n = if semi { c.m.r16(sp + 20) as i16 as i32 } else { 1 };
                for _ in 0..n {
                    self.overlay.push(OverlayCmd::Quad { pts: rect_pts(x, y, w, h), colours: [a0 & 0xFF_FFFF; 4], tex: None, semi, abr: 0 });
                }
                0
            }
            0x8004_D1C4 => {
                // Movie_Play(movie): the host plays it (see `play_movie`); returns whether it
                // was skipped
                self.play_movie(c, a0 & 0xFF);
                c.m.r16(0x8005_2EBC) as i16 as i32 as u32
            }
            0x8003_AC64 => {
                // DrawCar(carInfo, ...): the car the showroom shows (type, colour)
                self.showroom_car = Some((c.m.r8(a0) as i8, c.m.r8(a0 + 1)));
                return None;
            }
            0x800B_3428 if self.mode == Mode::FrontEnd && !self.capturing => {
                // R3DCar_InsertCarFacetMenuII(car, ..): the showroom car, drawn by the host
                let mut car = Box::new(crate::ps1::race::CarState::read(&mut c.m, a0));
                // its light level (halved for the reflection), left in the car cache as the
                // original does (the floor's light uses it)
                let level = if c.m.r32(GP + 3532) & 0x80 != 0 { a1 >> 1 } else { a1 } & 0xFF;
                c.m.w32(0x1F80_0000 + 140, level * 0x01_0101);
                // the model is the selected one (DrawCar's); the object keeps an older code
                if let Some(code) = self.showroom_car.and_then(|(t, _)| u32::try_from(t).ok()).and_then(crate::ps1::race::car_code) {
                    car.code = code.to_string();
                }
                // DrawC_PrimStart as the original calls it here: the reflection maps (as in
                // races, see record_car_facet)
                let ok = c.call_nested(DRAWC_PRIM_START, &[R3DCAR_CENTER, a0, level, CAR_CACHE]) as i32 >= 0;
                // its shadow on the floor (DrawC_ShadowPrimClip, not for the reflection): the
                // original's primitives, drawn with the floor before the car
                if ok && c.m.r32(GP + 3532) & 0x80 == 0 {
                    c.r[4] = 0x8011_7BA0;
                    c.r[5] = CAR_CACHE;
                    let saved = std::mem::take(&mut self.overlay);
                    self.capture_prims(c, DRAWC_SHADOW_PRIM_CLIP);
                    self.showroom_shadow = std::mem::replace(&mut self.overlay, saved);
                }
                let env = ok.then(|| {
                    let map = |c: &mut Cpu, o: u32| (c.m.r32(CAR_CACHE + o) != 0).then(|| std::array::from_fn(|i| c.m.r32(CAR_CACHE + o + 4 * i as u32)));
                    CarEnv { a: map(c, 100), b: map(c, 116), scroll: c.m.r32(CAR_CACHE + 132), colour: c.m.r32(CAR_CACHE + 144) & 0xFF_FFFF, yaw: c.m.r16(R3DCAR_CENTER + 0x14) as i16 }
                });
                // the lights' halos: the original itself, its body drawing skipped
                let mut halos = Vec::new();
                if ok {
                    let start = c.m.r32(0x1F80_0004);
                    let args = [a0, a1, a2, a3];
                    HALO_PASS.set(true);
                    c.inner_hook = Some(record_car_facet);
                    let saved_host = self.capturing;
                    self.capturing = true;
                    c.call_nested(0x800B_3428, &args);
                    self.capturing = saved_host;
                    c.inner_hook = None;
                    HALO_PASS.set(false);
                    let end = c.m.r32(0x1F80_0004);
                    if end > start && end - start < 0x1_0000 {
                        let saved = std::mem::take(&mut self.overlay);
                        self.record_packets(c, start, end);
                        halos = std::mem::replace(&mut self.overlay, saved);
                    }
                }
                let view = RENDER_VIEW;
                self.overlay.push(OverlayCmd::ShowroomCar {
                    car,
                    parts: (0..57).map(|i| c.m.r8(R3DCAR_OBJECT_VISIBLE + i)).collect(),
                    cam_pos: std::array::from_fn(|i| c.m.r32(view + 8 + 4 * i as u32) as i32),
                    cam_rot: std::array::from_fn(|i| c.m.r32(view + 0x14 + 4 * i as u32) as i32),
                    h: c.gte_regs[32 + 26] & 0xFFFF,
                    ofx: c.gte_regs[32 + 24] as i32,
                    ofy: c.gte_regs[32 + 25] as i32,
                    mirrored: c.m.r32(0x8013_E608) & 0x80 != 0,
                    level,
                    env,
                });
                // after the car (drawn over it)
                self.overlay.extend(halos);
                0
            }
            0x800B_3850 if self.mode == Mode::FrontEnd => {
                // R3DCar_Showroom(view): the turntable, drawn by the host. DrawC_ShowroomPrims'
                // chasing lights: two runs of five segments from the tick counter (the second
                // one opposite, or turning the other way), each segment taking the dimmest
                // light that covers it
                let t = (c.m.r32(TICKS) as i32 % 256) >> 3;
                let (head, dir): ([i32; 2], [i32; 2]) = if c.m.r32(GP + 4792) == 0 { ([t, (t + 16) & 31], [-1, -1]) } else { ([t, 32 - t], [-1, 1]) };
                let mut seg = [-1i8; 32];
                for k in 0..2 {
                    for i in 0..5 {
                        let s = ((head[k] + i * dir[k]) & 31) as usize;
                        if (seg[s] as i32) < i {
                            seg[s] = i as i8;
                        }
                    }
                }
                // both project with the view's camera (its matrix transposed, y flipped) on the
                // GTE as the game sets it up, into the ordering table's first slot
                Self::showroom_gte(c, a0);
                let mut prims = Vec::new();
                let vertex = |c: &mut Cpu, base: u32, i: u32| -> [i16; 3] { std::array::from_fn(|k| c.m.r16(base + 6 * i + 2 * k as u32) as i16) };
                if c.m.r32(0x8005_199C) != 0 {
                    // the ring: 32 textured quads, added (ChangeTPage(.., 1)), each in the base
                    // colour and again in its light's
                    let tex_at = c.m.r32(SHOWROOM_TEX);
                    let tex: [u32; 4] = std::array::from_fn(|i| c.m.r32(tex_at + 4 * i as u32));
                    let clut = (tex[0] >> 16) as u16;
                    let tpage = ((tex[1] >> 16) as u16 & !0x60) | (1 << 5);
                    let uv = tex.map(|w| [w as u8 as u16, (w >> 8) as u8 as u16]);
                    for t in 0..32u32 {
                        let (a, b) = (2 * t, (2 * t + 2) % 64);
                        let pts = [a, b, a + 1, b + 1].map(|i| {
                            let v = vertex(c, SHOWROOM_RING, i);
                            Self::gte_project(c, v)
                        });
                        let light = seg[t as usize];
                        for colour in std::iter::once(0x30_0A00).chain((light >= 0).then(|| c.m.r32(SHOWROOM_LIGHTS + 4 * light as u32) & 0xFF_FFFF)) {
                            prims.push(OverlayCmd::Quad { pts, colours: [colour; 4], tex: Some(QuadTex { tpage, clut, uv }), semi: true, abr: 1 });
                        }
                    }
                }
                // DrawC_SpotPrims: the light on the floor, a fan of added gouraud triangles from
                // the centre (the car cache's light level) to a black rim
                let spot = c.m.r32(0x1F80_0000 + 140) & 0xFF_FFFF;
                // the centre follows the rim's 32 vertices
                let centre = vertex(c, SHOWROOM_RIM, 32);
                let centre = Self::gte_project(c, centre);
                for i in 0..32u32 {
                    let (p, q) = (vertex(c, SHOWROOM_RIM, i), vertex(c, SHOWROOM_RIM, (i + 1) % 32));
                    let (p, q) = (Self::gte_project(c, p), Self::gte_project(c, q));
                    prims.push(OverlayCmd::Quad { pts: [centre, p, q, q], colours: [spot, 0, 0, 0], tex: None, semi: true, abr: 1 });
                }
                // the car's shadow last (drawn over the ring and spot; the list is reversed)
                let mut all = std::mem::take(&mut self.showroom_shadow);
                all.reverse();
                all.extend(prims);
                self.overlay.push(OverlayCmd::ShowroomFloor(all));
                0
            }
            0x800B_F1D4 => {
                // Draw_DrawDirectScreen(shape, x, y): a picture loaded straight into the
                // display buffer (the loading screen); recorded, and loaded as the game does
                if let Some((w, h, rgba)) = Self::shape_rgba(c, a0) {
                    self.loading.push(OverlayCmd::Bitmap { x: a1 as i16, y: a2 as i16, w, h, rgba: std::sync::Arc::new(rgba) });
                }
                return None;
            }
            0x8004_E01C => {
                // DoTitleScreen: the title picture while the front end loads
                self.from_main = true;
                self.new_direct_screen(c);
                return None;
            }
            0x800A_512C => {
                // NFS4_LoadingIcon: a cleared screen with the loading icon
                self.new_direct_screen(c);
                return None;
            }
            0x800F_146C => {
                // movfxya(shape, x, y): a picture blitted into the display buffer
                if let Some((w, h, rgba)) = Self::shape_rgba(c, a0) {
                    self.loading.push(OverlayCmd::Bitmap { x: a1 as i16, y: a2 as i16, w, h, rgba: std::sync::Arc::new(rgba) });
                }
                return None;
            }
            0x800D_BCA4 => {
                // Loading_DrawLoadingScreen: a new loading screen (background and track map)
                self.new_direct_screen(c);
                return None;
            }
            0x800D_BDEC => {
                // Loading_UpdateLoadingScreen(step): show the screen for the time loading took
                // so far, then let the game draw the progress bar's next part
                self.loading_frames(c);
                return None;
            }
            QUIT_CALLBACK => {
                // the game setup menu's Quit (see `add_quit_item`)
                self.quit = true;
                0
            }
            0x8002_7B38 => {
                // Front_Menu: the front end runs (also its post-race screens, before
                // Front_BuildStream)
                self.mode = Mode::FrontEnd;
                self.from_main = true;
                return None;
            }
            0x8002_A7B0 => {
                // Front_BuildStream: the front end runs
                self.mode = Mode::FrontEnd;
                self.from_main = true;
                return None;
            }
            0x800B_7A20 => {
                // Sim_MainGameLoop: a race runs (after the rest of the loading screen's time)
                if !self.loading.is_empty() {
                    self.loading_frames(c);
                    self.loading.clear();
                }
                self.mode = Mode::Race;
                return None;
            }
            _ => return None,
        })
    }
}
