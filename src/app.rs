//! Window + event loop (winit 0.30 ApplicationHandler): front-end menu and races.

use crate::assets::{Atlas, GameData};
use crate::game::{Game, Input, Phase, RaceConfig};
use crate::menu::{CONDITIONS, Menu, MenuAction, MenuInput, TRACKS};
use crate::render::{FrameParams, Renderer};
use glam::{Mat4, Vec3};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

enum Screen {
    Menu,
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
    reset: bool,
    camera: bool,
    backspace: bool,
    tab: bool,
    start: bool,
}

pub struct App {
    data: GameData,
    screen: Screen,
    menu: Menu,
    menu_font: Option<crate::game::hud::Font>,
    menu_art: crate::menu::MenuArt,
    audio: Option<crate::audio::Audio>,
    engine: Option<crate::audio::EngineSound>,
    music: Option<crate::audio::VoiceId>,
    music_rx: Option<std::sync::mpsc::Receiver<crate::formats::audio::Sample>>,
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    keys: Keys,
    autopilot: bool,
    gilrs: Option<gilrs::Gilrs>,
    last: Instant,
    fps: (u32, Instant),
    /// Start straight into a race (command-line `play` with explicit options).
    direct: Option<RaceConfigOwned>,
    /// The front end's movie being shown, and its soundtrack's voice.
    movie: Option<crate::game::fe::MoviePlayer>,
    movie_voice: Option<crate::audio::VoiceId>,
    /// The front end's showroom car model.
    showroom: Option<crate::game::fe::Showroom>,
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
    fn start_race(&mut self, cfg: RaceConfigOwned) {
        let mut atlas = Atlas::new(2048);
        let rc = RaceConfig { track: cfg.track, variant: &cfg.variant, player_car: &cfg.player_car, opponents: cfg.opponents.clone(), laps: cfg.laps, original_sim: true, traffic: Vec::new() };
        let game = match Game::load(&self.data, &mut atlas, &rc) {
            Ok(g) => g,
            Err(e) => {
                log::error!("could not load race: {e:#}");
                self.enter_menu();
                self.menu.error = Some(format!("{e:#}"));
                return;
            }
        };
        if let Some(r) = &mut self.renderer {
            r.set_atlas(&atlas);
            r.set_world(&game.track.verts, &game.track.indices);
        }
        self.stop_sounds();
        if let Some(audio) = &self.audio {
            let code = &game.player().name;
            let bank = self
                .data
                .read(&format!("Z{code}ENG.VIV"))
                .ok()
                .and_then(|d| crate::formats::viv::Viv::parse(&d).ok())
                .and_then(|v| v.find_suffix(".bnk").and_then(|(_, b)| crate::formats::audio::Bank::parse(b).ok()));
            self.engine = bank.and_then(|b| crate::audio::EngineSound::new(audio, &b));
        }
        self.queue_music("game");
        self.screen = Screen::Race(Box::new(game));
    }

    /// Power on the original game: its front end, from which it starts its own races.
    fn enter_front_end(&mut self) {
        match crate::game::original::OriginalSim::start_main(&self.data) {
            Ok(sim) => {
                if let Some(r) = &mut self.renderer {
                    r.clear_world();
                }
                self.stop_sounds();
                self.queue_music("zmenu");
                self.screen = Screen::FrontEnd(Box::new(sim));
            }
            Err(e) => {
                log::error!("the original front end did not start ({e:#}); using the engine's menu");
                self.enter_menu();
            }
        }
    }

    /// The original left its front end for a race: load what it races and show it.
    fn front_end_race(&mut self, sim: Box<crate::game::original::OriginalSim>) {
        let cfg = race_of(sim.snapshot());
        let rc = RaceConfig { track: cfg.track, variant: &cfg.variant, player_car: &cfg.player_car, opponents: cfg.opponents.clone(), laps: cfg.laps, original_sim: true, traffic: Vec::new() };
        log::info!("the original starts a race: track {}{} with {}", cfg.track, cfg.variant, cfg.player_car);
        let mut atlas = Atlas::new(2048);
        match Game::load_with(&self.data, &mut atlas, &rc, Some(*sim)) {
            Ok(game) => self.begin_race(game, atlas),
            Err(e) => {
                log::error!("could not load the race: {e:#}");
                self.enter_menu();
            }
        }
    }

    /// Show a loaded race (assets, sounds).
    fn begin_race(&mut self, game: Game, atlas: Atlas) {
        if let Some(r) = &mut self.renderer {
            r.set_atlas(&atlas);
            r.set_world(&game.track.verts, &game.track.indices);
        }
        self.stop_sounds();
        if let Some(audio) = &self.audio {
            let code = &game.player().name;
            let bank = self
                .data
                .read(&format!("Z{code}ENG.VIV"))
                .ok()
                .and_then(|d| crate::formats::viv::Viv::parse(&d).ok())
                .and_then(|v| v.find_suffix(".bnk").and_then(|(_, b)| crate::formats::audio::Bank::parse(b).ok()));
            self.engine = bank.and_then(|b| crate::audio::EngineSound::new(audio, &b));
        }
        self.queue_music("game");
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
        if let Some(g) = &mut self.gilrs {
            while g.next_event().is_some() {}
            if let Some((_, pad)) = g.gamepads().next() {
                use gilrs::{Axis, Button};
                let (x, y) = (pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY));
                i.up |= pad.is_pressed(Button::DPadUp) || y > 0.6;
                i.down |= pad.is_pressed(Button::DPadDown) || y < -0.6;
                i.left |= pad.is_pressed(Button::DPadLeft) || x < -0.6;
                i.right |= pad.is_pressed(Button::DPadRight) || x > 0.6;
                i.accept |= pad.is_pressed(Button::South);
                i.back |= pad.is_pressed(Button::North);
                i.circle |= pad.is_pressed(Button::East);
                i.square |= pad.is_pressed(Button::West);
                i.start |= pad.is_pressed(Button::Start);
                i.select |= pad.is_pressed(Button::Select);
            }
        }
        i
    }

    fn enter_menu(&mut self) {
        let mut atlas = Atlas::new(2048);
        self.menu_font = Some(crate::game::hud::Font::new(&mut atlas));
        self.menu_art = crate::menu::MenuArt::load(&self.data, &mut atlas);
        if let Some(r) = &mut self.renderer {
            r.set_atlas(&atlas);
            r.clear_world();
        }
        self.stop_sounds();
        self.queue_music("zmenu");
        self.screen = Screen::Menu;
    }

    fn stop_sounds(&mut self) {
        if let Some(audio) = &self.audio {
            if let Some(e) = self.engine.take() {
                e.stop(audio);
            }
            if let Some(m) = self.music.take() {
                audio.stop(m);
            }
        }
        self.music_rx = None;
    }

    /// Decode a random music stream whose name starts with `prefix` (menu: ZZZZAMUS, race: ZZZZYMUS).
    fn queue_music(&mut self, prefix: &'static str) {
        if self.audio.is_none() {
            return;
        }
        let root = self.data.root.clone();
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos() as usize).unwrap_or(0);
        let file = if prefix == "game" { "ZZZZYMUS.VIV" } else { "ZZZZAMUS.VIV" };
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let data = GameData::new(&root);
            let Ok(viv) = data.read(file).and_then(|d| crate::formats::viv::Viv::parse(&d)) else { return };
            let tracks: Vec<_> = viv.entries.iter().filter(|(n, _)| n.starts_with(prefix)).collect();
            if tracks.is_empty() {
                return;
            }
            let (name, d) = tracks[seed % tracks.len()];
            if let Ok(s) = crate::formats::audio::decode_stream(d) {
                log::info!("music: {name}");
                let _ = tx.send(s);
            }
        });
        self.music_rx = Some(rx);
    }

    fn update_audio(&mut self, dt: f32) {
        let Some(audio) = &self.audio else { return };
        if let Some(rx) = &self.music_rx {
            if let Ok(s) = rx.try_recv() {
                self.music = Some(audio.play(Arc::new(s), 0.45, 1.0, true));
                self.music_rx = None;
            }
        }
        if let (Some(engine), Screen::Race(game)) = (&mut self.engine, &self.screen) {
            let p = game.player();
            engine.update(audio, p.car.rpm / p.car.spec.redline, p.throttle, 0.55, dt);
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
            reset: k.reset,
            camera: k.camera,
            autopilot: self.autopilot,
            start: k.escape,
            accept: k.enter,
            ..Default::default()
        };
        if let Some(g) = &mut self.gilrs {
            while g.next_event().is_some() {}
            if let Some((_, pad)) = g.gamepads().next() {
                use gilrs::{Axis, Button};
                let stick = pad.value(Axis::LeftStickX);
                if stick.abs() > 0.12 {
                    i.steer_axis = stick;
                }
                let rt = pad.button_data(Button::RightTrigger2).map(|b| b.value()).unwrap_or(0.0);
                let lt = pad.button_data(Button::LeftTrigger2).map(|b| b.value()).unwrap_or(0.0);
                i.throttle_axis = rt.max(pad.is_pressed(Button::South) as u8 as f32);
                i.brake_axis = lt.max(pad.is_pressed(Button::West) as u8 as f32);
                i.left |= pad.is_pressed(Button::DPadLeft);
                i.right |= pad.is_pressed(Button::DPadRight);
                i.handbrake |= pad.is_pressed(Button::East);
                i.camera |= pad.is_pressed(Button::North);
                i.reset |= pad.is_pressed(Button::Select);
            }
        }
        i
    }

    fn menu_input(&mut self) -> MenuInput {
        let k = self.keys;
        let mut m = MenuInput { up: k.up, down: k.down, left: k.left, right: k.right, accept: k.enter, back: k.escape };
        if let Some(g) = &mut self.gilrs {
            while g.next_event().is_some() {}
            if let Some((_, pad)) = g.gamepads().next() {
                use gilrs::{Axis, Button};
                let (x, y) = (pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY));
                m.up |= pad.is_pressed(Button::DPadUp) || y > 0.6;
                m.down |= pad.is_pressed(Button::DPadDown) || y < -0.6;
                m.left |= pad.is_pressed(Button::DPadLeft) || x < -0.6;
                m.right |= pad.is_pressed(Button::DPadRight) || x > 0.6;
                m.accept |= pad.is_pressed(Button::South) || pad.is_pressed(Button::Start);
                m.back |= pad.is_pressed(Button::East);
            }
        }
        m
    }

    fn frame(&mut self, el: &ActiveEventLoop) {
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
        if let Screen::FrontEnd(_) = &self.screen {
            let input = self.front_end_input();
            if let Screen::FrontEnd(sim) = &mut self.screen {
                sim.advance(dt, &input);
                if sim.stopped() {
                    el.exit();
                    return;
                }
                if !sim.in_front_end() {
                    let Screen::FrontEnd(mut sim) = std::mem::replace(&mut self.screen, Screen::Menu) else { unreachable!() };
                    sim.reset_race();
                    self.front_end_race(sim);
                }
            }
        }
        match &self.screen {
            Screen::FrontEnd(_) => {}
            Screen::Menu => {
                let mi = self.menu_input();
                match self.menu.update(mi) {
                    MenuAction::Quit => el.exit(),
                    MenuAction::Start => {
                        let m = &self.menu;
                        let cfg = RaceConfigOwned {
                            track: TRACKS[m.track].0,
                            variant: CONDITIONS[m.condition].0.to_string(),
                            player_car: crate::menu::CARS[m.car].0.to_string(),
                            opponents: m.opponent_codes(),
                            laps: m.laps as i32,
                        };
                        self.start_race(cfg);
                    }
                    MenuAction::None => {}
                }
            }
            Screen::Race(_) => {
                let input = self.race_input();
                let keys = self.keys;
                if let Screen::Race(game) = &mut self.screen {
                    game.update(dt, input);
                    // a race the original started from its front end: back to it when it is
                    if game.sim.as_ref().is_some_and(|s| s.in_front_end()) {
                        self.keys = Keys::default();
                        let sim = game.sim.take().map(Box::new);
                        if let (Some(sim), Some(r)) = (sim, &mut self.renderer) {
                            r.clear_world();
                            self.stop_sounds();
                            self.queue_music("zmenu");
                            self.screen = Screen::FrontEnd(sim);
                        }
                        return;
                    }
                    let finished = game.phase == Phase::Finished;
                    // with the original running, Escape is its Start button (pause menu) and
                    // the race ends when the original leaves it
                    let quit = match &game.sim {
                        Some(sim) => sim.stopped(),
                        None => keys.escape,
                    };
                    if quit || (finished && keys.enter) {
                        self.keys.escape = false;
                        self.keys.enter = false;
                        self.enter_menu();
                    }
                }
            }
        }
        self.update_audio(dt);
        // a front-end movie has ended: its soundtrack stops and the menu music comes back
        let playing = match &self.screen {
            Screen::FrontEnd(sim) => sim.snapshot().overlay.iter().any(|o| matches!(o, crate::ps1::host::OverlayCmd::Movie { .. })),
            _ => false,
        };
        if !playing && self.movie.take().is_some() {
            if let (Some(audio), Some(v)) = (&self.audio, self.movie_voice.take()) {
                audio.stop(v);
            }
            if self.music.is_none() && self.music_rx.is_none() {
                self.queue_music("zmenu");
            }
        }
        let Some(r) = &mut self.renderer else { return };
        let (w, h) = r.size;
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
                            if let Some(v) = self.music.take() {
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
                    match crate::game::fe::FeScene::car_code(&snap.overlay) {
                        Some(code) if self.showroom.as_ref().is_none_or(|s| s.car != code) => {
                            self.showroom = crate::game::fe::Showroom::load(&self.data, &code);
                        }
                        _ => {}
                    }
                    let scene = crate::game::fe::FeScene::build(&snap.overlay, vram, (snap.display[2], snap.display[3]), w as f32, h as f32, self.showroom.as_ref());
                    draw_front_end(r, scene, false);
                }
            }
            Screen::Menu => {
                let hud = self.menu_font.as_ref().map(|f| self.menu.draw(f, &self.menu_art, w as f32, h as f32)).unwrap_or_default();
                let params = FrameParams {
                    world_ranges: None,
                    shadows: Vec::new(),
                    subtract_late: Vec::new(),
                    ambient: 1.0,
                    headlight: (0.0, Vec3::ZERO, Vec3::Z),
                    sky_view_proj: Mat4::IDENTITY,
                    sky_top: Vec3::new(0.0, 0.01, 0.04),
                    view_proj: Mat4::IDENTITY,
                    cam_pos: Vec3::ZERO,
                    fog_color: Vec3::new(0.0, 0.0, 0.01),
                    fog_start: 0.0,
                    fog_end: 1.0,
                    cam_fwd: Vec3::Z,
                    depth_cue: None,
                };
                r.render(&params, &[], &[], &hud);
            }
            Screen::Race(game) => {
                let params = game.frame_params(w as f32 / h.max(1) as f32);
                let dynamic = game.dynamic_geometry();
                let mut hud = race_hud(r, game, w as f32, h as f32);
                if game.phase == Phase::Finished {
                    let s = (h as f32 / 240.0).max(1.0).floor();
                    game.font().text(&mut hud, "PRESS ENTER", w as f32 * 0.5 - 33.0 * s * 1.5, h as f32 * 0.85, s * 1.5, [1.0, 1.0, 1.0, 1.0]);
                }
                r.render(&params, &game.track.sky, &dynamic, &hud);
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Need for Speed: High Stakes - Rust rebuild")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 960));
        let window = Arc::new(el.create_window(attrs).expect("window"));
        let size = window.inner_size();
        let atlas = Atlas::new(256);
        let r = pollster::block_on(Renderer::new(Some(window.clone()), (size.width, size.height), &atlas)).expect("renderer");
        self.renderer = Some(r);
        self.window = Some(window);
        match crate::audio::Audio::new() {
            Ok(a) => self.audio = Some(a),
            Err(e) => log::warn!("audio disabled: {e}"),
        }
        match self.direct.take() {
            Some(cfg) => self.start_race(cfg),
            None if std::env::var_os("NFSHS_ENGINE_MENU").is_some() => self.enter_menu(),
            None => self.enter_front_end(),
        }
        self.last = Instant::now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
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
                    KeyCode::KeyR => k.reset = down,
                    KeyCode::KeyC => k.camera = down,
                    KeyCode::Backspace => k.backspace = down,
                    KeyCode::Tab => k.tab = down,
                    KeyCode::F1 => k.start = down,
                    KeyCode::KeyP if down && !repeat => self.autopilot = !self.autopilot,
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                self.frame(el);
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}

pub fn run(data: &GameData, direct: Option<RaceConfigOwned>) -> anyhow::Result<()> {
    let el = EventLoop::new()?;
    let mut app = App {
        data: GameData::new(&data.root),
        screen: Screen::Menu,
        menu: Menu::default(),
        menu_font: None,
        menu_art: Default::default(),
        audio: None,
        engine: None,
        music: None,
        music_rx: None,
        renderer: None,
        window: None,
        keys: Keys::default(),
        autopilot: false,
        gilrs: gilrs::Gilrs::new().map_err(|e| log::warn!("gamepad support disabled: {e}")).ok(),
        last: Instant::now(),
        fps: (0, Instant::now()),
        direct,
        movie: None,
        movie_voice: None,
        showroom: None,
    };
    el.run_app(&mut app)?;
    Ok(())
}

/// Headless: simulate `seconds` with a scripted input and save a screenshot.
pub fn screenshot(data: &GameData, cfg: &RaceConfig, out: &std::path::Path, seconds: f32, input: Input, taps: &[(char, f32)]) -> anyhow::Result<()> {
    let mut atlas = Atlas::new(2048);
    let mut game = Game::load(data, &mut atlas, cfg)?;
    let mut r = pollster::block_on(Renderer::new(None, (960, 720), &atlas))?;
    r.set_world(&game.track.verts, &game.track.indices);
    let mut t = 0.0;
    let mut next_log = 10.0;
    let mut flipped = false;
    let trace = std::env::var_os("SHOT_TRACE").is_some();
    while t < seconds {
        let before = game.player().car.pos;
        let mut input = input;
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
        game.update(1.0 / 60.0, input);
        t += 1.0 / 60.0;
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
        if t >= next_log {
            next_log += 10.0;
            let p = game.player();
            let order: Vec<String> = game.standings().iter().map(|&i| format!("{}:{}/{}", game.racers[i].name, game.racers[i].laps_done, game.racers[i].progress)).collect();
            println!("  t={t:5.1} {:?} speed={:.0} km/h  order={}", game.phase, p.car.speed() * 3.6, order.join(" "));
        }
    }
    let p = game.player();
    println!(
        "after {seconds}s: pos={:?} speed={:.1} km/h gear={} progress={}/{} on_ground={}",
        p.car.pos,
        p.car.speed() * 3.6,
        p.car.gear + 1,
        p.progress,
        game.path.len(),
        p.car.on_ground
    );
    // NFSHS_CAM="x,y,z,tx,ty,tz": look from a fixed point (debugging)
    if let Ok(c) = std::env::var("NFSHS_CAM") {
        let v: Vec<f32> = c.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if v.len() == 6 {
            game.cam_pos = glam::Vec3::new(v[0], v[1], v[2]);
            game.cam_target = glam::Vec3::new(v[3], v[4], v[5]);
            game.cam_up = glam::Vec3::Y;
        }
    }
    let params = game.frame_params(960.0 / 720.0);
    let hud = race_hud(&mut r, &game, 960.0, 720.0);
    let img = r.screenshot(&params, &game.track.sky, &game.dynamic_geometry(), &hud);
    img.save(out)?;
    Ok(())
}

/// Frame parameters for drawing 2D only (no world).
pub fn flat_params() -> FrameParams {
    FrameParams {
        world_ranges: None,
        shadows: Vec::new(),
        subtract_late: Vec::new(),
        ambient: 1.0,
        headlight: (0.0, Vec3::ZERO, Vec3::Z),
        sky_view_proj: Mat4::IDENTITY,
        sky_top: Vec3::ZERO,
        view_proj: Mat4::IDENTITY,
        cam_pos: Vec3::ZERO,
        fog_color: Vec3::ZERO,
        fog_start: 0.0,
        fog_end: 1.0,
        cam_fwd: Vec3::Z,
        depth_cue: None,
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

/// Headless: the app's own path through the original game (front end, its race, back), with
/// pad input per stretch of frames (`script`: seconds and an Input) and screenshots of the
/// frames in `shots` written to `dir`.
pub fn front_end_play(data: &GameData, script: &[(f32, Input)], shots: &[u32], dir: &std::path::Path) -> anyhow::Result<()> {
    use crate::game::original::OriginalSim;
    let (w, h) = (960u32, 720u32);
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
                    let rc = RaceConfig { track: cfg.track, variant: &cfg.variant, player_car: &cfg.player_car, opponents: cfg.opponents.clone(), laps: cfg.laps, original_sim: true, traffic: Vec::new() };
                    let mut atlas = Atlas::new(2048);
                    let g = Game::load_with(data, &mut atlas, &rc, Some(*s))?;
                    r.set_atlas(&atlas);
                    r.set_world(&g.track.verts, &g.track.indices);
                    game = Some(Box::new(g));
                }
            } else if let Some(g) = &mut game {
                g.update(1.0 / 60.0, input);
                if std::env::var_os("NFSHS_CARSLOG").is_some() && frame % 60 == 0 {
                    if let Some(sim) = &g.sim {
                        let s = sim.snapshot();
                        let p = s.cars[0].position;
                        let d = |c: &crate::ps1::race::CarState| (((c.position[0] - p[0]) as f64).hypot((c.position[2] - p[2]) as f64) / 65536.0) as i32;
                        println!("frame {frame}: speed {:.0} km/h, cars {:?}", s.cars[0].speed as f32 / 65536.0 * 3.6, s.cars.iter().skip(1).map(|c| format!("{} {}m", c.code, d(c))).collect::<Vec<_>>());
                    }
                }
                if std::env::var_os("NFSHS_PARTSLOG").is_some() && frame % 30 == 0 {
                    if let Some(s) = g.sim.as_ref() {
                        let snap = s.snapshot();
                        for (k, f) in snap.car_facets.iter().enumerate() {
                            let Some(f) = f else { continue };
                            let c = &snap.cars[k];
                            println!("frame {frame}: car {k} {} pos {:?} orient {:?} parts {}", c.code, c.position.map(|v| v >> 16), &c.orient[..3], f.parts.iter().enumerate().filter(|(_, v)| **v != 0).map(|(i, v)| format!("{i}:{v}")).collect::<Vec<_>>().join(" "));
                        }
                    }
                }
                if std::env::var_os("NFSHS_FLIPLOG").is_some() {
                    let c = &g.player().car;
                    if c.up.y < 0.7 {
                        println!("frame {frame}: TIPPED up={:.2} pos={:.1?} speed={:.0} km/h", c.up.y, c.pos, c.speed() * 3.6);
                    }
                }
                if g.sim.as_ref().is_some_and(|s| s.in_front_end()) {
                    println!("frame {frame}: back in the front end");
                    sim = g.sim.take().map(Box::new);
                    game = None;
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
                if let Some(code) = crate::game::fe::FeScene::car_code(&snap.overlay) {
                    if showroom.as_ref().is_none_or(|sr| sr.car != code) {
                        showroom = crate::game::fe::Showroom::load(data, &code);
                    }
                }
                let scene = crate::game::fe::FeScene::build(&snap.overlay, vram, (snap.display[2], snap.display[3]), w as f32, h as f32, showroom.as_ref());
                if let Some(img) = draw_front_end(&mut r, scene, true) {
                    img.save(&out)?;
                }
            } else if let Some(g) = &game {
                let params = g.frame_params(w as f32 / h as f32);
                let hud = race_hud(&mut r, g, w as f32, h as f32);
                r.screenshot(&params, &g.track.sky, &g.dynamic_geometry(), &hud).save(&out)?;
            }
            println!("frame {frame}: {} written", out.display());
        }
    }
    Ok(())
}

/// A race's 2D: the HUD the original drew (recorded, with its own atlas), or the host's port.
pub fn race_hud(r: &mut Renderer, game: &Game, w: f32, h: f32) -> Vec<crate::assets::Vertex> {
    match game.hud_frame(w, h) {
        Some((f, sky, glows)) if !game.paused() => {
            r.write_overlay_atlas(&f.atlas);
            r.use_overlay_atlas = true;
            r.hud_subtract = f.subtract;
            r.sky_layer = sky;
            r.glows = glows;
            f.vertices
        }
        Some((f, sky, glows)) => {
            r.glows = glows;
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
            game.hud(w, h)
        }
    }
}

/// Draw a front-end scene (2D, and the showroom car in 3D between its layers).
pub fn draw_front_end(r: &mut Renderer, scene: crate::game::fe::FeScene, shot: bool) -> Option<image::RgbaImage> {
    r.write_atlas(&scene.atlas);
    r.use_overlay_atlas = false;
    r.sky_layer = Vec::new();
    r.glows = Vec::new();
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
    let img = if shot {
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

/// Headless render of the front-end menu.
pub fn menu_screenshot(data: &GameData, out: &std::path::Path) -> anyhow::Result<()> {
    let mut atlas = Atlas::new(2048);
    let font = crate::game::hud::Font::new(&mut atlas);
    let art = crate::menu::MenuArt::load(data, &mut atlas);
    let mut r = pollster::block_on(Renderer::new(None, (960, 720), &atlas))?;
    let menu = Menu::default();
    let params = FrameParams {
        world_ranges: None,
        shadows: Vec::new(),
        subtract_late: Vec::new(),
        ambient: 1.0,
        headlight: (0.0, Vec3::ZERO, Vec3::Z),
        sky_view_proj: Mat4::IDENTITY,
        sky_top: Vec3::new(0.0, 0.01, 0.04),
        view_proj: Mat4::IDENTITY,
        cam_pos: Vec3::ZERO,
        fog_color: Vec3::new(0.0, 0.0, 0.01),
        fog_start: 0.0,
        fog_end: 1.0,
                    cam_fwd: Vec3::Z,
                    depth_cue: None,
    };
    r.screenshot(&params, &[], &[], &menu.draw(&font, &art, 960.0, 720.0)).save(out)?;
    Ok(())
}
