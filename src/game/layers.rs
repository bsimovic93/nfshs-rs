//! The race's 2D as the original drew it (recorded): the HUD, the sun's flare, the light
//! glows and smoke, the weather, placed for the engine's camera.

use super::*;

/// A light glow from one frame to the next: its world anchor, its car and the view's place in
/// the window (split screen).
pub(super) type GlowKey = ([i32; 3], u32, [i32; 2]);

/// The world anchor standing for the sun in [`Game::glow_offsets`].
const SUN_KEY: [i32; 3] = [i32::MIN; 3];
/// The lens flare's depth among the glows: nearer than anything (the original puts it in its
/// ordering table's nearest entry).
const LENS_DEPTH: f32 = 0.9999;

/// The centre of the quads among `cmds` (PS1 screen pixels).
fn quads_centre<'a>(cmds: impl IntoIterator<Item = &'a crate::ps1::host::OverlayCmd>) -> Option<Vec2> {
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for c in cmds {
        if let crate::ps1::host::OverlayCmd::Quad { pts, .. } = c {
            for p in pts {
                let v = Vec2::new(p[0] as f32, p[1] as f32);
                (lo, hi) = (lo.min(v), hi.max(v));
            }
        }
    }
    (lo.x <= hi.x).then(|| (lo + hi) * 0.5)
}

impl Game {
    /// The HUD as the original drew it this frame (its views' primitives, recorded), when it
    /// was recorded: 2D for a `w` x `h` window with its own atlas.
    pub fn hud_frame(&self, w: f32, h: f32) -> Option<(fe::FeFrame, Vec<Vertex>, Vec<Vertex>, Vec<Vertex>)> {
        let s = self.sim.snapshot();
        let vram = s.vram.as_ref()?;
        let screen = (s.display[2].max(1), s.display[3].max(1));
        // the sky layer first (its own vertices), the light glows, then the HUD, in one
        // atlas; paused, the HUD is the host's (empty here)
        let (atlas, sky, glows, glows_sub) = self.layers(crate::assets::Atlas::new(1024), w, h, [0.0, 0.0, w, h], [0, 0])?;
        // the weather (rain streaks, drops on the lens) under the HUD
        let mut cmds = s.weather.clone();
        if !s.paused {
            cmds.extend(s.overlay.iter().cloned());
        }
        let hud = fe::FeFrame::build_in(atlas, &cmds, vram, screen, w, h);
        Some((hud, sky, glows, glows_sub))
    }

    /// The current view's 2D (split screen) in a `w` x `h` window, the view at `vp` there:
    /// its sky layer, glows (normal and subtracted) and weather, added to `atlas`.
    #[allow(clippy::type_complexity)]
    pub fn view_2d(&self, atlas: crate::assets::Atlas, w: f32, h: f32, vp: [f32; 4]) -> Option<(fe::FeFrame, Vec<Vertex>, Vec<Vertex>, Vec<Vertex>)> {
        let s = self.sim.snapshot();
        let vram = s.vram.as_ref()?;
        let screen = (s.display[2].max(1), s.display[3].max(1));
        let (rect, _) = self.view_rect(self.sim.view());
        let ofs = [rect[0] as i16, rect[1] as i16];
        let (atlas, sky, glows, glows_sub) = self.layers(atlas, w, h, vp, ofs)?;
        let weather: Vec<_> = s.weather.iter().map(|c| c.offset(ofs[0], ofs[1])).collect();
        Some((fe::FeFrame::build_in(atlas, &weather, vram, screen, w, h), sky, glows, glows_sub))
    }

    /// The HUD over the whole window (split screen: after the views), added to `atlas`.
    pub fn hud_2d(&self, atlas: crate::assets::Atlas, w: f32, h: f32) -> Option<fe::FeFrame> {
        let s = self.sim.snapshot();
        let vram = s.vram.as_ref()?;
        let screen = (s.display[2].max(1), s.display[3].max(1));
        let cmds = if s.paused { &[][..] } else { &s.overlay[..] };
        Some(fe::FeFrame::build_in(atlas, cmds, vram, screen, w, h))
    }

    /// The current view's sky layer and light glows for a `w` x `h` window, the view's 3D at
    /// `vp` there and its 2D at `ofs` on the PS1 screen.
    #[allow(clippy::type_complexity)]
    pub(super) fn layers(&self, atlas: crate::assets::Atlas, w: f32, h: f32, vp: [f32; 4], ofs: [i16; 2]) -> Option<(crate::assets::Atlas, Vec<Vertex>, Vec<Vertex>, Vec<Vertex>)> {
        let sim = &self.sim;
        let s = sim.snapshot();
        let vram = s.vram.as_ref()?;
        let screen = (s.display[2].max(1), s.display[3].max(1));
        let moved = |cmds: &[crate::ps1::host::OverlayCmd]| -> Vec<crate::ps1::host::OverlayCmd> { cmds.iter().map(|c| c.offset(ofs[0], ofs[1])).collect() };
        let mut sky = fe::FeFrame::build_in(atlas, &moved(&s.sky_overlay), vram, screen, w, h);
        let mut atlas = std::mem::replace(&mut sky.atlas, crate::assets::Atlas::new(1));
        let mut glows = Vec::new();
        // (the subtracted ones: PS1 semi-transparency mode 2, the smoke's dark puffs)
        let mut glows_sub = Vec::new();
        // each glow moved from where the original's camera saw its light to where this
        // frame's (interpolated) camera sees it, and set at the depth the original sorts it
        // at (half way to the light: drawn over what is further, behind what is nearer)
        let aspect = vp[2] / vp[3].max(1.0);
        let now = self.frame_params(aspect).view_proj;
        let orig = sim.sim_camera().map(|c| self.view_proj_of(&c, aspect));
        let to_px = |m: Mat4, p: Vec3| -> Option<(Vec2, f32)> {
            let c = m * p.extend(1.0);
            (c.w > 0.05).then(|| (Vec2::new(vp[0] + (c.x / c.w + 1.0) * 0.5 * vp[2], vp[1] + (1.0 - c.y / c.w) * 0.5 * vp[3]), c.z / c.w))
        };
        let poses = sim.poses().0;
        // the PS1 puts its 2D on whole pixels of its 320 x 240 screen: a light's glow is up to
        // a pixel off its light, differently every frame, which this picture's finer pixels
        // show as a wobble. Each glow keeps a smoothed offset from its light's exact
        // projection and is drawn at that, never more than a PS1 pixel from where the PS1 drew
        // it (a car's halos move with its lights, which the smoothing must not lag behind).
        let (aw, ah) = if w * 3.0 > h * 4.0 { (h * 4.0 / 3.0, h) } else { (w, w * 0.75) };
        let scale = Vec2::new(aw / screen.0 as f32, ah / screen.1 as f32);
        let origin = Vec2::new((w - aw) / 2.0, (h - ah) / 2.0);
        let view_at = [vp[0] as i32, vp[1] as i32];
        let mut offsets = self.glow_offsets.borrow_mut();
        let mut seen = Vec::new();
        let mut steadied = |key: GlowKey, centre: Option<Vec2>, p_orig: Vec2| -> Vec2 {
            let Some(centre) = centre else { return Vec2::ZERO };
            seen.push(key);
            let off = (origin + centre * scale - p_orig) / scale;
            let e = offsets.entry(key).or_insert((off, s.frame));
            // (another shape, or the glow came back: start over)
            if (off - e.0).length() > 4.0 {
                e.0 = off;
            } else if e.1 != s.frame {
                e.0 += (off - e.0) * 0.15;
            }
            e.1 = s.frame;
            (off - e.0).clamp(Vec2::splat(-1.0), Vec2::splat(1.0)) * scale
        };
        // the sun as seen by this frame's camera: its flare moved from where the original's
        // camera saw the sun's direction to where this one does (the sky is drawn with it)
        let mut sun_moved = None;
        if let (Some(orig_vp), Some(cam)) = (orig, sim.sim_camera()) {
            let dir = Vec3::new(-(s.sun_dir[0] as f32), s.sun_dir[1] as f32, s.sun_dir[2] as f32).normalize_or_zero() * 500.0;
            if let (Some((p_now, _)), Some((p_orig, _))) = (to_px(now, self.cam_pos + dir), to_px(orig_vp, cam.pos + dir)) {
                let first = s.sky_overlay.iter().find(|c| matches!(c, crate::ps1::host::OverlayCmd::Quad { .. }));
                let centre = quads_centre(first.map(|c| c.offset(ofs[0], ofs[1])).as_ref());
                let d = p_now - p_orig - steadied((SUN_KEY, 0, view_at), centre, p_orig);
                for v in &mut sky.vertices {
                    v.pos[0] += d.x;
                    v.pos[1] += d.y;
                }
                sun_moved = Some((d, centre));
            }
        }
        for g in &s.glows {
            let raw = Vec3::new(-(g.anchor[0] as f32), g.anchor[1] as f32, g.anchor[2] as f32) / 65536.0;
            // a car's light follows the drawn (interpolated) car; its size keeps the glow in
            // front of the car's own body
            let car = g.car.and_then(|a| s.cars.iter().position(|c| c.addr == a));
            let (now_p, radius) = match car {
                Some(k) if k < self.racers.len() => (raw + (self.racers[k].car.pos - original::pose(&s.cars[k]).pos), self.models[self.racers[k].model].half_extents.length()),
                Some(k) => {
                    let drawn = poses.get(k).map_or(raw, |p| raw + (p.pos - original::pose(&s.cars[k]).pos));
                    let size = self.traffic.iter().find(|t| t.0 == k).and_then(|t| self.models.get(t.1)).map_or(2.5, |m| m.half_extents.length());
                    (drawn, size)
                }
                None => (raw, 0.0),
            };
            let (Some(orig_vp), Some((p_now, _))) = (orig, to_px(now, now_p)) else { continue };
            let Some((p_orig, _)) = to_px(orig_vp, raw) else { continue };
            // (a puff is reprojected afterwards: not cut at the original's screen edges)
            let f = fe::FeFrame::build_clipped(atlas, &moved(&g.cmds), vram, screen, w, h, !g.smoke);
            atlas = f.atlas;
            // the depth it is tested at: a light on a car just in front of that car (the PS1
            // draws the halos over the body); any other just in front of its light, by twice
            // the glow's own size (the PS1 sorts it at its light's depth: over its lamp and the
            // wall it is on, seen however slanting, and under whatever is nearer)
            let dist = now_p.distance(self.cam_pos);
            let toward = if radius > 0.0 {
                (radius + 0.2).min(dist * 0.9)
            } else {
                let (lo, hi) = f.vertices.iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(lo, hi), v| (lo.min(Vec2::new(v.pos[0], v.pos[1])), hi.max(Vec2::new(v.pos[0], v.pos[1]))));
                let half_px = if hi.x >= lo.x { ((hi - lo) * 0.5).max_element() } else { 0.0 };
                let size = half_px * 2.0 * dist * self.tan_half_v / vp[3].max(1.0);
                (2.0 * size + 1.0).min(dist * 0.5)
            };
            let front = now_p + (self.cam_pos - now_p).normalize_or_zero() * toward;
            let Some((_, depth)) = to_px(now, front) else { continue };
            let d = if g.smoke { p_now - p_orig } else { p_now - p_orig - steadied((g.anchor, g.car.unwrap_or(0), view_at), quads_centre(&moved(&g.cmds)), p_orig) };
            if g.smoke && std::env::var_os("NFSHS_NOSMOKE").is_some() {
                continue;
            }
            let is_sub = |i: usize| f.subtract.iter().any(|r| r.contains(&(i as u32)));
            if !g.smoke {
                for (i, v) in f.vertices.iter().enumerate() {
                    let v = Vertex { pos: [v.pos[0] + d.x, v.pos[1] + d.y, 1.0 - 2.0 * depth], ..*v };
                    if is_sub(i) { glows_sub.push(v) } else { glows.push(v) }
                }
                continue;
            }
            // a smoke puff, close to the camera: each corner taken back into the world at the
            // puff's depth as the original's camera saw it, then seen by this frame's camera
            let Some((_, z_orig)) = to_px(orig_vp, raw) else { continue };
            let (orig_inv, inv) = (orig_vp.inverse(), now.inverse());
            let ndc = |x: f32, y: f32| Vec2::new((x - vp[0]) / vp[2] * 2.0 - 1.0, 1.0 - (y - vp[1]) / vp[3] * 2.0);
            // the puff is a flat picture standing on the ground: below its anchor it covers the
            // road nearer the camera, which must not hide it (the PS1 draws it whole, sorted by
            // its anchor), so there it takes the depth of the road it covers
            let road_depth = |x: f32, y: f32| -> Option<f32> {
                let n = ndc(x, y);
                let near = inv.project_point3(n.extend(1.0));
                let dir = (inv.project_point3(n.extend(0.5)) - near).normalize_or_zero();
                let t = (raw.y + 0.05 - near.y) / dir.y;
                (dir.y < -1e-4 && t > 0.0).then(|| to_px(now, near + dir * t).map(|(_, z)| z)).flatten()
            };
            for (i, &v) in f.vertices.iter().enumerate() {
                let world = orig_inv.project_point3(ndc(v.pos[0], v.pos[1]).extend(z_orig));
                // (a vertex is never dropped: the list is triangles)
                let p = to_px(now, world).map_or(Vec2::new(v.pos[0] + d.x, v.pos[1] + d.y), |(p, _)| p);
                let pulled = world + (self.cam_pos - world).normalize_or_zero() * 0.5;
                let z = to_px(now, pulled).map_or(depth, |(_, z)| z);
                let z = match road_depth(p.x, p.y) {
                    Some(r) if r > z => r,
                    _ => z,
                };
                let v = Vertex { pos: [p.x, p.y, 1.0 - 2.0 * z], ..v };
                if is_sub(i) { glows_sub.push(v) } else { glows.push(v) }
            }
        }
        // the lens flare over the world, each piece following the sun as the original placed
        // it (mirrored through the screen's centre), and where the sun (the white quad the
        // original measures the flare by, drawn first) is tested
        self.sun_probe.set(None);
        if let (Some(lens), Some((d, centre))) = (&s.lens_flare, sun_moved) {
            if let Some(c) = centre {
                self.sun_probe.set(Some(((origin + c * scale + d).into(), scale.into())));
            }
            for p in lens {
                let f = fe::FeFrame::build_in(atlas, &moved(&p.cmds), vram, screen, w, h);
                atlas = f.atlas;
                let is_sub = |i: usize| f.subtract.iter().any(|r| r.contains(&(i as u32)));
                for (i, v) in f.vertices.iter().enumerate() {
                    let v = Vertex { pos: [v.pos[0] + d.x * p.shift, v.pos[1] + d.y * p.shift, 1.0 - 2.0 * LENS_DEPTH], ..*v };
                    if is_sub(i) { glows_sub.push(v) } else { glows.push(v) }
                }
            }
        }
        offsets.retain(|k, _| k.2 != view_at || seen.contains(k));
        Some((atlas, sky.vertices, glows, glows_sub))
    }

    /// Where the sun is to be tested for the original's lens flare (see
    /// [`crate::ps1::host::SUN_VISIBLE`]): its centre in window pixels and a PS1 pixel's size
    /// there, when the flare was drawn this frame (set by the 2D's layers).
    pub fn sun_probe(&self) -> Option<([f32; 2], [f32; 2])> {
        self.sun_probe.get()
    }

    /// The view-projection of a camera the original computed (as `frame_params` builds it).
    pub(super) fn view_proj_of(&self, c: &original::Camera, aspect: f32) -> Mat4 {
        let tan_half_v = c.tan_half_h * self.view_ratio;
        let fov = 2.0 * tan_half_v.atan();
        let squash = Mat4::from_scale(Vec3::new(1.0, c.y_scale, 1.0));
        let proj = squash * Mat4::perspective_infinite_reverse_rh(fov, aspect, 0.1);
        proj * Mat4::look_at_rh(c.pos, c.pos + c.forward, c.up)
    }

    /// The host's port of the original HUD (shown while the race is paused, under the pause
    /// menu, when the recorded 2D is kept).
    pub fn hud(&self, w: f32, h: f32) -> Vec<Vertex> {
        let Some(hud) = &self.ps1hud else { return Vec::new() };
        let s = self.sim.snapshot();
        let frame = ps1hud::HudFrame {
            dash: &s.dash,
            layout: &s.hud_layout,
            map: &s.map,
            blink: (s.sim_ticks >> 4) & 1 != 0,
            countdown: s.countdown,
            countdown_ticks: s.ticks.wrapping_sub(s.countdown_start) as i32,
            overlay: &s.overlay,
        };
        hud.vertices(&frame, w, h)
    }
}
