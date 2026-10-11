//! Front-end: race setup menu (track, conditions, car, opponents, laps).

use crate::assets::{Atlas, GameData, Rect, Vertex};
use crate::formats::{psh::Psh, viv::Viv};
use crate::game::hud::Font;

/// Original front-end artwork from ZZZFE.VIV packed into the menu atlas.
#[derive(Default)]
pub struct MenuArt {
    pub title: Option<Rect>,
    pub tracks: Vec<Option<Rect>>,
    pub cars: Vec<Option<Rect>>,
    pub atlas_size: f32,
}

/// Composite a tiled front-end image: every tile named like the first one is placed at
/// (-center.x, -center.y).
fn composite(psh: &Psh) -> Option<(u32, u32, Vec<u8>)> {
    let first = psh.shapes.first()?.name.clone();
    let tiles: Vec<_> = psh.shapes.iter().filter(|s| s.name == first && s.width > 1).collect();
    let w = tiles.iter().map(|t| (-(t.center.0 as i32)) as u32 + t.width as u32).max()?;
    let h = tiles.iter().map(|t| (-(t.center.1 as i32)) as u32 + t.height as u32).max()?;
    let (w, h) = (w.min(1024), h.min(512));
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for t in tiles {
        let (ox, oy) = ((-(t.center.0 as i32)) as u32, (-(t.center.1 as i32)) as u32);
        for y in 0..t.height as u32 {
            for x in 0..t.width as u32 {
                let (dx, dy) = (ox + x, oy + y);
                if dx < w && dy < h {
                    let si = ((y * t.width as u32 + x) * 4) as usize;
                    let di = ((dy * w + dx) * 4) as usize;
                    rgba[di..di + 4].copy_from_slice(&t.rgba[si..si + 4]);
                    rgba[di + 3] = 255;
                }
            }
        }
    }
    Some((w, h, rgba))
}

impl MenuArt {
    pub fn load(data: &GameData, atlas: &mut Atlas) -> Self {
        let mut art = MenuArt { atlas_size: atlas.size as f32, ..Default::default() };
        let Ok(viv) = data.read("ZZZFE.VIV").and_then(|d| Viv::parse(&d)) else { return art };
        let mut add = |name: &str| -> Option<Rect> {
            let psh = Psh::parse(viv.get(name)?).ok()?;
            let (w, h, rgba) = composite(&psh)?;
            Some(atlas.add(w, h, &rgba))
        };
        art.title = add("ztitle.psh");
        art.tracks = TRACKS.iter().map(|(n, _)| add(&format!("zyTR{n:02}.psh"))).collect();
        art.cars = CARS.iter().map(|(c, _)| add(&format!("zz{c}1.psh"))).collect();
        art
    }

    fn quad(&self, out: &mut Vec<Vertex>, r: Rect, x: f32, y: f32, w: f32, h: f32, crop_v: f32) {
        let a = self.atlas_size;
        let (u0, v0, u1, v1) = (r.x as f32 / a, r.y as f32 / a, (r.x + r.w) as f32 / a, (r.y as f32 + r.h as f32 * crop_v) / a);
        let alpha = 1.0;
        let c = [0.5, 0.5, 0.5, alpha];
        let q = [([x, y], [u0, v0]), ([x + w, y], [u1, v0]), ([x + w, y + h], [u1, v1]), ([x, y + h], [u0, v1])];
        for k in [0, 1, 2, 0, 2, 3] {
            out.push(Vertex { pos: [q[k].0[0], q[k].0[1], 0.0], uv: q[k].1, color: c });
        }
    }
}

/// Tracks present on the NTSC-U disc (Millennia, track 01, is not included).
pub const TRACKS: &[(u32, &str)] = &[
    (0, "SNOWY RIDGE"),
    (2, "DOLPHIN COVE"),
    (3, "ROUTE ADONF"),
    (4, "KINDIAK PARK"),
    (5, "CELTIC RUINS"),
    (6, "LANDSTRASSE"),
    (7, "DURHAM ROAD"),
    (8, "RACEWAY 2"),
    (9, "RACEWAY"),
    (10, "RACEWAY 3"),
];

/// Player-selectable cars: (file code, name from ZTEXT.ENG).
pub const CARS: &[(&str, &str)] = &[
    ("MSLK", "MERCEDES SLK 230"),
    ("BMWZ", "BMW Z3 ROADSTER 2.8"),
    ("CMRO", "CAMARO Z28"),
    ("TRAM", "FIREBIRD T/A"),
    ("ADB7", "ASTON MARTIN DB7"),
    ("JXKR", "JAGUAR XKR"),
    ("BMWM", "BMW M5"),
    ("CORV", "CORVETTE"),
    ("F550", "FERRARI 550 MARANELLO"),
    ("P993", "PORSCHE 911 TURBO"),
    ("F375", "FERRARI F50"),
    ("DIAB", "LAMBORGHINI DIABLO SV"),
    ("MCLK", "MERCEDES CLK-GTR"),
    ("MCF1", "MCLAREN F1 GTR"),
    ("BNS1", "PHANTOM"),
    ("BNS2", "TITAN"),
];

pub const CONDITIONS: &[(&str, &str)] = &[("", "DAY"), ("N", "NIGHT"), ("W", "WET"), ("S", "SNOW")];

#[derive(Clone, Copy, Default)]
pub struct MenuInput {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub accept: bool,
    pub back: bool,
}

pub enum MenuAction {
    None,
    Start,
    Quit,
}

pub struct Menu {
    pub row: usize,
    pub track: usize,
    pub condition: usize,
    pub car: usize,
    pub opponents: usize,
    pub laps: usize,
    prev: MenuInput,
}

const ROWS: usize = 7;

impl Default for Menu {
    fn default() -> Self {
        Self { row: 5, track: 0, condition: 0, car: 6, opponents: 5, laps: 2, prev: MenuInput::default() }
    }
}

impl Menu {
    pub fn update(&mut self, i: MenuInput) -> MenuAction {
        let edge = |now: bool, before: bool| now && !before;
        let p = self.prev;
        self.prev = i;
        if edge(i.up, p.up) {
            self.row = (self.row + ROWS - 1) % ROWS;
        }
        if edge(i.down, p.down) {
            self.row = (self.row + 1) % ROWS;
        }
        let delta = edge(i.right, p.right) as i32 - edge(i.left, p.left) as i32;
        let step = |v: &mut usize, n: usize| *v = ((*v as i32 + delta).rem_euclid(n as i32)) as usize;
        if delta != 0 {
            match self.row {
                0 => step(&mut self.track, TRACKS.len()),
                1 => step(&mut self.condition, CONDITIONS.len()),
                2 => step(&mut self.car, CARS.len()),
                3 => step(&mut self.opponents, 8),
                4 => {
                    step(&mut self.laps, 10);
                    self.laps = self.laps.max(1);
                }
                _ => {}
            }
        }
        if edge(i.back, p.back) {
            return MenuAction::Quit;
        }
        if edge(i.accept, p.accept) {
            return match self.row {
                6 => MenuAction::Quit,
                _ => MenuAction::Start,
            };
        }
        MenuAction::None
    }

    /// Opponent car codes: the next cars in the list after the player's choice.
    pub fn opponent_codes(&self) -> Vec<String> {
        (1..=self.opponents).map(|k| CARS[(self.car + k * 3) % CARS.len()].0.to_string()).collect()
    }

    pub fn draw(&self, font: &Font, art: &MenuArt, w: f32, h: f32) -> Vec<Vertex> {
        let mut out = Vec::new();
        let s = (h / 240.0).max(1.0).floor();
        if let Some(t) = art.title {
            // the title screen is 512x256 at PS1 hi-res; stretch to the window (top 60%)
            art.quad(&mut out, t, 0.0, 0.0, w, h * 0.45, 0.72);
        }
        let banner_w = (w * 0.42).min(400.0 * s / 2.0 * 1.6);
        if let Some(Some(r)) = art.tracks.get(self.track) {
            let bh = banner_w * r.h as f32 / r.w as f32;
            art.quad(&mut out, *r, w - banner_w - 12.0 * s, h * 0.47, banner_w, bh, 1.0);
        }
        if let Some(Some(r)) = art.cars.get(self.car) {
            let bh = banner_w * r.h as f32 / r.w as f32;
            art.quad(&mut out, *r, w - banner_w - 12.0 * s, h * 0.47 + banner_w * 0.25 + 8.0 * s, banner_w, bh, 1.0);
        }
        let yellow = [1.0, 0.85, 0.2, 1.0];
        let white = [1.0, 1.0, 1.0, 1.0];
        let grey = [0.6, 0.6, 0.65, 1.0];
        let centred = |out: &mut Vec<Vertex>, t: &str, y: f32, sc: f32, c: [f32; 4]| {
            font.text(out, t, w * 0.5 - t.len() as f32 * 3.0 * sc, y, sc, c)
        };
        if art.title.is_none() {
            centred(&mut out, "NEED FOR SPEED", 20.0 * s, s * 3.0, yellow);
            centred(&mut out, "HIGH STAKES", 46.0 * s, s * 3.0, white);
        }
        let rows = [
            ("TRACK", TRACKS[self.track].1.to_string()),
            ("CONDITIONS", CONDITIONS[self.condition].1.to_string()),
            ("CAR", CARS[self.car].1.to_string()),
            ("OPPONENTS", format!("{}", self.opponents)),
            ("LAPS", format!("{}", self.laps)),
            ("START RACE", String::new()),
            ("QUIT", String::new()),
        ];
        let x = 24.0 * s;
        for (k, (label, value)) in rows.iter().enumerate() {
            let y = h * 0.48 + k as f32 * 15.0 * s;
            let col = if k == self.row { yellow } else { white };
            if k == self.row {
                font.text(&mut out, "-", x - 12.0 * s, y, s * 1.5, yellow);
            }
            font.text(&mut out, label, x, y, s * 1.2, col);
            if !value.is_empty() {
                font.text(&mut out, value, x + 92.0 * s, y, s * 1.2, if k == self.row { yellow } else { grey });
            }
        }
        centred(&mut out, "ARROWS/DPAD: SELECT  ENTER/A: GO  ESC: QUIT", h - 20.0 * s, s, grey);
        out
    }
}
