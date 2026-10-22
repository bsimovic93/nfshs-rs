//! Window + event loop (winit 0.30 ApplicationHandler): the original's front end and races.

use crate::assets::{Atlas, GameData};
use crate::game::{Game, Input, RaceConfig};
use crate::render::{FrameParams, Renderer};
use crate::settings::{Controls, Settings, WindowMode};
use glam::{Mat4, Vec3};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

enum Screen {
    /// Before the window opens, and after the game could not go on (the app then closes).
    Off,
    /// The original game's own front end, running in the recompiled original.
    FrontEnd(Box<crate::game::original::OriginalSim>),
    Race(Box<Game>),
}

#[derive(Default, Clone, Copy)]
struct Keys {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    space: bool,
    enter: bool,
    escape: bool,
    camera: bool,
    backspace: bool,
    tab: bool,
    start: bool,
}

pub struct App {
    data: GameData,
    screen: Screen,
    /// Why the game could not go on (returned by [`run`]).
    error: Option<anyhow::Error>,
    audio: Option<crate::audio::Audio>,
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    keys: Keys,
    autopilot: bool,
    gilrs: Option<gilrs::Gilrs>,
    /// The force feedback effects of the connected gamepads (the game's DualShock motors).
    rumble: Vec<Rumble>,
    last: Instant,
    fps: (u32, Instant),
    /// The frame-rate counter at the top left (F3 toggles it): frames counted since when.
    counter: Option<(u32, Instant)>,
    /// When the next frame is due (with `settings.fps`).
    next_frame: Instant,
    /// The settings (nfshs.ini; the function keys change them for the session).
    settings: Settings,
    settings_path: PathBuf,
    /// The settings file's time when it was last read, and when that was last looked at.
    settings_seen: (Option<SystemTime>, Instant),
    /// Start straight into a race (command-line `play` with explicit options).
    direct: Option<RaceConfigOwned>,
    /// The front end's movie being shown, and its soundtrack's voice.
    movie: Option<crate::game::fe::MoviePlayer>,
    movie_voice: Option<crate::audio::VoiceId>,
    /// The front end's showroom car model.
    showroom: Option<crate::game::fe::Showroom>,
    /// The window was closed (in the front end): when, while the game saves before it quits.
    closing: Option<Instant>,
}

#[derive(Clone)]
pub struct RaceConfigOwned {
    pub track: u32,
    pub variant: String,
    pub player_car: String,
    pub opponents: Vec<String>,
    pub laps: i32,
}

impl App {
    /// The game cannot go on: the app closes and [`run`] returns `e`.
    fn fail(&mut self, e: anyhow::Error) {
        log::error!("{e:#}");
        self.error = Some(e);
        self.screen = Screen::Off;
    }

    /// Straight into a race (the command line's `play`).
    fn start_race(&mut self, cfg: RaceConfigOwned) {
        let mut atlas = Atlas::race();
        let rc = RaceConfig { track: cfg.track, variant: &cfg.variant, player_car: &cfg.player_car, opponents: cfg.opponents.clone(), laps: cfg.laps, traffic: Vec::new() };
        match Game::load(&self.data, &mut atlas, &rc) {
            Ok(game) => self.begin_race(game, atlas),
            Err(e) => self.fail(e.context("could not load the race")),
        }
    }

    /// Power on the original game: its front end, from which it starts its own races.
    fn enter_front_end(&mut self) {
        match crate::game::original::OriginalSim::start_main(&self.data) {
            Ok(sim) => {
                if let Some(r) = &mut self.renderer {
                    r.clear_world();
                }
                self.screen = Screen::FrontEnd(Box::new(sim));
            }
            Err(e) => self.fail(e.context("the original front end did not start")),
        }
    }

    /// The original left its front end for a race: load what it races and show it.
    fn front_end_race(&mut self, sim: Box<crate::game::original::OriginalSim>) {
        let cfg = race_of(sim.snapshot());
        let rc = RaceConfig { track: cfg.track, variant: &cfg.variant, player_car: &cfg.player_car, opponents: cfg.opponents.clone(), laps: cfg.laps, traffic: Vec::new() };
        log::info!("the original starts a race: track {}{} with {}", cfg.track, cfg.variant, cfg.player_car);
        let mut atlas = Atlas::race();
        match Game::load_with(&self.data, &mut atlas, &rc, *sim) {
            Ok(game) => self.begin_race(game, atlas),
            Err(e) => self.fail(e.context("could not load the race")),
        }
    }

    /// Show a loaded race.
    fn begin_race(&mut self, game: Game, atlas: Atlas) {
        log::info!("race atlas: {:.0}% used", atlas.used() * 100.0);
        if let Some(r) = &mut self.renderer {
            r.set_atlas(&atlas);
            r.set_world(&game.track.verts, &game.track.indices);
        }
        self.screen = Screen::Race(Box::new(game));
    }

    /// The front end's pad from keyboard and gamepad.
    fn front_end_input(&mut self) -> Input {
        let k = self.keys;
        let mut i = Input {
            up: k.up,
            down: k.down,
            left: k.left,
            right: k.right,
            accept: k.enter || k.space,
            back: k.escape,
            circle: k.backspace,
            square: k.tab,
            start: k.start,
            ..Default::default()
        };
        // (the stick goes to the original as a stick: it reads it as directions too)
        self.read_gamepads(&mut i, [false; 2]);
        i
    }

    /// The first two gamepads into `i` as the PS1's controllers 1 and 2, in the settings'
    /// layout; `driving`: each one drives a car rather than working a menu.
    fn read_gamepads(&mut self, i: &mut Input, driving: [bool; 2]) {
        let controls = self.settings.controls;
        let Some(g) = &mut self.gilrs else { return };
        while g.next_event().is_some() {}
        for (k, (_, pad)) in g.gamepads().take(2).enumerate() {
            let (buttons, sticks) = (ps1_buttons(&pad, controls, driving[k]), Some(ps1_sticks(&pad)));
            if k == 0 {
                (i.pad1, i.sticks) = (buttons, sticks);
            } else {
                (i.pad2, i.pad2_sticks) = (buttons, sticks);
            }
        }
    }

    /// Merge keyboard and the first connected gamepad into game input.
    fn race_input(&mut self) -> Input {
        let k = self.keys;
        let mut i = Input {
            up: k.up,
            down: k.down,
            left: k.left,
            right: k.right,
            handbrake: k.space,
            camera: k.camera,
            autopilot: self.autopilot,
            start: k.escape,
            accept: k.enter,
            ..Default::default()
        };
        // the pause menu and, for player 1, the results screens after the finish (the
        // original drives the car then) are menus
        let driving = match &self.screen {
            Screen::Race(game) => {
                let s = game.sim.frame_snapshot();
                [!s.paused && !s.player_finished, !s.paused]
            }
            _ => [false; 2],
        };
        self.read_gamepads(&mut i, driving);
        i
    }

    /// The game's DualShock motors on the gamepads: controller 1 the first gamepad, 2 the
    /// second (as the input), the big motor (0..255) the strong one and the small one (on or
    /// off) the weak one. None with the settings' `rumble` off or NFSHS_NO_RUMBLE=1.
    fn update_rumble(&mut self) {
        let state = match &self.screen {
            Screen::FrontEnd(sim) => sim.frame_snapshot().rumble,
            Screen::Race(game) => game.sim.frame_snapshot().rumble,
            Screen::Off => [[0; 2]; 2],
        };
        let Some(g) = &mut self.gilrs else { return };
        let pads: Vec<gilrs::GamepadId> = g.gamepads().map(|(id, _)| id).take(2).collect();
        self.rumble.retain(|r| pads.contains(&r.id));
        if !self.settings.rumble || std::env::var_os("NFSHS_NO_RUMBLE").is_some() {
            // (dropping the effects stops them)
            self.rumble.clear();
            return;
        }
        for (port, &id) in pads.iter().enumerate() {
            if !self.rumble.iter().any(|r| r.id == id) {
                if !g.gamepad(id).is_ff_supported() {
                    continue;
                }
                match (Rumble::effect(g, id, true), Rumble::effect(g, id, false)) {
                    (Ok(strong), Ok(weak)) => self.rumble.push(Rumble { id, strong, weak, level: [0.0; 2] }),
                    (Err(e), _) | (_, Err(e)) => {
                        log::warn!("no force feedback on gamepad {id}: {e}");
                        continue;
                    }
                }
            }
            let Some(r) = self.rumble.iter_mut().find(|r| r.id == id) else { continue };
            let level = [state[port][1] as f32 / 255.0, if state[port][0] != 0 { 1.0 } else { 0.0 }];
            if level != r.level {
                let _ = r.strong.set_gain(level[0]);
                let _ = r.weak.set_gain(level[1]);
                r.level = level;
            }
        }
    }

    /// With `settings.fps`: wait until the next frame is due. Sleeps to within a couple of
    /// milliseconds and spins the rest, as sleeps overshoot. A frame late by less than a
    /// period keeps the schedule (the next ones make it up); later, the clock restarts rather
    /// than rushing the following frames.
    fn pace(&mut self) {
        let fps = self.settings.fps;
        if fps == 0 {
            return;
        }
        let period = Duration::from_secs_f64(1.0 / fps as f64);
        let now = Instant::now();
        if self.next_frame > now {
            let left = self.next_frame - now;
            if left > Duration::from_millis(2) {
                std::thread::sleep(left - Duration::from_millis(2));
            }
            while Instant::now() < self.next_frame {
                std::hint::spin_loop();
            }
            self.next_frame += period;
        } else if now - self.next_frame < period {
            self.next_frame += period;
        } else {
            self.next_frame = now + period;
        }
    }

    /// Put the settings into effect (`old`: the ones in effect before, None at start).
    fn apply_settings(&mut self, old: Option<&Settings>) {
        let s = &self.settings;
        if let Some(r) = &mut self.renderer {
            if old.is_none_or(|o| (o.fps == 0) != (s.fps == 0)) {
                r.set_vsync(s.fps == 0);
            }
            r.supersample = s.supersample;
            r.msaa = s.msaa;
            r.bilinear = s.bilinear;
            r.mipmaps = s.mipmaps;
            r.mip_bias = s.mip_bias;
            r.crt = s.crt;
            r.set_crt(&s.crt_params);
            r.bloom = s.bloom;
            r.set_bloom(&s.bloom_params);
        }
        if old.is_some_and(|o| o.window_mode != s.window_mode || o.resolution != s.resolution) {
            self.apply_window();
        }
        if old.is_none_or(|o| o.show_fps != s.show_fps) {
            self.show_counter(s.show_fps);
        }
        self.next_frame = Instant::now();
    }

    /// The window as the settings have it: windowed at the resolution, full screen at the
    /// desktop's, or full screen at the resolution (the display's mode of that size with the
    /// highest refresh rate; at the desktop's when it has none).
    fn apply_window(&self) {
        let Some(w) = &self.window else { return };
        let (rw, rh) = self.settings.resolution;
        match self.settings.window_mode {
            WindowMode::Windowed => {
                w.set_fullscreen(None);
                let _ = w.request_inner_size(winit::dpi::PhysicalSize::new(rw, rh));
            }
            WindowMode::Borderless => w.set_fullscreen(Some(Fullscreen::Borderless(None))),
            WindowMode::Fullscreen => {
                let mode = w.current_monitor().and_then(|m| {
                    m.video_modes().filter(|v| v.size() == winit::dpi::PhysicalSize::new(rw, rh)).max_by_key(|v| (v.refresh_rate_millihertz(), v.bit_depth()))
                });
                if mode.is_none() {
                    log::warn!("the display has no {rw}x{rh} mode: full screen at its own resolution");
                }
                w.set_fullscreen(Some(mode.map_or(Fullscreen::Borderless(None), Fullscreen::Exclusive)));
            }
        }
    }

    /// The settings to the game options' graphics screen, and what the player chose there
    /// into effect and into the settings file.
    fn sync_graphics(&mut self) {
        crate::ps1::host::show_graphics(&self.settings);
        let mut new = self.settings.clone();
        if !crate::ps1::host::take_graphics_choice(&mut new) || new == self.settings {
            return;
        }
        let old = std::mem::replace(&mut self.settings, new);
        self.apply_settings(Some(&old));
        match self.settings.save_graphics(&self.settings_path) {
            // (not read back: the function keys' changes for the session stay)
            Ok(()) => self.settings_seen.0 = std::fs::metadata(&self.settings_path).and_then(|m| m.modified()).ok(),
            Err(e) => log::warn!("settings: could not write {}: {e}", self.settings_path.display()),
        }
    }

    fn show_counter(&mut self, on: bool) {
        self.counter = on.then(|| (0, Instant::now()));
        if let (false, Some(r)) = (on, &mut self.renderer) {
            r.set_stats_text("");
        }
    }

    /// Read the settings file again when it was saved since (looked at twice a second).
    fn reload_settings(&mut self) {
        if self.settings_seen.1.elapsed() < Duration::from_millis(500) {
            return;
        }
        self.settings_seen.1 = Instant::now();
        let modified = std::fs::metadata(&self.settings_path).and_then(|m| m.modified()).ok();
        if modified == self.settings_seen.0 {
            return;
        }
        self.settings_seen.0 = modified;
        let new = Settings::load(&self.settings_path);
        log::info!("settings: read {} again", self.settings_path.display());
        let old = std::mem::replace(&mut self.settings, new);
        self.apply_settings(Some(&old));
    }

    fn frame(&mut self, el: &ActiveEventLoop) {
        self.reload_settings();
        self.sync_graphics();
        // closing, the game is given a moment to save first
        if self.closing.is_some_and(|t| t.elapsed() > Duration::from_secs(3)) {
            el.exit();
            return;
        }
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.1);
        self.last = now;
        // NFSHS_FPS=1: the window's frame rate every few seconds
        if std::env::var_os("NFSHS_FPS").is_some() {
            self.fps.0 += 1;
            let secs = (now - self.fps.1).as_secs_f32();
            if secs >= 3.0 {
                log::info!("{:.1} fps", self.fps.0 as f32 / secs);
                self.fps = (0, now);
            }
        }
        // the counter shows each half second's frame rate
        if let (Some((frames, since)), Some(r)) = (&mut self.counter, &mut self.renderer) {
            *frames += 1;
            let secs = (now - *since).as_secs_f32();
            if secs >= 0.5 {
                r.set_stats_text(&format!("{:.0} FPS", *frames as f32 / secs));
                (*frames, *since) = (0, now);
            }
        }
        if let Screen::FrontEnd(_) = &self.screen {
            let input = self.front_end_input();
            if let Screen::FrontEnd(sim) = &mut self.screen {
                sim.advance(dt, &input);
                if sim.stopped() {
                    el.exit();
                    return;
                }
                if !sim.in_front_end() {
                    let Screen::FrontEnd(mut sim) = std::mem::replace(&mut self.screen, Screen::Off) else { unreachable!() };
                    sim.reset_race();
                    self.front_end_race(sim);
                }
            }
        }
        if let Screen::Race(_) = &self.screen {
            let input = self.race_input();
            if let Screen::Race(game) = &mut self.screen {
                game.update(dt, input);
                // the original has quit (its pause menu's Quit in a race started straight in)
                if game.sim.stopped() {
                    el.exit();
                    return;
                }
                // a race the original started from its front end: back to it when it is
                if game.sim.in_front_end() {
                    self.keys = Keys::default();
                    let Screen::Race(game) = std::mem::replace(&mut self.screen, Screen::Off) else { unreachable!() };
                    if let Some(r) = &mut self.renderer {
                        r.clear_world();
                    }
                    self.screen = Screen::FrontEnd(Box::new(game.sim));
                    return;
                }
            }
        }
        if let Screen::Off = self.screen {
            el.exit();
            return;
        }
        self.update_rumble();
        // a front-end movie has ended: its soundtrack stops
        let playing = match &self.screen {
            Screen::FrontEnd(sim) => sim.snapshot().overlay.iter().any(|o| matches!(o, crate::ps1::host::OverlayCmd::Movie { .. })),
            _ => false,
        };
        if !playing && self.movie.take().is_some() {
            if let (Some(audio), Some(v)) = (&self.audio, self.movie_voice.take()) {
                audio.stop(v);
            }
        }
        let Some(r) = &mut self.renderer else { return };
        let (w, h) = r.size;
        if let Screen::Race(game) = &mut self.screen {
            draw_race(r, game, w as f32, h.max(1) as f32, false);
            return;
        }
        match &self.screen {
            Screen::FrontEnd(sim) => {
                let snap = sim.snapshot();
                let movie = snap.overlay.iter().find_map(|o| match o {
                    crate::ps1::host::OverlayCmd::Movie { file, frame, fps } => Some((file.clone(), *frame, *fps)),
                    _ => None,
                });
                if let Some((file, frame, fps)) = movie {
                    // a movie: decode the frame shown; its soundtrack starts with it
                    if self.movie.as_ref().is_none_or(|m| m.file != file) {
                        self.movie = crate::game::fe::MoviePlayer::load(&self.data, &file);
                        if let (Some(audio), Some(m)) = (&self.audio, &self.movie) {
                            if let Some(v) = self.movie_voice.take() {
                                audio.stop(v);
                            }
                            self.movie_voice = Some(audio.play(Arc::new(m.audio_from(frame, fps)), 1.0, 1.0, false));
                        }
                    }
                    if let Some(m) = &mut self.movie {
                        let fe = m.frame(frame, w as f32, h as f32);
                        r.write_atlas(&fe.atlas);
                        r.render(&flat_params(), &[], &[], &fe.vertices);
                    }
                    return;
                }
                if let Some(vram) = &snap.vram {
                    // the showroom car, loaded when the front end shows a different one
                    let codes = crate::game::fe::FeScene::car_codes(&snap.overlay);
                    if !codes.is_empty() && self.showroom.as_ref().is_none_or(|s| s.cars != codes) {
                        self.showroom = crate::game::fe::Showroom::load(&self.data, &codes);
                    }
                    let scene = crate::game::fe::FeScene::build(&snap.overlay, vram, (snap.display[2], snap.display[3]), w as f32, h as f32, self.showroom.as_ref());
                    draw_front_end(r, scene, false);
                }
            }
            Screen::Off | Screen::Race(_) => {}
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let (rw, rh) = self.settings.resolution;
        let attrs = Window::default_attributes()
            .with_title("Need for Speed: High Stakes - Rust rebuild")
            .with_inner_size(winit::dpi::PhysicalSize::new(rw, rh))
            .with_fullscreen((self.settings.window_mode == WindowMode::Borderless).then_some(Fullscreen::Borderless(None)));
        let window = Arc::new(el.create_window(attrs).expect("window"));
        let size = window.inner_size();
        let atlas = Atlas::new(256);
        let r = pollster::block_on(Renderer::new(Some(window.clone()), (size.width, size.height), &atlas)).expect("renderer");
        self.renderer = Some(r);
        self.window = Some(window);
        if self.settings.window_mode == WindowMode::Fullscreen {
            self.apply_window();
        }
        self.apply_settings(None);
        match crate::audio::Audio::new() {
            Ok(a) => self.audio = Some(a),
            Err(e) => log::warn!("audio disabled: {e}"),
        }
        match self.direct.take() {
            Some(cfg) => self.start_race(cfg),
            None => self.enter_front_end(),
        }
        self.last = Instant::now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                // in the front end the game saves first (when autosaving) and then quits; a
                // second close does not wait
                if matches!(self.screen, Screen::FrontEnd(_)) && self.settings.autosave && self.closing.is_none() {
                    crate::ps1::host::request_close();
                    self.closing = Some(Instant::now());
                } else {
                    el.exit();
                }
            }
            WindowEvent::Resized(s) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(s.width, s.height);
                }
            }
            WindowEvent::KeyboardInput { event: KeyEvent { physical_key: PhysicalKey::Code(code), state, repeat, .. }, .. } => {
                let down = state == ElementState::Pressed;
                let k = &mut self.keys;
                match code {
                    KeyCode::ArrowUp | KeyCode::KeyW => k.up = down,
                    KeyCode::ArrowDown | KeyCode::KeyS => k.down = down,
                    KeyCode::ArrowLeft | KeyCode::KeyA => k.left = down,
                    KeyCode::ArrowRight | KeyCode::KeyD => k.right = down,
                    KeyCode::Space => k.space = down,
                    KeyCode::Enter | KeyCode::NumpadEnter => k.enter = down,
                    KeyCode::Escape => k.escape = down,
                    KeyCode::KeyC => k.camera = down,
                    KeyCode::Backspace => k.backspace = down,
                    KeyCode::Tab => k.tab = down,
                    KeyCode::F1 => k.start = down,
                    KeyCode::KeyP if down && !repeat => self.autopilot = !self.autopilot,
                    KeyCode::F4 if down && !repeat => {
                        self.settings.bilinear = !self.settings.bilinear;
                        if let Some(r) = &mut self.renderer {
                            r.bilinear = self.settings.bilinear;
                        }
                        log::info!("bilinear filtering {}", if self.settings.bilinear { "on" } else { "off" });
                    }
                    KeyCode::F3 if down && !repeat => {
                        self.settings.show_fps = !self.settings.show_fps;
                        self.show_counter(self.settings.show_fps);
                    }
                    KeyCode::F5 if down && !repeat => {
                        self.settings.window_mode = match self.settings.window_mode {
                            WindowMode::Windowed => WindowMode::Borderless,
                            _ => WindowMode::Windowed,
                        };
                        self.apply_window();
                    }
                    KeyCode::F6 if down && !repeat => {
                        self.settings.crt = !self.settings.crt;
                        if let Some(r) = &mut self.renderer {
                            r.crt = self.settings.crt;
                        }
                        log::info!("CRT filter {}", if self.settings.crt { "on" } else { "off" });
                    }
                    KeyCode::F7 if down && !repeat => {
                        self.settings.bloom = !self.settings.bloom;
                        if let Some(r) = &mut self.renderer {
                            r.bloom = self.settings.bloom;
                        }
                        log::info!("bloom {}", if self.settings.bloom { "on" } else { "off" });
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                self.pace();
                self.frame(el);
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}

pub fn run(data: &GameData, direct: Option<RaceConfigOwned>, settings: Settings, settings_path: PathBuf) -> anyhow::Result<()> {
    let el = EventLoop::new()?;
    let mut app = App {
        data: GameData::new(&data.root),
        screen: Screen::Off,
        error: None,
        audio: None,
        renderer: None,
        window: None,
        keys: Keys::default(),
        autopilot: false,
        gilrs: gilrs::Gilrs::new().map_err(|e| log::warn!("gamepad support disabled: {e}")).ok(),
        rumble: Vec::new(),
        last: Instant::now(),
        fps: (0, Instant::now()),
        next_frame: Instant::now(),
        counter: None,
        settings_seen: (std::fs::metadata(&settings_path).and_then(|m| m.modified()).ok(), Instant::now()),
        settings,
        settings_path,
        direct,
        movie: None,
        movie_voice: None,
        showroom: None,
        closing: None,
    };
    el.run_app(&mut app)?;
    match app.error.take() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Frame parameters for drawing 2D only (no world).
pub fn flat_params() -> FrameParams {
    FrameParams {
        atlas_writes: Vec::new(),
        world_ranges: None,
        objects: Vec::new(),
        shadows: Vec::new(),
        subtract_late: Vec::new(),
        sky_view_proj: Mat4::IDENTITY,
        sky_dome: 0,
        sky_top: Vec3::ZERO,
        view_proj: Mat4::IDENTITY,
        mirrored: false,
        cam_pos: Vec3::ZERO,
        fog_color: Vec3::ZERO,
        fog_start: 0.0,
        fog_end: 1.0,
        cam_fwd: Vec3::Z,
        depth_cue: None,
        night_beam: None,
    }
}

/// The race the original has started (its first race frame): track, cars and laps.
pub fn race_of(snap: &crate::ps1::race::Snapshot) -> RaceConfigOwned {
    RaceConfigOwned {
        track: snap.track,
        variant: snap.variant.clone(),
        player_car: snap.cars.first().map(|c| c.code.clone()).unwrap_or_default(),
        opponents: snap.cars.iter().skip(1).map(|c| c.code.clone()).filter(|c| crate::ps1::race::car_type(c).is_some()).collect(),
        laps: snap.laps.max(1) as i32,
    }
}

/// A gamepad as a PS1 pad's buttons in the layout `controls` (`driving`: a car, not a menu).
/// Original: the buttons as they sit on a DualShock (bumpers L1 and R1, triggers L2 and R2),
/// so the game's own controller configuration decides what they do. Modern (Underground 2,
/// Most Wanted, Carbon) is for the game's default configuration A (its table at 0x801220EC:
/// Cross accelerate, Square brake/reverse, Circle handbrake, Triangle camera, R1 and R2 shift
/// up and down, L2 look behind, Up horn, L1 the lights and police modifier, Select reset):
/// driving, the triggers accelerate and brake, A is the handbrake, Y the camera, B looks
/// behind, RB and LB shift; in a menu A accepts (Cross) and B goes back (Triangle). The sticks
/// are the DualShock's (see [`ps1_sticks`]).
fn ps1_buttons(pad: &gilrs::Gamepad, controls: Controls, driving: bool) -> u16 {
    use crate::ps1::race::pad::*;
    use gilrs::Button::*;
    let held = |b: gilrs::Button| match b {
        LeftTrigger2 | RightTrigger2 => pad.button_data(b).is_some_and(|d| d.value() > 0.3),
        _ => pad.is_pressed(b),
    };
    let layout: &[(gilrs::Button, u16)] = match (controls, driving) {
        (Controls::Original, _) => {
            &[(South, CROSS), (East, CIRCLE), (West, SQUARE), (North, TRIANGLE), (LeftTrigger, L1), (RightTrigger, R1), (LeftTrigger2, L2), (RightTrigger2, R2)]
        }
        (Controls::Modern, true) => &[
            (RightTrigger2, CROSS),
            (LeftTrigger2, SQUARE),
            (South, CIRCLE),
            (North, TRIANGLE),
            (East, L2),
            (RightTrigger, R1),
            (LeftTrigger, R2),
            (West, UP),
            (LeftThumb, L1),
        ],
        (Controls::Modern, false) => {
            &[(South, CROSS), (East, TRIANGLE), (West, SQUARE), (North, CIRCLE), (LeftTrigger, L1), (RightTrigger, R1), (LeftTrigger2, L2), (RightTrigger2, R2)]
        }
    };
    let common = [(DPadUp, UP), (DPadDown, DOWN), (DPadLeft, LEFT), (DPadRight, RIGHT), (Start, START), (Select, SELECT)];
    layout.iter().chain(&common).filter(|(b, _)| held(*b)).fold(0, |m, (_, bit)| m | bit)
}

/// A gamepad's force feedback: two endless effects whose gains follow the game's motors.
struct Rumble {
    id: gilrs::GamepadId,
    strong: gilrs::ff::Effect,
    weak: gilrs::ff::Effect,
    level: [f32; 2],
}

impl Rumble {
    fn effect(g: &mut gilrs::Gilrs, id: gilrs::GamepadId, strong: bool) -> Result<gilrs::ff::Effect, gilrs::ff::Error> {
        use gilrs::ff::{BaseEffect, BaseEffectType, EffectBuilder, Replay, Ticks};
        let kind = if strong { BaseEffectType::Strong { magnitude: u16::MAX } } else { BaseEffectType::Weak { magnitude: u16::MAX } };
        let effect = EffectBuilder::new()
            .add_effect(BaseEffect { kind, scheduling: Replay { play_for: Ticks::from_ms(1000), ..Default::default() }, envelope: Default::default() })
            .gamepads(&[id])
            .gain(0.0)
            .finish(g)?;
        effect.play()?;
        Ok(effect)
    }
}

/// A gamepad's sticks as a DualShock's in analog mode (RX, RY, LX, LY; 0x80 centred, 0 left
/// and up).
fn ps1_sticks(pad: &gilrs::Gamepad) -> [u8; 4] {
    use gilrs::Axis;
    let byte = |v: f32| ((v.clamp(-1.0, 1.0) + 1.0) * 127.5).round() as u8;
    [byte(pad.value(Axis::RightStickX)), byte(-pad.value(Axis::RightStickY)), byte(pad.value(Axis::LeftStickX)), byte(-pad.value(Axis::LeftStickY))]
}

/// A race frame: the world and the original's 2D, one view or split screen (each player's
/// view in its band of the screen, then the HUD over both). Returns the picture when `shot`.
pub fn draw_race(r: &mut Renderer, game: &mut Game, w: f32, h: f32, shot: bool) -> Option<image::RgbaImage> {
    if game.view_count() < 2 {
        let params = game.frame_params(w / h.max(1.0));
        let dynamic = game.dynamic_geometry();
        let hud = race_hud(r, game, w, h);
        if shot {
            return Some(r.screenshot(&params, &game.track.sky, &dynamic, &hud));
        }
        r.render(&params, &game.track.sky, &dynamic, &hud);
        return None;
    }
    let mut atlas = Atlas::new(1024);
    let mut views = Vec::new();
    for k in 0..game.view_count() {
        game.set_view(k);
        let vp = game.view_viewport(k, w, h);
        let params = game.frame_params(vp[2] / vp[3].max(1.0));
        let dynamic = game.dynamic_geometry();
        let (over, sky_layer, glows, glows_sub) = match game.view_2d(atlas, w, h, vp) {
            Some(v) => v,
            None => {
                let empty = crate::game::fe::FeFrame { atlas: Atlas::new(1024), vertices: Vec::new(), subtract: Vec::new(), under: 0, floor: 0, more_cars: Vec::new(), first_car: None };
                (empty, Vec::new(), Vec::new(), Vec::new())
            }
        };
        atlas = over.atlas;
        views.push(crate::render::ViewDraw { viewport: vp, params, dynamic, sky_layer, glows, glows_sub, over: over.vertices, over_subtract: over.subtract });
    }
    game.set_view(0);
    let hud = match game.hud_2d(atlas, w, h) {
        Some(f) if !game.paused() => {
            r.write_overlay_atlas(&f.atlas);
            r.use_overlay_atlas = true;
            r.hud_subtract = f.subtract;
            f.vertices
        }
        Some(f) => {
            // paused: the host's pause menu over the kept picture
            r.write_overlay_atlas(&f.atlas);
            r.use_overlay_atlas = false;
            r.hud_subtract = Vec::new();
            game.hud(w, h)
        }
        None => {
            r.use_overlay_atlas = false;
            r.hud_subtract = Vec::new();
            game.hud(w, h)
        }
    };
    if shot {
        return Some(r.screenshot_views(&views, &game.track.sky, &hud));
    }
    r.render_views(&views, &game.track.sky, &hud);
    None
}

/// A race's 2D: the HUD the original drew (recorded, with its own atlas), or the host's port.
pub fn race_hud(r: &mut Renderer, game: &Game, w: f32, h: f32) -> Vec<crate::assets::Vertex> {
    // the sun as the renderer last saw it, for the original's lens flare (its 25 points
    // tested a frame or two behind, like the PS1 reading back its last frame)
    if let Some(bits) = r.sun_visible() {
        if std::env::var_os("NFSHS_SUNLOG").is_some() {
            eprintln!("sun seen: {bits:025b}");
        }
        crate::ps1::host::SUN_VISIBLE.store(bits, std::sync::atomic::Ordering::Relaxed);
    }
    let hud = race_hud_2d(r, game, w, h);
    r.sun_probe = game.sun_probe();
    if std::env::var_os("NFSHS_SUNLOG").is_some() {
        eprintln!("sun probe {:?} ({w}x{h})", r.sun_probe);
    }
    hud
}

fn race_hud_2d(r: &mut Renderer, game: &Game, w: f32, h: f32) -> Vec<crate::assets::Vertex> {
    match game.hud_frame(w, h) {
        Some((f, sky, glows, glows_sub)) if !game.paused() => {
            r.write_overlay_atlas(&f.atlas);
            r.use_overlay_atlas = true;
            r.hud_subtract = f.subtract;
            r.sky_layer = sky;
            r.glows = glows;
            r.glows_sub = glows_sub;
            f.vertices
        }
        Some((f, sky, glows, glows_sub)) => {
            r.glows = glows;
            r.glows_sub = glows_sub;
            // paused: the host's pause menu over the kept picture, the sky layer as recorded
            r.write_overlay_atlas(&f.atlas);
            r.use_overlay_atlas = false;
            r.hud_subtract = Vec::new();
            r.sky_layer = sky;
            game.hud(w, h)
        }
        None => {
            r.use_overlay_atlas = false;
            r.hud_subtract = Vec::new();
            r.sky_layer = Vec::new();
            r.glows = Vec::new();
            r.glows_sub = Vec::new();
            game.hud(w, h)
        }
    }
}

/// Draw a front-end scene (2D, and the showroom car in 3D between its layers).
pub fn draw_front_end(r: &mut Renderer, scene: crate::game::fe::FeScene, shot: bool) -> Option<image::RgbaImage> {
    r.sun_probe = None;
    r.write_atlas(&scene.atlas);
    r.use_overlay_atlas = false;
    r.sky_layer = Vec::new();
    r.glows = Vec::new();
    r.glows_sub = Vec::new();
    let mut params = flat_params();
    if let Some(vp) = scene.view_proj {
        params.view_proj = vp;
    }
    // no fog in the showroom
    params.fog_start = 1.0e9;
    params.fog_end = 2.0e9;
    r.hud_subtract = scene.subtract;
    r.hud_under = scene.under;
    r.hud_under_subtract = scene.under_subtract;
    r.hud_mid = scene.mid;
    r.hud_mid_subtract = scene.mid_subtract;
    r.dyn_mid = scene.car_split;
    let img = if !scene.more.is_empty() {
        // more cars in views of their own (the split car selects): a layer each
        let showroom_params = |view_proj: Mat4| {
            let mut p = flat_params();
            p.view_proj = view_proj;
            p.fog_start = 1.0e9;
            p.fog_end = 2.0e9;
            p
        };
        let mut layers = vec![crate::render::Layer { params, dynamic: scene.car, dyn_mid: scene.car_split, over: scene.over, over_subtract: r.hud_subtract.clone() }];
        for c in scene.more {
            let p = showroom_params(c.view_proj);
            layers.push(crate::render::Layer { params: p, dynamic: c.car, dyn_mid: c.car_split, over: c.over, over_subtract: c.subtract });
        }
        if shot {
            Some(r.screenshot_layers(&layers))
        } else {
            r.render_layers(&layers);
            None
        }
    } else if shot {
        Some(r.screenshot(&params, &[], &scene.car, &scene.over))
    } else {
        r.render(&params, &[], &scene.car, &scene.over);
        None
    };
    r.hud_subtract = Vec::new();
    r.hud_under = Vec::new();
    r.hud_under_subtract = Vec::new();
    r.hud_mid = Vec::new();
    r.hud_mid_subtract = Vec::new();
    r.dyn_mid = 0;
    img
}
