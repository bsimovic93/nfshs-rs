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
    /// Start straight into a race (command-line `play` with explicit options).
    direct: Option<RaceConfigOwned>,
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
        let rc = RaceConfig { track: cfg.track, variant: &cfg.variant, player_car: &cfg.player_car, opponents: cfg.opponents.clone(), laps: cfg.laps };
        let game = match Game::load(&self.data, &mut atlas, &rc) {
            Ok(g) => g,
            Err(e) => {
                log::error!("could not load race: {e:#}");
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
        match &self.screen {
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
                    let finished = game.phase == Phase::Finished;
                    if keys.escape || (finished && keys.enter) {
                        self.keys.escape = false;
                        self.keys.enter = false;
                        self.enter_menu();
                    }
                }
            }
        }
        self.update_audio(dt);
        let Some(r) = &mut self.renderer else { return };
        let (w, h) = r.size;
        match &self.screen {
            Screen::Menu => {
                let hud = self.menu_font.as_ref().map(|f| self.menu.draw(f, &self.menu_art, w as f32, h as f32)).unwrap_or_default();
                let params = FrameParams {
                    ambient: 1.0,
                    headlight: (0.0, Vec3::ZERO, Vec3::Z),
                    sky_view_proj: Mat4::IDENTITY,
                    sky_top: Vec3::new(0.0, 0.01, 0.04),
                    view_proj: Mat4::IDENTITY,
                    cam_pos: Vec3::ZERO,
                    fog_color: Vec3::new(0.0, 0.0, 0.01),
                    fog_start: 0.0,
                    fog_end: 1.0,
                };
                r.render(&params, &[], &[], &hud);
            }
            Screen::Race(game) => {
                let params = game.frame_params(w as f32 / h.max(1) as f32);
                let dynamic = game.dynamic_geometry();
                let mut hud = game.hud(w as f32, h as f32);
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
            None => self.enter_menu(),
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
        direct,
    };
    el.run_app(&mut app)?;
    Ok(())
}

/// Headless: simulate `seconds` with a scripted input and save a screenshot.
pub fn screenshot(data: &GameData, cfg: &RaceConfig, out: &std::path::Path, seconds: f32, input: Input) -> anyhow::Result<()> {
    let mut atlas = Atlas::new(2048);
    let mut game = Game::load(data, &mut atlas, cfg)?;
    let mut r = pollster::block_on(Renderer::new(None, (960, 720), &atlas))?;
    r.set_world(&game.track.verts, &game.track.indices);
    let mut t = 0.0;
    let mut next_log = 10.0;
    while t < seconds {
        game.update(1.0 / 60.0, input);
        t += 1.0 / 60.0;
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
    let params = game.frame_params(960.0 / 720.0);
    let img = r.screenshot(&params, &game.track.sky, &game.dynamic_geometry(), &game.hud(960.0, 720.0));
    img.save(out)?;
    Ok(())
}

/// Headless render of the front-end menu.
pub fn menu_screenshot(data: &GameData, out: &std::path::Path) -> anyhow::Result<()> {
    let mut atlas = Atlas::new(2048);
    let font = crate::game::hud::Font::new(&mut atlas);
    let art = crate::menu::MenuArt::load(data, &mut atlas);
    let mut r = pollster::block_on(Renderer::new(None, (960, 720), &atlas))?;
    let menu = Menu::default();
    let params = FrameParams {
        ambient: 1.0,
        headlight: (0.0, Vec3::ZERO, Vec3::Z),
        sky_view_proj: Mat4::IDENTITY,
        sky_top: Vec3::new(0.0, 0.01, 0.04),
        view_proj: Mat4::IDENTITY,
        cam_pos: Vec3::ZERO,
        fog_color: Vec3::new(0.0, 0.0, 0.01),
        fog_start: 0.0,
        fog_end: 1.0,
    };
    r.screenshot(&params, &[], &[], &menu.draw(&font, &art, 960.0, 720.0)).save(out)?;
    Ok(())
}
