//! Cover art: downloaded in the background, kept as textures, plus the colours picked
//! from each cover (the original's `Palette`: dark-vibrant and muted swatches).

use egui::{Color32, ColorImage, Context, TextureHandle, TextureOptions};
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

pub struct Loaded {
    pub tex: TextureHandle,
    pub dark_vibrant: Option<Color32>,
    pub muted: Option<Color32>,
}

enum State {
    Pending,
    /// The picture and the frame it was last asked for in (for freeing the least used ones).
    Ready(Loaded, u64),
    Failed,
}

/// Pictures kept as textures at most; beyond this the ones not asked for the longest are dropped.
const MAX_TEXTURES: usize = 700;
/// ...and about this many bytes of picture memory (RGBA).
const MAX_TEXTURE_BYTES: usize = 450 * 1024 * 1024;

type Decoded = (String, Option<(ColorImage, Option<Color32>, Option<Color32>)>);

pub struct Images {
    map: HashMap<String, State>,
    queue: Arc<(Mutex<VecDeque<String>>, Condvar)>,
    tick: u64,
    tx: Sender<Decoded>,
    rx: Receiver<Decoded>,
    workers_started: bool,
}

impl Images {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        Images { map: HashMap::new(), queue: Arc::new((Mutex::new(VecDeque::new()), Condvar::new())), tick: 0, tx, rx, workers_started: false }
    }

    fn start_workers(&mut self, ctx: &Context) {
        if self.workers_started {
            return;
        }
        self.workers_started = true;
        for _ in 0..6 {
            let q = self.queue.clone();
            let tx = self.tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || loop {
                // Sleep until a picture is wanted (no polling).
                let key = {
                    let (lock, cv) = &*q;
                    let mut guard = lock.lock().unwrap();
                    loop {
                        if let Some(k) = guard.pop_front() {
                            break k;
                        }
                        guard = cv.wait(guard).unwrap();
                    }
                };
                let result = load_bytes(&key).and_then(|b| decode(&b, &key));
                let _ = tx.send((key, result));
                ctx.request_repaint();
            });
        }
    }

    /// Call once per frame: turns finished downloads into textures.
    pub fn poll(&mut self, ctx: &Context) {
        self.start_workers(ctx);
        self.tick += 1;
        while let Ok((key, res)) = self.rx.try_recv() {
            let state = match res {
                Some((img, dv, mu)) => State::Ready(
                    Loaded { tex: ctx.load_texture(&key, img, TextureOptions::LINEAR), dark_vibrant: dv, muted: mu },
                    self.tick,
                ),
                None => State::Failed,
            };
            self.map.insert(key, state);
        }
        // Free the textures that haven't been drawn for the longest time.
        // Big pictures (banners) count by their size too, so memory use stays bounded.
        let bytes: usize = self.map.values().map(|v| if let State::Ready(l, _) = v { let z = l.tex.size(); z[0] * z[1] * 4 } else { 0 }).sum();
        if bytes > MAX_TEXTURE_BYTES {
            let mut ready: Vec<(u64, String)> = self.map.iter().filter_map(|(k, v)| if let State::Ready(_, t) = v { Some((*t, k.clone())) } else { None }).collect();
            ready.sort();
            let mut left = bytes;
            for (t, k) in ready {
                if left <= MAX_TEXTURE_BYTES * 3 / 4 || t + 1 >= self.tick {
                    break; // enough freed, or only pictures drawn this very frame remain
                }
                if let Some(State::Ready(l, _)) = self.map.remove(&k) {
                    let z = l.tex.size();
                    left = left.saturating_sub(z[0] * z[1] * 4);
                }
            }
        }
        if self.map.len() > MAX_TEXTURES {
            let mut ready: Vec<(u64, String)> = self.map.iter().filter_map(|(k, v)| if let State::Ready(_, t) = v { Some((*t, k.clone())) } else { None }).collect();
            ready.sort();
            for (_, k) in ready.into_iter().take(self.map.len() - MAX_TEXTURES + 100) {
                self.map.remove(&k);
            }
        }
    }

    /// The cover for a URL (or `file:<path>` for art inside a music file), loading it if needed.
    pub fn get(&mut self, key: &str) -> Option<&Loaded> {
        if key.is_empty() {
            return None;
        }
        if !self.map.contains_key(key) {
            self.map.insert(key.to_string(), State::Pending);
            let (lock, cv) = &*self.queue;
            lock.lock().unwrap().push_back(key.to_string());
            cv.notify_one();
            return None;
        }
        let tick = self.tick;
        match self.map.get_mut(key) {
            Some(State::Ready(l, t)) => {
                *t = tick;
                Some(&*l)
            }
            _ => None,
        }
    }

}

fn load_bytes(key: &str) -> Option<Vec<u8>> {
    if let Some(path) = key.strip_prefix("file:") {
        return crate::local::cover_bytes(path);
    }
    // A picture file the person chose (e.g. a playlist cover).
    if let Some(path) = key.strip_prefix("img:") {
        let meta = std::fs::metadata(path).ok()?;
        if meta.len() > 30 * 1024 * 1024 {
            return None;
        }
        return std::fs::read(path).ok();
    }
    // Only web pictures: nothing else is ever fetched by address here.
    if !key.starts_with("https://") && !key.starts_with("http://") {
        return None;
    }
    // One shared agent keeps connections to the picture servers open between pictures.
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    let agent = AGENT.get_or_init(|| ureq::AgentBuilder::new().max_idle_connections_per_host(8).build());
    let resp = agent.get(key).timeout(std::time::Duration::from_secs(15)).call().ok()?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader().take(8 * 1024 * 1024), &mut buf).ok()?;
    Some(buf)
}

use std::io::Read as _;

fn decode(bytes: &[u8], key: &str) -> Option<(ColorImage, Option<Color32>, Option<Color32>)> {
    let img = image::load_from_memory(bytes).ok()?;
    // Covers are shrunk to 600 px; a wide banner keeps its sharpness (it is drawn across the page).
    let max = if key.contains("w1920-h1080") { 1920 } else { 600 };
    let img = if img.width() > max || img.height() > max { img.thumbnail(max, max) } else { img };
    let rgba = img.to_rgba8();
    let small = image::imageops::thumbnail(&rgba, 50, 50);
    let (dv, mu) = palette(&small);
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some((ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()), dv, mu))
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;
    (h, s, l)
}

/// Swatches like AndroidX Palette: dark-vibrant (lightness ≈ 0.26, saturation high) and
/// muted (lightness ≈ 0.5, saturation ≈ 0.3), chosen from 5-bit-per-channel buckets.
fn palette(img: &image::RgbaImage) -> (Option<Color32>, Option<Color32>) {
    let mut buckets: HashMap<u16, (u32, u32, u32, u32)> = HashMap::new();
    for p in img.pixels() {
        if p[3] < 128 {
            continue;
        }
        let key = ((p[0] as u16 >> 3) << 10) | ((p[1] as u16 >> 3) << 5) | (p[2] as u16 >> 3);
        let e = buckets.entry(key).or_insert((0, 0, 0, 0));
        e.0 += p[0] as u32;
        e.1 += p[1] as u32;
        e.2 += p[2] as u32;
        e.3 += 1;
    }
    let max_pop = buckets.values().map(|b| b.3).max().unwrap_or(1) as f32;
    let pick = |target_s: f32, min_s: f32, max_s: f32, target_l: f32, min_l: f32, max_l: f32| -> Option<Color32> {
        let mut best: Option<(f32, Color32)> = None;
        for b in buckets.values() {
            let (r, g, bl) = (b.0 as f32 / b.3 as f32, b.1 as f32 / b.3 as f32, b.2 as f32 / b.3 as f32);
            let (_, s, l) = rgb_to_hsl(r / 255.0, g / 255.0, bl / 255.0);
            if s < min_s || s > max_s || l < min_l || l > max_l {
                continue;
            }
            let score = (1.0 - (s - target_s).abs()) * 3.0 + (1.0 - (l - target_l).abs()) * 6.5 + (b.3 as f32 / max_pop) * 0.5;
            if best.map(|x| score > x.0).unwrap_or(true) {
                best = Some((score, Color32::from_rgb(r as u8, g as u8, bl as u8)));
            }
        }
        best.map(|b| b.1)
    };
    (pick(1.0, 0.35, 1.0, 0.26, 0.0, 0.45), pick(0.3, 0.0, 0.4, 0.5, 0.3, 0.7))
}
