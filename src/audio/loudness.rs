//! Loudness matching for music-video sound. A music video's soundtrack is often mastered at another
//! level than the song's audio version. Instead of riding the volume while it plays (which makes the
//! sound swell and dip), each file's overall loudness is measured once and the video's sound gets ONE
//! fixed gain for the whole song, so it sits at the audio version's level and never moves.

use super::decode;
use std::collections::HashMap;
use std::sync::Mutex;

const RATE: u32 = 44_100;
/// Pieces of the song that are listened to (spread evenly), and how long each is.
const WINDOWS: u64 = 12;
const WINDOW_MS: u64 = 3_000;
/// Loudness is averaged over blocks of 400 ms, as loudness meters do.
const BLOCK: usize = (RATE as usize * 2) * 400 / 1000;

fn memo() -> &'static Mutex<HashMap<String, Option<f32>>> {
    static M: std::sync::OnceLock<Mutex<HashMap<String, Option<f32>>>> = std::sync::OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Overall loudness of a file in dB (mean power of the parts where music is playing; quiet scenes and
/// silence are left out). `None` when the file can't be read.
pub fn measure(path: &str) -> Option<f32> {
    if let Some(v) = memo().lock().unwrap().get(path) {
        return *v;
    }
    let v = measure_now(path);
    memo().lock().unwrap().insert(path.to_string(), v);
    v
}

fn measure_now(path: &str) -> Option<f32> {
    let mut src = decode::open(path, RATE, 0).ok()?;
    let dur = src.duration_ms();
    let mut blocks: Vec<f32> = Vec::new();
    let take = |src: &mut Box<dyn decode::PcmSource>, ms: u64, blocks: &mut Vec<f32>| {
        let want = (RATE as u64 * 2 * ms / 1000) as usize;
        let mut got = 0usize;
        let (mut sum, mut n) = (0.0f64, 0usize);
        while got < want {
            let Some(chunk) = src.next_chunk() else { break };
            for s in &chunk {
                sum += (*s as f64) * (*s as f64);
                n += 1;
                if n == BLOCK {
                    blocks.push((sum / n as f64) as f32);
                    sum = 0.0;
                    n = 0;
                }
            }
            got += chunk.len();
        }
    };
    if dur < 60_000 {
        // Short or of unknown length: listen to all of it (at most ten minutes).
        take(&mut src, 600_000, &mut blocks);
    } else {
        for i in 0..WINDOWS {
            let at = dur * 8 / 100 + (dur * 84 / 100) * i / WINDOWS;
            if !src.seek(at) {
                break;
            }
            take(&mut src, WINDOW_MS, &mut blocks);
        }
    }
    // Leave out silence, then everything more than 10 dB under the average (scenes without music).
    let loud: Vec<f32> = blocks.into_iter().filter(|p| *p > 1e-7).collect();
    if loud.len() < 5 {
        return None;
    }
    let mean = loud.iter().sum::<f32>() / loud.len() as f32;
    let gated: Vec<f32> = loud.into_iter().filter(|p| *p >= mean * 0.1).collect();
    let mean = gated.iter().sum::<f32>() / gated.len().max(1) as f32;
    Some(10.0 * mean.log10())
}

/// The fixed gain that brings `video` sound to the level of `reference` (the song's audio version).
/// At most about 4 dB up or 6 dB down; 1.0 when either file can't be measured or they already match.
pub fn match_gain(video: &str, reference: &str) -> f32 {
    let (Some(v), Some(r)) = (measure(video), measure(reference)) else { return 1.0 };
    let diff = r - v;
    if diff.abs() < 0.5 {
        return 1.0;
    }
    10f32.powf(diff / 20.0).clamp(0.5, 1.6)
}
