//! Turns an audio file into stereo f32 samples at the output rate.
//! Symphonia reads mp3, flac, wav, m4a/aac, alac, ogg/vorbis; anything else (opus, wma…)
//! goes through ffmpeg when it is installed.

use std::fs::File;
use std::io::Read;
use std::process::{Child, Stdio};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

pub trait PcmSource: Send {
    /// Next stereo interleaved chunk at the output rate; `None` at the end.
    fn next_chunk(&mut self) -> Option<Vec<f32>>;
    /// Jump to a position in milliseconds.
    fn seek(&mut self, ms: u64) -> bool;
    fn duration_ms(&self) -> u64;
}

pub fn open(path: &str, out_rate: u32, start_ms: u64) -> Result<Box<dyn PcmSource>, String> {
    let sym = SymphoniaSource::open(path, out_rate).and_then(|mut s| {
        // Make sure it really decodes before trusting it.
        let first = s.next_chunk().ok_or_else(|| "no audio could be decoded".to_string())?;
        if start_ms > 0 {
            s.seek(start_ms);
            Ok(Primed { first: None, inner: s })
        } else {
            Ok(Primed { first: Some(first), inner: s })
        }
    });
    match sym {
        Ok(s) => Ok(Box::new(s)),
        Err(e) => {
            let mut f = FfmpegSource::open(path, out_rate, start_ms).map_err(|e2| format!("Can't play this file ({e}; {e2})"))?;
            let first = f.next_chunk().ok_or_else(|| format!("Can't play this file ({e})"))?;
            Ok(Box::new(Primed { first: Some(first), inner: f }))
        }
    }
}

/// True when the file holds audio we can play.
pub fn is_playable(path: &str) -> bool {
    open(path, 48000, 0).is_ok()
}

/// A source whose first chunk was already read (to check it works).
struct Primed<S: PcmSource> {
    first: Option<Vec<f32>>,
    inner: S,
}

impl<S: PcmSource> PcmSource for Primed<S> {
    fn next_chunk(&mut self) -> Option<Vec<f32>> {
        if let Some(f) = self.first.take() {
            return Some(f);
        }
        self.inner.next_chunk()
    }
    fn seek(&mut self, ms: u64) -> bool {
        self.first = None;
        self.inner.seek(ms)
    }
    fn duration_ms(&self) -> u64 {
        self.inner.duration_ms()
    }
}

/// Simple linear resampler that keeps its phase across chunks.
struct Resampler {
    ratio: f64, // source frames per output frame
    pos: f64,
    prev: [f32; 2],
    have_prev: bool,
}

impl Resampler {
    fn new(src: u32, dst: u32) -> Self {
        Resampler { ratio: src as f64 / dst as f64, pos: 0.0, prev: [0.0; 2], have_prev: false }
    }
    fn reset(&mut self) {
        self.pos = 0.0;
        self.have_prev = false;
    }
    fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if (self.ratio - 1.0).abs() < 1e-9 {
            return input.to_vec();
        }
        let frames = input.len() / 2;
        if frames == 0 {
            return vec![];
        }
        // Virtual source: prev frame at index -1, then input frames 0..frames.
        let get = |i: isize, ch: usize, prev: &[f32; 2]| -> f32 {
            if i < 0 {
                prev[ch]
            } else {
                input[i as usize * 2 + ch]
            }
        };
        if !self.have_prev {
            self.prev = [input[0], input[1]];
            self.have_prev = true;
            self.pos = 1.0; // start at the first real frame
        }
        let mut out = Vec::with_capacity((frames as f64 / self.ratio) as usize * 2 + 4);
        // pos is measured with prev at 0 and input frame k at k+1.
        while self.pos < frames as f64 {
            let i = self.pos.floor() as isize - 1;
            let frac = (self.pos - self.pos.floor()) as f32;
            for ch in 0..2 {
                let a = get(i, ch, &self.prev);
                let b = get(i + 1, ch, &self.prev);
                out.push(a + (b - a) * frac);
            }
            self.pos += self.ratio;
        }
        self.pos -= frames as f64;
        self.prev = [input[(frames - 1) * 2], input[(frames - 1) * 2 + 1]];
        out
    }
}

fn to_stereo(samples: &[f32], channels: usize) -> Vec<f32> {
    match channels {
        0 => vec![],
        1 => samples.iter().flat_map(|s| [*s, *s]).collect(),
        2 => samples.to_vec(),
        n => samples.chunks(n).flat_map(|c| [c[0], c[1]]).collect(),
    }
}

pub struct SymphoniaSource {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    resampler: Resampler,
    duration_ms: u64,
    sample_buf: Option<SampleBuffer<f32>>,
    errors: u32,
}

impl SymphoniaSource {
    pub fn open(path: &str, out_rate: u32) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = std::path::Path::new(path).extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions { enable_gapless: true, ..Default::default() }, &MetadataOptions::default())
            .map_err(|e| e.to_string())?;
        let format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or("no audio track")?;
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| e.to_string())?;
        let src_rate = track.codec_params.sample_rate.unwrap_or(44100);
        let duration_ms = match (track.codec_params.n_frames, track.codec_params.time_base) {
            (Some(n), Some(tb)) => {
                let t = tb.calc_time(n);
                t.seconds * 1000 + (t.frac * 1000.0) as u64
            }
            (Some(n), None) => n * 1000 / src_rate as u64,
            _ => 0,
        };
        let track_id = track.id;
        Ok(SymphoniaSource {
            format,
            decoder,
            track_id,
            resampler: Resampler::new(src_rate, out_rate),
            duration_ms,
            sample_buf: None,
            errors: 0,
        })
    }
}

impl PcmSource for SymphoniaSource {
    fn next_chunk(&mut self) -> Option<Vec<f32>> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(_) => return None,
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    let cap = decoded.capacity() as u64;
                    let buf = match &mut self.sample_buf {
                        Some(b) if b.capacity() >= decoded.capacity() * spec.channels.count() => b,
                        _ => {
                            self.sample_buf = Some(SampleBuffer::new(cap, spec));
                            self.sample_buf.as_mut().unwrap()
                        }
                    };
                    buf.copy_interleaved_ref(decoded);
                    let stereo = to_stereo(buf.samples(), spec.channels.count());
                    return Some(self.resampler.process(&stereo));
                }
                Err(symphonia::core::errors::Error::DecodeError(_)) => {
                    self.errors += 1;
                    if self.errors > 200 {
                        return None;
                    }
                    continue;
                }
                Err(_) => return None,
            }
        }
    }

    fn seek(&mut self, ms: u64) -> bool {
        let time = Time { seconds: ms / 1000, frac: (ms % 1000) as f64 / 1000.0 };
        let ok = self
            .format
            .seek(SeekMode::Accurate, SeekTo::Time { time, track_id: Some(self.track_id) })
            .is_ok();
        self.decoder.reset();
        self.resampler.reset();
        ok
    }

    fn duration_ms(&self) -> u64 {
        self.duration_ms
    }
}

/// Fallback decoder: asks ffmpeg for raw stereo float samples.
pub struct FfmpegSource {
    path: String,
    rate: u32,
    child: Option<Child>,
    duration_ms: u64,
}

impl FfmpegSource {
    pub fn open(path: &str, rate: u32, start_ms: u64) -> Result<Self, String> {
        let mut s = FfmpegSource { path: path.to_string(), rate, child: None, duration_ms: probe_duration(path) };
        s.spawn(start_ms)?;
        Ok(s)
    }
    fn spawn(&mut self, start_ms: u64) -> Result<(), String> {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        let child = crate::sys::cmd(crate::sys::ffmpeg())
            .args(["-v", "quiet", "-ss", &format!("{:.3}", start_ms as f64 / 1000.0), "-i", &self.path])
            .args(["-f", "f32le", "-ac", "2", "-ar", &self.rate.to_string(), "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "ffmpeg is not installed".to_string())?;
        self.child = Some(child);
        Ok(())
    }
}

impl PcmSource for FfmpegSource {
    fn next_chunk(&mut self) -> Option<Vec<f32>> {
        let child = self.child.as_mut()?;
        let out = child.stdout.as_mut()?;
        let mut bytes = vec![0u8; 16384];
        let mut filled = 0;
        while filled < bytes.len() {
            match out.read(&mut bytes[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(_) => break,
            }
        }
        if filled < 8 {
            return None;
        }
        filled -= filled % 8;
        Some(bytes[..filled].chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
    }
    fn seek(&mut self, ms: u64) -> bool {
        self.spawn(ms).is_ok()
    }
    fn duration_ms(&self) -> u64 {
        self.duration_ms
    }
}

impl Drop for FfmpegSource {
    fn drop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// Track length from the file's tags/properties.
pub fn probe_duration(path: &str) -> u64 {
    use lofty::file::AudioFile;
    lofty::read_from_path(path).map(|f| f.properties().duration().as_millis() as u64).unwrap_or(0)
}
