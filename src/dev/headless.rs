//! The app's paths without a window: a race to a screenshot, and the whole game (front end,
//! its races) driven by a script.

use crate::app::{draw_front_end, draw_race, race_hud, race_of};
use crate::assets::{Atlas, GameData};
use crate::game::{Game, Input, RaceConfig};
use crate::render::Renderer;

/// SHOT_SIZE=WxH: the headless pictures' size (960x720 by default).
fn shot_size() -> (u32, u32) {
    std::env::var("SHOT_SIZE").ok().and_then(|v| v.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))).unwrap_or((960, 720))
}

/// Headless: simulate `seconds` with a scripted input and save a screenshot (`bilinear`:
/// with the 3D's textures filtered; `mipmaps`: from their mip levels; `supersample`: drawn at
/// that many times the size and averaged down; `msaa`: samples per pixel).
#[allow(clippy::too_many_arguments)]
pub fn screenshot(data: &GameData, cfg: &RaceConfig, out: &std::path::Path, seconds: f32, input: Input, taps: &[(char, f32)], bilinear: bool, mipmaps: bool, supersample: u32, msaa: u32) -> anyhow::Result<()> {
    let mut atlas = Atlas::race();
    let mut game = Game::load(data, &mut atlas, cfg)?;
    log::info!("race atlas: {:.0}% used", atlas.used() * 100.0);
    let (w, h) = shot_size();
    let (wf, hf) = (w as f32, h as f32);
    let mut r = pollster::block_on(Renderer::new(None, (w, h), &atlas))?;
    r.bilinear = bilinear;
    r.mipmaps = mipmaps;
    r.supersample = supersample;
    r.msaa = msaa;
    r.set_world(&game.track.verts, &game.track.indices);
    let mut t = 0.0;
    let mut next_log = 10.0;
    let mut flipped = false;
    let trace = std::env::var_os("SHOT_TRACE").is_some();
    // SHOT_SCRIPT="t:keys,...": from second t on, these keys (as --keys) instead
    let script: Vec<(f32, Input)> = std::env::var("SHOT_SCRIPT")
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| {
            let (at, k) = s.split_once(':')?;
            let has = |c| k.contains(c);
            Some((at.parse().ok()?, Input { up: has('u'), down: has('d'), left: has('l'), right: has('r'), handbrake: has('h'), autopilot: has('a'), ..Default::default() }))
        })
        .collect();
    while t < seconds {
        let before = game.player().car.pos;
        let mut input = script.iter().rev().find(|s| s.0 <= t).map_or(input, |s| s.1);
        for &(k, at) in taps {
            if (at..at + 0.1).contains(&t) {
                match k {
                    's' => input.start = true,
                    'd' => input.down = true,
                    'u' => input.up = true,
                    'x' => input.accept = true,
                    _ => {}
                }
            }
        }
        // SHOT_DT=SECS: the render step (a faster display than 60 Hz)
        let dt = std::env::var("SHOT_DT").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0 / 60.0);
        game.update(dt, input);
        t += dt;
        // SHOT_CAMLOG=1: each rendered frame's camera and player car (interpolated), to look for
        // jitter: the camera's step and its offset from the car
        if std::env::var_os("SHOT_CAMLOG").is_some() {
            if let (cars, Some(cam)) = game.sim.poses() {
                let car = cars[0].pos;
                println!("camlog {t:.4} cam {:.3} {:.3} {:.3} car {:.3} {:.3} {:.3} fwd {:.4} {:.4} {:.4} speed {:.1}", cam.pos.x, cam.pos.y, cam.pos.z, car.x, car.y, car.z, cam.forward.x, cam.forward.y, cam.forward.z, cars[0].vel.length() * 2.237);
            }
        }
        // NFSHS_VRAMDUMP=FILE: the original's VRAM (1024 x 512 16-bit) at the end
        if t + 1.0 / 60.0 >= seconds {
            if let (Some(path), Some(v)) = (std::env::var_os("NFSHS_VRAMDUMP"), game.sim.snapshot().vram.clone()) {
                std::fs::write(path, v.iter().flat_map(|p| p.to_le_bytes()).collect::<Vec<u8>>())?;
            }
        }
        // NFSHS_SCROLLLOG=1: the scrolling track textures' offsets every 10 frames
        if std::env::var_os("NFSHS_SCROLLLOG").is_some() && ((t * 60.0) as u32).is_multiple_of(10) {
            println!("  t={t:5.2} texture scroll {:?}", game.sim.snapshot().texture_scroll);
        }
        // NFSHS_LIGHTTABLELOG=1: a checksum of the original's light table every 5 s
        if std::env::var_os("NFSHS_LIGHTTABLELOG").is_some() && ((t * 60.0) as u32).is_multiple_of(300) {
            let lt = &game.sim.snapshot().light_table;
            let sum = lt.iter().fold(0u64, |a, &v| a.wrapping_mul(31).wrapping_add(v as u64));
            println!("  t={t:5.1} light table {sum:016x} [{:06x} {:06x} {:06x} {:06x}]", lt.get(16).unwrap_or(&0) & 0xFF_FFFF, lt.get(64).unwrap_or(&0) & 0xFF_FFFF, lt.get(128).unwrap_or(&0) & 0xFF_FFFF, lt.get(200).unwrap_or(&0) & 0xFF_FFFF);
        }
        // NFSHS_SKYLOG=1: the original's sky dome colours (zenith .. horizon) every 2 s
        if std::env::var_os("NFSHS_SKYLOG").is_some() && ((t * 60.0) as u32).is_multiple_of(120) {
            if let Some(sky) = game.sim.snapshot().sky.clone() {
                let e = game.sim.snapshot().view_env;
                println!("  t={t:5.1} sky {:?} band top {:?} far colour {:?} dqa {} dqb {} h {}", (0..5).map(|r| sky.dome_colors[r * 17 + 8]).collect::<Vec<_>>(), sky.top_colors.get(8), [e[0] / 16, e[1] / 16, e[2] / 16], e[3], e[4], e[5]);
            }
        }
        // NFSHS_FLIPLOG=1: when the car tips over (up vector far from vertical), where and how
        if std::env::var_os("NFSHS_FLIPLOG").is_some() {
            let c = &game.player().car;
            let up = c.up.y;
            if up < 0.7 && !flipped {
                println!("  t={t:6.2} TIPPED up={up:.2} pos={:.1?} speed={:.0} km/h", c.pos, c.speed() * 3.6);
            }
            flipped = up < 0.7;
        }
        if trace && seconds - t < 0.5 {
            println!("  t={t:6.3} moved {:.3} m", game.player().car.pos.distance(before));
        }
        // SHOT_SEQ=N: also the last N frames (OUT_seq<k>.png), to compare frame to frame
        if let Some(n) = std::env::var("SHOT_SEQ").ok().and_then(|v| v.parse::<f32>().ok()) {
            let k = ((seconds - t) / dt).round();
            if k < n {
                let params = game.frame_params(wf / hf);
                let hud = race_hud(&mut r, &game, wf, hf);
                let img = r.screenshot(&params, &game.track.sky, &game.dynamic_geometry(), &hud);
                img.save(format!("{}_seq{:02}.png", out.with_extension("").display(), (n - 1.0 - k) as u32))?;
            }
        }
        // SHOT_SERIES=SECS: also a picture every SECS seconds (OUT_<t>.png), to find a place
        if let Some(every) = std::env::var("SHOT_SERIES").ok().and_then(|v| v.parse::<f32>().ok()) {
            if (t / every).floor() > ((t - dt) / every).floor() {
                let params = game.frame_params(wf / hf);
                let hud = race_hud(&mut r, &game, wf, hf);
                let img = r.screenshot(&params, &game.track.sky, &game.dynamic_geometry(), &hud);
                let stem = out.with_extension("");
                img.save(format!("{}_{:03}.png", stem.display(), t.round() as u32))?;
                let p = game.player().car.pos;
                println!("  t={t:5.1} shot at pos ({:.1}, {:.1}, {:.1})", p.x, p.y, p.z);
            }
        }
        if t >= next_log {
            next_log += 10.0;
            let p = game.player();
            let order: Vec<String> = game.standings().iter().map(|&i| format!("{}:{}/{}", game.racers[i].name, game.racers[i].laps_done, game.racers[i].progress)).collect();
            println!("  t={t:5.1} {:?} speed={:.0} km/h  order={}", game.phase, p.car.speed() * 3.6, order.join(" "));
        }
    }
    // NFSHS_OBJLOG=1: the track objects the original drew in the last frame
    if std::env::var_os("NFSHS_OBJLOG").is_some() {
        for o in &game.sim.snapshot().objects {
            println!("  object model {:3} at ({:8.1}, {:6.1}, {:8.1}){} matrix {:?}", o.model, -(o.pos[0] as f32) / 65536.0, o.pos[1] as f32 / 65536.0, o.pos[2] as f32 / 65536.0, if o.billboard { " billboard" } else { "" }, o.matrix);
        }
    }
    // NFSHS_GLOWLOG=1: the light glows the original drew in the last frame (world anchor)
    if std::env::var_os("NFSHS_GLOWLOG").is_some() {
        for g in game.sim.snapshot().glows.iter().filter(|g| !g.smoke) {
            println!("  glow at ({:8.1}, {:6.1}, {:8.1}) car {:?}, {} commands", -(g.anchor[0] as f32) / 65536.0, g.anchor[1] as f32 / 65536.0, g.anchor[2] as f32 / 65536.0, g.car, g.cmds.len());
        }
    }
    let p = game.player();
    println!(
        "after {seconds}s: pos={:?} speed={:.1} km/h progress={}/{}",
        p.car.pos,
        p.car.speed() * 3.6,
        p.progress,
        game.path.len(),
    );
    // NFSHS_CAM="x,y,z,tx,ty,tz": look from a fixed point (debugging); several cameras
    // separated by ';' save OUT_cam<i>.png each
    if let Ok(c) = std::env::var("NFSHS_CAM") {
        let cams: Vec<Vec<f32>> = c.split(';').map(|c| c.split(',').filter_map(|x| x.trim().parse().ok()).collect()).filter(|v: &Vec<f32>| v.len() == 6).collect();
        for (i, v) in cams.iter().enumerate() {
            game.cam_pos = glam::Vec3::new(v[0], v[1], v[2]);
            game.cam_target = glam::Vec3::new(v[3], v[4], v[5]);
            game.cam_up = glam::Vec3::Y;
            if cams.len() > 1 {
                let params = game.frame_params(wf / hf);
                let hud = race_hud(&mut r, &game, wf, hf);
                let img = r.screenshot(&params, &game.track.sky, &game.dynamic_geometry(), &hud);
                img.save(format!("{}_cam{i}.png", out.with_extension("").display()))?;
            }
        }
    }
    super::refcmp::pick_quads(&game, data, cfg.track, cfg.variant, w, h)?;
    let params = game.frame_params(wf / hf);
    let hud = race_hud(&mut r, &game, wf, hf);
    let img = r.screenshot(&params, &game.track.sky, &game.dynamic_geometry(), &hud);
    img.save(out)?;
    Ok(())
}

/// Headless: the app's own path through the original game (front end, its race, back), with
/// pad input per stretch of frames (`script`: seconds and an Input) and screenshots of the
/// frames in `shots` written to `dir`.
pub fn front_end_play(data: &GameData, script: &[(f32, Input)], shots: &[u32], dir: &std::path::Path) -> anyhow::Result<()> {
    use crate::game::original::OriginalSim;
    let (w, h) = shot_size();
    let mut sim = Some(Box::new(OriginalSim::start_main(data)?));
    let mut game: Option<Box<Game>> = None;
    let mut showroom: Option<crate::game::fe::Showroom> = None;
    let mut r = pollster::block_on(Renderer::new(None, (w, h), &Atlas::new(256)))?;
    let total: u32 = script.iter().map(|(s, _)| (s * 60.0).round() as u32).sum();
    let mut frame = 0u32;
    for &(secs, input) in script {
        for _ in 0..(secs * 60.0).round() as u32 {
            frame += 1;
            if let Some(s) = &mut sim {
                s.advance(1.0 / 60.0, &input);
                if s.stopped() {
                    println!("frame {frame}: the original stopped");
                    return Ok(());
                }
                if !s.in_front_end() {
                    let mut s = sim.take().unwrap();
                    s.reset_race();
                    let cfg = race_of(s.snapshot());
                    println!("frame {frame}: race {}{} with {} vs {:?}, {} laps (cars {:?})", cfg.track, cfg.variant, cfg.player_car, cfg.opponents, cfg.laps, s.snapshot().cars.iter().map(|c| c.code.clone()).collect::<Vec<_>>());
                    let rc = RaceConfig { track: cfg.track, variant: &cfg.variant, player_car: &cfg.player_car, opponents: cfg.opponents.clone(), laps: cfg.laps, traffic: Vec::new() };
                    let mut atlas = Atlas::race();
                    let g = Game::load_with(data, &mut atlas, &rc, *s)?;
                    r.set_atlas(&atlas);
                    r.set_world(&g.track.verts, &g.track.indices);
                    game = Some(Box::new(g));
                }
            } else if let Some(g) = &mut game {
                g.update(1.0 / 60.0, input);
                if std::env::var_os("NFSHS_BEAMLOG").is_some() && frame.is_multiple_of(60) {
                    if let Some(b) = g.sim.snapshot().night_beam.clone() {
                        let nz = b.map.iter().filter(|&&v| v != 0).count();
                        println!("frame {frame}: beam flags {:#x} z0 {} xs {} zs {}, map cells lit {nz}, max {}", b.flags, b.z0, b.xs, b.zs, b.map.iter().max().unwrap_or(&0));
                    }
                }
                if std::env::var_os("NFSHS_CARSLOG").is_some() && frame.is_multiple_of(60) {
                    let s = g.sim.snapshot();
                    let p = s.cars[0].position;
                    let d = |c: &crate::ps1::race::CarState| (((c.position[0] - p[0]) as f64).hypot((c.position[2] - p[2]) as f64) / 65536.0) as i32;
                    println!("frame {frame}: speed {:.0} km/h, cars {:?}", s.cars[0].speed as f32 / 65536.0 * 3.6, s.cars.iter().skip(1).map(|c| format!("{} {}m", c.code, d(c))).collect::<Vec<_>>());
                    for (k, v) in g.sim.frame_snapshot().views.iter().enumerate() {
                        let m = |a: [i32; 3]| a.map(|x| x >> 16);
                        println!("    view {k}: player {} camera {:?} fwd {:?} car0 {:?} fwd {:?} car1 {:?}", v.player, m(v.camera_pos), &v.camera_rot[6..9], m(s.cars[0].position), &s.cars[0].orient[6..9], s.cars.get(1).map(|c| m(c.position)));
                    }
                }
                if std::env::var_os("NFSHS_PARTSLOG").is_some() && frame.is_multiple_of(std::env::var("NFSHS_PARTSLOG").ok().and_then(|v| v.parse().ok()).unwrap_or(30)) {
                    let snap = g.sim.snapshot();
                    for (k, f) in snap.car_facets.iter().enumerate() {
                        let Some(f) = f else { continue };
                        let c = &snap.cars[k];
                        println!("frame {frame}: car {k} {} pos {:?} orient {:?} parts {}", c.code, c.position.map(|v| v >> 16), &c.orient[..3], f.parts.iter().enumerate().filter(|(_, v)| **v != 0).map(|(i, v)| format!("{i}:{v}")).collect::<Vec<_>>().join(" "));
                    }
                }
                if std::env::var_os("NFSHS_FLIPLOG").is_some() {
                    let c = &g.player().car;
                    if c.up.y < 0.7 {
                        println!("frame {frame}: TIPPED up={:.2} pos={:.1?} speed={:.0} km/h", c.up.y, c.pos, c.speed() * 3.6);
                    }
                }
                if g.sim.in_front_end() {
                    println!("frame {frame}: back in the front end");
                    sim = game.take().map(|g| Box::new(g.sim));
                    r.clear_world();
                }
            }
            if !shots.contains(&frame) && frame != total {
                continue;
            }
            let out = dir.join(format!("play_{frame}.png"));
            if let Some(s) = &sim {
                let snap = s.snapshot();
                let Some(vram) = &snap.vram else { continue };
                let codes = crate::game::fe::FeScene::car_codes(&snap.overlay);
                if !codes.is_empty() && showroom.as_ref().is_none_or(|sr| sr.cars != codes) {
                    showroom = crate::game::fe::Showroom::load(data, &codes);
                }
                let scene = crate::game::fe::FeScene::build(&snap.overlay, vram, (snap.display[2], snap.display[3]), w as f32, h as f32, showroom.as_ref());
                if let Some(img) = draw_front_end(&mut r, scene, true) {
                    img.save(&out)?;
                }
            } else if let Some(g) = &mut game {
                if let Some(img) = draw_race(&mut r, g, w as f32, h as f32, true) {
                    img.save(&out)?;
                }
            }
            println!("frame {frame}: {} written", out.display());
        }
    }
    Ok(())
}
