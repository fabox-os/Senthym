//! Getting the audio of a YouTube Music song onto disk, using the open-source yt-dlp tool.
//! The app keeps its own up-to-date copy of yt-dlp (YouTube changes often and old copies
//! stop working), and falls back to one installed on the system.

use crate::store::{cache_dir, data_dir, Quality};
use std::path::{Path, PathBuf};
use std::sync::Mutex;


const SUMS_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS";
/// Size limit of the song cache, in bytes (estimated; the original uses ExoPlayer's cache).
pub const CACHE_LIMIT_BYTES: u64 = 1024 * 1024 * 1024;

static INSTALL_LOCK: Mutex<()> = Mutex::new(());

pub fn managed_path() -> PathBuf {
    data_dir().join("bin").join(crate::sys::ENGINE_FILE)
}

fn version_of(bin: &Path) -> Option<String> {
    let out = crate::sys::cmd(bin).arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The yt-dlp to use and its version, if any is available.
pub fn find() -> Option<(PathBuf, String)> {
    let managed = managed_path();
    if managed.exists() {
        if let Some(v) = version_of(&managed) {
            return Some((managed, v));
        }
    }
    let sys = PathBuf::from("yt-dlp");
    version_of(&sys).map(|v| (sys, v))
}

/// Download the latest yt-dlp release into the app's folder.
pub fn install_latest() -> Result<String, String> {
    let _g = INSTALL_LOCK.lock().unwrap();
    let dest = managed_path();
    let _ = std::fs::create_dir_all(dest.parent().unwrap());
    let resp = ureq::get(&format!("https://github.com/yt-dlp/yt-dlp/releases/latest/download/{}", crate::sys::ENGINE_ASSET))
        .timeout(std::time::Duration::from_secs(120))
        .call()
        .map_err(|e| format!("Couldn't download the Senthym streaming engine ({e})"))?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(resp.into_reader(), 80 * 1024 * 1024), &mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() < 100_000 {
        return Err("The Senthym streaming engine download looks broken".into());
    }
    // The program is run on this computer, so it is only installed when it matches the checksum that
    // the project publishes next to it.
    let sums = ureq::get(SUMS_URL)
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .map_err(|e| format!("Couldn't check the Senthym streaming engine download ({e})"))?
        .into_string()
        .map_err(|e| e.to_string())?;
    let expected = sums
        .lines()
        .find_map(|l| {
            let mut it = l.split_whitespace();
            match (it.next(), it.next()) {
                (Some(h), Some(n)) if n == crate::sys::ENGINE_ASSET => Some(h.to_lowercase()),
                _ => None,
            }
        })
        .ok_or("The Senthym streaming engine's checksum wasn't found")?;
    let got: String = {
        use sha2::Digest;
        sha2::Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect()
    };
    if got != expected {
        return Err("The Senthym streaming engine download doesn't match its checksum, so it was not installed".into());
    }
    let tmp = dest.with_extension("part");
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
    }
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    version_of(&dest).ok_or_else(|| "The Senthym streaming engine won't start".into())
}

/// The managed copy, installing it on first use.
fn ensure() -> Result<PathBuf, String> {
    // Starting yt-dlp just to read its version costs about half a second, so do it once per run.
    static VERIFIED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let managed = managed_path();
    if managed.exists() && VERIFIED.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(managed);
    }
    if managed.exists() && version_of(&managed).is_some() {
        VERIFIED.store(true, std::sync::atomic::Ordering::Relaxed);
        return Ok(managed);
    }
    match install_latest() {
        Ok(_) => Ok(managed),
        Err(e) => find().map(|(p, _)| p).ok_or(e),
    }
}

fn audio_cache() -> PathBuf {
    let d = cache_dir().join("audio");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// A song already in the cache, if any.
pub fn cached(video_id: &str) -> Option<PathBuf> {
    let dir = audio_cache();
    for ext in ["m4a", "webm", "opus", "mp4", "mp3"] {
        let p = dir.join(format!("{video_id}.{ext}"));
        if p.exists() && std::fs::metadata(&p).map(|m| m.len() > 10_000).unwrap_or(false) {
            return Some(p);
        }
        if p.exists() {
            let _ = std::fs::remove_file(&p);
        }
    }
    None
}

/// Remove sign-in copies left behind if the app was closed while a download was running.
pub fn cleanup_temp() {
    if let Ok(rd) = std::fs::read_dir(cache_dir()) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with("yt-login-") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// Use the YouTube sign-in saved in this browser (firefox, chrome, chromium, brave, edge…).
static BROWSER: Mutex<String> = Mutex::new(String::new());

pub fn set_browser(name: &str) {
    *BROWSER.lock().unwrap() = name.to_string();
}

/// The browsers the sign-in can be read from, in the order they are offered (the names the helper uses).
pub const KNOWN_BROWSERS: &[&str] = if cfg!(target_os = "macos") {
    &["safari", "chrome", "firefox", "edge", "brave", "vivaldi", "opera", "chromium"]
} else {
    &["chrome", "edge", "firefox", "brave", "vivaldi", "opera", "chromium"]
};

/// What this computer has: the browsers the helper can read from, and the one that is the system's default.
pub struct Browsers {
    pub installed: Vec<&'static str>,
    pub default: Option<&'static str>,
}

/// Looked up once (it asks the system a few questions), the first time it is needed.
pub fn browsers() -> &'static Browsers {
    static B: std::sync::OnceLock<Browsers> = std::sync::OnceLock::new();
    B.get_or_init(|| {
        let installed = detect_installed();
        let default = crate::sys::default_browser().filter(|d| KNOWN_BROWSERS.contains(d));
        Browsers { installed, default }
    })
}

/// The browser to suggest when none was chosen yet: the system's default, else the first one found.
pub fn suggested_browser() -> Option<&'static str> {
    let b = browsers();
    b.default.or_else(|| b.installed.first().copied())
}

/// Where each browser keeps its profile on this system (a browser counts as installed when it exists).
fn browser_profile_dirs() -> Vec<(&'static str, Vec<std::path::PathBuf>)> {
    #[cfg(not(target_os = "windows"))]
    let home = dirs::home_dir().unwrap_or_default();
    #[cfg(target_os = "windows")]
    {
        let local = dirs::data_local_dir().unwrap_or_default();
        let roaming = dirs::data_dir().unwrap_or_default();
        vec![
            ("chrome", vec![local.join("Google/Chrome/User Data")]),
            ("edge", vec![local.join("Microsoft/Edge/User Data")]),
            ("firefox", vec![roaming.join("Mozilla/Firefox/Profiles")]),
            ("brave", vec![local.join("BraveSoftware/Brave-Browser/User Data")]),
            ("vivaldi", vec![local.join("Vivaldi/User Data")]),
            ("opera", vec![roaming.join("Opera Software/Opera Stable")]),
            ("chromium", vec![local.join("Chromium/User Data")]),
        ]
    }
    #[cfg(target_os = "macos")]
    {
        let base = home.join("Library/Application Support");
        vec![
            ("safari", vec![home.join("Library/Cookies/Cookies.binarycookies"), home.join("Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies")]),
            ("chrome", vec![base.join("Google/Chrome")]),
            ("firefox", vec![base.join("Firefox/Profiles")]),
            ("edge", vec![base.join("Microsoft Edge")]),
            ("brave", vec![base.join("BraveSoftware/Brave-Browser")]),
            ("vivaldi", vec![base.join("Vivaldi")]),
            ("opera", vec![base.join("com.operasoftware.Opera")]),
            ("chromium", vec![base.join("Chromium")]),
        ]
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let cfg = dirs::config_dir().unwrap_or_default();
        vec![
            ("chrome", vec![cfg.join("google-chrome")]),
            ("edge", vec![cfg.join("microsoft-edge")]),
            ("firefox", vec![home.join(".mozilla/firefox"), home.join("snap/firefox/common/.mozilla/firefox"), home.join(".var/app/org.mozilla.firefox")]),
            ("brave", vec![cfg.join("BraveSoftware/Brave-Browser")]),
            ("vivaldi", vec![cfg.join("vivaldi")]),
            ("opera", vec![cfg.join("opera")]),
            ("chromium", vec![cfg.join("chromium")]),
        ]
    }
}

fn detect_installed() -> Vec<&'static str> {
    let dirs = browser_profile_dirs();
    KNOWN_BROWSERS.iter().copied().filter(|name| dirs.iter().any(|(n, paths)| n == name && paths.iter().any(|p| p.exists()))).collect()
}

/// A short, friendly name for a browser.
pub fn browser_label(name: &str) -> &'static str {
    match name {
        "chrome" => "Google Chrome",
        "chromium" => "Chromium",
        "edge" => "Microsoft Edge",
        "firefox" => "Firefox",
        "brave" => "Brave",
        "vivaldi" => "Vivaldi",
        "opera" => "Opera",
        "safari" => "Safari",
        _ => "Browser",
    }
}

/// Words that help when reading the sign-in from a browser fails, by what went wrong.
pub fn browser_help(browser: &str, raw: &str) -> String {
    let name = browser_label(browser);
    let low = raw.to_lowercase();
    let chromium_family = matches!(browser, "chrome" | "edge" | "brave" | "vivaldi" | "opera" | "chromium");
    if low.contains("could not copy") || low.contains("database is locked") || low.contains("permission denied") || low.contains("being used by another process") {
        return format!("{name} is still open and keeps its sign-in locked. Close {name} completely (also from the tray icon near the clock) and press Connect again.");
    }
    if low.contains("dpapi") || low.contains("decrypt") || low.contains("app-bound") || low.contains("app bound") {
        return if cfg!(windows) && chromium_family {
            format!("{name} on Windows protects its sign-in so other programs can't read it. Pick Firefox instead (sign in to music.youtube.com there once), or use Senthym without an account.")
        } else {
            format!("Senthym couldn't decrypt {name}'s sign-in ({raw}).")
        };
    }
    if low.contains("could not find") || low.contains("no such file") || low.contains("not found") || low.contains("unsupported browser") {
        return format!("Senthym couldn't find a {name} profile on this computer. Pick the browser you use, and sign in to music.youtube.com in it first.");
    }
    if low.contains("keyring") || low.contains("secretstorage") || low.contains("kwallet") {
        return format!("{name}'s sign-in is stored in your system keyring, which Senthym couldn't open. Unlock the keyring and try again.");
    }
    format!("Couldn't read your Google sign-in from {name}: {raw}")
}

/// JavaScript runtimes found on this computer. yt-dlp needs one to solve YouTube's signature
/// challenge; without it, signed-in requests come back with only pictures ("Requested format is
/// not available"). yt-dlp enables deno by default; node and bun have to be named.
fn js_runtimes() -> &'static Vec<&'static str> {
    static FOUND: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    FOUND.get_or_init(|| {
        ["deno", "node", "bun"]
            .into_iter()
            .filter(|n| crate::sys::cmd(n).arg("--version").output().map(|o| o.status.success()).unwrap_or(false))
            .collect()
    })
}

/// The YouTube sign-in handed to yt-dlp. The app's own saved copy of the Google sign-in is used when
/// there is one (no browser to open, no lock on its cookie database, no password-keyring prompt on
/// every song); otherwise the browser the person picked. A private copy of the file is used per run
/// (yt-dlp rewrites the file it is given) and removed afterwards.
struct Login {
    args: Vec<String>,
    temp: Option<PathBuf>,
}

impl Drop for Login {
    fn drop(&mut self) {
        if let Some(t) = &self.temp {
            let _ = std::fs::remove_file(t);
        }
    }
}

fn login() -> Option<Login> {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    if crate::providers::google_connected() {
        let saved = data_dir().join("youtube-cookies.txt");
        if std::fs::metadata(&saved).map(|m| m.len() > 0).unwrap_or(false) {
            let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let copy = cache_dir().join(format!("yt-login-{}-{n}.txt", std::process::id()));
            if std::fs::copy(&saved, &copy).is_ok() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&copy, std::fs::Permissions::from_mode(0o600));
                }
                return Some(Login { args: vec!["--cookies".into(), copy.to_string_lossy().into_owned()], temp: Some(copy) });
            }
        }
    }
    let browser = BROWSER.lock().unwrap().clone();
    if browser.is_empty() {
        None
    } else {
        Some(Login { args: vec!["--cookies-from-browser".into(), browser], temp: None })
    }
}

/// The order to try things in: the plain request first (fastest, and enough for most songs, signed in
/// or not), then the browser's sign-in with the runtime (for age-restricted or bot-checked songs),
/// then plain with the runtime.
fn attempt_plan() -> Vec<(bool, bool)> {
    let signed_in = !BROWSER.lock().unwrap().is_empty() || crate::providers::google_connected();
    let mut plan = if signed_in { vec![(false, false), (true, true), (false, true)] } else { vec![(false, false), (false, true)] };
    // The way that worked last time goes first (for ten minutes), so a request isn't slowed by tries
    // that YouTube has just been refusing.
    if let Some((step, at)) = *LAST_OK.lock().unwrap() {
        if at.elapsed() < std::time::Duration::from_secs(600) {
            if let Some(i) = plan.iter().position(|p| *p == step) {
                let s = plan.remove(i);
                plan.insert(0, s);
            }
        }
    }
    plan
}

static LAST_OK: Mutex<Option<((bool, bool), std::time::Instant)>> = Mutex::new(None);

fn note_ok(step: (bool, bool)) {
    *LAST_OK.lock().unwrap() = Some((step, std::time::Instant::now()));
}

/// Try the fastest way that is likely to work first.
/// - Without an account: plain request, no JavaScript runtime (about twice as fast, since the
///   runtime's challenge solving is skipped); only if that fails, again with the runtime.
/// - With the browser's sign-in: sign-in plus runtime (YouTube hides the audio formats otherwise),
///   then the plain ways as a fallback.
fn run_ytdlp(bin: &Path, url: &str, quality: Quality, out_template: &str) -> Result<String, String> {
    let plan = attempt_plan();
    let mut first_err: Option<String> = None;
    let mut last_err = String::from("unknown error");
    for (i, (cookies, js)) in plan.iter().enumerate() {
        // The runtime step is pointless when none is installed.
        if *js && js_runtimes().is_empty() && i > 0 {
            continue;
        }
        let _ = i;
        match run_ytdlp_once(bin, url, quality, out_template, *cookies, *js) {
            Ok(p) => {
                note_ok((*cookies, *js));
                return Ok(p);
            }
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(e.clone());
                }
                last_err = e;
            }
        }
    }
    // Keep the sign-in advice when that is what the first attempt asked for.
    match first_err {
        Some(e) if e.contains("Sign in") || e.contains("cookies") => Err(e),
        _ => Err(last_err),
    }
}

fn run_ytdlp_once(bin: &Path, url: &str, quality: Quality, out_template: &str, with_cookies: bool, with_js: bool) -> Result<String, String> {
    let mut cmd = crate::sys::cmd(bin);
    if with_js {
        for rt in js_runtimes() {
            cmd.args(["--js-runtimes", rt]);
        }
    }
    let _login = if with_cookies { login() } else { None };
    if let Some(l) = &_login {
        cmd.args(&l.args);
    }
    let out = cmd
        .args(["-q", "--no-warnings", "--no-playlist", "--no-mtime"])
        .args(["-f", quality.ytdlp_format()])
        .args(["-o", out_template, "--print", "after_move:filepath", &url])
        .output()
        .map_err(|e| format!("Couldn't start the Senthym streaming engine ({e})"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let path = stdout.lines().last().unwrap_or("").trim().to_string();
    if out.status.success() && !path.is_empty() && Path::new(&path).exists() {
        return Ok(path);
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let msg = err.lines().filter(|l| l.contains("ERROR")).last().unwrap_or("unknown error").replace("ERROR: ", "");
    if msg.contains("Sign in to confirm") || msg.contains("age") && msg.contains("Sign in") || msg.contains("cookies") {
        return Err(format!("{msg} — open Settings › YouTube Music account and pick the browser where you're signed in to YouTube"));
    }
    Err(msg)
}


/// Ask yt-dlp only for the direct audio address (no download), using the same attempt order.
/// Returns (address, file extension).
fn resolve_direct(bin: &Path, page_url: &str, quality: Quality) -> Result<(String, String), String> {
    resolve_direct_fmt(bin, page_url, quality.ytdlp_format())
}

/// One try at finding the direct address, a given way (with / without the sign-in and the JS runtime).
fn attempt_direct(bin: &Path, page_url: &str, format: &str, cookies: bool, js: bool) -> Result<(String, String), String> {
    let mut cmd = crate::sys::cmd(bin);
    if js {
        for rt in js_runtimes() {
            cmd.args(["--js-runtimes", rt]);
        }
    }
    let _login = if cookies { login() } else { None };
    if let Some(l) = &_login {
        cmd.args(&l.args);
    }
    let out = cmd
        .args(["-q", "--no-warnings", "--no-playlist", "-f", format])
        .args(["--print", "%(url)s\t%(ext)s", page_url])
        .output()
        .map_err(|e| format!("Couldn't start the Senthym streaming engine ({e})"))?;
    if out.status.success() {
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(line) = text.lines().rev().find(|l| l.starts_with("http")) {
            let mut it = line.splitn(2, '\t');
            let url = it.next().unwrap_or("").to_string();
            let ext = it.next().unwrap_or("m4a").trim().to_string();
            if !url.is_empty() {
                note_ok((cookies, js));
                return Ok((url, if ext.is_empty() { "m4a".into() } else { ext }));
            }
        }
    }
    let err = String::from_utf8_lossy(&out.stderr);
    Err(err.lines().filter(|l| l.contains("ERROR")).last().unwrap_or("unknown error").replace("ERROR: ", ""))
}

/// Find the direct address. The two most likely ways are tried at the same time and the first answer
/// wins, so a way that YouTube is refusing right now doesn't hold up the one that works; any further
/// ways are tried one after another only if both fail.
fn resolve_direct_fmt(bin: &Path, page_url: &str, format: &str) -> Result<(String, String), String> {
    resolve_with(bin, page_url, format, attempt_direct)
}

/// Run one kind of lookup the quickest way that works: the two likeliest ways side by side, then the rest.
fn resolve_with<T: Send + 'static>(bin: &Path, page_url: &str, format: &str, attempt: fn(&Path, &str, &str, bool, bool) -> Result<T, String>) -> Result<T, String> {
    let plan: Vec<(bool, bool)> = attempt_plan().into_iter().filter(|(_, js)| !*js || !js_runtimes().is_empty()).collect();
    let mut last = String::from("unknown error");
    let racing = plan.len().min(2);
    if racing >= 2 {
        let (tx, rx) = std::sync::mpsc::channel();
        for &(cookies, js) in &plan[..racing] {
            let (tx, bin, url, fmt) = (tx.clone(), bin.to_path_buf(), page_url.to_string(), format.to_string());
            std::thread::spawn(move || {
                let _ = tx.send(attempt(&bin, &url, &fmt, cookies, js));
            });
        }
        drop(tx);
        for _ in 0..racing {
            match rx.recv() {
                Ok(Ok(r)) => return Ok(r),
                Ok(Err(e)) => last = e,
                Err(_) => break,
            }
        }
    }
    for &(cookies, js) in plan.iter().skip(if racing >= 2 { racing } else { 0 }) {
        match attempt(bin, page_url, format, cookies, js) {
            Ok(r) => return Ok(r),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Recent download speed in kilobits per second (0 = not measured yet). Used to pick a video quality
/// the connection can keep up with.
static MEASURED_KBPS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn measured_kbps() -> u64 {
    MEASURED_KBPS.load(std::sync::atomic::Ordering::Relaxed)
}

fn record_speed(bytes: usize, started: std::time::Instant) {
    let ms = started.elapsed().as_millis().max(1) as u64;
    // Small files are dominated by the first round trip; only trust bigger ones.
    if bytes < 1_500_000 {
        return;
    }
    let kbps = bytes as u64 * 8 / ms;
    let old = MEASURED_KBPS.load(std::sync::atomic::Ordering::Relaxed);
    let new = if old == 0 { kbps } else { (old * 2 + kbps) / 3 };
    MEASURED_KBPS.store(new, std::sync::atomic::Ordering::Relaxed);
}

/// Time a 2 MB piece of a direct address to learn how fast the connection is right now.
pub fn probe_speed(url: &str) {
    let started = std::time::Instant::now();
    let Ok(resp) = ureq::get(url).set("Range", "bytes=0-2097151").timeout(std::time::Duration::from_secs(15)).call() else { return };
    let mut buf = Vec::new();
    if std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf).is_ok() {
        record_speed(buf.len(), started);
    }
}

/// Download a direct address in parallel pieces (YouTube serves ranged requests far faster than one
/// long request). Returns the bytes.
fn download_parallel(url: &str) -> Result<Vec<u8>, String> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    const CHUNK: usize = 1024 * 1024;
    let get_range = |a: usize, b: usize| -> Result<(Vec<u8>, Option<usize>), String> {
        let resp = ureq::get(url)
            .set("Range", &format!("bytes={a}-{b}"))
            .timeout(std::time::Duration::from_secs(30))
            .call()
            .map_err(|e| e.to_string())?;
        let total = resp
            .header("Content-Range")
            .and_then(|v| v.rsplit('/').next())
            .and_then(|v| v.trim().parse::<usize>().ok());
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf).map_err(|e| e.to_string())?;
        Ok((buf, total))
    };
    let started = std::time::Instant::now();
    let (first, total) = get_range(0, CHUNK - 1)?;
    let total = total.ok_or("the server didn't say how big the song is")?;
    if total == 0 || total > 200 * 1024 * 1024 {
        return Err("unexpected file size".into());
    }
    if first.len() >= total {
        record_speed(first.len(), started);
        return Ok(first);
    }
    let chunks = (total + CHUNK - 1) / CHUNK;
    let results: Arc<Mutex<Vec<Option<Vec<u8>>>>> = Arc::new(Mutex::new(vec![None; chunks]));
    results.lock().unwrap()[0] = Some(first);
    let next = Arc::new(AtomicUsize::new(1));
    let failed: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    std::thread::scope(|sc| {
        for _ in 0..6.min(chunks) {
            let (results, next, failed) = (results.clone(), next.clone(), failed.clone());
            sc.spawn(move || loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= chunks || failed.lock().unwrap().is_some() {
                    break;
                }
                let a = i * CHUNK;
                let b = ((i + 1) * CHUNK).min(total) - 1;
                // One retry per piece.
                let r = get_range(a, b).or_else(|_| get_range(a, b));
                match r {
                    Ok((buf, _)) if buf.len() == b - a + 1 => results.lock().unwrap()[i] = Some(buf),
                    Ok(_) => *failed.lock().unwrap() = Some("a piece of the song came back short".into()),
                    Err(e) => *failed.lock().unwrap() = Some(e),
                }
            });
        }
    });
    if let Some(e) = failed.lock().unwrap().take() {
        return Err(e);
    }
    let mut out = Vec::with_capacity(total);
    for part in results.lock().unwrap().iter() {
        out.extend_from_slice(part.as_ref().ok_or("a piece of the song is missing")?);
    }
    record_speed(out.len(), started);
    Ok(out)
}

/// The picture qualities offered, best first.
pub const VIDEO_HEIGHTS: [u32; 5] = [1080, 720, 480, 360, 240];

/// Every address a lookup printed, in order.
fn attempt_lines(bin: &Path, page_url: &str, format: &str, cookies: bool, js: bool) -> Result<Vec<String>, String> {
    let mut cmd = crate::sys::cmd(bin);
    if js {
        for rt in js_runtimes() {
            cmd.args(["--js-runtimes", rt]);
        }
    }
    let _login = if cookies { login() } else { None };
    if let Some(l) = &_login {
        cmd.args(&l.args);
    }
    let out = cmd
        .args(["-q", "--no-warnings", "--no-playlist", "-f", format])
        .args(["--print", "%(url)s", page_url])
        .output()
        .map_err(|e| format!("Couldn't start the Senthym streaming engine ({e})"))?;
    let lines: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().filter(|l| l.starts_with("http")).map(String::from).collect();
    if out.status.success() && !lines.is_empty() {
        note_ok((cookies, js));
        return Ok(lines);
    }
    let err = String::from_utf8_lossy(&out.stderr);
    Err(err.lines().filter(|l| l.contains("ERROR")).last().unwrap_or("unknown error").replace("ERROR: ", ""))
}

/// The picture addresses of a video for every quality, from ONE lookup (it costs the same as asking
/// for one), so changing quality later needs no lookup at all.
pub fn video_urls(video_id: &str) -> Result<Vec<(u32, String)>, String> {
    let bin = ensure()?;
    let fmt = VIDEO_HEIGHTS.iter().map(|h| format!("bestvideo[height<={h}][vcodec^=avc1]/bestvideo[height<={h}]/worstvideo[height<=1080]")).collect::<Vec<_>>().join(",");
    let lines = resolve_with(&bin, &format!("https://music.youtube.com/watch?v={video_id}"), &fmt, attempt_lines)?;
    if lines.len() != VIDEO_HEIGHTS.len() {
        return Err("unexpected answer for the picture addresses".into());
    }
    Ok(VIDEO_HEIGHTS.iter().copied().zip(lines).collect())
}

/// A direct address of a video-only stream (no sound) at most `height` pixels tall, for the music video.
pub fn video_url(video_id: &str, height: u32) -> Result<String, String> {
    let bin = ensure()?;
    // Never above 1080p, whatever was asked for.
    let height = height.clamp(144, 1080);
    let fmt = format!("bestvideo[height<={height}][vcodec^=avc1]/bestvideo[height<={height}]/worstvideo[height<=1080]");
    resolve_direct_fmt(&bin, &format!("https://music.youtube.com/watch?v={video_id}"), &fmt).map(|(u, _)| u)
}

/// The quick way to get a YouTube song: find the audio address, then download it in parallel pieces.
fn fetch_fast(video_id: &str, quality: Quality) -> Result<PathBuf, String> {
    let bin = ensure()?;
    let (url, ext) = resolve_direct(&bin, &format!("https://music.youtube.com/watch?v={video_id}"), quality)?;
    let bytes = download_parallel(&url)?;
    let dest = audio_cache().join(format!("{video_id}.{ext}"));
    let tmp = audio_cache().join(format!("{video_id}.{ext}.part"));
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    let path = dest.to_string_lossy().into_owned();
    if !crate::audio::decode::is_playable(&path) {
        let _ = std::fs::remove_file(&dest);
        return Err("the downloaded audio is damaged".into());
    }
    prune_cache();
    Ok(dest)
}

/// The cached file, or the quick fetch only (no slow fallback).
pub fn fetch_quick(video_id: &str, quality: Quality) -> Result<PathBuf, String> {
    if let Some(p) = cached(video_id) {
        if crate::audio::decode::is_playable(&p.to_string_lossy()) {
            touch(&p);
            return Ok(p);
        }
        let _ = std::fs::remove_file(&p);
    }
    fetch_fast(video_id, quality)
}

/// The full download (with its own retries and a helper update), used when the quick way failed.
pub fn fetch_full(video_id: &str, quality: Quality) -> Result<PathBuf, String> {
    fetch_target(&format!("https://music.youtube.com/watch?v={video_id}"), quality)
}

/// Make sure a song's audio is on disk (fetching it if needed) and return the file.
pub fn fetch(video_id: &str, quality: Quality) -> Result<PathBuf, String> {
    if let Some(p) = cached(video_id) {
        if crate::audio::decode::is_playable(&p.to_string_lossy()) {
            touch(&p);
            return Ok(p);
        }
        // A broken or half-written file: fetch it again.
        let _ = std::fs::remove_file(&p);
    }
    // Quick way first; the full yt-dlp download (with its own retries and updates) is the fallback.
    match fetch_fast(video_id, quality) {
        Ok(p) => Ok(p),
        Err(e) => {
            eprintln!("[senthym] quick fetch of {video_id} failed ({e}); using the full download");
            fetch_target(&format!("https://music.youtube.com/watch?v={video_id}"), quality)
        }
    }
}

/// Audio of any link yt-dlp can play (e.g. a SoundCloud track).
pub fn fetch_url(url: &str, quality: Quality) -> Result<PathBuf, String> {
    fetch_target(url, quality)
}

fn fetch_target(target: &str, quality: Quality) -> Result<PathBuf, String> {
    let bin = ensure()?;
    let template = audio_cache().join("%(id)s.%(ext)s").to_string_lossy().into_owned();
    let result = run_ytdlp(&bin, target, quality, &template);
    let path = match result {
        Ok(p) => p,
        Err(e) => {
            // An old helper is the usual reason: update once and try again.
            if install_latest().is_ok() {
                run_ytdlp(&managed_path(), target, quality, &template).map_err(|e| format!("Couldn't get this song: {e}"))?
            } else {
                return Err(format!("Couldn't get this song: {e}"));
            }
        }
    };
    prune_cache();
    if !crate::audio::decode::is_playable(&path) {
        let _ = std::fs::remove_file(&path);
        return Err("the downloaded audio is damaged, please try again".into());
    }
    Ok(PathBuf::from(path))
}

fn touch(p: &Path) {
    if let Ok(f) = std::fs::File::options().append(true).open(p) {
        let _ = f.set_modified(std::time::SystemTime::now());
    }
}

/// Keep the cache under its size limit, removing the songs used longest ago.
fn prune_cache() {
    let Ok(rd) = std::fs::read_dir(audio_cache()) else { return };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((m.modified().ok()?, m.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort_by_key(|f| f.0);
    for (_, len, p) in files {
        if total <= CACHE_LIMIT_BYTES {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            total -= len;
        }
    }
}

pub fn cache_size() -> u64 {
    std::fs::read_dir(audio_cache())
        .map(|rd| rd.filter_map(|e| e.ok()).filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum())
        .unwrap_or(0)
}

pub fn clear_cache() {
    if let Ok(rd) = std::fs::read_dir(audio_cache()) {
        for e in rd.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

fn safe_name(s: &str) -> String {
    let cleaned: String = s.chars().map(|c| if "/\\:*?\"<>|".contains(c) || c.is_control() { '_' } else { c }).collect();
    cleaned.trim().chars().take(120).collect()
}

/// Save a song into the music folder with its title, artist and album written into the file.
pub fn download(track: &crate::model::Track, video_id: &str, quality: Quality, folder: &str) -> Result<String, String> {
    if let crate::model::Source::Local(p) = &track.source {
        return Ok(p.clone());
    }
    let src = fetch(video_id, quality)?;
    let dir = PathBuf::from(folder);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Can't create the music folder ({e})"))?;
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("m4a");
    let name = safe_name(&format!("{} - {}", track.artist, track.title));
    let dest = dir.join(format!("{name}.{ext}"));
    std::fs::copy(&src, &dest).map_err(|e| e.to_string())?;
    write_tags(&dest, track);
    Ok(dest.to_string_lossy().into_owned())
}

fn write_tags(path: &Path, t: &crate::model::Track) {
    use lofty::config::WriteOptions;
    use lofty::file::TaggedFileExt;
    use lofty::tag::{Accessor, Tag, TagExt};
    let Ok(mut file) = lofty::read_from_path(path) else { return };
    let tag_type = file.primary_tag_type();
    if file.primary_tag().is_none() {
        file.insert_tag(Tag::new(tag_type));
    }
    if let Some(tag) = file.primary_tag_mut() {
        tag.set_title(t.title.clone());
        tag.set_artist(t.artist.clone());
        if !t.album.is_empty() {
            tag.set_album(t.album.clone());
        }
        let _ = tag.save_to_path(path, WriteOptions::default());
    }
}

/// Windows only: make sure ffmpeg is available (it decodes the picture and some sound files). The program
/// is one .exe, so a copy is downloaded once into the app's folder; it is only kept when it matches the
/// checksum published next to it.
#[cfg(windows)]
pub fn ensure_ffmpeg() -> Result<(), String> {
    use std::io::{Cursor, Read};
    let dest = crate::store::data_dir().join("bin").join("ffmpeg.exe");
    if dest.exists() || crate::sys::cmd("ffmpeg").arg("-version").output().is_ok() {
        return Ok(());
    }
    let _ = std::fs::create_dir_all(dest.parent().unwrap());
    const BASE: &str = "https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/";
    const FILE: &str = "ffmpeg-master-latest-win64-lgpl.zip";
    let sums = ureq::get(&format!("{BASE}checksums.sha256")).timeout(std::time::Duration::from_secs(30)).call().map_err(|e| e.to_string())?.into_string().map_err(|e| e.to_string())?;
    let expected = sums
        .lines()
        .find_map(|l| {
            let mut it = l.split_whitespace();
            match (it.next(), it.next()) {
                (Some(h), Some(n)) if n.trim_start_matches('*') == FILE => Some(h.to_lowercase()),
                _ => None,
            }
        })
        .ok_or("ffmpeg checksum not found")?;
    let resp = ureq::get(&format!("{BASE}{FILE}")).timeout(std::time::Duration::from_secs(900)).call().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    Read::read_to_end(&mut Read::take(resp.into_reader(), 400 * 1024 * 1024), &mut bytes).map_err(|e| e.to_string())?;
    let got: String = {
        use sha2::Digest;
        sha2::Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect()
    };
    if got != expected {
        return Err("ffmpeg download doesn't match its checksum".into());
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        if f.name().ends_with("/bin/ffmpeg.exe") {
            let mut out = Vec::new();
            f.read_to_end(&mut out).map_err(|e| e.to_string())?;
            let tmp = dest.with_extension("part");
            std::fs::write(&tmp, out).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    Err("ffmpeg.exe not found in the download".into())
}

#[cfg(test)]
mod browser_tests {
    use super::browser_help;

    #[test]
    fn help_for_a_locked_browser_says_to_close_it() {
        let t = browser_help("chrome", "Could not copy Chrome cookie database. See https://github.com/yt-dlp/yt-dlp/issues/7271");
        assert!(t.contains("Close Google Chrome completely"), "{t}");
    }

    #[test]
    fn help_for_encrypted_cookies_points_to_firefox_on_windows_only() {
        let t = browser_help("edge", "Failed to decrypt with DPAPI");
        if cfg!(windows) {
            assert!(t.contains("Pick Firefox"), "{t}");
        } else {
            assert!(t.contains("couldn't decrypt"), "{t}");
        }
    }

    #[test]
    fn unknown_errors_keep_their_text() {
        assert!(browser_help("firefox", "something odd").contains("something odd"));
    }
}
