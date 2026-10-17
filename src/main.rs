mod app;
mod assets;
mod audio;
mod formats;
mod game;
mod dev;
mod iso;
mod ps1;
mod render;
mod settings;

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

/// The command (the first argument, unless the arguments start with options).
fn command(args: &[String]) -> Option<&str> {
    args.get(1).map(String::as_str).filter(|a| !a.starts_with("--"))
}

/// Value of `--name value` style option.
pub(crate) fn opt<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn main() -> Result<()> {
    // windows (the game, play) must be created on the main thread; races run the original
    // simulation on its own thread. The research commands run recompiled code directly and
    // recurse like the original, so they get a generous stack.
    let windowed = matches!(command(&std::env::args().collect::<Vec<_>>()), None | Some("play"));
    if windowed {
        return real_main();
    }
    std::thread::Builder::new().stack_size(256 << 20).spawn(real_main)?.join().map_err(|_| anyhow::anyhow!("main thread panicked"))?
}

fn real_main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args: Vec<String> = std::env::args().collect();
    let data_dir = find_data_dir(opt(&args, "--data"));
    let data = assets::GameData::new(&data_dir);
    // the settings file, then what the command line sets
    let settings_path = settings::path(opt(&args, "--config"), &data_dir);
    let mut settings = settings::Settings::load(&settings_path);
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
    if let Some(n) = opt(&args, "--fps") {
        settings.fps = match n.parse() {
            Ok(n) if n == 0 || (10..=1000).contains(&n) => n,
            _ => anyhow::bail!("--fps {n}: not 0 (vsync) or a frame rate from 10 to 1000"),
        };
    }
    if args.iter().any(|a| a == "--bilinear") {
        settings.bilinear = true;
    }
    settings::init(settings.clone());
    let cfg = game::RaceConfig { track, variant: &variant, player_car: &car, opponents, laps, traffic: Vec::new() };
    match command(&args) {
        None => app::run(&data, None, settings, settings_path)?,
        Some("play") => app::run(
            &data,
            Some(app::RaceConfigOwned {
                track,
                variant: variant.clone(),
                player_car: car.clone(),
                opponents: cfg.opponents.clone(),
                laps,
            }),
            settings,
            settings_path,
        )?,
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
        Some(cmd) => {
            if !dev::run(cmd, &args, &data, &cfg)? {
                eprintln!(
                    "usage:
  nfshs [--data DIR] [--config FILE] [--fps N] [--bilinear]   the game (the original's front
                                         end); the settings are in nfshs.ini (or FILE), --fps
                                         and --bilinear override its fps and texture filter
  nfshs play [--fps N] [--bilinear] [--track N] [--variant N|S|W] [--car CODE] [--opponents A,B|none] [--laps N]
  nfshs extract <image.bin> [outdir]
developer commands (see src/dev): {}",
                    dev::COMMANDS.join(", ")
                );
            }
        }
    }
    Ok(())
}
