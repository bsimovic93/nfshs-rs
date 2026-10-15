//! Window + event loop (winit 0.30 ApplicationHandler): the original's front end and races.

use crate::assets::{Atlas, GameData};
use crate::game::{Game, Input, RaceConfig};
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
    /// The game cannot go on: the app closes and [`run`] returns `e`.
    fn fail(&mut self, e: anyhow::Error) {
        log::error!("{e:#}");
        self.error = Some(e);
        self.screen = Screen::Off;
    }

    /// Straight into a race (the command line's `play`).
    fn start_race(&mut self, cfg: RaceConfigOwned) {
        let mut atlas = Atlas::new(2048);
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
        let mut atlas = Atlas::new(2048);
        match Game::load_with(&self.data, &mut atlas, &rc, *sim) {
            Ok(game) => self.begin_race(game, atlas),
            Err(e) => self.fail(e.context("could not load the race")),
        }
    }

    /// Show a loaded race.
    fn begin_race(&mut self, game: Game, atlas: Atlas) {
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
        if let Some(g) = &mut self.gilrs {
            while g.next_event().is_some() {}
            if let Some((_, pad)) = g.gamepads().next() {
                use gilrs::Button;
                // (the stick goes to the original as a stick: it reads it as directions too)
                i.up |= pad.is_pressed(Button::DPadUp);
                i.down |= pad.is_pressed(Button::DPadDown);
                i.left |= pad.is_pressed(Button::DPadLeft);
                i.right |= pad.is_pressed(Button::DPadRight);
                i.accept |= pad.is_pressed(Button::South);
                i.back |= pad.is_pressed(Button::North);
                i.circle |= pad.is_pressed(Button::East);
                i.square |= pad.is_pressed(Button::West);
                i.start |= pad.is_pressed(Button::Start);
                i.select |= pad.is_pressed(Button::Select);
                i.sticks = Some(ps1_sticks(&pad));
            }
            i.pad2 = g.gamepads().nth(1).map_or(0, |(_, p)| ps1_buttons(&p));
            i.pad2_sticks = g.gamepads().nth(1).map(|(_, p)| ps1_sticks(&p));
        }
        i
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
                // Start pauses; the pause menu takes the d-pad and Cross
                i.start |= pad.is_pressed(Button::Start);
                i.up |= pad.is_pressed(Button::DPadUp);
                i.down |= pad.is_pressed(Button::DPadDown);
                i.accept |= pad.is_pressed(Button::South);
                i.sticks = Some(ps1_sticks(&pad));
            }
            i.pad2 = g.gamepads().nth(1).map_or(0, |(_, p)| ps1_buttons(&p));
            i.pad2_sticks = g.gamepads().nth(1).map(|(_, p)| ps1_sticks(&p));
        }
        i
    }

    /// The game's DualShock motors on the gamepads: controller 1 the first gamepad, 2 the
    /// second (as the input), the big motor (0..255) the strong one and the small one (on or
    /// off) the weak one. NFSHS_NO_RUMBLE=1: none.
    fn update_rumble(&mut self) {
        let state = match &self.screen {
            Screen::FrontEnd(sim) => sim.frame_snapshot().rumble,
            Screen::Race(game) => game.sim.frame_snapshot().rumble,
            Screen::Off => [[0; 2]; 2],
        };
        let Some(g) = &mut self.gilrs else { return };
        let pads: Vec<gilrs::GamepadId> = g.gamepads().map(|(id, _)| id).take(2).collect();
        self.rumble.retain(|r| pads.contains(&r.id));
        if std::env::var_os("NFSHS_NO_RUMBLE").is_some() {
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
        direct,
        movie: None,
        movie_voice: None,
        showroom: None,
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
        world_ranges: None,
        shadows: Vec::new(),
        subtract_late: Vec::new(),
        sky_view_proj: Mat4::IDENTITY,
        sky_dome: 0,
        sky_top: Vec3::ZERO,
        view_proj: Mat4::IDENTITY,
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

/// A gamepad as a PS1 pad's buttons (the second player's): face buttons as they sit, the
/// triggers also Cross and Square (accelerate and brake in a race). The sticks are the
/// DualShock's (see [`ps1_sticks`]).
fn ps1_buttons(pad: &gilrs::Gamepad) -> u16 {
    use crate::ps1::race::pad;
    use gilrs::Button;
    let trigger = |b: Button| pad.button_data(b).is_some_and(|d| d.value() > 0.3);
    let mut b = 0;
    for (on, bit) in [
        (pad.is_pressed(Button::DPadUp), pad::UP),
        (pad.is_pressed(Button::DPadDown), pad::DOWN),
        (pad.is_pressed(Button::DPadLeft), pad::LEFT),
        (pad.is_pressed(Button::DPadRight), pad::RIGHT),
        (pad.is_pressed(Button::South) || trigger(Button::RightTrigger2), pad::CROSS),
        (pad.is_pressed(Button::West) || trigger(Button::LeftTrigger2), pad::SQUARE),
        (pad.is_pressed(Button::East), pad::CIRCLE),
        (pad.is_pressed(Button::North), pad::TRIANGLE),
        (pad.is_pressed(Button::Start), pad::START),
        (pad.is_pressed(Button::Select), pad::SELECT),
        (pad.is_pressed(Button::LeftTrigger), pad::L1),
        (pad.is_pressed(Button::RightTrigger), pad::R1),
    ] {
        if on {
            b |= bit;
        }
    }
    b
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
