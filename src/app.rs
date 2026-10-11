//! Window + event loop (winit 0.30 ApplicationHandler).

use crate::assets::{Atlas, GameData};
use crate::game::{Game, Input, RaceConfig};
use crate::render::Renderer;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

pub struct App {
    game: Game,
    data: GameData,
    audio: Option<crate::audio::Audio>,
    engine: Option<crate::audio::EngineSound>,
    music_rx: Option<std::sync::mpsc::Receiver<crate::formats::audio::Sample>>,
    atlas: Option<Atlas>,
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    input: Input,
    last: Instant,
}

impl App {
    fn start_audio(&mut self) {
        let audio = match crate::audio::Audio::new() {
            Ok(a) => a,
            Err(e) => {
                log::warn!("audio disabled: {e}");
                return;
            }
        };
        // engine bank of the player's car: Z<code>ENG.VIV -> first .bnk
        let code = self.game.player().name.clone();
        let bank = self
            .data
            .read(&format!("Z{code}ENG.VIV"))
            .ok()
            .and_then(|d| crate::formats::viv::Viv::parse(&d).ok())
            .and_then(|v| v.find_suffix(".bnk").and_then(|(_, b)| crate::formats::audio::Bank::parse(b).ok()));
        if let Some(bank) = bank {
            self.engine = crate::audio::EngineSound::new(&audio, &bank);
        }
        // race music: decode one of the in-game tracks in the background
        let root = self.data.root.clone();
        let pick = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) % 18) as usize;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let data = GameData::new(&root);
            let Ok(viv) = data.read("ZZZZYMUS.VIV").and_then(|d| crate::formats::viv::Viv::parse(&d)) else { return };
            let tracks: Vec<_> = viv.entries.iter().filter(|(n, _)| n.starts_with("game")).collect();
            if let Some((name, d)) = tracks.get(pick % tracks.len().max(1)) {
                if let Ok(s) = crate::formats::audio::decode_stream(d) {
                    log::info!("music: {name}");
                    let _ = tx.send(s);
                }
            }
        });
        self.music_rx = Some(rx);
        self.audio = Some(audio);
    }

    fn update_audio(&mut self, dt: f32) {
        let Some(audio) = &self.audio else { return };
        if let Some(rx) = &self.music_rx {
            if let Ok(s) = rx.try_recv() {
                audio.play(std::sync::Arc::new(s), 0.45, 1.0, true);
                self.music_rx = None;
            }
        }
        if let Some(engine) = &mut self.engine {
            let p = self.game.player();
            engine.update(audio, p.car.rpm / p.car.spec.redline, p.throttle, 0.55, dt);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Need for Speed: High Stakes — Rust rebuild")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 960));
        let window = Arc::new(el.create_window(attrs).expect("window"));
        let size = window.inner_size();
        let atlas = self.atlas.take().expect("atlas");
        let mut r = pollster::block_on(Renderer::new(Some(window.clone()), (size.width, size.height), &atlas)).expect("renderer");
        r.set_world(&self.game.track.verts, &self.game.track.indices);
        self.renderer = Some(r);
        self.window = Some(window);
        self.start_audio();
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
            WindowEvent::KeyboardInput { event: KeyEvent { physical_key: PhysicalKey::Code(code), state, .. }, .. } => {
                let down = state == ElementState::Pressed;
                match code {
                    KeyCode::ArrowUp | KeyCode::KeyW => self.input.up = down,
                    KeyCode::ArrowDown | KeyCode::KeyS => self.input.down = down,
                    KeyCode::ArrowLeft | KeyCode::KeyA => self.input.left = down,
                    KeyCode::ArrowRight | KeyCode::KeyD => self.input.right = down,
                    KeyCode::Space => self.input.handbrake = down,
                    KeyCode::KeyR => self.input.reset = down,
                    KeyCode::KeyC => self.input.camera = down,
                    KeyCode::KeyP if down => self.input.autopilot = !self.input.autopilot,
                    KeyCode::Escape => el.exit(),
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last).as_secs_f32();
                self.last = now;
                self.game.update(dt, self.input);
                self.update_audio(dt);
                if let Some(r) = &mut self.renderer {
                    let (w, h) = r.size;
                    let params = self.game.frame_params(w as f32 / h.max(1) as f32);
                    let dynamic = self.game.dynamic_geometry();
                    let hud = self.game.hud(w as f32, h as f32);
                    r.render(&params, &self.game.track.sky, &dynamic, &hud);
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}

pub fn run(data: &GameData, cfg: &RaceConfig) -> anyhow::Result<()> {
    let mut atlas = Atlas::new(2048);
    let game = Game::load(data, &mut atlas, cfg)?;
    let el = EventLoop::new()?;
    let mut app = App {
        game,
        data: GameData::new(&data.root),
        audio: None,
        engine: None,
        music_rx: None,
        atlas: Some(atlas),
        renderer: None,
        window: None,
        input: Input::default(),
        last: Instant::now(),
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
