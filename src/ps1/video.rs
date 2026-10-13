//! The front end's streamed videos (VIDEO_create / spoolfile / startplayback / state /
//! updateframexy / abortplayback / destroy): the video wall plays STR movies whose frames the
//! game has decoded into VRAM, from where it draws them as textures. The host decodes them
//! (formats::str) and writes the frames into its VRAM itself.

use std::collections::HashMap;
use std::sync::Arc;

use crate::formats::str::{Decoder, Movie};

struct Stream {
    file: Option<String>,
    movie: Option<Arc<Movie>>,
    /// VIDEO_state: 0 idle or finished, 3 playing.
    state: u32,
    /// Virtual clock (1/1920 s) when playback started, the frames' rate and the last frame shown.
    start: u64,
    fps: f32,
    shown: i64,
}

#[derive(Default)]
pub struct Videos {
    streams: HashMap<u32, Stream>,
    cache: HashMap<String, (Arc<Movie>, f32)>,
    decoder: Option<Decoder>,
}

/// What the host must do after a VIDEO_* call.
pub enum VideoAction {
    Return(u32),
    /// VIDEO_create: the handle is a block of the game's heap (its callers free it after
    /// VIDEO_destroy); allocate one and pass it to [`Videos::created`].
    Create,
    /// Write a frame of 16-bit pixels at (x, y), w x h, then return 1.
    Upload { x: u32, y: u32, w: u32, h: u32, pixels: Vec<u16> },
}

impl Videos {
    pub fn created(&mut self, handle: u32) {
        self.streams.insert(handle, Stream { file: None, movie: None, state: 0, start: 0, fps: 15.0, shown: -1 });
    }

    /// `load` reads a file by the game's name; `clock` is the virtual clock.
    pub fn call(&mut self, addr: u32, args: [u32; 4], name: impl Fn(u32) -> String, load: impl Fn(&str) -> Option<Vec<u8>>, clock: u64) -> Option<VideoAction> {
        let [a0, a1, a2, _] = args;
        Some(VideoAction::Return(match addr {
            0x8005_11B0 => return Some(VideoAction::Create),
            0x8005_1328 => {
                self.streams.remove(&a0);
                0
            }
            0x8005_1380 => {
                // VIDEO_spoolfile(handle, file)
                let file = name(a1);
                if let Some(s) = self.streams.get_mut(&a0) {
                    s.file = Some(file);
                }
                1
            }
            0x8005_13DC => {
                // VIDEO_startplayback(handle)
                let Some(s) = self.streams.get_mut(&a0) else { return Some(VideoAction::Return(0)) };
                if let Some(file) = &s.file {
                    if !self.cache.contains_key(file) {
                        if let Some(data) = load(file) {
                            let (m, fps) = if data.starts_with(b"mTCD") {
                                (Movie::parse_dct(&data), 15.0)
                            } else {
                                let m = Movie::parse(&data);
                                let fps = if m.frames.is_empty() { 15.0 } else { 150.0 * m.frames.len() as f32 / (data.len() / 2336).max(1) as f32 };
                                (m, fps)
                            };
                            self.cache.insert(file.clone(), (Arc::new(m), fps));
                        }
                    }
                    if let Some((m, fps)) = self.cache.get(file) {
                        s.movie = Some(m.clone());
                        s.fps = *fps;
                        s.state = 3;
                        s.start = clock;
                        s.shown = -1;
                    }
                }
                1
            }
            0x8005_143C => {
                if let Some(s) = self.streams.get_mut(&a0) {
                    s.state = 0;
                }
                0
            }
            0x8005_1480 => self.streams.get(&a0).map_or(0, |s| s.state),
            0x8005_1528 => {
                // VIDEO_updateframexy(handle, x, y): the frame due now into VRAM at (x, y)
                let Some(s) = self.streams.get_mut(&a0) else { return Some(VideoAction::Return(0)) };
                let Some(m) = s.movie.clone().filter(|_| s.state == 3) else { return Some(VideoAction::Return(0)) };
                let due = ((clock - s.start) as f32 / 1920.0 * s.fps) as i64;
                if due <= s.shown {
                    return Some(VideoAction::Return(0));
                }
                if due as usize >= m.frames.len() {
                    s.state = 0;
                    return Some(VideoAction::Return(0));
                }
                s.shown = due;
                let rgba = self.decoder.get_or_insert_with(Decoder::default).frame(&m.frames[due as usize], m.width, m.height);
                let pixels = rgba.chunks_exact(4).map(|p| (p[0] as u16 >> 3) | ((p[1] as u16 >> 3) << 5) | ((p[2] as u16 >> 3) << 10)).collect();
                return Some(VideoAction::Upload { x: a1, y: a2, w: m.width, h: m.height, pixels });
            }
            _ => return None,
        }))
    }
}
