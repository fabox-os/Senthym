//! The audio output: two "decks" mixed together so one song can fade into the next.
//!
//! Crossfade rules come from the original `SongPlayer.performCrossfade`: 50 steps,
//! each `max(ms/50, 20)` ms long, outgoing volume cos(angle), incoming sin(angle),
//! and in DJ mode a low-pass sweep (20 kHz → 200 Hz) on the old song and a high-pass
//! sweep (2 kHz → 20 Hz) on the new one, shaped by a sigmoid.

use super::biquad::{exp_interpolate, sigmoid, Biquad, FilterType};
use super::decode;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub const CROSSFADE_STEPS: u32 = 50;
/// Steps 0..=50 each last one delay: 51 delays in all (performCrossfade$job$1 @0x0090).
pub const CROSSFADE_STEP_COUNT_INCL_START: u64 = 51;
pub const CROSSFADE_MIN_STEP_MS: u64 = 20;
pub const CF_LPF_START_HZ: f32 = 20000.0;
pub const CF_LPF_END_HZ: f32 = 200.0;
pub const CF_HPF_START_HZ: f32 = 2000.0;
pub const CF_HPF_END_HZ: f32 = 20.0;

/// How many seconds of decoded audio each deck keeps ready.
const BUFFER_SECONDS: usize = 3;

struct Deck {
    generation: u64,
    buf: VecDeque<f32>,
    /// Frames played since `base_ms`.
    frames: u64,
    base_ms: u64,
    duration_ms: u64,
    eof: bool,
    volume: f32,
    /// Fixed loudness-matching gain for this file (1.0 = untouched); never changes while it plays.
    gain: f32,
    filter: Biquad,
    seek_to: Option<u64>,
    error: Option<String>,
}

impl Deck {
    fn position_ms(&self, rate: u32) -> u64 {
        self.base_ms + self.frames * 1000 / rate as u64
    }
    fn finished(&self) -> bool {
        self.eof && self.buf.is_empty() && self.seek_to.is_none()
    }
}

struct Fade {
    from: usize,
    to: usize,
    /// Frames per step: max(ms / 50, 20) ms.
    step_frames: u64,
    total_frames: u64,
    elapsed: u64,
    dj: bool,
    last_step: u32,
}

struct Mixer {
    rate: u32,
    decks: [Option<Deck>; 2],
    primary: usize,
    playing: bool,
    master: f32,
    fade: Option<Fade>,
    fade_done: bool,
}

fn gains() -> &'static Mutex<std::collections::HashMap<String, f32>> {
    static G: std::sync::OnceLock<Mutex<std::collections::HashMap<String, f32>>> = std::sync::OnceLock::new();
    G.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// Remember the fixed gain a file is played with (set before it is loaded). Currently unused:
/// automatic video matching no longer swaps in the video's own sound (that fetch used to block
/// the song's start), so nothing measures a gain to set — kept for the day that swap is made to
/// happen in the background instead, without delaying playback.
#[allow(dead_code)]
pub fn set_path_gain(path: &str, gain: f32) {
    gains().lock().unwrap().insert(path.to_string(), gain.clamp(0.25, 2.0));
}

fn path_gain(path: &str) -> f32 {
    gains().lock().unwrap().get(path).copied().unwrap_or(1.0)
}

/// Keeps peaks from clipping without touching normal levels.
fn soft_limit(x: f32) -> f32 {
    let a = x.abs();
    if a <= 0.85 {
        x
    } else {
        x.signum() * (0.85 + 0.15 * ((a - 0.85) / 0.15).tanh())
    }
}

impl Mixer {
    fn render(&mut self, out: &mut [f32]) {
        // `out` is interleaved stereo.
        for frame in out.chunks_mut(2) {
            let (mut l, mut r) = (0.0f32, 0.0f32);
            if self.playing {
                self.step_fade();
                for slot in 0..2 {
                    if let Some(d) = self.decks[slot].as_mut() {
                        if d.seek_to.is_some() || d.buf.len() < 2 {
                            continue;
                        }
                        let mut a = d.buf.pop_front().unwrap_or(0.0);
                        let mut b = d.buf.pop_front().unwrap_or(0.0);
                        d.frames += 1;
                        d.filter.process(&mut a, &mut b);
                        if d.gain != 1.0 {
                            a *= d.gain;
                            b *= d.gain;
                            if d.gain > 1.0 {
                                a = soft_limit(a);
                                b = soft_limit(b);
                            }
                        }
                        l += a * d.volume;
                        r += b * d.volume;
                    }
                }
            }
            let m = self.master;
            frame[0] = (l * m).clamp(-1.0, 1.0);
            if frame.len() > 1 {
                frame[1] = (r * m).clamp(-1.0, 1.0);
            }
        }
    }

    fn step_fade(&mut self) {
        let Some(f) = self.fade.as_mut() else { return };
        // Only advance while the incoming song actually has sound ready.
        let ready = self.decks[f.to].as_ref().map(|d| d.buf.len() >= 2 || d.eof).unwrap_or(false);
        if !ready {
            return;
        }
        f.elapsed += 1;
        // Step k (0..=50) takes effect at k step-delays; after step 50 one more delay, then hand over.
        let step = ((f.elapsed - 1) / f.step_frames.max(1)).min(CROSSFADE_STEPS as u64) as u32;
        if step != f.last_step || f.elapsed == 1 {
            f.last_step = step;
            let progress = step as f32 / CROSSFADE_STEPS as f32;
            let angle = progress * std::f32::consts::FRAC_PI_2;
            let (from, to, dj) = (f.from, f.to, f.dj);
            if let Some(d) = self.decks[from].as_mut() {
                d.volume = angle.cos();
                if dj {
                    d.filter.set(FilterType::LowPass, exp_interpolate(CF_LPF_START_HZ, CF_LPF_END_HZ, sigmoid(progress)));
                }
            }
            if let Some(d) = self.decks[to].as_mut() {
                d.volume = angle.sin();
                if dj {
                    d.filter.set(FilterType::HighPass, exp_interpolate(CF_HPF_START_HZ, CF_HPF_END_HZ, sigmoid(progress)));
                }
            }
        }
        let f = self.fade.as_ref().unwrap();
        let from_gone = self.decks[f.from].as_ref().map(|d| d.finished()).unwrap_or(true);
        if f.elapsed >= f.total_frames || from_gone {
            self.finish_fade();
        }
    }

    fn finish_fade(&mut self) {
        if let Some(f) = self.fade.take() {
            self.decks[f.from] = None;
            if let Some(d) = self.decks[f.to].as_mut() {
                d.volume = 1.0;
                d.filter.set_enabled(false);
            }
            self.primary = f.to;
            self.fade_done = true;
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub position_ms: u64,
    pub duration_ms: u64,
    pub playing: bool,
    pub loaded: bool,
    pub finished: bool,
    #[allow(dead_code)]
    pub buffering: bool,
    pub fading: bool,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct Engine {
    mixer: Arc<Mutex<Mixer>>,
    gen: Arc<AtomicU64>,
    pub device_name: String,
    _stream: Arc<Mutex<Option<StreamHolder>>>,
}

struct StreamHolder(#[allow(dead_code)] cpal::Stream);
// The stream is only kept alive here, never used across threads.
unsafe impl Send for StreamHolder {}

impl Engine {
    pub fn new() -> Engine {
        let host = cpal::default_host();
        let device = host.default_output_device();
        let config = device.as_ref().and_then(|d| d.default_output_config().ok());
        let rate = config.as_ref().map(|c| c.sample_rate().0).unwrap_or(48000);
        let mixer = Arc::new(Mutex::new(Mixer {
            rate,
            decks: [None, None],
            primary: 0,
            playing: false,
            master: 1.0,
            fade: None,
            fade_done: false,
        }));
        let mut device_name = "No sound device".to_string();
        let mut holder = None;
        if let (Some(dev), Some(cfg)) = (device, config) {
            device_name = dev.name().unwrap_or_else(|_| "This computer".into());
            if let Some(s) = build_stream(&dev, &cfg, mixer.clone()) {
                if s.play().is_ok() {
                    holder = Some(StreamHolder(s));
                }
            }
        }
        if holder.is_none() {
            // No sound card: keep time moving with a silent clock so playback still advances.
            let m = mixer.clone();
            thread::spawn(move || {
                let mut buf = vec![0.0f32; (rate as usize / 50) * 2];
                loop {
                    thread::sleep(Duration::from_millis(20));
                    m.lock().unwrap().render(&mut buf);
                }
            });
        }
        Engine { mixer, gen: Arc::new(AtomicU64::new(1)), device_name, _stream: Arc::new(Mutex::new(holder)) }
    }

    pub fn rate(&self) -> u32 {
        self.mixer.lock().unwrap().rate
    }

    fn spawn_decoder(&self, slot: usize, path: String, start_ms: u64, duration_hint: u64, volume: f32) {
        let generation = self.gen.fetch_add(1, Ordering::SeqCst);
        let rate = self.rate();
        {
            let mut m = self.mixer.lock().unwrap();
            m.decks[slot] = Some(Deck {
                generation,
                buf: VecDeque::with_capacity(rate as usize * 2 * BUFFER_SECONDS),
                frames: 0,
                base_ms: start_ms,
                duration_ms: duration_hint,
                eof: false,
                volume,
                gain: path_gain(&path),
                filter: Biquad::new(rate),
                seek_to: None,
                error: None,
            });
        }
        let mixer = self.mixer.clone();
        thread::spawn(move || {
            let mut src = match decode::open(&path, rate, start_ms) {
                Ok(s) => s,
                Err(e) => {
                    let mut m = mixer.lock().unwrap();
                    if let Some(d) = m.decks[slot].as_mut().filter(|d| d.generation == generation) {
                        d.error = Some(e);
                        d.eof = true;
                    }
                    return;
                }
            };
            {
                let mut m = mixer.lock().unwrap();
                if let Some(d) = m.decks[slot].as_mut().filter(|d| d.generation == generation) {
                    let real = src.duration_ms();
                    if real > 0 {
                        d.duration_ms = real;
                    }
                }
            }
            let cap = rate as usize * 2 * BUFFER_SECONDS;
            loop {
                // Check our deck still exists and whether it wants something.
                let (want_seek, room) = {
                    let mut m = mixer.lock().unwrap();
                    let Some(d) = m.decks[slot].as_mut().filter(|d| d.generation == generation) else { return };
                    (d.seek_to.take(), cap.saturating_sub(d.buf.len()))
                };
                if let Some(ms) = want_seek {
                    src.seek(ms);
                    let mut m = mixer.lock().unwrap();
                    if let Some(d) = m.decks[slot].as_mut().filter(|d| d.generation == generation) {
                        d.buf.clear();
                        d.frames = 0;
                        d.base_ms = ms;
                        d.eof = false;
                    }
                    continue;
                }
                if room < 8192 {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                match src.next_chunk() {
                    Some(chunk) => {
                        let mut m = mixer.lock().unwrap();
                        let Some(d) = m.decks[slot].as_mut().filter(|d| d.generation == generation) else { return };
                        if d.seek_to.is_none() {
                            d.buf.extend(chunk);
                        }
                    }
                    None => {
                        let mut m = mixer.lock().unwrap();
                        let Some(d) = m.decks[slot].as_mut().filter(|d| d.generation == generation) else { return };
                        if d.seek_to.is_none() {
                            d.eof = true;
                        }
                        drop(m);
                        thread::sleep(Duration::from_millis(30));
                    }
                }
            }
        });
    }

    /// Replace whatever plays with this file.
    pub fn load(&self, path: &str, start_ms: u64, duration_hint: u64, autoplay: bool) {
        let slot = {
            let mut m = self.mixer.lock().unwrap();
            m.fade = None;
            m.fade_done = false;
            m.decks = [None, None];
            m.primary = 0;
            m.playing = autoplay;
            0
        };
        self.spawn_decoder(slot, path.to_string(), start_ms, duration_hint, 1.0);
    }

    /// Start the next song on the other deck and fade into it over `effective_ms`.
    pub fn crossfade_to(&self, path: &str, duration_hint: u64, effective_ms: u64, dj: bool) {
        let (to, rate) = {
            let m = self.mixer.lock().unwrap();
            (1 - m.primary, m.rate)
        };
        self.spawn_decoder(to, path.to_string(), 0, duration_hint, 0.0);
        let step_ms = (effective_ms / CROSSFADE_STEPS as u64).max(CROSSFADE_MIN_STEP_MS);
        let total_ms = step_ms * CROSSFADE_STEP_COUNT_INCL_START;
        let mut m = self.mixer.lock().unwrap();
        let from = m.primary;
        if dj {
            if let Some(d) = m.decks[from].as_mut() {
                d.filter.set(FilterType::LowPass, CF_LPF_START_HZ);
                d.filter.set_enabled(true);
            }
            if let Some(d) = m.decks[to].as_mut() {
                d.filter.set(FilterType::HighPass, CF_HPF_START_HZ);
                d.filter.set_enabled(true);
            }
        }
        m.fade = Some(Fade { from, to, step_frames: step_ms * rate as u64 / 1000, total_frames: total_ms * rate as u64 / 1000, elapsed: 0, dj, last_step: 0 });
        m.fade_done = false;
        m.playing = true;
    }

    pub fn cancel_fade(&self) {
        let mut m = self.mixer.lock().unwrap();
        if let Some(f) = m.fade.take() {
            m.decks[f.to] = None;
            if let Some(d) = m.decks[f.from].as_mut() {
                d.volume = 1.0;
                d.filter.set_enabled(false);
            }
        }
    }

    /// True once after a crossfade has handed over to the new song.
    pub fn take_fade_done(&self) -> bool {
        let mut m = self.mixer.lock().unwrap();
        std::mem::take(&mut m.fade_done)
    }

    pub fn set_playing(&self, playing: bool) {
        self.mixer.lock().unwrap().playing = playing;
    }

    pub fn stop(&self) {
        let mut m = self.mixer.lock().unwrap();
        m.decks = [None, None];
        m.fade = None;
        m.playing = false;
    }

    pub fn seek(&self, ms: u64) {
        let mut m = self.mixer.lock().unwrap();
        if m.fade.is_some() {
            drop(m);
            self.cancel_fade();
            m = self.mixer.lock().unwrap();
        }
        let p = m.primary;
        if let Some(d) = m.decks[p].as_mut() {
            d.seek_to = Some(ms);
            d.buf.clear();
            d.base_ms = ms;
            d.frames = 0;
            d.eof = false;
        }
    }

    pub fn set_volume(&self, v: f32) {
        self.mixer.lock().unwrap().master = v.clamp(0.0, 1.0);
    }

    pub fn status(&self) -> Status {
        let m = self.mixer.lock().unwrap();
        let rate = m.rate;
        match m.decks[m.primary].as_ref() {
            Some(d) => Status {
                position_ms: d.position_ms(rate).min(if d.duration_ms > 0 { d.duration_ms } else { u64::MAX }),
                duration_ms: d.duration_ms,
                playing: m.playing,
                loaded: true,
                finished: d.finished(),
                buffering: !d.eof && d.buf.len() < 2,
                fading: m.fade.is_some(),
                error: d.error.clone(),
            },
            None => Status { playing: m.playing, ..Default::default() },
        }
    }
}

fn build_stream(dev: &cpal::Device, cfg: &cpal::SupportedStreamConfig, mixer: Arc<Mutex<Mixer>>) -> Option<cpal::Stream> {
    let channels = cfg.channels() as usize;
    let config: cpal::StreamConfig = cfg.clone().into();
    let err = |e| eprintln!("audio stream error: {e}");
    let mut scratch: Vec<f32> = Vec::new();
    macro_rules! stream {
        ($t:ty, $conv:expr) => {{
            dev.build_output_stream(
                &config,
                move |data: &mut [$t], _| {
                    let frames = data.len() / channels;
                    scratch.resize(frames * 2, 0.0);
                    match mixer.lock() {
                        Ok(mut m) => m.render(&mut scratch),
                        Err(_) => scratch.iter_mut().for_each(|s| *s = 0.0),
                    }
                    for (i, frame) in data.chunks_mut(channels).enumerate() {
                        let l = scratch[i * 2];
                        let r = scratch[i * 2 + 1];
                        for (c, s) in frame.iter_mut().enumerate() {
                            let v = if c % 2 == 0 { l } else { r };
                            *s = $conv(v);
                        }
                    }
                },
                err,
                None,
            )
            .ok()
        }};
    }
    match cfg.sample_format() {
        cpal::SampleFormat::F32 => stream!(f32, |v: f32| v),
        cpal::SampleFormat::I16 => stream!(i16, |v: f32| (v * i16::MAX as f32) as i16),
        cpal::SampleFormat::U16 => stream!(u16, |v: f32| ((v * 0.5 + 0.5) * u16::MAX as f32) as u16),
        cpal::SampleFormat::I32 => stream!(i32, |v: f32| (v as f64 * i32::MAX as f64) as i32),
        _ => None,
    }
}
