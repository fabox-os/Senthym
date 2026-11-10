//! Senthym — a desktop music player for Linux, built with Rust and egui.
//!
//! Run normally, or with `--screenshot <file.png>` to draw about 120 frames, save a
//! picture of the window and exit.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod audio;
mod fallback;
mod helper;
mod images;
mod local;
mod lyrics;
mod model;
mod providers;
mod spotify;
mod store;
mod sys;
mod taste;
mod ui;
mod update;
mod video;
mod ytm;

use eframe::egui;

struct Gui {
    app: app::App,
    screenshot: Option<std::path::PathBuf>,
    frames: u32,
    requested: bool,
    wait_for_play: bool,
    /// Test hooks: play this long before the screenshot, and quality changes at given positions.
    play_wait_ms: u64,
    /// Test hook: check for an update at once and install it without a click.
    test_update: bool,
    test_quality: Vec<(u64, u32)>,
    started: std::time::Instant,
}

impl eframe::App for Gui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.images.poll(ctx);
        self.app.update();
        ui::draw(ctx, &mut self.app);
        self.app.apply_actions();
        if self.app.status.playing || self.app.loading {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }

        if self.test_update {
            match &self.app.update {
                app::UpdateState::Idle => self.app.actions.push(app::Action::CheckUpdate),
                app::UpdateState::Available(_) => self.app.actions.push(app::Action::DownloadUpdate),
                _ => {}
            }
        }
        let pos = self.app.status.position_ms;
        while self.test_quality.first().map(|(at, _)| *at <= pos).unwrap_or(false) {
            let (_, h) = self.test_quality.remove(0);
            if h >= 10_000 {
                // Values from 10000 up are a position to seek to (ms), to test the picture after a seek.
                eprintln!("[senthym] test: seeking to {h} ms at {pos} ms");
                self.app.actions.push(app::Action::Seek(h as u64));
            } else if h <= 1 {
                // 0 / 1: switch to the audio / video version, to test the picture across the switch.
                eprintln!("[senthym] test: switching to {} at {pos} ms", if h == 1 { "video" } else { "audio" });
                self.app.actions.push(app::Action::SetVideoMode(h == 1));
            } else {
                eprintln!("[senthym] test: asking for {h}p at {pos} ms");
                self.app.actions.push(app::Action::SetVideoHeight(h));
            }
        }
        if let Some(path) = self.screenshot.clone() {
            self.frames += 1;
            ctx.request_repaint();
            // Wait for the home feed (or 10 s), then let ~120 frames draw before the picture.
            let mut feed_ready = !matches!(self.app.home, app::Loadable::Loading);
            if self.app.autoplay_query.is_some() || self.app.loading || self.app.test_seek_end || self.app.test_wait_fade || (self.wait_for_play && self.app.status.position_ms < self.play_wait_ms) {
                feed_ready = false; // with --play, wait until the song has played a few seconds
            }
            let waited = self.started.elapsed() > std::time::Duration::from_secs(if self.wait_for_play { 120 } else { 10 });
            if !self.requested && self.frames >= 120 && (feed_ready || waited) && self.started.elapsed() > std::time::Duration::from_millis(2500) {
                self.requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            }
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = shot {
                let [w, h] = img.size;
                let mut buf = Vec::with_capacity(w * h * 4);
                for p in &img.pixels {
                    buf.extend_from_slice(&[p.r(), p.g(), p.b(), 255]);
                }
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match image::RgbaImage::from_raw(w as u32, h as u32, buf).map(|i| i.save(&path)) {
                    Some(Ok(())) => println!("saved screenshot to {}", path.display()),
                    _ => eprintln!("couldn't save the screenshot"),
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if self.started.elapsed() > std::time::Duration::from_secs(150) {
                eprintln!("screenshot timed out");
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

/// Write every panic (with where it happened and a backtrace) to crash.log in the data folder, so a crash
/// leaves something to read. The usual message still goes to the terminal.
fn install_crash_log() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let path = store::data_dir().join("crash.log");
        // Keep the file small: start over once it is big.
        if std::fs::metadata(&path).map(|m| m.len() > 256 * 1024).unwrap_or(false) {
            let _ = std::fs::remove_file(&path);
        }
        let when = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let thread = std::thread::current();
        let text = format!("--- unix time {when}, thread {:?}\n{info}\n{}\n", thread.name().unwrap_or("?"), std::backtrace::Backtrace::force_capture());
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            use std::io::Write;
            let _ = f.write_all(text.as_bytes());
        }
        default(info);
    }));
}

fn main() -> eframe::Result<()> {
    install_crash_log();
    sys::clean_old_update();
    let args: Vec<String> = std::env::args().collect();
    if let Some(f) = arg_value(&args, "--decode-test") {
        // Developer check: decode a whole file and report its length.
        match audio::decode::open(&f, 48000, 0) {
            Ok(mut src) => {
                let mut n = 0usize;
                let mut chunks = 0;
                while let Some(c) = src.next_chunk() {
                    n += c.len() / 2;
                    chunks += 1;
                }
                println!("decoded {} s in {chunks} chunks (reported {} ms)", n / 48000, src.duration_ms());
            }
            Err(e) => println!("error: {e}"),
        }
        return Ok(());
    }
    if let Some(b) = arg_value(&args, "--debug-google") {
        // Developer check: connect the Google account from a browser and list its library.
        println!("connect: {:?}", providers::connect_google(&b));
        match providers::soundcloud_search("kygo firestone") {
            Ok(v) => println!("soundcloud: {} results, first {:?}", v.len(), v.first().map(|i| i.title.clone())),
            Err(e) => println!("soundcloud error: {e}"),
        }
        match providers::deezer_search("kygo firestone") {
            Ok(v) => println!("deezer: {} results, first {:?}", v.len(), v.first().map(|i| i.title.clone())),
            Err(e) => println!("deezer error: {e}"),
        }
        match providers::google_library() {
            Ok(l) => println!("liked {} | playlists {} | albums {} | artists {}", l.liked.len(), l.playlists.len(), l.albums.len(), l.artists.len()),
            Err(e) => println!("library error: {e}"),
        }
        return Ok(());
    }
    if args.iter().any(|a| a == "--debug-update") {
        // Developer check: install the newest yt-dlp (with its checksum check).
        println!("{:?}", helper::install_latest());
        return Ok(());
    }
    if let Some(spec) = arg_value(&args, "--debug-video") {
        // Developer check: "video id|height" → resolve the picture's address and decode two seconds.
        let p: Vec<&str> = spec.split('|').collect();
        let id = p.first().unwrap_or(&"").to_string();
        let h: u32 = p.get(1).and_then(|v| v.parse().ok()).unwrap_or(720);
        let t0 = std::time::Instant::now();
        match helper::video_url(&id, h) {
            Ok(url) => {
                println!("address in {} ms", t0.elapsed().as_millis());
                let (w, hh) = ((h as usize * 16 / 9) & !1, h as usize);
                let t1 = std::time::Instant::now();
                match video::VideoStream::start(&url, 20_000, w, hh) {
                    Ok(mut st) => {
                        let mut frames = 0;
                        let mut pos = 20_000u64;
                        while t1.elapsed().as_secs() < 6 && frames < 48 {
                            if let video::Poll::Frame(_) = st.poll(pos) {
                                frames += 1;
                            }
                            pos = 20_000 + t1.elapsed().as_millis() as u64;
                            std::thread::sleep(std::time::Duration::from_millis(5));
                        }
                        println!("{w}x{hh}: first frames after {} ms, {frames} frame batches in {} ms", t1.elapsed().as_millis().min(9999), t1.elapsed().as_millis());
                    }
                    Err(e) => println!("ffmpeg: {e}"),
                }
            }
            Err(e) => println!("address error: {e}"),
        }
        return Ok(());
    }
    if let Some(kind) = arg_value(&args, "--debug-update-check") {
        // Developer check: look for a release as an older version (SENTHYM_FAKE_VERSION), for the package
        // kind named (flatpak, appimage, deb, rpm, windows, macos), and download it (SENTHYM_UPDATE_DIR).
        let k = match kind.as_str() {
            "flatpak" => update::Kind::Flatpak,
            "appimage" => update::Kind::AppImage,
            "deb" => update::Kind::Deb,
            "rpm" => update::Kind::Rpm,
            "windows" => update::Kind::Windows,
            "macos" => update::Kind::MacOs,
            _ => update::Kind::detect(),
        };
        println!("installed as {k:?}, version {}", update::current());
        match update::check(&k) {
            Ok(Some(rel)) => {
                println!("newer release {} -> {:?}", rel.version, rel.asset);
                if args.iter().any(|a| a == "--download") {
                    let got = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
                    let t = std::time::Instant::now();
                    println!("download: {:?} in {} ms ({} bytes read)", update::download(&rel, got.clone()), t.elapsed().as_millis(), got.load(std::sync::atomic::Ordering::Relaxed));
                }
            }
            Ok(None) => println!("up to date"),
            Err(e) => println!("error: {e}"),
        }
        return Ok(());
    }
    if let Some(spec) = arg_value(&args, "--debug-fallback") {
        // Developer check: "video id|title|artist|duration_ms" -> where the sound comes from when the id fails.
        let p: Vec<&str> = spec.split('|').collect();
        let t = model::Track {
            source: model::Source::YouTube(p.first().unwrap_or(&"").to_string()),
            title: p.get(1).unwrap_or(&"").to_string(),
            artist: p.get(2).unwrap_or(&"").to_string(),
            artist_id: None,
            album: String::new(),
            album_id: None,
            duration_ms: p.get(3).and_then(|d| d.parse().ok()).unwrap_or(0),
            thumb: None,
        };
        let at = std::time::Instant::now();
        println!("soundcloud matches: {:?}", fallback::soundcloud_matches(&t));
        println!("soundcloud alone: {:?}", fallback::from_soundcloud(&t, store::Quality::Normal));
        let (r, alt) = fallback::fetch_song(&t, p.first().unwrap_or(&""), store::Quality::Normal);
        println!("result: {r:?}, other source: {alt:?}, {} ms", at.elapsed().as_millis());
        return Ok(());
    }
    if let Some(spec) = arg_value(&args, "--debug-taste") {
        // Developer check: "song one|song two|..." (newest choice first) -> the blended autoplay.
        let seeds: Vec<&str> = spec.split('|').filter(|s| !s.trim().is_empty()).collect();
        let at = std::time::Instant::now();
        let mut lists: Vec<Vec<model::Track>> = Vec::new();
        for q in &seeds {
            let found = ytm::search(q, Some(ytm::FILTER_SONGS)).ok().and_then(|v| v.into_iter().find_map(|i| i.track));
            let Some(t) = found else {
                println!("seed {q:?}: not found");
                continue;
            };
            let list = t.video_id().and_then(|id| ytm::radio(id, None).ok()).unwrap_or_default();
            println!("seed {:?} by {} -> {} similar songs", t.title, t.artist, list.len());
            lists.push(list);
        }
        let out = taste::blend(&lists, 20);
        println!("\nblended autoplay ({} songs, {} ms):", out.len(), at.elapsed().as_millis());
        for (i, t) in out.iter().enumerate() {
            println!("{:2}. {} — {}", i + 1, t.title, t.artist);
        }
        return Ok(());
    }
    if let Some(q) = arg_value(&args, "--debug-dedupe") {
        // Developer check: a search with all sources, before and after removing repeated songs.
        let (q, filter) = match q.strip_prefix("songs:") {
            Some(rest) => (rest.to_string(), Some(ytm::FILTER_SONGS)),
            None => (q, None),
        };
        let mut v = ytm::search(&q, filter).unwrap_or_default();
        v.extend(providers::deezer_search(&q).unwrap_or_default());
        v.extend(providers::soundcloud_search(&q).unwrap_or_default());
        let before: Vec<String> = v.iter().filter(|i| i.kind == model::ItemKind::Song).map(|i| format!("{} | {} | {}", i.title, i.subtitle, i.track.as_ref().map(|t| t.duration_ms / 1000).unwrap_or(0))).collect();
        fallback::dedupe_songs(&mut v);
        let after: Vec<String> = v.iter().filter(|i| i.kind == model::ItemKind::Song).map(|i| format!("{} | {} | {}", i.title, i.subtitle, i.track.as_ref().map(|t| t.duration_ms / 1000).unwrap_or(0))).collect();
        println!("songs before: {}, after: {}", before.len(), after.len());
        for b in &before {
            println!("{} {b}", if after.contains(b) { "keep" } else { "DROP" });
        }
        return Ok(());
    }
    if let Some(spec) = arg_value(&args, "--debug-loudness") {
        // Developer check: "video-sound file|audio-version file" -> measured levels and the fixed gain.
        let p: Vec<&str> = spec.split('|').collect();
        let t = std::time::Instant::now();
        let (v, r) = (p.first().copied().unwrap_or(""), p.get(1).copied().unwrap_or(""));
        println!("video {:?} dB, reference {:?} dB", audio::loudness::measure(v), audio::loudness::measure(r));
        let g = audio::loudness::match_gain(v, r);
        println!("gain x{g:.3} ({:+.2} dB), measured in {} ms", 20.0 * g.log10(), t.elapsed().as_millis());
        return Ok(());
    }
    if let Some(spec) = arg_value(&args, "--debug-audio") {
        // Developer check: "video id|title|artist|duration_ms" → the id that would be played.
        let p: Vec<&str> = spec.split('|').collect();
        let id = p.first().unwrap_or(&"").to_string();
        let t = model::Track {
            source: model::Source::YouTube(id.clone()),
            title: p.get(1).unwrap_or(&"").to_string(),
            artist: p.get(2).unwrap_or(&"").to_string(),
            artist_id: None,
            album: String::new(),
            album_id: None,
            duration_ms: p.get(3).and_then(|d| d.parse().ok()).unwrap_or(0),
            thumb: None,
        };
        println!("{id} -> {}", ytm::audio_id_for(&t, &id));
        println!("video: {:?}", ytm::video_id_for(&t, &id));
        if let Ok(items) = ytm::search(&format!("{} {} official video", t.title, t.artist), Some(ytm::FILTER_VIDEOS)) {
            for i in items.iter().take(6) {
                println!("  {} | {} | {} | {:?}", i.id, i.title, i.subtitle, ytm::video_type(&i.id));
            }
        }
        println!("cands: {:?}", spotify::candidates(&t, true));
        return Ok(());
    }
    if let Some(spec) = arg_value(&args, "--debug-match") {
        // Developer check: "title|artist|album|duration_ms" → YouTube Music match and lyrics.
        let p: Vec<&str> = spec.split('|').collect();
        let t = model::Track {
            source: model::Source::Spotify("test".into()),
            title: p.first().unwrap_or(&"").to_string(),
            artist: p.get(1).unwrap_or(&"").to_string(),
            artist_id: None,
            album: p.get(2).unwrap_or(&"").to_string(),
            album_id: None,
            duration_ms: p.get(3).and_then(|d| d.parse().ok()).unwrap_or(0),
            thumb: None,
        };
        let m = spotify::find_on_youtube(&t, true);
        println!("match: {m:?}");
        if let Ok(id) = &m {
            println!("video (cross-checked against Spotify's title/artist): {:?}", ytm::video_id_for(&t, id));
        }
        let l = lyrics::fetch(&t.title, &t.artist, &t.album, t.duration_ms);
        println!("lyrics: {}", l.map(|l| format!("{} lines, synced {}", l.lines.len(), l.synced)).unwrap_or_else(|| "none".into()));
        return Ok(());
    }
    if let Some(q) = arg_value(&args, "--debug-search") {
        // Developer check: print what a search returns.
        for filter in [None, Some(ytm::FILTER_SONGS)] {
            match ytm::search(&q, filter) {
                Ok(items) => {
                    for it in items.iter().take(6) {
                        let t = it.track.as_ref();
                        println!("{:?} {} | {} | artist={:?} album={:?}", it.kind, it.id, it.title, t.map(|t| &t.artist), t.map(|t| &t.album));
                    }
                }
                Err(e) => println!("error: {e}"),
            }
            println!("--");
        }
        if let Ok(r) = ytm::radio(&q, None) {
            for t in r.iter().take(3) {
                println!("radio: {} | {:?} | {:?}", t.title, t.artist, t.thumb);
            }
        }
        return Ok(());
    }
    let screenshot = args.iter().position(|a| a == "--screenshot").and_then(|i| args.get(i + 1)).map(std::path::PathBuf::from);
    // The window / taskbar icon: the Senthym logo, compiled into the program.
    let icon = image::load_from_memory(include_bytes!("../assets/icon-256.png")).ok().map(|i| {
        let i = i.to_rgba8();
        egui::IconData { width: i.width(), height: i.height(), rgba: i.into_raw() }
    });
    let mut viewport = egui::ViewportBuilder::default()
            .with_title("Senthym")
            .with_decorations(!sys::CUSTOM_FRAME)
            .with_app_id("senthym")
            .with_inner_size([1400.0, 880.0])
            .with_min_inner_size([960.0, 600.0]);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "Senthym",
        options,
        Box::new(move |cc| {
            ui::widgets::setup_style(&cc.egui_ctx);
            let shot_mode = screenshot.is_some();
            let mut app = app::App::new(cc.egui_ctx.clone(), shot_mode);
            let wait_for_play = arg_value(&args, "--play").is_some();
            if let Some(q) = arg_value(&args, "--play") {
                app.autoplay_query = Some(q);
            }
            if let Some(v) = arg_value(&args, "--volume").and_then(|v| v.parse::<f32>().ok()) {
                app.engine.set_volume(v);
                app.store.settings.volume = v;
            }
            if args.iter().any(|a| a == "--test-crossfade") {
                app.test_seek_end = true;
                app.test_wait_fade = true;
                app.store.settings.crossfade_ms = 6000;
                app.store.settings.crossfade_dj = true;
            }
            if let Some(img) = arg_value(&args, "--set-playlist-cover") {
                // Developer check: give the first playlist you made this picture as its cover.
                if let Some(p) = app.store.playlists.first() {
                    app.route = app::Route::UserPlaylist(p.id.clone());
                    app.actions.push(app::Action::UpdatePlaylist { pid: p.id.clone(), name: p.name.clone(), desc: "Test description".into(), cover: Some(Some(img)) });
                }
            }
            if args.iter().any(|a| a == "--edit-first-playlist") {
                // Developer check: open "Edit details" for the first playlist you made.
                if let Some(p) = app.store.playlists.first() {
                    app.route = app::Route::UserPlaylist(p.id.clone());
                    app.ui_edit_playlist = Some(app::EditPlaylist { pid: p.id.clone(), name: p.name.clone(), desc: p.description.clone(), new_cover: None, remove_cover: false });
                }
            }
            if let Some(p) = arg_value(&args, "--page") {
                app.route = match p.as_str() {
                    "search" => app::Route::Search,
                    "library" => app::Route::Library,
                    "player" => app::Route::Player,
                    "lyrics" => app::Route::Lyrics,
                    "queue" => app::Route::Queue,
                    "settings" => app::Route::Settings,
                    "liked" => app::Route::Liked,
                    "local" => app::Route::LocalFiles,
                    "history" => app::Route::History,
                    "downloads" => app::Route::Downloads,
                    "spotify" => app::Route::SpotifyLogin,
                    "playlist1" => app.store.playlists.first().map(|p| app::Route::UserPlaylist(p.id.clone())).unwrap_or(app::Route::Home),
                    other if other.starts_with("artist:") => app::Route::Artist(other["artist:".len()..].to_string()),
                    _ => app::Route::Home,
                };
            }
            Ok(Box::new(Gui {
                app,
                screenshot,
                frames: 0,
                requested: false,
                wait_for_play,
                test_update: args.iter().any(|a| a == "--test-update"),
                play_wait_ms: arg_value(&args, "--play-seconds").and_then(|v| v.parse::<u64>().ok()).map(|v| v * 1000).unwrap_or(3000),
                // "--test-quality 8000:240,16000:1080": ask for a quality when the song reaches a position.
                test_quality: arg_value(&args, "--test-quality")
                    .map(|v| v.split(',').filter_map(|p| p.split_once(':')).filter_map(|(at, h)| Some((at.parse().ok()?, h.parse().ok()?))).collect())
                    .unwrap_or_default(),
                started: std::time::Instant::now(),
            }))
        }),
    )
}
