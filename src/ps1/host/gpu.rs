//! libgpu, replaced: drawing and display environments, VRAM transfers, ordering tables
//! (recorded, see `overlay`).

use super::*;

impl GameHost {
    /// libgpu: drawing environments, VRAM transfers, ordering tables. `None`: not one of these (or the original's own code runs).
    pub(super) fn gpu_call(&mut self, c: &mut Cpu, addr: u32) -> Option<u32> {
        let (a0, a1, a2, a3) = (c.r[4], c.r[5], c.r[6], c.r[7]);
        Some(match addr {
            // ---- libgpu
            0x800E_DFCC => 0,                  // ResetGraph
            0x800E_E140 => 0,                  // SetDispMask
            0x800E_E1D8 | 0x800F_0318 => 0,    // DrawSync
            0x800E_E35C => {
                // ClearImage(rect, r, g, b)
                self.vram_dirty = true;
                let (x, y, w, h) = Self::rect(c, a0);
                let col = ((a1 as u16 >> 3) & 31) | (((a2 as u16 >> 3) & 31) << 5) | (((a3 as u16 >> 3) & 31) << 10);
                for yy in y..(y + h).min(VRAM_H) {
                    for xx in x..(x + w).min(VRAM_W) {
                        self.vram[yy * VRAM_W + xx] = col;
                    }
                }
                0
            }
            0x800E_E3EC => {
                // LoadImage(rect, data)
                self.vram_dirty = true;
                let (x, y, w, h) = Self::rect(c, a0);
                let mut p = a1;
                for yy in y..y + h {
                    for xx in x..x + w {
                        let v = c.m.r16(p);
                        p += 2;
                        if yy < VRAM_H && xx < VRAM_W {
                            self.vram[yy * VRAM_W + xx] = v;
                        }
                    }
                }
                0
            }
            0x800E_E44C => {
                // StoreImage(rect, data)
                let (x, y, w, h) = Self::rect(c, a0);
                if self.sun_pixels(c, a1, w, h) {
                    return Some(0);
                }
                let mut p = a1;
                for yy in y..y + h {
                    for xx in x..x + w {
                        let v = if yy < VRAM_H && xx < VRAM_W { self.vram[yy * VRAM_W + xx] } else { 0 };
                        c.m.w16(p, v);
                        p += 2;
                    }
                }
                0
            }
            0x800E_E4AC => {
                // MoveImage(rect, x, y)
                self.vram_dirty = true;
                let (x, y, w, h) = Self::rect(c, a0);
                let (dx, dy) = (a1 as usize, a2 as usize);
                let src: Vec<u16> = (0..h)
                    .flat_map(|j| (0..w).map(move |i| (x + i, y + j)))
                    .map(|(sx, sy)| if sy < VRAM_H && sx < VRAM_W { self.vram[sy * VRAM_W + sx] } else { 0 })
                    .collect();
                for j in 0..h {
                    for i in 0..w {
                        let (tx, ty) = (dx + i, dy + j);
                        if ty < VRAM_H && tx < VRAM_W {
                            self.vram[ty * VRAM_W + tx] = src[j * w + i];
                        }
                    }
                }
                0
            }
            0x800E_E564 => {
                // ClearOTagR(ot, n): each entry links to the previous one, ot[0] terminates
                for i in 0..a1 {
                    let v = if i == 0 { 0x00FF_FFFF } else { (a0 + (i - 1) * 4) & 0x00FF_FFFF };
                    c.m.w32(a0 + i * 4, v);
                }
                a0
            }
            0x800E_E610 => {
                if std::env::var_os("NFSHS_ENVDBG").is_some() {
                    eprintln!("DrawOTag {a0:#x} ra {:#x} gp4704 {}", c.r[31], c.m.r32(GP + 4704));
                }
                if self.mode == Mode::FrontEnd && std::env::var_os("PS1_MISSLOG").is_some() {
                    self.report_unrecorded(c, a0);
                }
                // DrawOTag: rendering is done by the host engine. In races the views after
                // the world's (HUD, tachometer, map) are recorded as 2D
                if self.mode == Mode::Race && !Self::paused(c) {
                    // (the world's list, thousands of primitives, is only counted)
                    let saved = std::mem::take(&mut self.overlay);
                    if self.otags > 0 || Self::ot_len(c, a0) < 1500 {
                        self.record_ot(c, a0);
                    }
                    let list = std::mem::replace(&mut self.overlay, saved);
                    self.frame_lists.push(list);
                    self.otags += 1;
                }
                0
            }
            0x800E_E680 => {
                // PutDrawEnv(env): a full-screen one is the frame's drawing buffer (where the
                // recorded HUD views' areas and offsets are)
                if std::env::var_os("NFSHS_ENVDBG").is_some() {
                    eprintln!("PutDrawEnv {:?} ofs {:?} ra {:#x}", (0..4).map(|i| c.m.r16(a0 + 2 * i)).collect::<Vec<_>>(), (c.m.r16(a0 + 8), c.m.r16(a0 + 10)), c.r[31]);
                }
                // (a split screen's views are full width, half height)
                if c.m.r16(a0 + 4) >= 256 && c.m.r16(a0 + 6) >= 200 {
                    self.draw_origin = [c.m.r16(a0) as i32, c.m.r16(a0 + 2) as i32];
                }
                a0
            }
            0x800E_ED6C => {
                // SetDrawEnv(packet, env): remember the environment the packet sets (the HUD
                // views' clip and offset, for recording them)
                let clip = std::array::from_fn(|i| c.m.r16(a1 + 2 * i as u32) as i16);
                let ofs = [c.m.r16(a1 + 8) as i16, c.m.r16(a1 + 10) as i16];
                self.draw_envs.insert(a0, (clip, ofs));
                if std::env::var_os("NFSHS_ENVDBG").is_some() {
                    eprintln!("SetDrawEnv {a0:#x} {clip:?} ofs {ofs:?} ra {:#x}", c.r[31]);
                }
                // a full-screen one is the drawing buffer the views are in
                // (or the first view's: a split screen has no full-screen one)
                let view0 = |c: &mut Cpu, buf: u32| [0, 1].map(|k| c.m.r16(DRAW_VIEWS + 8 + 92 * buf + 2 * k) as i16);
                let first_view = [view0(c, 0), view0(c, 1)].contains(&[clip[0], clip[1]]) && clip[2] >= 256;
                if (clip[2] >= 256 && clip[3] >= 200) || first_view {
                    self.draw_origin = [clip[0] as i32, clip[1] as i32];
                }
                return None;
            }
            0x800E_EC70 => {
                // SetDrawArea(DR_AREA, rect): the front end clips some drawing (scrolling text)
                if self.mode == Mode::FrontEnd {
                    // in VRAM terms, inside one of the two 256-line drawing buffers
                    let mut r = |o: u32| c.m.r16(a1 + o) as i16;
                    let (x, y) = (r(0), r(2));
                    let oy = if y >= 256 { 256 } else { 0 };
                    self.overlay.push(OverlayCmd::Clip { x, y: y - oy, w: r(4), h: r(6) });
                }
                return None;
            }
            0x800E_E740 => {
                // PutDispEnv: remember the display area (x, y, w, h)
                self.display = [0, 2, 4, 6].map(|o| c.m.r16(a0 + o));
                // the front end's frames end in the engine's flip (Draw_ flip: VSync, PutDispEnv,
                // DrawOTag per view)
                if self.mode == Mode::FrontEnd && self.frame.on_frame.is_some() && c.r[31] == FLIP_PUT_DISP_ENV_RA {
                    let _ = self.frame_top(c);
                }
                a0
            }
            0x8010_53B8 => 0,                  // GPU_cw
            0x800F_24B8 => 0,                  // GetVideoMode: NTSC
            _ => return None,
        })
    }
}
