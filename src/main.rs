mod app;
mod assets;
mod audio;
mod formats;
mod game;
mod iso;
mod menu;
mod ps1;
mod refcmp;
mod render;
mod soft;

use anyhow::Context as _;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Locate the extracted disc: `--data`, `NFSHS_DATA`, or a `gamedata` folder in the current
/// directory, next to the executable, or in any parent of the executable (so
/// `target/release/nfshs` finds the project's `gamedata`).
fn find_data_dir(explicit: Option<&str>) -> PathBuf {
    if let Some(d) = explicit {
        return PathBuf::from(d);
    }
    if let Ok(d) = std::env::var("NFSHS_DATA") {
        return PathBuf::from(d);
    }
    let is_data = |p: &Path| p.join("NFS4.EXE").is_file() || p.join("ZTR00.GRP").is_file();
    let mut candidates = vec![PathBuf::from("gamedata")];
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        candidates.extend(dir.ancestors().map(|a| a.join("gamedata")));
    }
    for c in &candidates {
        if is_data(c) {
            log::info!("game data: {}", c.display());
            return c.clone();
        }
    }
    log::warn!("no extracted game data found (looked for gamedata/ near the working dir and executable); use --data <dir>");
    PathBuf::from("gamedata")
}

/// Value of `--name value` style option.
fn opt<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn main() -> Result<()> {
    // windows (menu, play) must be created on the main thread; races run the original
    // simulation on its own thread. The research commands run recompiled code directly and
    // recurse like the original, so they get a generous stack.
    let windowed = matches!(std::env::args().nth(1).as_deref(), None | Some("menu" | "play"));
    if windowed {
        return real_main();
    }
    std::thread::Builder::new().stack_size(256 << 20).spawn(real_main)?.join().map_err(|_| anyhow::anyhow!("main thread panicked"))?
}

fn real_main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args: Vec<String> = std::env::args().collect();
    let data = assets::GameData::new(&find_data_dir(opt(&args, "--data")));
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
    let cfg = game::RaceConfig { track, variant: &variant, player_car: &car, opponents, laps, original_sim: true, traffic: Vec::new() };
    match args.get(1).map(String::as_str) {
        None | Some("menu") => app::run(&data, None)?,
        Some("play") => app::run(
            &data,
            Some(app::RaceConfigOwned {
                track,
                variant: variant.clone(),
                player_car: car.clone(),
                opponents: cfg.opponents.clone(),
                laps,
            }),
        )?,
        Some("shot-menu") => app::menu_screenshot(&data, Path::new(opt(&args, "--out").unwrap_or("menu.png")))?,
        Some("ref-compare") => refcmp::compare(&data, Path::new(&args[2]))?,
        // match-shapes REF.png FILE.PSH NAME...: where each shape appears in a reference picture
        Some("match-shapes") => refcmp::match_shapes(Path::new(&args[2]), Path::new(&args[3]), &args[4..])?,
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
            // --taps "s@5,d@5.5,x@6": press Start / Down / Up / accept (x) briefly at those times
            let taps: Vec<(char, f32)> = opt(&args, "--taps")
                .unwrap_or("")
                .split(',')
                .filter_map(|t| {
                    let (k, at) = t.split_once('@')?;
                    Some((k.chars().next()?, at.parse().ok()?))
                })
                .collect();
            app::screenshot(&data, &cfg, Path::new(out), secs, input, &taps)?;
        }
        Some("extract") => {
            let bin = PathBuf::from(&args[2]);
            let out = PathBuf::from(args.get(3).map(String::as_str).unwrap_or("gamedata"));
            let mut cd = iso::CdImage::open(&bin)?;
            // EXTRACT_ONLY=suffix: only files ending in it (e.g. .XA)
            let only = std::env::var("EXTRACT_ONLY").ok().map(|s| s.to_ascii_uppercase());
            for e in cd.list_files()? {
                if only.as_ref().is_some_and(|o| !e.is_dir && !e.path.to_ascii_uppercase().ends_with(o.as_str())) {
                    continue;
                }
                let dst = out.join(&e.path);
                if e.is_dir {
                    std::fs::create_dir_all(&dst)?;
                    continue;
                }
                std::fs::create_dir_all(dst.parent().unwrap())?;
                println!("{:8} {:10} {}", e.lba, e.size, e.path);
                if e.path.to_ascii_uppercase().ends_with(".XA") {
                    // XA streams (video and audio sectors, Mode 2 Form 2): whole sectors after the
                    // sync and header (8-byte subheader + 2328 bytes), as the movie player reads them
                    let mut d = Vec::new();
                    for i in 0..(e.size as usize).div_ceil(iso::USER_DATA) {
                        d.extend_from_slice(&cd.read_raw_sector(e.lba + i as u32)?[16..]);
                    }
                    std::fs::write(&dst, d)?;
                    continue;
                }
                std::fs::write(&dst, cd.read_file(&e)?)?;
            }
        }
        Some("dump-shape-pal") => {
            // dump-shape-pal FILE.PSH NAME PALETTE_SHAPE out.png: a shape drawn with another
            // shape's palette
            let psh = formats::psh::Psh::parse(&std::fs::read(&args[2])?)?;
            let sh = psh.shapes.iter().find(|s| s.name.eq_ignore_ascii_case(&args[3])).context("no such shape")?;
            let pal = psh.shapes.iter().find(|s| s.name.eq_ignore_ascii_case(&args[4])).context("no palette shape")?;
            println!("{} indices {} palette {} / {}", sh.name, sh.indices.len(), sh.palette.len(), pal.palette.len());
            let rgba: Vec<u8> = sh.indices.iter().flat_map(|&i| formats::psh::ps1_color(pal.palette.get(i as usize).copied().unwrap_or(0))).collect();
            let img = image::RgbaImage::from_raw(sh.width as u32, sh.height as u32, rgba).context("size")?;
            image::imageops::resize(&img, img.width() * 2, img.height() * 2, image::imageops::FilterType::Nearest).save(&args[5])?;
        }
        Some("dump-shape") => {
            // dump-shape FILE.PSH NAME out.png [scale]: one texture shape as PNG
            let psh = formats::psh::Psh::parse(&std::fs::read(&args[2])?)?;
            let sh = psh.shapes.iter().find(|s| s.name.eq_ignore_ascii_case(&args[3])).context("no such shape")?;
            let img = image::RgbaImage::from_raw(sh.width as u32, sh.height as u32, sh.rgba.clone()).context("shape size")?;
            let k: u32 = args.get(5).and_then(|v| v.parse().ok()).unwrap_or(4);
            image::imageops::resize(&img, img.width() * k, img.height() * k, image::imageops::FilterType::Nearest).save(&args[4])?;
        }
        Some("crop") => {
            // crop IN.png X Y W H OUT.png [scale]: a zoomed piece of a picture (for comparing details)
            let n = |i: usize| -> Result<u32> { Ok(args[i].parse()?) };
            let img = image::open(&args[2])?.to_rgba8();
            let piece = image::imageops::crop_imm(&img, n(3)?, n(4)?, n(5)?, n(6)?).to_image();
            let k: u32 = args.get(8).and_then(|v| v.parse().ok()).unwrap_or(8);
            image::imageops::resize(&piece, piece.width() * k, piece.height() * k, image::imageops::FilterType::Nearest).save(&args[7])?;
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
                println!("{i}: path {:?} ground={:?} walls={:?} lim={:?}", p, gh, walls.map(|w| w.1), g.walls[i]);
            }
            if let Some(at) = opt(&args, "--at") {
                let v: Vec<f32> = at.split(',').map(|x| x.parse().unwrap()).collect();
                let p = glam::Vec3::new(v[0], v[1], v[2]);
                for r in [0.5f32, 1.0, 2.0] {
                    println!("at {p:?} r={r}: walls={:?}", g.world.walls(p + glam::Vec3::Y * 0.5, r, p.y + 0.2, p.y + 1.2));
                }
                println!("ground {:?}", g.world.ground(p + glam::Vec3::Y, 2.0).map(|h| (h.height, h.normal)));
            }
        }
        Some("dump-audio") => {
            // dump-audio <file.bnk|.viv|stream> <outdir>: decode banks/streams to WAV with stats
            let d = std::fs::read(&args[2])?;
            let out = Path::new(&args[3]);
            std::fs::create_dir_all(out)?;
            let mut items: Vec<(String, Vec<u8>)> = Vec::new();
            if d.starts_with(b"BIGF") {
                items = formats::viv::Viv::parse(&d)?.entries;
            } else {
                items.push((Path::new(&args[2]).file_name().unwrap().to_string_lossy().into(), d));
            }
            let stats = |pcm: &[i16]| {
                let n = pcm.len().max(1) as f64;
                let rms = (pcm.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / n).sqrt();
                let diff = pcm.windows(2).map(|w| (w[1] as f64 - w[0] as f64).abs()).sum::<f64>() / n;
                (rms, diff)
            };
            for (name, data) in items {
                let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' { c } else { '_' }).collect();
                if data.starts_with(b"BNKl") {
                    let bank = formats::audio::Bank::parse(&data)?;
                    for (i, s) in bank.sounds.iter().enumerate() {
                        if let Some(s) = s {
                            let (rms, diff) = stats(&s.pcm);
                            println!("{name}#{i}: {} ch {} Hz {} samples loop={:?}-{:?} rms={rms:.0} meandiff={diff:.0}", s.channels, s.rate, s.pcm.len(), s.loop_start, s.loop_end);
                            formats::audio::write_wav(&out.join(format!("{safe}_{i:02}.wav")), s)?;
                        }
                    }
                } else if data.starts_with(b"SCHl") {
                    let s = formats::audio::decode_stream(&data)?;
                    let (rms, diff) = stats(&s.pcm);
                    println!("{name}: {} ch {} Hz {:.1}s rms={rms:.0} meandiff={diff:.0}", s.channels, s.rate, s.pcm.len() as f32 / s.channels as f32 / s.rate as f32);
                    formats::audio::write_wav(&out.join(format!("{safe}.wav")), &s)?;
                }
            }
        }
        Some("verify-recomp") => {
            // verify-recomp <snapdir>: replay a DuckStation snapshot through the recompiled code
            let dir = Path::new(&args[2]);
            let rd = |n: &str| std::fs::read(dir.join(n));
            let mut mem = ps1::Mem::default();
            mem.ram.copy_from_slice(&rd("before_ram.bin")?);
            mem.scratch.copy_from_slice(&rd("before_scratch.bin")?);
            let regs: Vec<u32> = rd("before_regs.bin")?.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
            let mut cpu = ps1::new_cpu(mem);
            cpu.r.copy_from_slice(&regs[..32]);
            cpu.lo = regs[33];
            cpu.hi = regs[34];
            let func = u32::from_str_radix(std::fs::read_to_string(dir.join("func.txt"))?.trim().trim_start_matches("0x"), 16)?;
            let t = std::time::Instant::now();
            cpu.call(func);
            let el = t.elapsed();
            let after_ram = rd("after_ram.bin")?;
            let after_scratch = rd("after_scratch.bin")?;
            let diff: Vec<usize> = (0..after_ram.len()).step_by(4).filter(|&i| cpu.m.ram[i..i + 4] != after_ram[i..i + 4]).collect();
            let sdiff = (0..after_scratch.len()).step_by(4).filter(|&i| cpu.m.scratch[i..i + 4] != after_scratch[i..i + 4]).count();
            println!("{func:#x}: {:?}, RAM words differing: {} {:x?}, scratch words differing: {sdiff}", el, diff.len(), &diff.iter().take(10).map(|i| 0x8000_0000u32 + *i as u32).collect::<Vec<_>>());
        }
        Some("boot-race") => {
            // boot-race [--track N --variant V --car C --opponents A,B --laps N]: run the
            // recompiled original race simulation headless. BOOT_SCRIPT="secs:buttons,..."
            // (x cross, s square, c circle, t triangle, l/r/u/d pad, 1-4 L1/R1/L2/R2, - none);
            // GS_SET="hexoff=value,..." pokes GameSetup words.
            use ps1::race::{FrameInput, RaceSetup, RaceThread, pad};
            let mut cars = vec![car.clone()];
            cars.extend(cfg.opponents.iter().cloned());
            let setup = RaceSetup { track, variant: variant.clone(), cars, laps: laps as u32 };
            let mut gs = setup.game_setup()?;
            for kv in std::env::var("GS_SET").unwrap_or_default().split(',').filter(|s| !s.is_empty()) {
                let (o, v) = kv.split_once('=').expect("off=value");
                let o = usize::from_str_radix(o, 16)?;
                gs[o..o + 4].copy_from_slice(&v.parse::<i32>()?.to_le_bytes());
            }
            let t = std::time::Instant::now();
            let mut sim = RaceThread::start(data.root.clone(), gs)?;
            println!("booted into race in {:?}", t.elapsed());
            let show = |s: &ps1::race::Snapshot, label: &str| {
                for (k, c) in s.cars.iter().enumerate() {
                    let f = |v: i32| v as f32 / 65536.0;
                    println!(
                        "  {label} car[{k}] {:4} pos=({:8.2},{:6.2},{:8.2}) speed={:6.2} m/s ({:5.1} mph) rpm={:5} gear={} steer={} lap={}",
                        c.code,
                        f(c.position[0]),
                        f(c.position[1]),
                        f(c.position[2]),
                        f(c.speed),
                        f(c.speed) * 2.237,
                        c.rpm,
                        c.gear,
                        c.steering,
                        c.lap
                    );
                }
            };
            show(&sim.last, "boot");
            let show_cam = |s: &ps1::race::Snapshot| {
                let f = |v: i32| v as f32 / 65536.0;
                println!(
                    "  env {:?}
  camera mode={} H={} display={:?} pos=({:.2},{:.2},{:.2}) rot={:?}",
                    s.view_env,
                    s.camera_mode,
                    s.projection,
                    s.display,
                    f(s.camera_pos[0]),
                    f(s.camera_pos[1]),
                    f(s.camera_pos[2]),
                    s.camera_rot
                );
            };
            let script = std::env::var("BOOT_SCRIPT").unwrap_or_else(|_| "8:-,3:x".into());
            let mut stage = sim.last.countdown;
            for st in script.split(',').filter(|s| !s.is_empty()) {
                let (secs, b) = st.split_once(':').unwrap_or((st, "-"));
                let secs: f32 = secs.parse()?;
                let mut buttons = 0u16;
                for ch in b.chars() {
                    buttons |= match ch {
                        'x' => pad::CROSS,
                        's' => pad::SQUARE,
                        'c' => pad::CIRCLE,
                        't' => pad::TRIANGLE,
                        'l' => pad::LEFT,
                        'r' => pad::RIGHT,
                        'u' => pad::UP,
                        'd' => pad::DOWN,
                        '1' => pad::L1,
                        '2' => pad::R1,
                        '3' => pad::L2,
                        '4' => pad::R2,
                        'p' => pad::START,
                        _ => 0,
                    };
                }
                let t = std::time::Instant::now();
                let tick0 = sim.last.ticks;
                let trace = std::env::var_os("BOOT_TRACE").is_some();
                for _ in 0..(secs * 60.0).round() as u32 {
                    let prev: Vec<[i32; 3]> = sim.last.cars.iter().map(|c| c.position).collect();
                    let s = sim.step(FrameInput { buttons, want_ram: false })?;
                    if trace {
                        let d: Vec<String> = s
                            .cars
                            .iter()
                            .zip(&prev)
                            .take(3)
                            .map(|(c, p)| {
                                let dx = (c.position[0] - p[0]) as f32 / 65536.0;
                                let dz = (c.position[2] - p[2]) as f32 / 65536.0;
                                format!("{:6.3}", (dx * dx + dz * dz).sqrt())
                            })
                            .collect();
                        println!("  frame {} ticks {} sim {} moved {}", s.frame, s.ticks, s.sim_ticks, d.join(" "));
                    }
                    if s.countdown != stage {
                        println!("  frame {}: countdown stage {}", s.frame, s.countdown);
                        stage = s.countdown;
                    }
                }
                println!("{st}: frame {} in {:?}, game ticks +{}, sim ticks {}, overlay {:?}", sim.last.frame, t.elapsed(), sim.last.ticks.wrapping_sub(tick0), sim.last.sim_ticks, sim.last.overlay);
                show(&sim.last, st);
                show_cam(&sim.last);
            }
            if std::env::var_os("BOOT_RAM").is_some() {
                let s = sim.step(FrameInput { buttons: 0, want_ram: true })?;
                std::fs::create_dir_all("re/boot")?;
                std::fs::write("re/boot/ram.bin", s.ram.as_deref().unwrap_or_default())?;
                println!("RAM written to re/boot/ram.bin");
            }
        }
        Some("boot-main") => {
            // boot-main: run the original main (front end, then races) headless, for exploring
            let exe = std::fs::read(data.root.join("NFS4.EXE"))?;
            let mut mem = ps1::Mem::default();
            mem.load_exe(&exe);
            for a in (ps1::race::addr::BSS_START..ps1::race::addr::BSS_END).step_by(4) {
                mem.w32(a, 0);
            }
            let mut cpu = ps1::new_cpu(mem);
            cpu.host = Some(Box::new(ps1::host::GameHost::new(data.root.clone())));
            cpu.trace = std::env::var("PS1_TRACE").is_ok();
            cpu.tick_limit = std::env::var("TICKS").ok().and_then(|t| t.parse().ok()).unwrap_or(2_000_000_000);
            // busy-waits on the timer (a race frame stays far below this between host calls)
            cpu.spin_limit = 20_000;
            // BOOT_SCRIPT="secs:buttons,...": pad input per stretch of frames (as boot-race; 'p'
            // Start); the run ends after the script
            let mut script: Vec<(u32, u16)> = Vec::new();
            for st in std::env::var("BOOT_SCRIPT").unwrap_or_else(|_| "30:-".into()).split(',').filter(|s| !s.is_empty()) {
                let (secs, b) = st.split_once(':').unwrap_or((st, "-"));
                let mut buttons = 0u16;
                for ch in b.chars() {
                    use ps1::race::pad;
                    buttons |= match ch {
                        'x' => pad::CROSS, 's' => pad::SQUARE, 'c' => pad::CIRCLE, 't' => pad::TRIANGLE,
                        'l' => pad::LEFT, 'r' => pad::RIGHT, 'u' => pad::UP, 'd' => pad::DOWN, 'p' => pad::START, 'e' => pad::SELECT,
                        _ => 0,
                    };
                }
                script.push(((secs.parse::<f32>()? * 60.0).round() as u32, buttons));
            }
            let mut frame = 0u32;
            // FE_SHOT=frame,frame,...:dir: render those front-end frames to dir/fe_<frame>.png
            let shot: Option<(Vec<u32>, String)> = std::env::var("FE_SHOT")
                .ok()
                .and_then(|v| v.split_once(':').map(|(f, o)| (f.split(',').filter_map(|n| n.parse().ok()).collect(), o.to_string())));
            let shot_at: Vec<u32> = shot.as_ref().map(|s| s.0.clone()).unwrap_or_default();
            let grabbed = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let grab = grabbed.clone();
            let started = std::time::Instant::now();
            let ram_at: Option<u32> = std::env::var("BOOT_RAM_FRAME").ok().and_then(|f| f.parse().ok());
            let on_frame: ps1::host::FrameFn = Box::new(move |c, info| {
                frame += 1;
                if Some(frame) == ram_at {
                    let _ = std::fs::create_dir_all("re/boot");
                    let _ = std::fs::write("re/boot/fe_ram.bin", &c.m.ram);
                }
                if shot_at.contains(&frame) {
                    grab.borrow_mut().push((frame, info.overlay.clone(), info.vram.clone(), (info.display[2], info.display[3])));
                }
                let mut t = frame;
                for &(n, b) in &script {
                    if t <= n {
                        if std::env::var_os("BOOT_FRAMES").is_some() {
                            println!("frame {frame} t {:.2}s ticks {} calls {} field {} {} overlay {} display {:?}", started.elapsed().as_secs_f32(), c.ticks, c.calls, info.field, if info.front_end { "fe" } else { "race" }, info.overlay.len(), info.display);
                            if std::env::var("BOOT_DUMP_FRAME").ok().and_then(|f| f.parse::<u32>().ok()) == Some(frame) {
                                for o in &info.overlay {
                                    println!("  {o:?}");
                                }
                            }
                        }
                        return Some(b);
                    }
                    t -= n;
                }
                None
            });
            let host = cpu.host.as_mut().expect("host");
            let host = unsafe { &mut *(host.as_mut() as *mut dyn ps1::Host as *mut ps1::host::GameHost) };
            host.frame.on_frame = Some(on_frame);
            let t0 = std::time::Instant::now();
            let r = ps1::race::call(&mut cpu, 0x800A_52A4, &[]);
            println!("main returned: {r:?}");
            println!("{:?}: {} calls, {} interpreted, {} backward branches", t0.elapsed(), cpu.calls, cpu.interpreted, cpu.ticks);
            if let Some((_, dir)) = shot {
                let mut r = None;
                for (f, overlay, vram, screen) in grabbed.borrow_mut().drain(..) {
                    let Some(vram) = vram else { continue };
                    // FE_VRAM=x,y,w,h: also that VRAM region (16-bit pixels) as dir/vram_<frame>.png
                    if let Some(r) = std::env::var("FE_VRAM").ok().map(|v| v.split(',').filter_map(|n| n.parse::<usize>().ok()).collect::<Vec<_>>()).filter(|r| r.len() == 4) {
                        let mut img = image::RgbaImage::new(r[2] as u32, r[3] as u32);
                        for (x, y, p) in img.enumerate_pixels_mut() {
                            let c = vram[(r[1] + y as usize) * 1024 + r[0] + x as usize];
                            let rgba = crate::formats::psh::ps1_color(c);
                            *p = image::Rgba([rgba[0], rgba[1], rgba[2], 255]);
                        }
                        img.save(std::path::Path::new(&dir).join(format!("vram_{f}.png")))?;
                    }
                    let showroom = game::fe::FeScene::car_code(&overlay).and_then(|code| game::fe::Showroom::load(&data, &code));
                    let scene = game::fe::FeScene::build(&overlay, &vram, screen, 960.0, 720.0, showroom.as_ref());
                    let n = (scene.under.len() + scene.over.len()) / 6;
                    let r = match &mut r {
                        Some(r) => r,
                        None => r.insert(pollster::block_on(render::Renderer::new(None, (960, 720), &scene.atlas))?),
                    };
                    let out = std::path::Path::new(&dir).join(format!("fe_{f}.png"));
                    app::draw_front_end(r, scene, true).context("shot")?.save(&out)?;
                    println!("front-end frame {f} written ({n} quads)");
                }
            }
        }
        Some("dump-movie") => {
            // dump-movie FILE.XA FRAME out.png [audio.wav]: one decoded frame (and the audio)
            let bytes = std::fs::read(&args[2])?;
            let m = if bytes.starts_with(b"mTCD") { formats::str::Movie::parse_dct(&bytes) } else { formats::str::Movie::parse(&bytes) };
            println!("{}x{}, {} frames, {} audio samples at {} Hz{}", m.width, m.height, m.frames.len(), m.audio.len(), m.rate, if m.stereo { " stereo" } else { "" });
            let n: usize = args[3].parse()?;
            let rgba = formats::str::Decoder::default().frame(&m.frames[n], m.width, m.height);
            image::RgbaImage::from_raw(m.width, m.height, rgba).context("image")?.save(&args[4])?;
            if let Some(wav) = args.get(5) {
                let mut out = Vec::new();
                let ch = if m.stereo { 2u16 } else { 1 };
                let bytes = (m.audio.len() * 2) as u32;
                out.extend_from_slice(b"RIFF");
                out.extend_from_slice(&(36 + bytes).to_le_bytes());
                out.extend_from_slice(b"WAVEfmt ");
                out.extend_from_slice(&16u32.to_le_bytes());
                out.extend_from_slice(&1u16.to_le_bytes());
                out.extend_from_slice(&ch.to_le_bytes());
                out.extend_from_slice(&m.rate.to_le_bytes());
                out.extend_from_slice(&(m.rate * 2 * ch as u32).to_le_bytes());
                out.extend_from_slice(&(2 * ch).to_le_bytes());
                out.extend_from_slice(&16u16.to_le_bytes());
                out.extend_from_slice(b"data");
                out.extend_from_slice(&bytes.to_le_bytes());
                for s in &m.audio {
                    out.extend_from_slice(&s.to_le_bytes());
                }
                std::fs::write(wav, out)?;
            }
        }
        Some("fe-play") => {
            // fe-play SCRIPT SHOTS DIR: the app's path through the original (front end, race,
            // back) headless. SCRIPT "secs:keys,..." (x Cross/accept, t Triangle, c Circle,
            // s Square, p Start, u/d/l/r d-pad; in races u accelerates as in the window), SHOTS
            // "frame,frame,..." screenshots into DIR
            let mut script = Vec::new();
            for st in args[2].split(',').filter(|s| !s.is_empty()) {
                let (secs, keys) = st.split_once(':').unwrap_or((st, "-"));
                let i = game::Input {
                    up: keys.contains('u'),
                    down: keys.contains('d'),
                    left: keys.contains('l'),
                    right: keys.contains('r'),
                    accept: keys.contains('x'),
                    back: keys.contains('t'),
                    circle: keys.contains('c'),
                    square: keys.contains('s'),
                    start: keys.contains('p'),
                    select: keys.contains('e'),
                    autopilot: keys.contains('a'),
                    ..Default::default()
                };
                script.push((secs.parse::<f32>()?, i));
            }
            let shots: Vec<u32> = args[3].split(',').filter_map(|s| s.parse().ok()).collect();
            app::front_end_play(&data, &script, &shots, Path::new(&args[4]))?;
            // the script is done: don't wait for the original to wind down (it would run on,
            // unpaced, through its results and front end)
            use std::io::Write;
            std::io::stdout().flush().ok();
            std::process::exit(0);
        }
        Some("contact") => {
            // contact OUT COLS IMG...: the images at half size in a grid (for reviewing shots)
            let cols: u32 = args[3].parse()?;
            let imgs: Vec<image::RgbaImage> = args[4..].iter().map(|p| image::open(p).map(|i| i.to_rgba8())).collect::<Result<_, _>>()?;
            let (w, h) = imgs.iter().fold((0, 0), |(w, h), i| (w.max(i.width() / 2), h.max(i.height() / 2)));
            let rows = (imgs.len() as u32).div_ceil(cols);
            let mut out = image::RgbaImage::new(w * cols, h * rows);
            for (k, i) in imgs.iter().enumerate() {
                let s = image::imageops::resize(i, i.width() / 2, i.height() / 2, image::imageops::FilterType::Triangle);
                image::imageops::overlay(&mut out, &s, ((k as u32 % cols) * w) as i64, ((k as u32 / cols) * h) as i64);
            }
            out.save(&args[2])?;
        }
        Some("heap-free") => {
            // heap-free <ram.bin>: the game's largest free heap block in a RAM image
            let mut mem = ps1::Mem::default();
            mem.ram.copy_from_slice(&std::fs::read(&args[2])?);
            let mut cpu = ps1::new_cpu(mem);
            cpu.host = Some(Box::new(ps1::host::GameHost::new(data.root.clone())));
            println!("largest free block: {:#x}", ps1::race::call(&mut cpu, 0x800E_5E70, &[])?);
        }
        Some("run-snap") => {
            // run-snap <snapdir> [--trace]: run the snapshotted function natively with host HLE
            let dir = Path::new(&args[2]);
            let rd = |n: &str| std::fs::read(dir.join(n));
            let mut mem = ps1::Mem::default();
            mem.ram.copy_from_slice(&rd("before_ram.bin")?);
            mem.scratch.copy_from_slice(&rd("before_scratch.bin")?);
            let regs: Vec<u32> = rd("before_regs.bin")?.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
            let mut cpu = ps1::new_cpu(mem);
            cpu.r.copy_from_slice(&regs[..32]);
            cpu.lo = regs[33];
            cpu.hi = regs[34];
            cpu.trace = args.iter().any(|a| a == "--trace");
            cpu.host = Some(Box::new(ps1::host::GameHost::new(data.root.clone())));
            let func = u32::from_str_radix(std::fs::read_to_string(dir.join("func.txt"))?.trim().trim_start_matches("0x"), 16)?;
            let names: std::collections::HashMap<u32, String> = std::fs::read_to_string("re/retail_syms.tsv")
                .unwrap_or_default()
                .lines()
                .filter_map(|l| {
                    let mut it = l.split('\t');
                    Some((u32::from_str_radix(it.next()?, 16).ok()?, it.next()?.to_string()))
                })
                .collect();
            let t = std::time::Instant::now();
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cpu.call(func)));
            match res {
                Ok(()) => {
                    println!("{func:#x} returned v0={:#x} in {:?}, {} calls, {} io accesses", cpu.r[2], t.elapsed(), cpu.calls, cpu.m.io_accesses);
                    std::fs::write(dir.join("native_after_ram.bin"), &cpu.m.ram)?;
                }
                Err(_) => {
                    println!("{func:#x} stopped after {:?} (last io {:#x}); call stack:", t.elapsed(), cpu.m.last_io);
                    for a in cpu.stack.iter().rev().take(12) {
                        println!("    {a:#010x} {}", names.get(a).map(String::as_str).unwrap_or("?"));
                    }
                }
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
                let psh = formats::psh::Psh::parse(&e)?;
                println!("-- {n}: {}", psh.shapes.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(" "));
                pshs.push(psh);
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
            "{:4} kind={:#x} {}x{} center={:?} vram={:?} pal={} abr={}",
            s.name, s.kind, s.width, s.height, s.center, s.vram, s.palette.len(), s.translucency
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
        if parts.is_some() {
            let tex: std::collections::BTreeSet<u16> = part.polys.iter().map(|p| p.texture as u16).collect();
            println!("part {pi}: {} polys, textures {tex:?}, flags {:x?}", part.polys.len(), part.polys.iter().map(|p| (p.flags, p.flags2)).collect::<std::collections::BTreeSet<_>>());
        }
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
    if std::env::var_os("NFSHS_BLOCKS").is_some() {
        for (i, b) in track.blocks.iter().enumerate().take(12) {
            let ymax = |m: &formats::grp::Mesh| m.verts.iter().map(|v| v[1] as f32 / 64.0).fold(f32::MIN, f32::max);
            let ymin = |m: &formats::grp::Mesh| m.verts.iter().map(|v| v[1] as f32 / 64.0).fold(f32::MAX, f32::min);
            println!(
                "block {i}: origin {:?} ground {}+{} far quads (y {:.1}..{:.1}), detail {} (y {:.1}..{:.1}), placements {}",
                b.origin, b.ground.quads.len(), b.ground.far.len(), ymin(&b.ground), ymax(&b.ground), b.detail.quads.len(), ymin(&b.detail), ymax(&b.detail), b.placements.len()
            );
        }
    }
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
