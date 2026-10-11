//! Window + event loop (winit 0.30 ApplicationHandler).

use crate::assets::{Atlas, GameData};
use crate::game::{Game, Input};
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
    atlas: Option<Atlas>,
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    input: Input,
    last: Instant,
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
                if let Some(r) = &mut self.renderer {
                    let (w, h) = r.size;
                    let params = self.game.frame_params(w as f32 / h.max(1) as f32);
                    let dynamic = self.game.dynamic_geometry();
                    let hud = self.game.hud(w as f32, h as f32);
                    r.render(&params, &dynamic, &hud);
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}

pub fn run(data: &GameData, track: u32, variant: &str, car: &str) -> anyhow::Result<()> {
    let mut atlas = Atlas::new(2048);
    let game = Game::load(data, &mut atlas, track, variant, car)?;
    let el = EventLoop::new()?;
    let mut app = App { game, atlas: Some(atlas), renderer: None, window: None, input: Input::default(), last: Instant::now() };
    el.run_app(&mut app)?;
    Ok(())
}

/// Headless: simulate `seconds` with a scripted input and save a screenshot.
pub fn screenshot(data: &GameData, track: u32, variant: &str, car: &str, out: &std::path::Path, seconds: f32, input: Input) -> anyhow::Result<()> {
    let mut atlas = Atlas::new(2048);
    let mut game = Game::load(data, &mut atlas, track, variant, car)?;
    let mut r = pollster::block_on(Renderer::new(None, (960, 720), &atlas))?;
    r.set_world(&game.track.verts, &game.track.indices);
    let mut t = 0.0;
    let mut next_log = 10.0;
    while t < seconds {
        game.update(1.0 / 60.0, input);
        t += 1.0 / 60.0;
        if t >= next_log {
            next_log += 10.0;
            println!("  t={t:5.1} lap={} progress={}/{} speed={:.0} km/h", game.lap, game.progress, game.path.len(), game.car.speed() * 3.6);
        }
    }
    println!(
        "after {seconds}s: pos={:?} speed={:.1} km/h gear={} progress={}/{} on_ground={}",
        game.car.pos,
        game.car.speed() * 3.6,
        game.car.gear + 1,
        game.progress,
        game.path.len(),
        game.car.on_ground
    );
    let params = game.frame_params(960.0 / 720.0);
    let img = r.screenshot(&params, &game.dynamic_geometry(), &game.hud(960.0, 720.0));
    img.save(out)?;
    Ok(())
}
