//! Music-video pictures: ffmpeg decodes the video stream into raw frames, and the app shows the frame
//! that belongs to the audio position. The sound itself comes from the normal audio engine, so
//! pausing, seeking and the volume all keep working as they do for songs.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};

pub const FPS: u64 = 24;

pub struct VideoStream {
    child: Child,
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
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub enum Poll {
    /// The newest frame that is due (RGBA, w*h*4 bytes).
    Frame(Vec<u8>),
    /// Nothing new yet.
    Wait,
    /// The stream is far ahead of the audio position (after a seek back): start it again at the position.
    Restart,
    /// The stream can't keep up (slow connection or machine): start again at the position, lower quality.
    Lag,
}

impl VideoStream {
    /// Start decoding `url` (a direct video address) from `start_ms`, scaled to fit `w` x `h`.
    pub fn start(url: &str, start_ms: u64, w: usize, h: usize) -> Result<VideoStream, String> {
        // The picture is never bigger than 1920x1080 (1080p).
        let (w, h) = (w.clamp(2, 1920), h.clamp(2, 1080));
        // Only a web address, and only web protocols: ffmpeg could otherwise be made to read local files.
        if !url.starts_with("https://") {
            return Err("unexpected video address".into());
        }
        let secs = format!("{:.3}", start_ms as f64 / 1000.0);
        let filter = format!("fps={FPS},scale={w}:{h}:flags=fast_bilinear:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:black");
        let args: Vec<String> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-protocol_whitelist", "https,tls,tcp,crypto",
            "-reconnect", "1", "-reconnect_streamed", "1", "-reconnect_delay_max", "2",
            "-probesize", "1000000", "-analyzeduration", "500000", "-threads", "3",
            "-ss", &secs, "-i", url,
            "-an", "-sn", "-vf", &filter, "-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1"]
            .iter().map(|s| s.to_string()).collect();
        let spawn = |cmd: &mut Command| cmd.args(&args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
        // The picture runs at a lower CPU priority than the sound and the window, so decoding a big
        // video can never make the audio stutter. Falls back to plain ffmpeg if `nice` isn't there.
        let mut child = spawn(crate::sys::cmd("nice").args(["-n", "10"]).arg(crate::sys::ffmpeg()))
            .or_else(|_| spawn(&mut crate::sys::cmd(crate::sys::ffmpeg())))
            .map_err(|e| format!("Couldn't start ffmpeg ({e}); it is needed to show music videos"))?;
        let mut out = child.stdout.take().ok_or("no video output")?;
        // A short queue: ffmpeg waits when it is full, so only a moment of video is read ahead.
        let size = w * h * 4;
        // About 20 MB of read-ahead at most (a big picture needs fewer frames queued).
        let (tx, rx) = sync_channel::<Vec<u8>>((20 * 1024 * 1024 / size.max(1)).clamp(2, 8));
        std::thread::spawn(move || loop {
            let mut buf = vec![0u8; size];
            if out.read_exact(&mut buf).is_err() {
                break;
            }
            if tx.send(buf).is_err() {
                break;
            }
        });
        Ok(VideoStream { child, rx, base_ms: start_ms, count: 0, pending: None, w, h, ended: false })
    }

    fn next_pts(&self) -> u64 {
        self.base_ms + self.count * 1000 / FPS
    }

    /// The frame for the audio position `pos_ms`.
    pub fn poll(&mut self, pos_ms: u64) -> Poll {
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
            if self.next_pts() <= pos_ms {
                self.count += 1;
                latest = Some(f);
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
