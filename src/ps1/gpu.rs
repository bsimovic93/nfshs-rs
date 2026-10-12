//! PS1 GPU: VRAM plus a software rasterizer for GP0 command lists (what `DrawOTag` sends by
//! DMA), following the hardware rules described in psx-spx ("GPU"): 15-bit VRAM, CLUT 4/8-bit
//! and direct 15-bit textures, texture window, semi-transparency, dithering, mask bit and the
//! drawing area / offset.

use super::Mem;

pub const VRAM_W: usize = 1024;
pub const VRAM_H: usize = 512;

const DITHER: [[i32; 4]; 4] = [[-4, 0, -3, 1], [2, -2, 3, -1], [-3, 1, -4, 0], [3, -1, 2, -2]];

#[derive(Clone, Copy, Default, Debug)]
struct Vertex {
    x: i32,
    y: i32,
    /// 8-bit colour
    c: [i32; 3],
    u: i32,
    v: i32,
}

pub struct Gpu {
    pub vram: Vec<u16>,
    /// Draw mode (texpage) register: bits 0-3 base x/64, 4 base y/256, 5-6 semi-transparency,
    /// 7-8 colour depth, 9 dither, 10 draw to display, 11 texture disable, 12/13 rect flip.
    texpage: u32,
    tex_window: (u32, u32, u32, u32),
    area: (i32, i32, i32, i32),
    offset: (i32, i32),
    mask_set: bool,
    mask_check: bool,
    /// Display area (x, y, w, h) from PutDispEnv.
    pub display: [u16; 4],
    /// Primitives drawn since the last reset (diagnostics).
    pub prims: u32,
    /// Packets still to print (diagnostics).
    pub dump: u32,
}

impl Default for Gpu {
    fn default() -> Self {
        Self {
            vram: vec![0; VRAM_W * VRAM_H],
            texpage: 0,
            tex_window: (0, 0, 0, 0),
            area: (0, 0, 1023, 511),
            offset: (0, 0),
            mask_set: false,
            mask_check: false,
            display: [0, 0, 320, 240],
            prims: 0,
            dump: std::env::var("PS1_GPU_DUMP").ok().and_then(|v| v.parse().ok()).unwrap_or(0),
        }
    }
}

fn sext11(v: u32) -> i32 {
    ((v << 21) as i32) >> 21
}

fn rgb8(w: u32) -> [i32; 3] {
    [(w & 0xFF) as i32, ((w >> 8) & 0xFF) as i32, ((w >> 16) & 0xFF) as i32]
}

impl Gpu {
    pub fn area(&self) -> (i32, i32, i32, i32) {
        self.area
    }
    pub fn offset(&self) -> (i32, i32) {
        self.offset
    }

    // ---- environment

    /// PutDrawEnv(DRAWENV *env).
    pub fn put_draw_env(&mut self, m: &mut Mem, env: u32) {
        let s16 = |m: &mut Mem, o: u32| m.r16(env + o) as i16 as i32;
        let (x, y, w, h) = (s16(m, 0), s16(m, 2), s16(m, 4), s16(m, 6));
        self.area = (x, y, (x + w - 1).min(1023), (y + h - 1).min(511));
        self.offset = (s16(m, 8), s16(m, 10));
        let (tx, ty, tw, th) = (s16(m, 12), s16(m, 14), s16(m, 16), s16(m, 18));
        // texture window in 8-pixel units: mask = inverse of the window size
        let mask = |s: i32| if s == 0 { 0 } else { ((256 - s) as u32 >> 3) & 0x1F };
        self.tex_window = (mask(tw), mask(th), (tx as u32 >> 3) & 0x1F, (ty as u32 >> 3) & 0x1F);
        let tpage = m.r16(env + 0x14) as u32;
        let dtd = m.r8(env + 0x16) as u32;
        let dfe = m.r8(env + 0x17) as u32;
        self.texpage = (tpage & 0x1FF) | (dtd & 1) << 9 | (dfe & 1) << 10;
        if m.r8(env + 0x18) != 0 {
            let col = [m.r8(env + 0x19), m.r8(env + 0x1A), m.r8(env + 0x1B)];
            let c = (col[0] as u16 >> 3) | (col[1] as u16 >> 3) << 5 | (col[2] as u16 >> 3) << 10;
            let (x0, y0, x1, y1) = self.area;
            for yy in y0.max(0)..=y1 {
                for xx in x0.max(0)..=x1 {
                    self.vram[yy as usize * VRAM_W + xx as usize] = c;
                }
            }
        }
    }

    /// PutDispEnv(DISPENV *env): the displayed VRAM area.
    pub fn put_disp_env(&mut self, m: &mut Mem, env: u32) {
        self.display = [0, 2, 4, 6].map(|o| m.r16(env + o));
    }

    /// The displayed area as RGBA8.
    pub fn display_rgba(&self) -> (u32, u32, Vec<u8>) {
        let [x, y, w, h] = self.display.map(|v| v as usize);
        let (w, h) = (w.clamp(1, VRAM_W), h.clamp(1, VRAM_H));
        let mut out = Vec::with_capacity(w * h * 4);
        for j in 0..h {
            for i in 0..w {
                let p = self.vram[((y + j) % VRAM_H) * VRAM_W + (x + i) % VRAM_W];
                let c = |s: u32| {
                    let v = (p as u32 >> s) & 31;
                    (v << 3 | v >> 2) as u8
                };
                out.extend_from_slice(&[c(0), c(5), c(10), 255]);
            }
        }
        (w as u32, h as u32, out)
    }

    // ---- command processing

    /// DrawOTag: walk the ordering table linked list from `ot` and execute every packet.
    pub fn draw_ot(&mut self, m: &mut Mem, ot: u32) {
        let mut p = ot & 0x00FF_FFFF;
        let mut guard = 0;
        while p != 0x00FF_FFFF && guard < 1_000_000 {
            guard += 1;
            let hdr = m.r32(p | 0x8000_0000);
            let n = hdr >> 24;
            if n > 0 {
                let words: Vec<u32> = (0..n).map(|i| m.r32((p + 4 + i * 4) | 0x8000_0000)).collect();
                if self.dump > 0 {
                    self.dump -= 1;
                    eprintln!("  packet @{p:06x}: {words:08x?}");
                }
                self.gp0_list(&words);
            }
            p = hdr & 0x00FF_FFFF;
        }
    }

    /// Execute a GP0 word stream.
    pub fn gp0_list(&mut self, w: &[u32]) {
        let mut i = 0;
        while i < w.len() {
            let used = self.gp0(&w[i..]);
            i += used.max(1);
        }
    }

    /// Execute the GP0 command at the start of `w`; returns the number of words consumed.
    fn gp0(&mut self, w: &[u32]) -> usize {
        let cmd = w[0] >> 24;
        let get = |i: usize| w.get(i).copied().unwrap_or(0);
        match cmd {
            0x02 => {
                // fill rectangle (ignores drawing area, offset and mask)
                let c = rgb8(w[0]);
                let col = (c[0] as u16 >> 3) | (c[1] as u16 >> 3) << 5 | (c[2] as u16 >> 3) << 10;
                let (x, y) = (get(1) & 0x3F0, (get(1) >> 16) & 0x1FF);
                let (ww, hh) = (((get(2) & 0x3FF) + 0xF) & !0xF, (get(2) >> 16) & 0x1FF);
                for yy in 0..hh {
                    for xx in 0..ww {
                        self.vram[((y + yy) as usize % VRAM_H) * VRAM_W + (x + xx) as usize % VRAM_W] = col;
                    }
                }
                3
            }
            0x20..=0x3F => self.polygon(w),
            0x40..=0x5F => self.line(w),
            0x60..=0x7F => self.rect(w),
            0x80..=0x9F => {
                let (sx, sy) = (get(1) & 0x3FF, (get(1) >> 16) & 0x1FF);
                let (dx, dy) = (get(2) & 0x3FF, (get(2) >> 16) & 0x1FF);
                let (ww, hh) = (((get(3) & 0xFFFF).wrapping_sub(1) & 0x3FF) + 1, (((get(3) >> 16).wrapping_sub(1)) & 0x1FF) + 1);
                let src: Vec<u16> = (0..hh)
                    .flat_map(|j| (0..ww).map(move |i| (i, j)))
                    .map(|(i, j)| self.vram[((sy + j) as usize % VRAM_H) * VRAM_W + (sx + i) as usize % VRAM_W])
                    .collect();
                for j in 0..hh {
                    for i in 0..ww {
                        self.vram[((dy + j) as usize % VRAM_H) * VRAM_W + (dx + i) as usize % VRAM_W] = src[(j * ww + i) as usize];
                    }
                }
                4
            }
            0xA0..=0xBF => {
                let (x, y) = (get(1) & 0x3FF, (get(1) >> 16) & 0x1FF);
                let (ww, hh) = (((get(2) & 0xFFFF).wrapping_sub(1) & 0x3FF) + 1, (((get(2) >> 16).wrapping_sub(1)) & 0x1FF) + 1);
                let n = (ww * hh) as usize;
                for k in 0..n {
                    let word = get(3 + k / 2);
                    let v = if k % 2 == 0 { word as u16 } else { (word >> 16) as u16 };
                    let (i, j) = (k as u32 % ww, k as u32 / ww);
                    self.vram[((y + j) as usize % VRAM_H) * VRAM_W + (x + i) as usize % VRAM_W] = v;
                }
                3 + n.div_ceil(2)
            }
            0xC0..=0xDF => 3,
            0xE1 => {
                self.texpage = (self.texpage & !0x3FFF) | (w[0] & 0x3FFF);
                1
            }
            0xE2 => {
                let v = w[0];
                self.tex_window = (v & 0x1F, (v >> 5) & 0x1F, (v >> 10) & 0x1F, (v >> 15) & 0x1F);
                1
            }
            0xE3 => {
                self.area.0 = (w[0] & 0x3FF) as i32;
                self.area.1 = ((w[0] >> 10) & 0x1FF) as i32;
                1
            }
            0xE4 => {
                self.area.2 = (w[0] & 0x3FF) as i32;
                self.area.3 = ((w[0] >> 10) & 0x1FF) as i32;
                1
            }
            0xE5 => {
                self.offset = (sext11(w[0]), sext11(w[0] >> 11));
                1
            }
            0xE6 => {
                self.mask_set = w[0] & 1 != 0;
                self.mask_check = w[0] & 2 != 0;
                1
            }
            _ => 1,
        }
    }

    fn polygon(&mut self, w: &[u32]) -> usize {
        let cmd = w[0] >> 24;
        let gouraud = cmd & 0x10 != 0;
        let quad = cmd & 0x08 != 0;
        let textured = cmd & 0x04 != 0;
        let semi = cmd & 0x02 != 0;
        let raw = cmd & 0x01 != 0;
        let n = if quad { 4 } else { 3 };
        let per = 1 + textured as usize + gouraud as usize;
        let len = 1 + n * per - gouraud as usize;
        if w.len() < len {
            return w.len();
        }
        let mut vs = [Vertex::default(); 4];
        let mut i = 1;
        let mut clut = 0;
        let mut color = rgb8(w[0]);
        for (k, v) in vs.iter_mut().enumerate().take(n) {
            if gouraud && k > 0 {
                color = rgb8(w[i]);
                i += 1;
            }
            v.c = color;
            v.x = sext11(w[i]) + self.offset.0;
            v.y = sext11(w[i] >> 16) + self.offset.1;
            i += 1;
            if textured {
                v.u = (w[i] & 0xFF) as i32;
                v.v = ((w[i] >> 8) & 0xFF) as i32;
                match k {
                    0 => clut = w[i] >> 16,
                    1 => self.texpage = (self.texpage & !0x9FF) | ((w[i] >> 16) & 0x9FF),
                    _ => {}
                }
                i += 1;
            }
        }
        let tex = textured.then_some((self.texpage, clut));
        let modulate = textured && !raw;
        let dither = self.texpage & 0x200 != 0 && (gouraud || modulate);
        self.prims += 1;
        self.triangle([vs[0], vs[1], vs[2]], gouraud, tex, modulate, semi, dither);
        if quad {
            self.triangle([vs[1], vs[2], vs[3]], gouraud, tex, modulate, semi, dither);
        }
        len
    }

    fn line(&mut self, w: &[u32]) -> usize {
        let cmd = w[0] >> 24;
        let gouraud = cmd & 0x10 != 0;
        let poly = cmd & 0x08 != 0;
        let semi = cmd & 0x02 != 0;
        let mut pts = Vec::new();
        let mut i = 1;
        let mut color = rgb8(w[0]);
        loop {
            if i >= w.len() {
                break;
            }
            if !pts.is_empty() && gouraud {
                if poly && w[i] & 0xF000_F000 == 0x5000_5000 {
                    i += 1;
                    break;
                }
                color = rgb8(w[i]);
                i += 1;
                if i >= w.len() {
                    break;
                }
            }
            if poly && !pts.is_empty() && !gouraud && w[i] & 0xF000_F000 == 0x5000_5000 {
                i += 1;
                break;
            }
            pts.push(Vertex { x: sext11(w[i]) + self.offset.0, y: sext11(w[i] >> 16) + self.offset.1, c: color, u: 0, v: 0 });
            i += 1;
            if !poly && pts.len() == 2 {
                break;
            }
        }
        let dither = self.texpage & 0x200 != 0 && gouraud;
        for p in pts.windows(2) {
            self.draw_line(p[0], p[1], semi, dither);
        }
        self.prims += 1;
        i
    }

    fn rect(&mut self, w: &[u32]) -> usize {
        let cmd = w[0] >> 24;
        let textured = cmd & 0x04 != 0;
        let semi = cmd & 0x02 != 0;
        let raw = cmd & 0x01 != 0;
        let size = (cmd >> 3) & 3;
        let len = 2 + textured as usize + (size == 0) as usize;
        if w.len() < len {
            return w.len();
        }
        let c = rgb8(w[0]);
        let x0 = sext11(w[1]) + self.offset.0;
        let y0 = sext11(w[1] >> 16) + self.offset.1;
        let (mut u0, mut v0, mut clut) = (0, 0, 0);
        if textured {
            u0 = (w[2] & 0xFF) as i32;
            v0 = ((w[2] >> 8) & 0xFF) as i32;
            clut = w[2] >> 16;
        }
        let (ww, hh) = match size {
            0 => {
                let s = w[len - 1];
                ((s & 0x3FF) as i32, ((s >> 16) & 0x1FF) as i32)
            }
            1 => (1, 1),
            2 => (8, 8),
            _ => (16, 16),
        };
        let flip_x = self.texpage & 0x1000 != 0;
        let flip_y = self.texpage & 0x2000 != 0;
        self.prims += 1;
        for j in 0..hh {
            let y = y0 + j;
            if y < self.area.1 || y > self.area.3 {
                continue;
            }
            for i in 0..ww {
                let x = x0 + i;
                if x < self.area.0 || x > self.area.2 {
                    continue;
                }
                let mut col = [c[0] >> 3, c[1] >> 3, c[2] >> 3];
                let mut stp = false;
                if textured {
                    let u = if flip_x { u0 - i } else { u0 + i } & 0xFF;
                    let v = if flip_y { v0 - j } else { v0 + j } & 0xFF;
                    let t = self.texel(self.texpage, clut, u, v);
                    if t == 0 {
                        continue;
                    }
                    stp = t & 0x8000 != 0;
                    col = self.modulate(t, c, !raw, None);
                }
                self.plot(x, y, col, stp, semi && (!textured || stp));
            }
        }
        len
    }

    // ---- rasterization

    fn texel(&self, tp: u32, clut: u32, u: i32, v: i32) -> u16 {
        let (mx, my, ox, oy) = self.tex_window;
        let u = ((u as u32) & !(mx * 8)) | ((ox & mx) * 8);
        let v = ((v as u32) & !(my * 8)) | ((oy & my) * 8);
        let bx = (tp & 0xF) as usize * 64;
        let by = ((tp >> 4) & 1) as usize * 256;
        let row = (by + v as usize) % VRAM_H * VRAM_W;
        let cx = (clut & 0x3F) as usize * 16;
        let cy = ((clut >> 6) & 0x1FF) as usize;
        match (tp >> 7) & 3 {
            0 => {
                let p = self.vram[row + (bx + u as usize / 4) % VRAM_W];
                let idx = (p >> ((u & 3) * 4)) & 0xF;
                self.vram[cy * VRAM_W + (cx + idx as usize) % VRAM_W]
            }
            1 => {
                let p = self.vram[row + (bx + u as usize / 2) % VRAM_W];
                let idx = (p >> ((u & 1) * 8)) & 0xFF;
                self.vram[cy * VRAM_W + (cx + idx as usize) % VRAM_W]
            }
            _ => self.vram[row + (bx + u as usize) % VRAM_W],
        }
    }

    /// Texel x vertex colour / 128 (5-bit result); `dither` adds the 4x4 pattern offset.
    fn modulate(&self, t: u16, c: [i32; 3], modulate: bool, dither: Option<i32>) -> [i32; 3] {
        let tc = [(t & 31) as i32, ((t >> 5) & 31) as i32, ((t >> 10) & 31) as i32];
        if !modulate {
            return tc;
        }
        std::array::from_fn(|i| {
            let v = (tc[i] << 3) * c[i] / 128 + dither.unwrap_or(0);
            v.clamp(0, 255) >> 3
        })
    }

    fn plot(&mut self, x: i32, y: i32, col: [i32; 3], stp: bool, semi: bool) {
        let idx = y as usize * VRAM_W + x as usize;
        let back = self.vram[idx];
        if self.mask_check && back & 0x8000 != 0 {
            return;
        }
        let mut c = col;
        if semi {
            let b = [(back & 31) as i32, ((back >> 5) & 31) as i32, ((back >> 10) & 31) as i32];
            c = std::array::from_fn(|i| {
                match (self.texpage >> 5) & 3 {
                    0 => (b[i] + c[i]) >> 1,
                    1 => b[i] + c[i],
                    2 => b[i] - c[i],
                    _ => b[i] + (c[i] >> 2),
                }
                .clamp(0, 31)
            });
        }
        let mask = if self.mask_set || stp { 0x8000 } else { 0 };
        self.vram[idx] = c[0] as u16 | (c[1] as u16) << 5 | (c[2] as u16) << 10 | mask;
    }

    fn triangle(&mut self, v: [Vertex; 3], gouraud: bool, tex: Option<(u32, u32)>, modulate: bool, semi: bool, dither: bool) {
        // the GPU skips polygons spanning more than 1023 x 511 pixels
        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
            if (v[a].x - v[b].x).abs() >= 1024 || (v[a].y - v[b].y).abs() >= 512 {
                return;
            }
        }
        let area = (v[1].x - v[0].x) as i64 * (v[2].y - v[0].y) as i64 - (v[2].x - v[0].x) as i64 * (v[1].y - v[0].y) as i64;
        if area == 0 {
            return;
        }
        let (v0, v1, v2) = if area < 0 { (v[0], v[2], v[1]) } else { (v[0], v[1], v[2]) };
        let area = area.abs();
        let minx = v0.x.min(v1.x).min(v2.x).max(self.area.0);
        let maxx = v0.x.max(v1.x).max(v2.x).min(self.area.2 + 1);
        let miny = v0.y.min(v1.y).min(v2.y).max(self.area.1);
        let maxy = v0.y.max(v1.y).max(v2.y).min(self.area.3 + 1);
        // edge function e(a, b, p) with a top-left rule: pixels on right/bottom edges are out
        let edge = |a: &Vertex, b: &Vertex, x: i32, y: i32| (b.x - a.x) as i64 * (y - a.y) as i64 - (b.y - a.y) as i64 * (x - a.x) as i64;
        let bias = |a: &Vertex, b: &Vertex| {
            let dy = b.y - a.y;
            let dx = b.x - a.x;
            // top edge (horizontal, going left in this orientation) or left edge: inclusive
            if dy < 0 || (dy == 0 && dx > 0) { 0 } else { -1 }
        };
        let (b0, b1, b2) = (bias(&v1, &v2), bias(&v2, &v0), bias(&v0, &v1));
        for y in miny..maxy {
            for x in minx..maxx {
                let w0 = edge(&v1, &v2, x, y);
                let w1 = edge(&v2, &v0, x, y);
                let w2 = edge(&v0, &v1, x, y);
                if w0 + b0 < 0 || w1 + b1 < 0 || w2 + b2 < 0 {
                    continue;
                }
                let lerp = |a: i32, b: i32, c: i32| ((w0 * a as i64 + w1 * b as i64 + w2 * c as i64 + area / 2) / area) as i32;
                let c = if gouraud {
                    [lerp(v0.c[0], v1.c[0], v2.c[0]), lerp(v0.c[1], v1.c[1], v2.c[1]), lerp(v0.c[2], v1.c[2], v2.c[2])]
                } else {
                    v0.c
                };
                let d = dither.then(|| DITHER[(y & 3) as usize][(x & 3) as usize]);
                let (col, stp) = match tex {
                    Some((tp, clut)) => {
                        let u = lerp(v0.u, v1.u, v2.u);
                        let vv = lerp(v0.v, v1.v, v2.v);
                        let t = self.texel(tp, clut, u & 0xFF, vv & 0xFF);
                        if t == 0 {
                            continue;
                        }
                        (self.modulate(t, c, modulate, d), t & 0x8000 != 0)
                    }
                    None => (std::array::from_fn(|i| (c[i] + d.unwrap_or(0)).clamp(0, 255) >> 3), false),
                };
                self.plot(x, y, col, stp, semi && (tex.is_none() || stp));
            }
        }
    }

    fn draw_line(&mut self, a: Vertex, b: Vertex, semi: bool, dither: bool) {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        if dx.abs() >= 1024 || dy.abs() >= 512 {
            return;
        }
        let steps = dx.abs().max(dy.abs()).max(1);
        for k in 0..=steps {
            let x = a.x + dx * k / steps;
            let y = a.y + dy * k / steps;
            if x < self.area.0 || x > self.area.2 || y < self.area.1 || y > self.area.3 {
                continue;
            }
            let c: [i32; 3] = std::array::from_fn(|i| a.c[i] + (b.c[i] - a.c[i]) * k / steps);
            let d = if dither { DITHER[(y & 3) as usize][(x & 3) as usize] } else { 0 };
            self.plot(x, y, c.map(|v| (v + d).clamp(0, 255) >> 3), false, semi);
        }
    }
}
