mod app;
mod assets;
mod formats;
mod game;
mod iso;
mod render;
mod soft;

use anyhow::Result;
use std::path::{Path, PathBuf};

/// Value of `--name value` style option.
fn opt<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args: Vec<String> = std::env::args().collect();
    let data = assets::GameData::new(Path::new(opt(&args, "--data").unwrap_or("gamedata")));
    let track: u32 = opt(&args, "--track").unwrap_or("0").parse()?;
    let variant = opt(&args, "--variant").unwrap_or("").to_ascii_uppercase();
    let car = opt(&args, "--car").unwrap_or("BMWM").to_ascii_uppercase();
    let opponents: Vec<String> = opt(&args, "--opponents")
        .unwrap_or("MCF1,F550,DIAB,P993,CORV")
        .split(',')
        .filter(|s| !s.is_empty() && *s != "none")
        .map(|s| s.to_ascii_uppercase())
        .collect();
    let laps: i32 = opt(&args, "--laps").unwrap_or("2").parse()?;
    let cfg = game::RaceConfig { track, variant: &variant, player_car: &car, opponents, laps };
    match args.get(1).map(String::as_str) {
        None | Some("play") => app::run(&data, &cfg)?,
        Some("shot") => {
            let secs: f32 = opt(&args, "--time").unwrap_or("0").parse()?;
            let keys = opt(&args, "--keys").unwrap_or("");
            let input = game::Input {
                up: keys.contains('u'),
                down: keys.contains('d'),
                left: keys.contains('l'),
                right: keys.contains('r'),
                handbrake: keys.contains('h'),
                autopilot: keys.contains('a'),
                ..Default::default()
            };
            let out = opt(&args, "--out").unwrap_or("shot.png");
            app::screenshot(&data, &cfg, Path::new(out), secs, input)?;
        }
        Some("extract") => {
            let bin = PathBuf::from(&args[2]);
            let out = PathBuf::from(args.get(3).map(String::as_str).unwrap_or("gamedata"));
            let mut cd = iso::CdImage::open(&bin)?;
            for e in cd.list_files()? {
                let dst = out.join(&e.path);
                if e.is_dir {
                    std::fs::create_dir_all(&dst)?;
                    continue;
                }
                std::fs::create_dir_all(dst.parent().unwrap())?;
                println!("{:8} {:10} {}", e.lba, e.size, e.path);
                std::fs::write(&dst, cd.read_file(&e)?)?;
            }
        }
        Some("dump-psh") => dump_psh(Path::new(&args[2]), Path::new(&args[3]))?,
        Some("dump-track") => dump_track(&args[2], &args[3], Path::new(&args[4]), args.get(5).map(String::as_str))?,
        Some("verify-decompress") => {
            // compare against reference outputs produced by tools/mipsemu.py (re/dec/<name>.bin)
            let (mut ok, mut bad) = (0, 0);
            for f in &args[2..] {
                let src = std::fs::read(f)?;
                let name = Path::new(f).file_name().unwrap().to_string_lossy().to_string();
                let reference = std::fs::read(format!("re/dec/{name}.bin"))?;
                match formats::compress::decompress(&src) {
                    Ok(out) if out == reference => ok += 1,
                    Ok(out) => {
                        bad += 1;
                        let first = out.iter().zip(&reference).position(|(a, b)| a != b);
                        println!("MISMATCH {name}: len {} vs {} first diff {:?}", out.len(), reference.len(), first);
                    }
                    Err(e) => {
                        bad += 1;
                        println!("ERROR {name}: {e}");
                    }
                }
            }
            println!("{ok} ok, {bad} bad");
        }
        Some("probe") => {
            let from: usize = opt(&args, "--from").unwrap_or("0").parse()?;
            let to: usize = opt(&args, "--to").unwrap_or("10").parse()?;
            let mut atlas = assets::Atlas::new(2048);
            let g = game::Game::load(&data, &mut atlas, &cfg)?;
            for i in from..to {
                let p = g.path[i];
                let gh = g.world.ground(p + glam::Vec3::Y * 3.0, 6.0).map(|h| h.height);
                let walls = g.world.walls(p + glam::Vec3::Y * 0.5, 2.0, p.y + 0.2, p.y + 1.2);
                println!("{i}: path y={:.2} ground={:?} walls={:?} lim={:?}", p.y, gh, walls.map(|w| w.1), g.walls[i]);
            }
        }
        Some("dump-car") => dump_car(Path::new(&args[2]), Path::new(&args[3]), args.get(4).map(String::as_str))?,
        _ => eprintln!(
            "usage:
  nfshs extract <image.bin> [outdir]
  nfshs play [--track N] [--variant N|S|W] [--car CODE] [--opponents A,B|none] [--laps N] [--data DIR]
  nfshs shot --out x.png [--time S] [--keys udlrh] ...
  nfshs dump-psh|dump-car|dump-track ..."
        ),
    }
    Ok(())
}

/// Pack every shape of a PSH (or every PSH inside a VIV) onto a sheet PNG.
fn dump_psh(src: &Path, out: &Path) -> Result<()> {
    let d = std::fs::read(src)?;
    let mut pshs = Vec::new();
    if &d[..4] == b"BIGF" {
        for (n, e) in formats::viv::Viv::parse(&d)?.entries {
            if e.starts_with(b"SHPP") {
                println!("-- {n}");
                pshs.push(formats::psh::Psh::parse(&e)?);
            }
        }
    } else {
        pshs.push(formats::psh::Psh::parse(&d)?);
    }
    let shapes: Vec<_> = pshs.iter().flat_map(|p| p.shapes.iter()).collect();
    let sheet_w = 1024u32;
    let (mut x, mut y, mut row_h) = (0u32, 0u32, 0u32);
    let mut placed = Vec::new();
    for s in &shapes {
        let (w, h) = (s.width as u32 * 2, s.height as u32 * 2);
        if x + w > sheet_w {
            x = 0;
            y += row_h + 2;
            row_h = 0;
        }
        placed.push((x, y));
        x += w + 2;
        row_h = row_h.max(h);
        println!(
            "{:4} kind={:#x} {}x{} center={:?} vram={:?} pal={}",
            s.name, s.kind, s.width, s.height, s.center, s.vram, s.palette.len()
        );
    }
    let mut img = image::RgbaImage::from_pixel(sheet_w, y + row_h + 2, image::Rgba([255, 0, 255, 255]));
    for (s, &(px, py)) in shapes.iter().zip(&placed) {
        for yy in 0..s.height as u32 * 2 {
            for xx in 0..s.width as u32 * 2 {
                let i = ((yy / 2) * s.width as u32 + xx / 2) as usize * 4;
                let mut c = [s.rgba[i], s.rgba[i + 1], s.rgba[i + 2], 255];
                if s.rgba[i + 3] == 0 {
                    c = if (xx / 4 + yy / 4) % 2 == 0 { [90, 90, 90, 255] } else { [60, 60, 60, 255] };
                }
                img.put_pixel(px + xx, py + yy, image::Rgba(c));
            }
        }
    }
    img.save(out)?;
    Ok(())
}

/// Build the 256x256 car texture page from every PSH inside a car VIV.
fn car_atlas(viv: &formats::viv::Viv) -> Result<formats::psh::Atlas> {
    let mut atlas = formats::psh::Atlas::new(256);
    // body page variants: skip the duplicate "d" sheet and the "g"/"i" extras for now
    for (name, data) in &viv.entries {
        let n = name.to_ascii_lowercase();
        if !n.ends_with(".psh") || n.ends_with("d.psh") || n.ends_with("g.psh") || n.ends_with("i.psh") {
            continue;
        }
        let psh = formats::psh::Psh::parse(data)?;
        atlas.place(&psh.shapes, 64);
    }
    Ok(atlas)
}

fn dump_car(src: &Path, out: &Path, parts: Option<&str>) -> Result<()> {
    use glam::{Mat4, Vec2, Vec3};
    let viv = formats::viv::Viv::parse(&std::fs::read(src)?)?;
    let (_, geo) = viv.find_suffix(".geo").ok_or_else(|| anyhow::anyhow!("no geo"))?;
    let geo = formats::geo::Geo::parse(geo)?;
    let atlas = car_atlas(&viv)?;
    image::RgbaImage::from_raw(256, 256, atlas.rgba.clone()).unwrap().save(out.with_extension("atlas.png"))?;
    let sel: Vec<usize> = match parts {
        Some(p) => p.split(',').map(|x| x.parse().unwrap()).collect(),
        None => (0..geo.parts.len()).collect(),
    };
    let mut tris = Vec::new();
    for &pi in &sel {
        let part = &geo.parts[pi];
        let off = Vec3::new(part.position[0] as f32, part.position[1] as f32, part.position[2] as f32) / 256.0;
        for poly in &part.polys {
            let p = poly.v.map(|i| {
                let v = part.verts.get(i as usize).copied().unwrap_or([0; 3]);
                Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) + off
            });
            let uv = poly.uv.map(|t| Vec2::new(t[0] as f32 + 0.5, t[1] as f32 + 0.5));
            tris.push(soft::Tri { p, uv, tex: 0, color: [255, 255, 255, 255] });
            // PS1 has no backface culling unless requested; emit reverse winding too
            tris.push(soft::Tri { p: [p[0], p[2], p[1]], uv: [uv[0], uv[2], uv[1]], tex: 0, color: [255; 4] });
        }
    }
    let tex = vec![soft::Texture { w: 256, h: 256, rgba: atlas.rgba }];
    let proj = Mat4::perspective_rh_gl(0.7, 4.0 / 3.0, 10.0, 10000.0);
    let views = [Vec3::new(2000.0, 0.0, 0.0), Vec3::new(-900.0, 400.0, -1100.0), Vec3::new(0.0, 1800.0, 1.0)];
    let mut sheet = image::RgbaImage::new(640 * 3, 480);
    for (k, eye) in views.iter().enumerate() {
        let view = Mat4::look_at_rh(*eye, Vec3::ZERO, Vec3::Y);
        let img = soft::render(&tris, &tex, proj * view, 640, 480, [40, 60, 90, 255]);
        image::imageops::overlay(&mut sheet, &img, 640 * k as i64, 0);
    }
    sheet.save(out)?;
    Ok(())
}

fn dump_track(grp: &str, psh: &str, out: &Path, view: Option<&str>) -> Result<()> {
    use glam::{Mat4, Vec2, Vec3};
    let track = formats::grp::Track::parse(&std::fs::read(grp)?)?;
    let psh = formats::psh::Psh::parse(&std::fs::read(psh)?)?;
    println!("nodes={} blocks={} textures={} vroad={}", track.nodes.len(), track.blocks.len(), track.textures.len(), track.vroad.len());
    let textures: Vec<soft::Texture> = psh
        .shapes
        .iter()
        .map(|s| soft::Texture { w: s.width.max(1) as u32, h: s.height.max(1) as u32, rgba: s.rgba.clone() })
        .collect();
    let mut tris = Vec::new();
    for b in &track.blocks {
        for mesh in [&b.ground, &b.detail] {
            for q in &mesh.quads {
                let te = track.textures.get(q.tex as usize);
                let shape = te.map(|t| t.shape as usize).unwrap_or(usize::MAX);
                let (w, h) = textures.get(shape).map(|t| (t.w as f32, t.h as f32)).unwrap_or((1.0, 1.0));
                let p: Vec<Vec3> = q
                    .v
                    .iter()
                    .map(|&i| {
                        let v = mesh.verts.get(i as usize).copied().unwrap_or([0; 4]);
                        let w = b.origin + Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) / 64.0;
                        Vec3::new(-w.x, w.y, w.z)
                    })
                    .collect();
                let uv = [Vec2::new(0.0, 0.0), Vec2::new(w, 0.0), Vec2::new(w, h), Vec2::new(0.0, h)];
                for (a, b2, c) in [(0, 1, 2), (0, 2, 3)] {
                    tris.push(soft::Tri { p: [p[a], p[b2], p[c]], uv: [uv[a], uv[b2], uv[c]], tex: shape, color: [255, 0, 255, 255] });
                }
            }
        }
    }
    let flip = |v: Vec3| Vec3::new(-v.x, v.y, v.z);
    let nodes: Vec<Vec3> = track.nodes.iter().map(|&v| flip(v)).collect();
    let n = nodes.len();
    let (eye, target, fov) = match view {
        Some(v) => {
            let i: usize = v.parse()?;
            let a = nodes[i % n];
            let b = nodes[(i + 3) % n];
            let dir = (b - a).normalize();
            (a - dir * 12.0 + Vec3::Y * 5.0, b + Vec3::Y * 1.0, 1.1)
        }
        None => {
            let min = nodes.iter().fold(Vec3::splat(f32::MAX), |m, v| m.min(*v));
            let max = nodes.iter().fold(Vec3::splat(f32::MIN), |m, v| m.max(*v));
            let c = (min + max) * 0.5;
            let ext = (max - min).max_element();
            (c + Vec3::new(0.0, ext * 1.2, 1.0), c, 0.9)
        }
    };
    let proj = Mat4::perspective_rh_gl(fov, 4.0 / 3.0, 0.5, 20000.0);
    let img = soft::render(&tris, &textures, proj * Mat4::look_at_rh(eye, target, Vec3::Y), 960, 720, [110, 150, 200, 255]);
    img.save(out)?;
    Ok(())
}
