//! Audio output (cpal) and a small software mixer: the original's own sound (the modelled
//! SPU's output, streamed from the game thread) plus voices the host plays itself (movie
//! soundtracks).

use crate::formats::audio::Sample;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// The SPU's output waiting to be played: the game thread adds a frame's worth at a time
/// (game time), the output device takes it in real time.
struct SpuQueue {
    buf: VecDeque<[i16; 2]>,
    /// Fraction of the way from `buf[0]` to `buf[1]`.
    pos: f64,
    /// Playing (false: filling up to the target first).
    started: bool,
}

static SPU_QUEUE: Mutex<SpuQueue> = Mutex::new(SpuQueue { buf: VecDeque::new(), pos: 0.0, started: false });
/// Queued SPU samples aimed for: enough to cover a late game frame.
const SPU_TARGET: usize = 44_100 * 3 / 40;
const SPU_MAX: usize = 44_100 / 2;

/// Queue SPU output (44.1 kHz, interleaved stereo).
pub fn push_spu(samples: &[i16]) {
    // NFSHS_PCM=file: also append it there (raw 44.1 kHz stereo s16le), for checking headless
    if let Some(path) = std::env::var_os("NFSHS_PCM") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
            let _ = f.write_all(&bytes);
        }
    }
    let Ok(mut q) = SPU_QUEUE.lock() else { return };
    q.buf.extend(samples.chunks_exact(2).map(|c| [c[0], c[1]]));
    // far ahead of the output (or no output at all): drop the oldest
    let excess = q.buf.len().saturating_sub(SPU_MAX);
    if excess > 0 {
        q.buf.drain(..excess + SPU_TARGET / 2);
    }
}

/// Take `frames` output frames of SPU sound at `rate` Hz, added into `out` (interleaved by
/// `channels`). The playback rate leans slightly towards keeping the queue at its target.
fn pull_spu(out: &mut [f32], channels: usize, rate: f32) {
    let Ok(mut q) = SPU_QUEUE.lock() else { return };
    if !q.started {
        if q.buf.len() < SPU_TARGET {
            return;
        }
        q.started = true;
    }
    let fill = q.buf.len() as f64;
    let lean = ((fill - SPU_TARGET as f64) / SPU_TARGET as f64 * 0.004).clamp(-0.01, 0.01);
    let step = 44_100.0 / rate as f64 * (1.0 + lean);
    for frame in out.chunks_mut(channels) {
        if q.buf.len() < 2 {
            q.started = false;
            break;
        }
        let (a, b, t) = (q.buf[0], q.buf[1], q.pos as f32);
        for (c, s) in frame.iter_mut().enumerate() {
            let k = c.min(1);
            *s += (a[k] as f32 * (1.0 - t) + b[k] as f32 * t) / 32768.0;
        }
        q.pos += step;
        while q.pos >= 1.0 && !q.buf.is_empty() {
            q.buf.pop_front();
            q.pos -= 1.0;
        }
    }
}

struct Voice {
    sample: Arc<Sample>,
    pos: f64,
    pitch: f32,
    gain: f32,
    /// Smoothed gain actually applied (avoids zipper noise between game frames).
    cur_gain: f32,
    looping: bool,
    done: bool,
}

impl Voice {
    /// Next stereo frame, resampled from the voice's native rate.
    fn next(&mut self, out_rate: f32) -> (f32, f32) {
        let s = &self.sample;
        let ch = s.channels.max(1) as usize;
        let frames = s.pcm.len() / ch;
        if frames < 2 || self.done {
            return (0.0, 0.0);
        }
        let (ls, le) = (s.loop_start.unwrap_or(0).min(frames - 1), s.loop_end.unwrap_or(frames).min(frames));
        let i = self.pos as usize;
        let t = (self.pos - i as f64) as f32;
        let j = if i + 1 >= le && self.looping { ls } else { (i + 1).min(frames - 1) };
        let get = |k: usize, c: usize| s.pcm[k * ch + c.min(ch - 1)] as f32 / 32768.0;
        let l = get(i, 0) * (1.0 - t) + get(j, 0) * t;
        let r = get(i, 1) * (1.0 - t) + get(j, 1) * t;
        self.cur_gain += (self.gain - self.cur_gain) * 0.002;
        self.pos += (s.rate as f32 * self.pitch / out_rate) as f64;
        if self.pos >= le as f64 {
            if self.looping {
                self.pos = ls as f64 + (self.pos - le as f64) % ((le - ls).max(1) as f64);
            } else {
                self.done = true;
            }
        }
        (l * self.cur_gain, r * self.cur_gain)
    }
}

#[derive(Default)]
struct Mixer {
    voices: Vec<Option<Voice>>,
    master: f32,
}

pub type VoiceId = usize;

pub struct Audio {
    _stream: cpal::Stream,
    mixer: Arc<Mutex<Mixer>>,
}

impl Audio {
    pub fn new() -> anyhow::Result<Self> {
        let host = cpal::default_host();
        // NFSHS_AUDIO_DEVICE=name: an output device other than the default (by part of its name)
        let chosen = std::env::var("NFSHS_AUDIO_DEVICE").ok().and_then(|want| {
            host.output_devices().ok()?.find(|d| d.description().is_ok_and(|n| n.name().contains(want.as_str())))
        });
        let device = chosen.or_else(|| host.default_output_device()).ok_or_else(|| anyhow::anyhow!("no audio output device"))?;
        let supported = device.default_output_config()?;
        let config: cpal::StreamConfig = supported.clone().into();
        let rate = config.sample_rate as f32;
        let channels = config.channels as usize;
        let mixer = Arc::new(Mutex::new(Mixer { voices: Vec::new(), master: 0.8 }));
        let m = mixer.clone();
        let err = |e| log::warn!("audio stream error: {e}");
        let mut render = move |data: &mut [f32]| {
            let Ok(mut mx) = m.lock() else { return };
            let master = mx.master;
            for frame in data.chunks_mut(channels) {
                let (mut l, mut r) = (0.0f32, 0.0f32);
                for v in mx.voices.iter_mut().flatten() {
                    let (a, b) = v.next(rate);
                    l += a;
                    r += b;
                }
                let (l, r) = ((l * master).tanh(), (r * master).tanh());
                for (c, s) in frame.iter_mut().enumerate() {
                    *s = if c % 2 == 0 { l } else { r };
                }
            }
            drop(mx);
            pull_spu(data, channels, rate);
            for s in data.iter_mut() {
                *s = s.clamp(-1.0, 1.0);
            }
            let Ok(mut mx) = m.lock() else { return };
            mx.voices.iter_mut().for_each(|v| {
                if v.as_ref().is_some_and(|v| v.done) {
                    *v = None;
                }
            });
        };
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => device.build_output_stream(config, move |d: &mut [f32], _| render(d), err, None)?,
            cpal::SampleFormat::I16 => {
                let mut buf = Vec::new();
                device.build_output_stream(
                    config,
                    move |d: &mut [i16], _| {
                        buf.resize(d.len(), 0.0);
                        render(&mut buf);
                        for (o, s) in d.iter_mut().zip(&buf) {
                            *o = (s * 32767.0) as i16;
                        }
                    },
                    err,
                    None,
                )?
            }
            f => anyhow::bail!("unsupported output sample format {f:?}"),
        };
        stream.play()?;
        log::info!("audio: {} Hz, {} channels", rate, channels);
        Ok(Self { _stream: stream, mixer })
    }

    pub fn play(&self, sample: Arc<Sample>, gain: f32, pitch: f32, looping: bool) -> VoiceId {
        let mut mx = self.mixer.lock().unwrap();
        let v = Voice { sample, pos: 0.0, pitch, gain, cur_gain: gain, looping, done: false };
        if let Some(i) = mx.voices.iter().position(Option::is_none) {
            mx.voices[i] = Some(v);
            i
        } else {
            mx.voices.push(Some(v));
            mx.voices.len() - 1
        }
    }

    pub fn set(&self, id: VoiceId, gain: f32, pitch: f32) {
        if let Some(Some(v)) = self.mixer.lock().unwrap().voices.get_mut(id) {
            v.gain = gain;
            v.pitch = pitch;
        }
    }

    pub fn stop(&self, id: VoiceId) {
        if let Some(v) = self.mixer.lock().unwrap().voices.get_mut(id) {
            *v = None;
        }
    }
}

/// Engine sound: four on-throttle and four off-throttle loops crossfaded by rpm.
pub struct EngineSound {
    on: Vec<(VoiceId, f32)>,
    off: Vec<(VoiceId, f32)>,
    throttle_mix: f32,
}

impl EngineSound {
    /// `bank`: decoded car engine bank. Layer layout inferred from the banks:
    /// entries 16..=19 on-throttle, 64..=67 off-throttle, ordered low to high rpm.
    pub fn new(audio: &Audio, bank: &crate::formats::audio::Bank) -> Option<Self> {
        let pick = |range: std::ops::RangeInclusive<usize>| -> Vec<Arc<Sample>> {
            range.filter_map(|i| bank.sounds.get(i).cloned().flatten()).filter(|s| s.channels == 1).map(Arc::new).collect()
        };
        let on = pick(16..=19);
        let off = pick(64..=67);
        if on.is_empty() {
            return None;
        }
        let layers = |v: Vec<Arc<Sample>>| -> Vec<(VoiceId, f32)> {
            let n = v.len() as f32;
            v.into_iter().enumerate().map(|(k, s)| (audio.play(s, 0.0, 1.0, true), (k as f32 + 1.0) / (n + 1.0))).collect()
        };
        Some(Self { on: layers(on), off: layers(off), throttle_mix: 0.0 })
    }

    /// `rpm_frac` = rpm / redline, `throttle` 0..1, `volume` overall gain.
    pub fn update(&mut self, audio: &Audio, rpm_frac: f32, throttle: f32, volume: f32, dt: f32) {
        self.throttle_mix += (throttle - self.throttle_mix) * (dt * 8.0).min(1.0);
        let rpm = rpm_frac.clamp(0.08, 1.05);
        let apply = |layers: &[(VoiceId, f32)], weight: f32| {
            let spacing = if layers.len() > 1 { layers[1].1 - layers[0].1 } else { 1.0 };
            for &(id, centre) in layers {
                let d = ((rpm - centre) / spacing).abs();
                let g = (1.0 - d).clamp(0.0, 1.0);
                // lowest layer also covers idle, highest covers the top end
                let g = if centre == layers[0].1 && rpm < centre { 1.0 } else if centre == layers[layers.len() - 1].1 && rpm > centre { 1.0 } else { g };
                audio.set(id, g * weight * volume, (rpm / centre).clamp(0.5, 2.0));
            }
        };
        let off_w = if self.off.is_empty() { 0.0 } else { 1.0 - self.throttle_mix };
        apply(&self.on, if self.off.is_empty() { 1.0 } else { 0.35 + 0.65 * self.throttle_mix });
        if !self.off.is_empty() {
            apply(&self.off, off_w * 0.7);
        }
    }

    pub fn stop(&self, audio: &Audio) {
        for &(id, _) in self.on.iter().chain(&self.off) {
            audio.stop(id);
        }
    }
}
