//! The player's settings: nfshs.ini, read at start and again whenever it is saved while the
//! game runs. Written with every setting and its default when there is none.

use crate::render::{Bloom, Crt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The file written when there is none (and so the defaults: [`Settings::default`] reads it).
const DEFAULT_INI: &str = r#"; Need for Speed: High Stakes - settings
; Read at start, and again whenever this file is saved while the game runs.
; Delete it to get the defaults back. ; or # start a comment.

[display]
; start in fullscreen (F5 toggles it)
fullscreen = false
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
; keep the save's locked tracks and cars (false: everything unlocked)
locked = false
"#;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub fullscreen: bool,
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
}

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
            fullscreen: false,
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
                ("display", "fullscreen") => set_bool(&mut self.fullscreen, value),
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
