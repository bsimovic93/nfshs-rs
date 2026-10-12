//! Side-by-side comparison with a reference frame captured from an emulator
//! (`tools/dsref.py`): the engine renders the same moment from the RAM dump (track, camera,
//! car poses) at the PS1's native 320x240.

use crate::assets::{Atlas, GameData};
use crate::game::{Game, RaceConfig};
use crate::ps1::race::car_type;
use crate::render::Renderer;
use anyhow::{Context, Result};
use glam::{Mat3, Quat, Vec3};
use image::{RgbaImage, imageops};
use std::path::Path;

pub const W: u32 = 320;
pub const H: u32 = 240;

/// Retail addresses read from the RAM dump.
mod at {
    pub const GAMESETUP: u32 = 0x8011_44A4;
    pub const CAR_LIST: u32 = 0x8011_0CA0;
    /// `DRender_tView` of the player view: id, player, then translation and rotation.
    pub const VIEW: u32 = 0x8011_8234;
    pub const CAMERA_ZOOM: u32 = 0x8013_DAD0;
}

struct Ram(Vec<u8>);

impl Ram {
    fn i32(&self, a: u32) -> i32 {
        let o = (a & 0x1F_FFFF) as usize;
        i32::from_le_bytes(self.0[o..o + 4].try_into().unwrap())
    }
    fn v3(&self, a: u32) -> Vec3 {
        // PS1 world space is the engine's with X mirrored; 16.16 fixed point
        Vec3::new(-(self.i32(a) as f32), self.i32(a + 4) as f32, self.i32(a + 8) as f32) / 65536.0
    }
    fn code(&self, car: u32) -> String {
        let o = ((car + 0x240) & 0x1F_FFFF) as usize;
        self.0[o..o + 4].iter().take_while(|&&b| b != 0).map(|&b| b as char).collect()
    }
}

/// The emulator's picture: the game area of the captured window, resampled to 320x240.
fn reference_picture(window: &RgbaImage) -> RgbaImage {
    let (w, h) = window.dimensions();
    // a column clear of DuckStation's on-screen messages (top left) and pause icon (top right)
    let x = w * 3 / 4;
    let dark = |y: u32| {
        let p = window.get_pixel(x, y);
        (p[0] as u32 + p[1] as u32 + p[2] as u32) < 30
    };
    // skip the menu bar, then the black letterbox above the picture
    let mut top = 30.min(h - 1);
    while top < h / 2 && dark(top) {
        top += 1;
    }
    let (pw, ph) = if (w as f32) / ((h - top) as f32) > 4.0 / 3.0 { ((h - top) * 4 / 3, h - top) } else { (w, w * 3 / 4) };
    let left = (w - pw) / 2;
    let crop = imageops::crop_imm(window, left, top, pw.min(w - left), ph.min(h - top)).to_image();
    imageops::resize(&crop, W, H, imageops::FilterType::Nearest)
}

pub fn compare(data: &GameData, dir: &Path) -> Result<()> {
    let ram = Ram(std::fs::read(dir.join("ram.bin")).context("ram.bin")?);
    let gs = at::GAMESETUP;
    let track = ram.i32(gs + 0x3C) as u32;
    let variant = match (ram.i32(gs + 0x48), ram.i32(gs + 0x54)) {
        (1, 1) => "S",
        (1, _) => "W",
        (_, 1) => "N",
        _ => "",
    };
    let cars: Vec<u32> = (0..9).map(|k| ram.i32(at::CAR_LIST + 4 * k) as u32).take_while(|&c| c != 0).collect();
    let player = ram.code(cars[0]);
    // the race cars (player first); traffic is not part of the engine's race yet
    let opponents: Vec<String> = cars[1..].iter().map(|&c| ram.code(c)).filter(|c| car_type(c).is_some()).collect();
    println!("track {track}{variant}, player {player}, opponents {opponents:?}");
    let mut atlas = Atlas::new(2048);
    let cfg = RaceConfig { track, variant, player_car: &player, opponents: opponents.clone(), laps: 1, original_sim: true };
    let mut game = Game::load(data, &mut atlas, &cfg)?;
    // let the original run a few frames so its view set-up (fog, colours) has happened
    if let Some(sim) = &mut game.sim {
        sim.advance(0.1, &crate::game::Input::default());
    }
    let race_cars: Vec<u32> = cars.iter().copied().filter(|&c| car_type(&ram.code(c)).is_some()).collect();
    for (r, &c) in game.racers.iter_mut().zip(&race_cars) {
        let row = |i: u32| ram.v3(c + 0xF0 + 12 * i).normalize_or_zero();
        let (right, up, forward) = (row(0), row(1), row(2));
        let m = Mat3::from_cols(-right, up, forward);
        // model origin at ground level: the body-box centre minus dimension.y along up
        r.car.pos = ram.v3(c + 0xA0) - up * (ram.i32(c + 0x138) as f32 / 65536.0);
        r.car.rot = Some(Quat::from_mat3(&m).normalize());
        r.car.up = up;
        r.car.yaw = forward.x.atan2(forward.z);
    }
    // the camera exactly as the game computed it for that frame
    let v = at::VIEW + 8;
    let rot = |i: u32| ram.v3(v + 12 + 12 * i).normalize_or_zero();
    game.cam_pos = ram.v3(v);
    game.cam_target = game.cam_pos + rot(2);
    game.cam_up = rot(1);
    let h = ram.i32(at::CAMERA_ZOOM).max(1) as f32;
    game.tan_half_v = 160.0 / h * 0.75;
    game.cam_mode = if game.racers[0].car.pos.distance(game.cam_pos) < 2.0 { crate::game::CameraMode::Bumper } else { crate::game::CameraMode::Chase };

    // NFSHS_PICK="x0,y0,x1,y1": list the track quads whose centre projects into that screen
    // rectangle (texture name, material flags, mesh) — for chasing rendering differences
    if let Ok(pick) = std::env::var("NFSHS_PICK") {
        let r: Vec<f32> = pick.split(',').filter_map(|v| v.parse().ok()).collect();
        let vp = game.frame_params(W as f32 / H as f32).view_proj;
        let psh = crate::formats::psh::Psh::parse(&data.read(&format!("ZTR{track:02}{variant}0.PSH"))?)?;
        let t = &game.track.track;
        for (bi, b) in t.blocks.iter().enumerate() {
            for (mesh, kind) in [(&b.ground, "ground"), (&b.detail, "detail")] {
                for q in &mesh.quads {
                    let c = q.v.iter().map(|&i| mesh.verts.get(i as usize).map(|v| Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) / 64.0).unwrap_or(Vec3::ZERO)).sum::<Vec3>() / 4.0;
                    let w = crate::assets::ps1_to_world(b.origin + c);
                    let p = vp * w.extend(1.0);
                    if p.w <= 0.1 {
                        continue;
                    }
                    let (sx, sy) = ((p.x / p.w * 0.5 + 0.5) * W as f32, (0.5 - p.y / p.w * 0.5) * H as f32);
                    if sx >= r[0] && sx < r[2] && sy >= r[1] && sy < r[3] {
                        let te = t.textures.get(q.tex as usize);
                        let name = te.and_then(|te| psh.shapes.get(te.shape as usize)).map(|s| s.name.as_str()).unwrap_or("?");
                        let pts: Vec<Vec3> = q.v.iter().map(|&i| mesh.verts.get(i as usize).map(|v| Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) / 64.0).unwrap_or(Vec3::ZERO)).collect();
                        let edges: Vec<String> = (0..4).map(|k| format!("{:.1}", pts[k].distance(pts[(k + 1) % 4]))).collect();
                        let shape = te.and_then(|te| psh.shapes.get(te.shape as usize)).map(|s| (s.width, s.height)).unwrap_or((0, 0));
                        println!("  quad edges {edges:?} (v0->v1, v1->v2, ...), texture {shape:?}, heights {:?}", pts.iter().map(|p| format!("{:.1}", p.y)).collect::<Vec<_>>());
                        println!(
                            "block {bi} {kind} strip={} tex {} {name} flag {:02x} uv {:02x} at ({sx:.0},{sy:.0}) dist {:.0}",
                            q.strip,
                            q.tex,
                            te.map(|t| t.mode).unwrap_or(0),
                            te.map(|t| t.flags).unwrap_or(0),
                            p.w
                        );
                    }
                }
            }
        }
    }
    let mut r = pollster::block_on(Renderer::new(None, (W, H), &atlas))?;
    r.set_world(&game.track.verts, &game.track.indices);
    let params = game.frame_params(W as f32 / H as f32);
    let mine = r.screenshot(&params, &game.track.sky, &game.dynamic_geometry(), &game.hud(W as f32, H as f32));
    mine.save(dir.join("mine.png"))?;

    let reference = reference_picture(&image::open(dir.join("window.png"))?.to_rgba8());
    reference.save(dir.join("reference.png"))?;
    let mut side = RgbaImage::new(W * 2, H);
    imageops::replace(&mut side, &reference, 0, 0);
    imageops::replace(&mut side, &mine, W as i64, 0);
    let side = imageops::resize(&side, W * 4, H * 2, imageops::FilterType::Nearest);
    side.save(dir.join("compare.png"))?;
    println!("wrote {}", dir.join("compare.png").display());
    Ok(())
}
