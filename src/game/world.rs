//! The race's 3D as the original decides it, drawn by the engine: the track chunks in its
//! build list, the cars and the parts it draws of each (lights, dents, reflections), the
//! traffic, the ground shadows and the tyre marks.

use super::*;

impl Game {
    pub fn frame_params(&self, aspect: f32) -> FrameParams {
        let fov = 2.0 * self.tan_half_v.atan();
        // reversed-Z projection (the renderer's depth test is Greater, cleared to 0)
        let squash = Mat4::from_scale(Vec3::new(1.0, self.y_scale, 1.0));
        let proj = squash * Mat4::perspective_infinite_reverse_rh(fov, aspect, 0.1);
        let view = Mat4::look_at_rh(self.cam_pos, self.cam_target, self.cam_up);
        let sky_view = Mat4::look_at_rh(Vec3::ZERO, self.cam_target - self.cam_pos, self.cam_up);
        FrameParams {
            world_ranges: self.chunk_list().map(|list| {
                let mut r = vec![self.track.global_range.clone()];
                for (chunk, flags, rez) in list {
                    let Some(b) = self.track.block_ranges.get(chunk) else { continue };
                    // flags: 1 ground (DrawW_DoTrough: full or low detail by the chunk's
                    // resolution, plus the detail surfaces), 2 objects (DrawW_DoObjects),
                    // 4 road lines (DrawW_DoLines)
                    if flags & 1 != 0 {
                        r.push(if rez != 0 { b[0].clone() } else { b[1].clone() });
                        r.push(b[2].clone());
                    }
                    if flags & 2 != 0 {
                        r.push(b[3].clone());
                    }
                    if flags & 4 != 0 {
                        r.push(b[4].clone());
                    }
                }
                r
            }),
            shadows: {
                let mut v = self.skids();
                v.extend(self.shadows());
                v
            },
            subtract_late: {
                let mut v = self.car_reflections().1;
                v.extend(self.car_arch_shadows());
                v
            },
            sky_dome: self.track.sky_dome as u32,
            sky_view_proj: squash * Mat4::perspective_rh(fov, aspect, 1.0, 1000.0) * sky_view,
            sky_top: self.track.sky_colors.0,
            view_proj: proj * view,
            cam_pos: self.cam_pos,
            fog_color: self.fog.0,
            fog_start: self.fog.1,
            fog_end: self.fog.2,
            cam_fwd: (self.cam_target - self.cam_pos).normalize_or(Vec3::Z),
            night_beam: {
                let snap = self.sim.snapshot();
                snap.night_beam.as_ref().filter(|b| b.flags & 1 != 0).map(|b| {
                let car = &self.racers[self.view_player].car;
                    crate::render::NightBeam {
                        origin: car.pos,
                        right: car.right(),
                        forward: car.forward(),
                        z0: b.z0 as f32,
                        xs: b.xs as f32,
                        zs: b.zs as f32,
                        data: std::sync::Arc::new(crate::render::NightBeam::pack(&b.map, &b.table, &snap.light_table)),
                    }
                })
            },
            depth_cue: {
                // the GTE far colour / depth-cue values the game set for this view
                let e = self.sim.snapshot().view_env;
                Some(crate::render::DepthCue {
                    far: Vec3::new(e[0] as f32, e[1] as f32, e[2] as f32) / 16.0,
                    dqa: if self.night_driving { 0.0 } else { e[3] as f32 },
                    dqb: if self.night_driving { 0.0 } else { e[4] as f32 },
                    h: e[5].max(1) as f32,
                })
            },
        }
    }

    /// Cars and camera-facing sprites transformed to world space.
    pub fn dynamic_geometry(&self) -> Vec<Vertex> {
        let mut out = Vec::new();
        // chunks whose objects are drawn
        let list = self.chunk_list();
        let objects_on = |block: usize| list.as_ref().is_none_or(|l| l.iter().any(|&(c, f, _)| c == block && f & 2 != 0));
        for s in self.track.sprites.iter().filter(|s| objects_on(s.block)) {
            let to_cam = (self.cam_pos - s.pos).with_y(0.0);
            if to_cam.length_squared() > self.fog.2 * self.fog.2 {
                continue;
            }
            let rot = Quat::from_rotation_y(to_cam.x.atan2(to_cam.z));
            for v in &s.verts {
                out.push(Vertex { pos: (s.pos + rot * Vec3::from(v.pos)).into(), ..*v });
            }
        }
        // keyframed scenery (windmill sails), looping through its frames (ticks per frame
        // counted in 60 Hz fields)
        let fields = self.sim.snapshot().field as f64;
        for a in &self.track.animated {
            let n = a.frames.len();
            let t = (fields / a.ticks as f64).rem_euclid(n as f64);
            let (k, f) = (t as usize % n, t.fract() as f32);
            let (p0, q0) = a.frames[k];
            let (p1, q1) = a.frames[(k + 1) % n];
            let (pos, rot) = (p0.lerp(p1, f), q0.slerp(q1, f));
            for v in &a.verts {
                out.push(Vertex { pos: (pos + rot * Vec3::from(v.pos)).into(), ..*v });
            }
        }
        for (i, r) in self.racers.iter().enumerate() {
            if i == self.view_player && self.cam_mode == CameraMode::Bumper {
                continue;
            }
            if r.car.pos.distance_squared(self.cam_pos) > self.fog.2 * self.fog.2 {
                continue;
            }
            let m = &self.models[r.model];
            let model = Mat4::from_rotation_translation(r.car.rot, r.car.pos);
            // the parts the game draws (R3DCar_ObjectVisible), once it has put cars into a view,
            // lit by the track's light at the car (DrawC: the average of its channels)
            let facets = self.car_facets();
            let (visible, tint, codes): (&[u8], f32, &[u16]) = match facets {
                Some(f) => match f.get(i).and_then(Option::as_ref) {
                    Some(f) => (&f.parts, car_light(f.light) as f32 / 128.0, &f.codes),
                    None => (&[], 1.0, &[]),
                },
                None => (&assets::CAR_PARTS_VISIBLE, 1.0, &[]),
            };
            let hide: Vec<usize> = std::env::var("NFSHS_HIDE_PART").map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
            for (pi, (part, &on)) in m.parts.iter().zip(visible).enumerate() {
                if on == 0 || on >= 17 || hide.contains(&pi) {
                    continue;
                }
                let tm = match part.wheel {
                    Some(w) => {
                        // the original: front wheels turned by wheelRot[0] and steered, rear by
                        // wheelRot[1] (R3DCar_InsertCarFacet)
                        let (rot, steer) = r.car.wheel_pose;
                        let (turn, steer) = (rot[(w >= 2) as usize], -steer);
                        let steer = if w < 2 { steer } else { 0.0 };
                        let spin = Quat::from_rotation_x(turn);
                        model * Mat4::from_rotation_translation(Quat::from_rotation_y(steer) * spin, part.hub)
                    }
                    None => model,
                };
                // parts sorted in front of their surroundings (negative bias) are drawn a little
                // towards the camera: the original orders by depth, not a depth buffer
                let pull = (-(assets::CAR_PART_BIAS.get(pi).copied().unwrap_or(0) as f32)).max(0.0) * 0.003;
                // the wheel arches' shadow is subtracted (drawn in the late subtract pass)
                if part.subtract {
                    continue;
                }
                for (t, tri) in part.verts.chunks_exact(3).enumerate() {
                    let w = [0, 1, 2].map(|k| tm.transform_point3(Vec3::from(tri[k].pos)));
                    if !front_facing(self.cam_pos, w) {
                        continue;
                    }
                    // a lit light or a dent: another texture, a lamp at full brightness
                    let sub = m.substitute(pi, t, codes);
                    let tint = if sub.is_some_and(|s| s.1) { 1.0 } else { tint };
                    for (k, (v, p)) in tri.iter().zip(w).enumerate() {
                        let c = v.color;
                        let p = p + (self.cam_pos - p).normalize_or_zero() * pull;
                        let uv = sub.map_or(v.uv, |s| s.0[k]);
                        out.push(Vertex { pos: p.into(), uv, color: [c[0] * tint, c[1] * tint, c[2] * tint, c[3]] });
                    }
                }
            }
        }
        out.extend(self.traffic_geometry());
        out.extend(self.car_reflections().0);
        out
    }

    /// The original's chunk build list this frame: (chunk, draw flags); None without one.
    pub(super) fn chunk_list(&self) -> Option<Vec<(usize, u8, u8)>> {
        let chunks = match &self.chunks_override {
            Some(c) => c.as_slice(),
            None => self.sim.snapshot().chunks.as_slice(),
        };
        // (chunk, draw flags, resolution)
        (!chunks.is_empty()).then(|| chunks.iter().filter(|c| c.0 >= 0).map(|&(c, f)| (c as usize, (f >> 8) as u8, f as u8)).collect())
    }

    /// What the game draws of each car this frame (None before it has put any car into a view).
    pub(super) fn car_facets(&self) -> Option<&[Option<crate::ps1::host::CarFacet>]> {
        let f = self.car_facets_override.as_deref().unwrap_or(&self.sim.snapshot().car_facets);
        f.iter().any(Option::is_some).then_some(f)
    }

    /// (R3DCar_ObjectVisible values 17 and up are light halos, drawn by the original's
    /// DrawC_PrimHalo as 2D, not the part's polygons.)
    /// The cars' reflections (DrawC_Prim's environment path, flags 9): each polygon of a
    /// reflection-mapped part again with reflection map A and, for polygons flagged 0x3F3,
    /// map B (scrolled), blended as the maps' texture pages say. Map A's coordinates come
    /// from the vertex normals turned by the camera's yaw around the car (RotMatrix of
    /// R3DCar_yawCam): u = u0 + 64 + x, v = v0 + |z|. Map B's from the normals as they are:
    /// u = u0 + 64 + y, v = v0 + scroll + z (low bytes), inside a texture window (GP0 E2,
    /// the car cache's packet at +72) that keeps them on the 128 x 64 map at (u0, v0).
    /// Returns (blended, subtractive).
    pub(super) fn car_reflections(&self) -> (Vec<Vertex>, Vec<Vertex>) {
        let (mut add, mut sub) = (Vec::new(), Vec::new());
        let (Some(facets), Some(page)) = (self.car_facets(), self.reflections) else { return (add, sub) };
        let atlas_size = self.atlas_size;
        for (i, (r, f)) in self.racers.iter().zip(facets).enumerate() {
            let Some(env) = f.as_ref().and_then(|f| f.env) else { continue };
            if i == self.view_player && self.cam_mode == CameraMode::Bumper {
                continue;
            }
            let m = &self.models[r.model];
            let visible = f.as_ref().map(|f| f.parts.as_slice()).unwrap_or(&[]);
            let model = Mat4::from_rotation_translation(r.car.rot, r.car.pos);
            let a = env.yaw as f32 / 4096.0 * std::f32::consts::TAU;
            let (sn, cs) = a.sin_cos();
            let rgb = [(env.colour & 0xFF) as f32 / 256.0, ((env.colour >> 8) & 0xFF) as f32 / 256.0, ((env.colour >> 16) & 0xFF) as f32 / 256.0];
            let uv = |u: u8, v: u8| [(page.x as f32 + u as f32 + 0.5) / atlas_size, (page.y as f32 + v as f32 + 0.5) / atlas_size];
            for (part, &on) in m.parts.iter().zip(visible) {
                let (Some((normals, flags)), true) = (&part.env, on != 0) else { continue };
                for (t, &fl) in flags.iter().enumerate() {
                    let tri = &part.verts[t * 3..t * 3 + 3];
                    // per corner: map A's (x, |z|), map B's (y, z), the position
                    let coords: Vec<([u8; 2], [u8; 2], Vec3)> = (0..3)
                        .map(|k| {
                            let n = normals[t * 3 + k];
                            let (nx, nz) = (n[0] as f32, n[2] as f32);
                            let x = (cs * nx + sn * nz) as i32;
                            let z = (-sn * nx + cs * nz) as i32;
                            let p = model.transform_point3(Vec3::from(tri[k].pos));
                            // a hair towards the camera, over the body it covers
                            let p = p + (self.cam_pos - p).normalize_or_zero() * 0.01;
                            ([x as u8, z.unsigned_abs() as u8], [n[1] as u8, n[2] as u8], p)
                        })
                        .collect();
                    for (map, scroll, needs) in [(env.a, 0u8, 0u16), (env.b, env.scroll as u8, 0x3F3)] {
                        let Some(map) = map else { continue };
                        if needs != 0 && fl & needs == 0 {
                            continue;
                        }
                        let (u0, v0) = (map[0] as u8, (map[0] >> 8) as u8);
                        let abr = (map[1] >> 16 >> 5) & 3;
                        for &(a, b, p) in &coords {
                            let (u, v) = if needs == 0 {
                                (u0.wrapping_add(64).wrapping_add(a[0]), v0.wrapping_add(a[1]))
                            } else {
                                let (u, v) = (u0.wrapping_add(64).wrapping_add(b[0]), v0.wrapping_add(scroll).wrapping_add(b[1]));
                                ((u & 0x7F) | (u0 & 0x80), (v & 0x3F) | (v0 & 0xC0))
                            };
                            let vert = Vertex {
                                pos: p.into(),
                                uv: uv(u, v),
                                color: [rgb[0], rgb[1], rgb[2], if abr == 1 { 0.0 } else { 0.5 }],
                            };
                            if abr == 2 { sub.push(vert) } else { add.push(vert) }
                        }
                    }
                }
            }
        }
        (add, sub)
    }

    /// The traffic cars as the original places them (its poses), the parts it draws.
    pub(super) fn traffic_geometry(&self) -> Vec<Vertex> {
        let mut out = Vec::new();
        let (poses, facets): (Vec<original::Pose>, Vec<Option<crate::ps1::host::CarFacet>>) = match &self.traffic_override {
            Some(o) => {
                let n = self.racers.len();
                let mut p = vec![original::Pose::default(); n];
                let mut f = vec![None; n];
                for (pose, facet) in o {
                    p.push(*pose);
                    f.push(Some(facet.clone()));
                }
                (p, f)
            }
            None => (self.sim.poses().0, self.sim.snapshot().car_facets.clone()),
        };
        for &(k, mi) in &self.traffic {
            let (Some(pose), Some(m)) = (poses.get(k), self.models.get(mi)) else { continue };
            if pose.pos.distance_squared(self.cam_pos) > self.fog.2 * self.fog.2 {
                continue;
            }
            if std::env::var_os("NFSHS_TRAFLOG").is_some() && pose.pos.distance(self.cam_pos) < 30.0 {
                eprintln!("traffic {k}: origin y {:.2} up {:.2?}", pose.pos.y, pose.up);
            }
            let facet = facets.get(k).and_then(Option::as_ref);
            // not put into the view by the original this frame, or its drawing set-up
            // (DrawC_PrimStart) refused the car (parked traffic waiting to enter): not drawn
            let Some(facet) = facet else { continue };
            let parked = self.sim.snapshot().cars.get(k).is_some_and(|c| c.parked);
            if self.traffic_override.is_none() && (facet.env.is_none() || parked) {
                continue;
            }
            let tint = car_light(facet.light) as f32 / 128.0;
            let model = Mat4::from_rotation_translation(pose.rot, pose.pos);
            for (pi, (part, &on)) in m.parts.iter().zip(&facet.parts).enumerate() {
                if on == 0 || on >= 17 || part.subtract {
                    continue;
                }
                let _ = pi;
                let tm = if part.wheel.is_some() { model * Mat4::from_translation(part.hub) } else { model };
                for (t, tri) in part.verts.chunks_exact(3).enumerate() {
                    let w = [0, 1, 2].map(|k| tm.transform_point3(Vec3::from(tri[k].pos)));
                    if !front_facing(self.cam_pos, w) {
                        continue;
                    }
                    let sub = m.substitute(pi, t, &facet.codes);
                    let tint = if sub.is_some_and(|s| s.1) { 1.0 } else { tint };
                    for (k, (v, p)) in tri.iter().zip(w).enumerate() {
                        let c = v.color;
                        let uv = sub.map_or(v.uv, |s| s.0[k]);
                        out.push(Vertex { pos: p.into(), uv, color: [c[0] * tint, c[1] * tint, c[2] * tint, c[3]] });
                    }
                }
            }
        }
        out
    }

    /// The cars' wheel-arch shadows (parts textured "shdw"), subtracted like the PS1 does.
    pub(super) fn car_arch_shadows(&self) -> Vec<Vertex> {
        let mut out = Vec::new();
        let facets = self.car_facets();
        for (i, r) in self.racers.iter().enumerate() {
            if i == self.view_player && self.cam_mode == CameraMode::Bumper {
                continue;
            }
            let m = &self.models[r.model];
            let visible: &[u8] = match facets {
                Some(f) => f.get(i).and_then(Option::as_ref).map(|f| f.parts.as_slice()).unwrap_or(&[]),
                None => &assets::CAR_PARTS_VISIBLE,
            };
            let model = Mat4::from_rotation_translation(r.car.rot, r.car.pos);
            let hide: Vec<usize> = std::env::var("NFSHS_HIDE_PART").map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
            for (pi, (part, &on)) in m.parts.iter().zip(visible).enumerate() {
                if on == 0 || on >= 17 || !part.subtract || hide.contains(&pi) {
                    continue;
                }
                for v in &part.verts {
                    out.push(Vertex { pos: model.transform_point3(Vec3::from(v.pos)).into(), ..*v });
                }
            }
        }
        out
    }

    /// A point on the road moved towards the camera along its view ray: the same place on
    /// screen, in front of the road (the PS1 draws the shadow over the road it lies on, sorted
    /// by depth, without a depth buffer). Far enough for the road's camber and slopes under a
    /// flat quad, not so far that kerbs and walls in front stop hiding it.
    pub(super) fn over_road(&self, p: Vec3) -> Vec3 {
        let to_cam = self.cam_pos - p;
        let d = to_cam.length();
        p + to_cam / d.max(1e-3) * (0.25 + d * 0.06).min(1.2).min(d * 0.5)
    }

    /// The cars' ground shadows (DrawC_ShadowPrim): the "shdw" texture on the quad
    /// Newton_CalcRealShadowCoordinates computed, coloured by the light, subtracted.
    pub(super) fn shadows(&self) -> Vec<Vertex> {
        let mut out = Vec::new();
        let Some(facets) = self.car_facets() else { return out };
        for (r, f) in self.racers.iter().zip(facets) {
            let (Some(f), Some(uv)) = (f, self.shadow_tex) else { continue };
            let Some((corners, rgb)) = f.shadow else { continue };
            let s = car_light(f.light);
            let c = rgb.map(|k| ((s * k as u32) >> 8) as f32 / 256.0);
            if corners.iter().all(|c| *c == [0; 3]) {
                continue;
            }
            // (the original has no depth buffer: keep the quad from sinking under the road)
            let ground = r.car.pos.y;
            let p = corners.map(|v| {
                let q = Vec3::new(-(v[0] as f32), v[1] as f32, v[2] as f32) / 65536.0;
                self.over_road(Vec3::new(r.car.pos.x + q.x, q.y.max(ground) + 0.03, r.car.pos.z + q.z))
            });
            for k in [0, 1, 3, 0, 3, 2] {
                out.push(Vertex { pos: p[k].into(), uv: uv[k], color: [c[0], c[1], c[2], 1.0] });
            }
        }
        // the traffic's shadows (their corners are relative to the car's simulated position)
        if let Some(uv) = self.shadow_tex {
            let snap = self.sim.snapshot();
            for &(k, _) in &self.traffic {
                let Some(f) = snap.car_facets.get(k).and_then(Option::as_ref) else { continue };
                if f.env.is_none() || snap.cars.get(k).is_some_and(|c| c.parked) {
                    continue;
                }
                let Some((corners, rgb)) = f.shadow else { continue };
                let Some(car) = snap.cars.get(k) else { continue };
                if corners.iter().all(|c| *c == [0; 3]) {
                    continue;
                }
                let s = car_light(f.light);
                let c = rgb.map(|k| ((s * k as u32) >> 8) as f32 / 256.0);
                let (cx, cz) = (-(car.position[0] as f32) / 65536.0, car.position[2] as f32 / 65536.0);
                let ground = car.position[1] as f32 / 65536.0 - car.dimension[1] as f32 / 65536.0;
                let p = corners.map(|v| {
                    let q = Vec3::new(-(v[0] as f32), v[1] as f32, v[2] as f32) / 65536.0;
                    self.over_road(Vec3::new(cx + q.x, q.y.max(ground) + 0.03, cz + q.z))
                });
                for k in [0, 1, 3, 0, 3, 2] {
                    out.push(Vertex { pos: p[k].into(), uv: uv[k], color: [c[0], c[1], c[2], 1.0] });
                }
            }
        }
        out
    }

    /// The tyre marks (Draw_kCtrlSkidmark): each entry's quad to the previous one, textured
    /// SKD0/SKD1 and subtracted, gouraud between the two entries' colours. Drawn for the
    /// track slices in view within 50 m (BWorld_IsSliceInBuildList, Draw_CircleClip).
    pub(super) fn skids(&self) -> Vec<Vertex> {
        let mut out = Vec::new();
        let snap = self.sim.snapshot();
        let (skids, chunks) = match &self.skids_override {
            Some(o) => (o, self.chunks_override.as_ref()),
            None => (&snap.skids, Some(&snap.chunks)),
        };
        let world = |p: [i32; 3]| Vec3::new(-(p[0] as f32), p[1] as f32, p[2] as f32) / 65536.0;
        // BWorld_IsSliceInBuildList: a slice is a track node, its chunk the node / 8
        let in_view = |slice: i16| chunks.is_none_or(|c| c.is_empty() || c.iter().any(|c| c.0 == slice / 8));
        if std::env::var_os("NFSHS_SKIDLOG").is_some() {
            let near = skids.iter().filter(|q| { let d = world(q.corners[0]) - self.cam_pos; d.x * d.x + d.z * d.z < 3600.0 }).count();
            let listed = skids.iter().filter(|q| in_view(q.slice)).count();
            let slices: std::collections::BTreeSet<i16> = skids.iter().map(|q| q.slice).collect();
            eprintln!("skids: {} quads, {near} near, {listed} in listed chunks; slices {slices:?}; chunks {:?}", skids.len(), chunks.map(|c| c.iter().map(|c| c.0).collect::<Vec<_>>()));
        }
        for q in skids {
            let Some(uv) = self.skid_tex[q.tex as usize & 1] else { continue };
            if !in_view(q.slice) {
                continue;
            }
            let p = q.corners.map(world);
            // Draw_CircleClip: within 50 m (its estimate: the larger of |dx|, |dz| plus a
            // quarter of the smaller)
            let d = (p[0] - self.cam_pos).abs();
            if d.x.max(d.z) + d.x.min(d.z) / 4.0 >= 50.0 {
                continue;
            }
            let p = p.map(|v| self.over_road(v + Vec3::Y * 0.02));
            let col = |c: u32| [(c & 0xFF) as f32 / 256.0, ((c >> 8) & 0xFF) as f32 / 256.0, ((c >> 16) & 0xFF) as f32 / 256.0, 1.0];
            // this entry's edge on the texture's top row, the previous entry's on its bottom
            let c = [col(q.colour[0]), col(q.colour[0]), col(q.colour[1]), col(q.colour[1])];
            for k in [0, 1, 3, 0, 3, 2] {
                out.push(Vertex { pos: p[k].into(), uv: uv[k], color: c[k] });
            }
        }
        out
    }
}

/// A car's flat lighting level from the track light (R3DCar_InsertCarFacetII: the average
/// of the light's channels, at least 24).
pub(super) fn car_light(light: u32) -> u32 {
    let (r, g, b) = (light & 0xFF, (light >> 8) & 0xFF, (light >> 16) & 0xFF);
    ((r + g + b) / 3).max(24)
}

/// A car polygon facing the camera: the PS1 draws the cars' polygons one-sided (DrawC_Prim
/// skips those whose NCLIP is not positive), so the underside and the far side are not seen.
fn front_facing(cam: Vec3, p: [Vec3; 3]) -> bool {
    (p[1] - p[0]).cross(p[2] - p[0]).dot(cam - p[0]) * CAR_WINDING > 0.0
}

/// The car models' winding in world space (their x is mirrored from the PS1's).
const CAR_WINDING: f32 = -1.0;
