//! Music-video pictures: ffmpeg decodes the video stream into raw frames, and the app shows the frame
//! that belongs to the audio position. The sound itself comes from the normal audio engine, so
//! pausing, seeking and the volume all keep working as they do for songs.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const FPS: u64 = 24;

pub struct VideoStream {
    /// The decoder, once it has been started (that happens on a background thread).
    child: Arc<Mutex<Option<Child>>>,
    /// Set when the stream is dropped, so a decoder that is only just starting stops at once.
    cancel: Arc<AtomicBool>,
    /// Why the picture couldn't be started, if it couldn't.
    error: Arc<Mutex<Option<String>>>,
    rx: Receiver<Vec<u8>>,
    /// Position (ms) the first frame of this stream belongs to.
    base_ms: u64,
    /// Frames taken so far.
    count: u64,
    /// A frame already read that is not due yet.
    pending: Option<Vec<u8>>,
    pub w: usize,
    pub h: usize,
    pub ended: bool,
}

impl Drop for VideoStream {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(mut c) = self.child.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

// ---------------------------------------------------------------- fast start

/// Where each piece (a few seconds, starting on a full picture) of a video file begins. With it the
/// app fetches the file itself in bounded ranges, which the server sends at full speed, and hands the
/// decoder exactly the piece that holds the wanted moment. Letting the decoder open the address and
/// seek takes seconds at high quality, because one long open-ended request is slowed down by the server.
struct Index {
    /// The file's header (what a decoder needs before any piece).
    init: Vec<u8>,
    /// (start in ms, byte offset) of every piece, in order.
    pieces: Vec<(u64, u64)>,
    total: u64,
}

fn agent() -> ureq::Agent {
    static A: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    A.get_or_init(|| ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(8)).timeout_read(Duration::from_secs(15)).max_idle_connections_per_host(4).build()).clone()
}

/// Bytes `a..=b` of the file, and the file's total size.
fn range(url: &str, a: u64, b: u64) -> Result<(Vec<u8>, u64), String> {
    let resp = agent().get(url).set("Range", &format!("bytes={a}-{b}")).call().map_err(|e| e.to_string())?;
    let total = resp.header("Content-Range").and_then(|v| v.rsplit('/').next()).and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(0);
    let mut buf = Vec::with_capacity((b - a + 1) as usize);
    resp.into_reader().take(b - a + 1).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok((buf, total))
}

fn be32(b: &[u8], at: usize) -> Option<u64> {
    b.get(at..at + 4).map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]) as u64)
}

fn be64(b: &[u8], at: usize) -> Option<u64> {
    b.get(at..at + 8).map(|x| u64::from_be_bytes([x[0], x[1], x[2], x[3], x[4], x[5], x[6], x[7]]))
}

/// Read the header and the piece index ("sidx" box) of an MP4 video file. `None` for other kinds of file.
fn read_index(url: &str) -> Option<Index> {
    let (mut head, total) = range(url, 0, 65_535).ok()?;
    if total == 0 {
        return None;
    }
    let mut pos = 0usize;
    loop {
        let size = be32(&head, pos)? as usize;
        let kind = head.get(pos + 4..pos + 8)?;
        if size < 8 {
            return None;
        }
        if kind == b"sidx" {
            if pos + size > head.len() {
                // A long video's index is bigger than the first read: get the rest (at most 2 MB).
                if size > 2 * 1024 * 1024 {
                    return None;
                }
                let (more, _) = range(url, head.len() as u64, (pos + size) as u64 - 1).ok()?;
                head.extend_from_slice(&more);
            }
            let b = head.get(pos + 8..pos + size)?;
            let version = *b.first()?;
            let timescale = be32(b, 8)?.max(1);
            let (mut time, first_offset, mut o) = if version == 0 { (be32(b, 12)?, be32(b, 16)?, 20usize) } else { (be64(b, 12)?, be64(b, 20)?, 28usize) };
            let count = u16::from_be_bytes([*b.get(o + 2)?, *b.get(o + 3)?]) as usize;
            o += 4;
            let mut offset = (pos + size) as u64 + first_offset;
            let mut pieces = Vec::with_capacity(count);
            for _ in 0..count {
                let r = be32(b, o)?;
                if r & 0x8000_0000 != 0 {
                    return None; // an index of indexes: not something these files use
                }
                let dur = be32(b, o + 4)?;
                pieces.push((time * 1000 / timescale, offset));
                offset += r & 0x7fff_ffff;
                time += dur;
                o += 12;
            }
            if pieces.is_empty() {
                return None;
            }
            return Some(Index { init: head[..pos].to_vec(), pieces, total });
        }
        if kind == b"moof" || kind == b"mdat" {
            return None;
        }
        pos += size;
    }
}

/// The index of an address, read once and remembered (`None` is remembered too).
fn index_for(url: &str) -> Option<Arc<Index>> {
    static CACHE: std::sync::OnceLock<Mutex<HashMap<String, Option<Arc<Index>>>>> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(v) = cache.lock().unwrap().get(url) {
        return v.clone();
    }
    let v = read_index(url).map(Arc::new);
    let mut c = cache.lock().unwrap();
    if c.len() > 200 {
        c.clear();
    }
    c.insert(url.to_string(), v.clone());
    v
}

/// Get ready for an address before its picture is asked for, so starting it is quicker.
pub fn warm(url: &str) {
    if url.starts_with("https://") {
        let url = url.to_string();
        std::thread::spawn(move || {
            let _ = index_for(&url);
        });
    }
}

pub enum Poll {
    /// The newest frame that is due (RGBA, w*h*4 bytes).
    Frame(Vec<u8>),
    /// Nothing new yet.
    Wait,
    /// The stream is far ahead of the audio position (after a seek back): start it again at the position.
    Restart,
    /// The stream is far behind the audio position: it can't keep up (slow connection or machine), the
    /// song was sought forwards, or nobody polled for a while. The app tells these apart.
    Lag,
}

impl VideoStream {
    /// Start decoding `url` (a direct video address) from `start_ms`, scaled to fit `w` x `h`. Returns
    /// at once: the decoder is started on a background thread and frames arrive when they are ready.
    pub fn start(url: &str, start_ms: u64, w: usize, h: usize) -> Result<VideoStream, String> {
        // The picture is never bigger than 1920x1080 (1080p).
        let (w, h) = (w.clamp(2, 1920), h.clamp(2, 1080));
        // Only a web address, and only web protocols: ffmpeg could otherwise be made to read local files.
        if !url.starts_with("https://") {
            return Err("unexpected video address".into());
        }
        let size = w * h * 4;
        // A short queue: ffmpeg waits when it is full, so only a moment of video is read ahead.
        // About 20 MB of read-ahead at most (a big picture needs fewer frames queued).
        let (tx, rx) = sync_channel::<Vec<u8>>((20 * 1024 * 1024 / size.max(1)).clamp(2, 8));
        let child: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
        let cancel = Arc::new(AtomicBool::new(false));
        let error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let (slot, stop, err, url) = (child.clone(), cancel.clone(), error.clone(), url.to_string());
        std::thread::spawn(move || {
            let scale = format!("fps={FPS},scale={w}:{h}:flags=fast_bilinear:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:black");
            let common = ["-hide_banner", "-loglevel", "error", "-probesize", "1000000", "-analyzeduration", "500000", "-threads", "3"];
            let tail = ["-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1"];
            // The quick way: the app fetches the file in pieces and feeds the decoder (see `Index`).
            let piped = index_for(&url).and_then(|idx| {
                let i = idx.pieces.iter().rposition(|(t, _)| *t <= start_ms).unwrap_or(0);
                let (piece_ms, offset) = idx.pieces[i];
                // The piece starts a little before the wanted moment: drop those frames in the decoder.
                let skip = format!("{:.3}", start_ms.saturating_sub(piece_ms) as f64 / 1000.0);
                let filter = format!("trim=start={skip},setpts=PTS-STARTPTS,{scale}");
                let mut args: Vec<String> = common.iter().map(|s| s.to_string()).collect();
                args.extend(["-i", "pipe:0", "-an", "-sn", "-vf", &filter].iter().map(|s| s.to_string()));
                args.extend(tail.iter().map(|s| s.to_string()));
                Some((args, idx, offset))
            });
            let (args, feed) = match piped {
                Some((args, idx, offset)) => (args, Some((idx, offset))),
                None => {
                    // Any other kind of file: the decoder opens the address itself.
                    let secs = format!("{:.3}", start_ms as f64 / 1000.0);
                    let mut args: Vec<String> = common.iter().map(|s| s.to_string()).collect();
                    args.extend(["-nostdin", "-protocol_whitelist", "https,tls,tcp,crypto", "-reconnect", "1", "-reconnect_streamed", "1", "-reconnect_delay_max", "2"].iter().map(|s| s.to_string()));
                    args.extend(["-ss", &secs, "-i", &url, "-an", "-sn", "-vf", &scale].iter().map(|s| s.to_string()));
                    args.extend(tail.iter().map(|s| s.to_string()));
                    (args, None)
                }
            };
            let stdin = if feed.is_some() { Stdio::piped() } else { Stdio::null() };
            let (mut out, input) = {
                let mut guard = slot.lock().unwrap();
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                let spawn = |cmd: &mut std::process::Command, stdin: Stdio| cmd.args(&args).stdin(stdin).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
                // The picture runs at a lower CPU priority than the sound and the window, so decoding a big
                // video can never make the audio stutter. Falls back to plain ffmpeg if `nice` isn't there.
                let started = match spawn(crate::sys::cmd("nice").args(["-n", "10"]).arg(crate::sys::ffmpeg()), if feed.is_some() { Stdio::piped() } else { Stdio::null() }) {
                    Ok(c) => Ok(c),
                    Err(_) => spawn(&mut crate::sys::cmd(crate::sys::ffmpeg()), stdin),
                };
                let mut c = match started {
                    Ok(c) => c,
                    Err(e) => {
                        *err.lock().unwrap() = Some(format!("Couldn't start ffmpeg ({e}); it is needed to show music videos"));
                        return;
                    }
                };
                let Some(out) = c.stdout.take() else { return };
                let input = c.stdin.take();
                *guard = Some(c);
                (out, input)
            };
            if let (Some((idx, offset)), Some(mut input)) = (feed, input) {
                let (url, stop) = (url.clone(), stop.clone());
                std::thread::spawn(move || {
                    const PIECE: u64 = 1024 * 1024;
                    if input.write_all(&idx.init).is_err() {
                        return;
                    }
                    let mut at = offset;
                    while at < idx.total && !stop.load(Ordering::SeqCst) {
                        let to = (at + PIECE).min(idx.total) - 1;
                        // A piece that fails is asked for again (twice) before the picture gives up.
                        let mut got = None;
                        for attempt in 0..3 {
                            match range(&url, at, to) {
                                Ok((buf, _)) if buf.len() as u64 == to - at + 1 => {
                                    got = Some(buf);
                                    break;
                                }
                                _ => std::thread::sleep(Duration::from_millis(250 * (attempt + 1))),
                            }
                            if stop.load(Ordering::SeqCst) {
                                return;
                            }
                        }
                        let Some(buf) = got else { return };
                        // Waits here while the decoder is ahead; fails when the decoder is gone.
                        if input.write_all(&buf).is_err() {
                            return;
                        }
                        at = to + 1;
                    }
                });
            }
            loop {
                let mut buf = vec![0u8; size];
                if out.read_exact(&mut buf).is_err() {
                    break;
                }
                if tx.send(buf).is_err() {
                    break;
                }
            }
        });
        Ok(VideoStream { child, cancel, error, rx, base_ms: start_ms, count: 0, pending: None, w, h, ended: false })
    }

    /// Why the picture couldn't be started (the decoder is missing), if that is what happened.
    pub fn failure(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }

    fn next_pts(&self) -> u64 {
        self.base_ms + self.count * 1000 / FPS
    }

    /// The frame for the audio position `pos_ms`.
    pub fn poll(&mut self, pos_ms: u64) -> Poll {
        // How far behind the position a frame may be and still be shown. While the decoder is
        // catching up (a cold start, a switch to video, a stall), the frames it races through are
        // dropped without being shown: showing each would fast-forward the picture across the
        // screen. The picture appears once the decoder reaches the part that is playing.
        const STALE_MS: u64 = 500;
        let mut latest = None;
        loop {
            let f = match self.pending.take() {
                Some(f) => f,
                None => match self.rx.try_recv() {
                    Ok(f) => f,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.ended = true;
                        break;
                    }
                },
            };
            let pts = self.next_pts();
            if pts <= pos_ms {
                self.count += 1;
                if pos_ms - pts <= STALE_MS {
                    latest = Some(f);
                }
            } else {
                self.pending = Some(f);
                break;
            }
        }
        if let Some(f) = latest {
            return Poll::Frame(f);
        }
        let next = self.next_pts();
        // Far ahead of the audio (a seek backwards) or far behind it (a seek forwards).
        if next > pos_ms + 1500 {
            return Poll::Restart;
        }
        // Behind the audio by 3 s once pictures were flowing = can't keep up. Before the first picture the
        // stream is only starting (connecting, finding the spot, decoding the first keyframe — slower
        // for bigger pictures), so give it far longer instead of mistaking that for a slow connection.
        let behind = if self.count > 0 { next + 3000 < pos_ms } else { self.base_ms + 20_000 < pos_ms };
        if !self.ended && behind {
            return Poll::Lag;
        }
        Poll::Wait
    }
}
