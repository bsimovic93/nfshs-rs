//! The player's settings: nfshs.ini, read at start and again whenever it is saved while the
//! game runs. Written with every setting and its default when there is none, and the game
//! options' graphics screen writes its choices into it.

use crate::render::{Bloom, Crt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The file written when there is none (and so the defaults: [`Settings::default`] reads it).
const DEFAULT_INI: &str = r#"; Need for Speed: High Stakes - settings
; Read at start, and again whenever this file is saved while the game runs.
; Delete it to get the defaults back. ; or # start a comment.

[display]
; windowed, borderless (full screen at the desktop's resolution) or fullscreen (full screen at
; the resolution below, when the display has it; else borderless). F5 toggles the window.
; Also in the game options' graphics screen, as are the resolution, antialiasing, texture
; filter, CRT filter and bloom: choosing them there writes them here.
mode = windowed
; the window's size, or the full screen's resolution (width x height in pixels)
resolution = 1280x960
; hold this frame rate without vsync (e.g. 30, 60, 144, 240); 0: vsync at the display's refresh
fps = 0
; the frame-rate counter at the top left (F3 toggles it)
show_fps = true
; antialiasing: smooth polygon edges (off, msaa2, msaa4 or msaa8; the most the GPU has up to it)
antialiasing = msaa4
; supersampling: draw the frame at this many times the window's width and height and average
; it down: also smooths the trees' cut-out edges and the textures (1: off, 2: no crawling
; edges and thin lines in the distance, 3 or 4: smoother still; costs more than antialiasing)
supersampling = 2

[textures]
; the 3D scene's textures: nearest (the PS1's sharp texels) or bilinear (smoothed; F4 toggles it)
filter = nearest
; smaller copies of the textures for the distance: no shimmer on far away ground
mipmaps = true
; mip levels further (> 0: blurrier in the distance) or nearer (< 0: sharper, more shimmer)
mip_bias = 0.0

[crt]
; the CRT screen filter (F6 toggles it)
enabled = false
; barrel distortion across and down (0: a flat screen)
curvature_x = 0.015
curvature_y = 0.02
; the aperture grille's dark stripes (1: no grille, lower: stronger)
mask_dark = 0.72
; the beam's width in lines on black and on white (lower: darker gaps between the scanlines)
beam_min = 0.22
beam_max = 0.42
gamma = 2.2
brightness = 1.2
; darkening towards the corners (0: none)
vignette = 0.12
; a scanline's height in window pixels (0: by the window's height)
line_px = 0

[bloom]
; the picture's bright parts glowing over what is around them (F7 toggles it)
enabled = false
; how strongly they glow
intensity = 0.5
; how bright a part must be to glow (0 to 1; lower: more of the picture glows)
threshold = 0.6

[input]
; gamepad vibration
rumble = true
; the gamepad's layout:
; 1: original - the buttons as they sit on the PlayStation pad (bumpers L1/R1, triggers L2/R2):
;    the game's own controller configuration applies (its default A: Cross accelerate, Square
;    brake/reverse, Circle handbrake, Triangle camera, R1/R2 shift up/down, L2 look behind,
;    d-pad up horn, L1 + a button: lights and police, Select reset)
; 2: modern - as in Underground 2, Most Wanted and Carbon (with the game's configuration A):
;    RT accelerate, LT brake/reverse, A handbrake, Y camera, B look behind, RB/LB shift up/down,
;    X horn, left stick click the L1 (lights and police) button, Back reset; menus: A accept, B back
controls = 1

[audio]
; an output device other than the default (part of its name; empty: the default)
device =

[game]
; the memory card image (empty: memcard.mcd next to the game data folder); a DuckStation card works as is
memcard =
; save the game to the memory card by itself (after a race, on leaving the game options and on
; quitting) instead of asking; it is loaded from the card at start either way
autosave = true
; keep the save's locked tracks and cars (false: everything unlocked)
locked = false
"#;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub window_mode: WindowMode,
    /// The window's size, or the full screen's resolution, in pixels.
    pub resolution: (u32, u32),
    /// The frame rate held to without vsync (0: vsync).
    pub fps: u32,
    pub show_fps: bool,
    /// MSAA samples per pixel (1: none).
    pub msaa: u32,
    /// Frames drawn at this many times the window's size (1: not supersampled).
    pub supersample: u32,
    pub bilinear: bool,
    pub mipmaps: bool,
    pub mip_bias: f32,
    pub crt: bool,
    pub crt_params: Crt,
    pub bloom: bool,
    pub bloom_params: Bloom,
    pub rumble: bool,
    pub controls: Controls,
    pub audio_device: String,
    pub memcard: String,
    pub locked: bool,
    pub autosave: bool,
}

/// How the game is shown (`[display] mode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowMode {
    Windowed,
    /// Full screen at the desktop's resolution.
    Borderless,
    /// Full screen at [`Settings::resolution`] (the display switched to it).
    Fullscreen,
}

impl WindowMode {
    pub const ALL: [WindowMode; 3] = [WindowMode::Windowed, WindowMode::Borderless, WindowMode::Fullscreen];

    pub fn name(self) -> &'static str {
        match self {
            WindowMode::Windowed => "windowed",
            WindowMode::Borderless => "borderless",
            WindowMode::Fullscreen => "fullscreen",
        }
    }
}

/// The resolutions the game options' graphics screen offers (any other can be set in the file).
pub const RESOLUTIONS: [(u32, u32); 12] = [
    (1024, 768),
    (1280, 720),
    (1280, 960),
    (1366, 768),
    (1600, 900),
    (1600, 1200),
    (1920, 1080),
    (1920, 1200),
    (2560, 1080),
    (2560, 1440),
    (3440, 1440),
    (3840, 2160),
];

/// The gamepad's layout (`[input] controls`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Controls {
    /// The PlayStation pad's buttons as they sit: the game's own configuration decides.
    Original,
    /// Underground 2 / Most Wanted / Carbon: triggers accelerate and brake.
    Modern,
}

impl Default for Settings {
    fn default() -> Self {
        let mut s = Self {
            window_mode: WindowMode::Windowed,
            resolution: (1280, 960),
            fps: 0,
            show_fps: true,
            msaa: 1,
            supersample: 1,
            bilinear: false,
            mipmaps: true,
            mip_bias: 0.0,
            crt: false,
            crt_params: Crt::default(),
            bloom: false,
            bloom_params: Bloom::default(),
            rumble: true,
            controls: Controls::Original,
            audio_device: String::new(),
            memcard: String::new(),
            locked: false,
            autosave: true,
        };
        s.apply(DEFAULT_INI, Path::new("(defaults)"));
        s
    }
}

impl Settings {
    /// The settings in `path` (written with the defaults if there is no such file). What it
    /// does not set, or sets wrong (with a warning), keeps its default.
    pub fn load(path: &Path) -> Self {
        let mut s = Self::default();
        match std::fs::read_to_string(path) {
            Ok(text) => s.apply(&text, path),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => match std::fs::write(path, DEFAULT_INI) {
                Ok(()) => log::info!("settings: wrote {} with the defaults", path.display()),
                Err(e) => log::warn!("settings: could not write {}: {e}", path.display()),
            },
            Err(e) => log::warn!("settings: could not read {}: {e}", path.display()),
        }
        s
    }

    /// Set what `text` (an ini file) sets.
    fn apply(&mut self, text: &str, path: &Path) {
        let mut section = String::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            let warn = |what: &str| log::warn!("settings: {}:{}: {what}: {line}", path.display(), n + 1);
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.trim().to_ascii_lowercase();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                warn("not key = value");
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();
            let ok = match (section.as_str(), key.as_str()) {
                ("display", "mode") => WindowMode::ALL.into_iter().find(|m| m.name().eq_ignore_ascii_case(value)).map(|m| self.window_mode = m).is_some(),
                // (before `mode`: true is borderless)
                ("display", "fullscreen") => {
                    let mut on = false;
                    set_bool(&mut on, value) && {
                        self.window_mode = if on { WindowMode::Borderless } else { WindowMode::Windowed };
                        true
                    }
                }
                ("display", "resolution") => value
                    .to_ascii_lowercase()
                    .split_once('x')
                    .and_then(|(w, h)| Some((w.trim().parse().ok()?, h.trim().parse().ok()?)))
                    .filter(|&(w, h): &(u32, u32)| (320..=16384).contains(&w) && (240..=16384).contains(&h))
                    .map(|r| self.resolution = r)
                    .is_some(),
                ("display", "fps") => value.parse().ok().filter(|f| *f == 0 || (10..=1000).contains(f)).map(|f| self.fps = f).is_some(),
                ("display", "show_fps") => set_bool(&mut self.show_fps, value),
                ("display", "antialiasing") => match value.to_ascii_lowercase().as_str() {
                    "off" | "none" | "0" | "1" => Some(1),
                    "msaa2" | "2" => Some(2),
                    "msaa4" | "4" => Some(4),
                    "msaa8" | "8" => Some(8),
                    _ => None,
                }
                .map(|n| self.msaa = n)
                .is_some(),
                ("display", "supersampling") => value.parse().ok().filter(|k| (1..=4).contains(k)).map(|k| self.supersample = k).is_some(),
                ("textures", "filter") => match value.to_ascii_lowercase().as_str() {
                    "nearest" => {
                        self.bilinear = false;
                        true
                    }
                    "bilinear" => {
                        self.bilinear = true;
                        true
                    }
                    _ => false,
                },
                ("textures", "mipmaps") => set_bool(&mut self.mipmaps, value),
                ("textures", "mip_bias") => set_f32(&mut self.mip_bias, value),
                ("crt", "enabled") => set_bool(&mut self.crt, value),
                ("crt", "curvature_x") => set_f32(&mut self.crt_params.curvature[0], value),
                ("crt", "curvature_y") => set_f32(&mut self.crt_params.curvature[1], value),
                ("crt", "mask_dark") => set_f32(&mut self.crt_params.mask_dark, value),
                ("crt", "beam_min") => set_f32(&mut self.crt_params.beam_min, value),
                ("crt", "beam_max") => set_f32(&mut self.crt_params.beam_max, value),
                ("crt", "gamma") => set_f32(&mut self.crt_params.gamma, value),
                ("crt", "brightness") => set_f32(&mut self.crt_params.brightness, value),
                ("crt", "vignette") => set_f32(&mut self.crt_params.vignette, value),
                ("crt", "line_px") => set_f32(&mut self.crt_params.line_px, value),
                ("bloom", "enabled") => set_bool(&mut self.bloom, value),
                ("bloom", "intensity") => set_f32(&mut self.bloom_params.intensity, value) && self.bloom_params.intensity >= 0.0,
                ("bloom", "threshold") => set_f32(&mut self.bloom_params.threshold, value),
                ("input", "rumble") => set_bool(&mut self.rumble, value),
                ("input", "controls") => match value.to_ascii_lowercase().as_str() {
                    "1" | "original" => Some(Controls::Original),
                    "2" | "modern" => Some(Controls::Modern),
                    _ => None,
                }
                .map(|c| self.controls = c)
                .is_some(),
                ("audio", "device") => {
                    self.audio_device = value.to_string();
                    true
                }
                ("game", "memcard") => {
                    self.memcard = value.to_string();
                    true
                }
                ("game", "locked") => set_bool(&mut self.locked, value),
                ("game", "autosave") => set_bool(&mut self.autosave, value),
                _ => {
                    warn("unknown setting");
                    continue;
                }
            };
            if !ok {
                warn("bad value");
            }
        }
    }
}

impl Settings {
    /// Write what the game options' graphics screen sets into the settings file at `path`
    /// (the rest of it, comments too, stays as it is).
    pub fn save_graphics(&self, path: &Path) -> std::io::Result<()> {
        let on_off = |b: bool| if b { "true" } else { "false" };
        let (w, h) = self.resolution;
        let res = format!("{w}x{h}");
        let aa = match self.msaa {
            1 => "off".to_string(),
            n => format!("msaa{n}"),
        };
        set_in_file(
            path,
            &[
                // (`fullscreen` is the older name for the mode)
                ("display", &["mode", "fullscreen"], self.window_mode.name()),
                ("display", &["resolution"], &res),
                ("display", &["antialiasing"], &aa),
                ("textures", &["filter"], if self.bilinear { "bilinear" } else { "nearest" }),
                ("crt", &["enabled"], on_off(self.crt)),
                ("bloom", &["enabled"], on_off(self.bloom)),
            ],
        )
    }
}

/// Set keys in the ini file at `path` (the defaults' file if there is none): each entry's
/// section, its key (and older names for it, whose line it takes) and value. A key the file
/// lacks goes after the last setting of its section, or into a new section at the end.
fn set_in_file(path: &Path, entries: &[(&str, &[&str], &str)]) -> std::io::Result<()> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DEFAULT_INI.to_string(),
        Err(e) => return Err(e),
    };
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let section_of = |lines: &[String], at: usize| {
        lines[..=at].iter().rev().find_map(|l| l.trim().strip_prefix('[').and_then(|l| l.strip_suffix(']')).map(|s| s.trim().to_ascii_lowercase())).unwrap_or_default()
    };
    let key_of = |line: &str| {
        let l = line.trim();
        (!l.starts_with(';') && !l.starts_with('#')).then(|| l.split_once('=').map(|(k, _)| k.trim().to_ascii_lowercase())).flatten()
    };
    for &(section, keys, value) in entries {
        let new = format!("{} = {value}", keys[0]);
        let mut done = false;
        let mut k = 0;
        while k < lines.len() {
            if key_of(&lines[k]).is_some_and(|key| keys.contains(&key.as_str())) && section_of(&lines, k) == section {
                if done {
                    // (an older name's line besides the key's own)
                    lines.remove(k);
                    continue;
                }
                lines[k] = new.clone();
                done = true;
            }
            k += 1;
        }
        if done {
            continue;
        }
        let header = lines.iter().position(|l| l.trim().strip_prefix('[').and_then(|l| l.strip_suffix(']')).is_some_and(|s| s.trim().eq_ignore_ascii_case(section)));
        match header {
            Some(h) => {
                let end = lines[h + 1..].iter().position(|l| l.trim().starts_with('[')).map_or(lines.len(), |p| h + 1 + p);
                let last = (h + 1..end).rev().find(|&i| key_of(&lines[i]).is_some()).unwrap_or(h);
                lines.insert(last + 1, new);
            }
            None => {
                lines.push(String::new());
                lines.push(format!("[{section}]"));
                lines.push(new);
            }
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    std::fs::write(path, out)
}

fn set_bool(v: &mut bool, s: &str) -> bool {
    match s.to_ascii_lowercase().as_str() {
        "true" | "on" | "yes" | "1" => *v = true,
        "false" | "off" | "no" | "0" => *v = false,
        _ => return false,
    }
    true
}

fn set_f32(v: &mut f32, s: &str) -> bool {
    s.parse().ok().filter(|f: &f32| f.is_finite()).map(|f| *v = f).is_some()
}

/// Where the settings are: `--config FILE`, or nfshs.ini in the current directory or next to
/// the executable if there is one there, else beside the game data folder (where the memory
/// card is).
pub fn path(explicit: Option<&str>, data_dir: &Path) -> PathBuf {
    if let Some(p) = explicit {
        return PathBuf::from(p);
    }
    const NAME: &str = "nfshs.ini";
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    for dir in [Some(PathBuf::from(".")), exe_dir].into_iter().flatten() {
        if dir.join(NAME).is_file() {
            return dir.join(NAME);
        }
    }
    match data_dir.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join(NAME),
        _ => PathBuf::from(NAME),
    }
}

static SETTINGS: OnceLock<Settings> = OnceLock::new();

/// Make `s` the settings [`get`] returns (once, at start).
pub fn init(s: Settings) {
    let _ = SETTINGS.set(s);
}

/// The settings read at start (the defaults if none were), for what only takes them then.
pub fn get() -> &'static Settings {
    SETTINGS.get_or_init(Settings::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphics_written_back() {
        let dir = std::env::temp_dir().join(format!("nfshs-settings-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("nfshs.ini");
        // an older file: `fullscreen` for the mode, no resolution, no [bloom]
        std::fs::write(&path, "[display]\n; full screen?\nfullscreen = true\nfps = 144\n\n[textures]\nfilter = nearest\n\n[crt]\nenabled = false\n").unwrap();
        let mut s = Settings::load(&path);
        assert_eq!((s.window_mode, s.fps), (WindowMode::Borderless, 144));
        s.window_mode = WindowMode::Fullscreen;
        s.resolution = (1920, 1080);
        s.msaa = 8;
        s.bilinear = true;
        s.crt = true;
        s.bloom = true;
        s.save_graphics(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("; full screen?\nmode = fullscreen\nfps = 144\nresolution = 1920x1080\nantialiasing = msaa8\n"), "{text}");
        assert!(!text.contains("fullscreen = "), "{text}");
        assert!(text.contains("[bloom]\nenabled = true"), "{text}");
        let again = Settings::load(&path);
        assert_eq!(again, s);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
