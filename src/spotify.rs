//! Spotify account: official sign-in (OAuth with PKCE, using the person's own free Client ID
//! from developer.spotify.com), their library, and finding each Spotify song on YouTube Music
//! so it can play — the same split the original uses (Spotify for the library, YouTube for audio).
//!
//! Matching rules come from the original's `SongPlayer.ytmusicTransferScore` / `isAcceptableMatch`.

use crate::model::*;
use crate::store::data_dir;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const REDIRECT_PORT: u16 = 8898;
pub const REDIRECT_URI: &str = "http://127.0.0.1:8898/callback";
const SCOPES: &str = "user-read-private user-read-email user-library-read playlist-read-private playlist-read-collaborative user-follow-read user-top-read user-read-recently-played";
const API: &str = "https://api.spotify.com/v1";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Auth {
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: String,
    /// Unix seconds.
    pub expires_at: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub name: String,
    pub image: Option<String>,
    pub product: String,
}

#[derive(Clone, Debug, Default)]
pub struct Library {
    pub liked: Vec<Track>,
    pub playlists: Vec<Item>,
    pub albums: Vec<Item>,
    pub artists: Vec<Item>,
    pub top: Vec<Track>,
    pub recent: Vec<Track>,
}

static AUTH: Mutex<Option<Auth>> = Mutex::new(None);

fn auth_path() -> std::path::PathBuf {
    data_dir().join("spotify.json")
}

pub fn load_auth() -> Option<Auth> {
    let a: Option<Auth> = std::fs::read_to_string(auth_path()).ok().and_then(|s| serde_json::from_str(&s).ok());
    *AUTH.lock().unwrap() = a.clone().filter(|a| !a.refresh_token.is_empty());
    a
}

fn save_auth(a: &Auth) {
    if let Ok(s) = serde_json::to_string(a) {
        let p = auth_path();
        let _ = std::fs::write(&p, s);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
        }
    }
    *AUTH.lock().unwrap() = Some(a.clone());
}

pub fn saved_client_id() -> String {
    std::fs::read_to_string(auth_path())
        .ok()
        .and_then(|s| serde_json::from_str::<Auth>(&s).ok())
        .map(|a| a.client_id)
        .unwrap_or_default()
}

pub fn logged_in() -> bool {
    AUTH.lock().unwrap().as_ref().map(|a| !a.refresh_token.is_empty()).unwrap_or(false)
}

/// Forget the sign-in (keeps the Client ID so signing in again is one click).
pub fn log_out() {
    let cid = AUTH.lock().unwrap().as_ref().map(|a| a.client_id.clone()).unwrap_or_else(saved_client_id);
    let a = Auth { client_id: cid, ..Default::default() };
    if let Ok(s) = serde_json::to_string(&a) {
        let _ = std::fs::write(auth_path(), s);
    }
    *AUTH.lock().unwrap() = None;
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).build()
}

fn random_string(n: usize) -> String {
    use rand::Rng;
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut r = rand::thread_rng();
    (0..n).map(|_| CHARS[r.gen_range(0..CHARS.len())] as char).collect()
}

fn challenge(verifier: &str) -> String {
    use base64::Engine;
    use sha2::Digest;
    let hash = sha2::Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash)
}

fn token_request(form: &[(&str, &str)]) -> Result<Value, String> {
    let resp = agent().post("https://accounts.spotify.com/api/token").send_form(form);
    match resp {
        Ok(r) => r.into_json::<Value>().map_err(|e| e.to_string()),
        Err(ureq::Error::Status(code, r)) => {
            let body: Value = r.into_json().unwrap_or(Value::Null);
            let msg = body["error_description"].as_str().or(body["error"].as_str()).unwrap_or("unknown error").to_string();
            Err(format!("Spotify refused the sign-in ({code}: {msg})"))
        }
        Err(e) => Err(format!("Couldn't reach Spotify ({e})")),
    }
}

/// Opens the Spotify sign-in page in the browser and waits (up to 5 minutes) for the
/// browser to come back to this computer with the approval.
pub fn login(client_id: &str) -> Result<User, String> {
    let client_id = client_id.trim();
    if client_id.len() < 20 || !client_id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("That Client ID doesn't look right: copy the 32-letter code from your Spotify app's settings".into());
    }
    let listener = TcpListener::bind(("127.0.0.1", REDIRECT_PORT))
        .map_err(|e| format!("Couldn't listen for Spotify's answer on port {REDIRECT_PORT} ({e})"))?;
    listener.set_nonblocking(true).ok();
    let verifier = random_string(64);
    let state = random_string(16);
    let url = format!(
        "https://accounts.spotify.com/authorize?response_type=code&client_id={}&scope={}&redirect_uri={}&state={}&code_challenge_method=S256&code_challenge={}",
        urlencoding::encode(client_id),
        urlencoding::encode(SCOPES),
        urlencoding::encode(REDIRECT_URI),
        state,
        challenge(&verifier)
    );
    open_in_browser(&url);

    let started = Instant::now();
    let (code, got_state) = loop {
        if started.elapsed() > Duration::from_secs(300) {
            return Err("Sign-in timed out. Try again and approve the request in your browser.".into());
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).ok();
                let mut first = String::new();
                let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
                reader.read_line(&mut first).ok();
                let target = first.split_whitespace().nth(1).unwrap_or("").to_string();
                if !target.starts_with("/callback") {
                    let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    continue;
                }
                let query = target.split_once('?').map(|x| x.1).unwrap_or("");
                let mut code = None;
                let mut st = None;
                let mut err = None;
                for pair in query.split('&') {
                    let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                    let v = urlencoding::decode(v).map(|s| s.into_owned()).unwrap_or_default();
                    match k {
                        "code" => code = Some(v),
                        "state" => st = Some(v),
                        "error" => err = Some(v),
                        _ => {}
                    }
                }
                let (title, text) = if err.is_some() {
                    ("Sign-in cancelled", "You can close this tab and try again in Senthym.")
                } else {
                    ("Signed in to Spotify", "All set. You can close this tab and go back to Senthym.")
                };
                let page = format!(
                    "<!doctype html><html><body style=\"background:#0b0b0f;color:#fff;font-family:sans-serif;text-align:center;padding-top:80px\"><h1 style=\"color:#1ed760\">{title}</h1><p>{text}</p></body></html>"
                );
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", page.len(), page);
                if let Some(e) = err {
                    return Err(if e == "access_denied" { "You cancelled the Spotify sign-in.".into() } else { format!("Spotify said: {e}") });
                }
                match code {
                    Some(c) => break (c, st),
                    None => continue,
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    };
    if got_state.as_deref() != Some(state.as_str()) {
        return Err("The sign-in answer didn't match this request; please try again.".into());
    }
    let v = token_request(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", REDIRECT_URI),
        ("client_id", client_id),
        ("code_verifier", &verifier),
    ])?;
    let auth = Auth {
        client_id: client_id.to_string(),
        access_token: v["access_token"].as_str().unwrap_or("").to_string(),
        refresh_token: v["refresh_token"].as_str().unwrap_or("").to_string(),
        expires_at: now() + v["expires_in"].as_i64().unwrap_or(3600) - 60,
    };
    if auth.access_token.is_empty() {
        return Err("Spotify didn't send a sign-in token".into());
    }
    save_auth(&auth);
    me()
}

pub fn open_in_browser(url: &str) {
    crate::sys::open_url(url);
}

fn token() -> Result<String, String> {
    let a = AUTH.lock().unwrap().clone().ok_or("Not signed in to Spotify")?;
    if a.expires_at > now() && !a.access_token.is_empty() {
        return Ok(a.access_token);
    }
    let v = token_request(&[("grant_type", "refresh_token"), ("refresh_token", &a.refresh_token), ("client_id", &a.client_id)])?;
    let fresh = Auth {
        client_id: a.client_id.clone(),
        access_token: v["access_token"].as_str().unwrap_or("").to_string(),
        refresh_token: v["refresh_token"].as_str().map(String::from).unwrap_or(a.refresh_token),
        expires_at: now() + v["expires_in"].as_i64().unwrap_or(3600) - 60,
    };
    save_auth(&fresh);
    Ok(fresh.access_token)
}

fn get(url: &str) -> Result<Value, String> {
    let full = if url.starts_with("http") { url.to_string() } else { format!("{API}{url}") };
    for attempt in 0..3 {
        let tok = token()?;
        match agent().get(&full).set("Authorization", &format!("Bearer {tok}")).call() {
            Ok(r) => return r.into_json::<Value>().map_err(|e| e.to_string()),
            Err(ureq::Error::Status(401, _)) if attempt == 0 => {
                if let Some(a) = AUTH.lock().unwrap().as_mut() {
                    a.expires_at = 0;
                }
            }
            Err(ureq::Error::Status(429, r)) => {
                let wait = r.header("Retry-After").and_then(|s| s.parse::<u64>().ok()).unwrap_or(2).min(10);
                std::thread::sleep(Duration::from_secs(wait));
            }
            Err(ureq::Error::Status(403, r)) => {
                let body: Value = r.into_json().unwrap_or(Value::Null);
                let msg = body["error"]["message"].as_str().unwrap_or("not allowed").to_string();
                return Err(format!("Spotify refused this ({msg}). If your Spotify app is in development mode, add your account under “User Management” in the Spotify dashboard."));
            }
            Err(ureq::Error::Status(code, _)) => return Err(format!("Spotify answered {code}")),
            Err(e) => return Err(format!("Couldn't reach Spotify ({e})")),
        }
    }
    Err("Spotify didn't answer".into())
}

/// Every page of a paged list, up to `max` items.
fn get_all(url: &str, max: usize) -> Result<Vec<Value>, String> {
    let mut out = vec![];
    let mut next = Some(url.to_string());
    while let Some(u) = next.take() {
        let v = get(&u)?;
        let page = if v.get("items").is_some() { v.clone() } else { v.as_object().and_then(|o| o.values().find(|x| x.get("items").is_some()).cloned()).unwrap_or(Value::Null) };
        if let Some(items) = page["items"].as_array() {
            out.extend(items.iter().cloned());
        }
        if out.len() >= max {
            out.truncate(max);
            break;
        }
        next = page["next"].as_str().map(String::from);
    }
    Ok(out)
}

fn best_image(v: &Value) -> Option<String> {
    v["images"].as_array().and_then(|a| a.first()).and_then(|i| i["url"].as_str()).map(String::from)
}

pub fn me() -> Result<User, String> {
    let v = get("/me")?;
    Ok(User {
        id: v["id"].as_str().unwrap_or("").to_string(),
        name: v["display_name"].as_str().or(v["id"].as_str()).unwrap_or("Spotify user").to_string(),
        image: best_image(&v),
        product: v["product"].as_str().unwrap_or("").to_string(),
    })
}

fn track_of(t: &Value) -> Option<Track> {
    let id = t["id"].as_str()?;
    if t["type"].as_str() == Some("episode") {
        return None;
    }
    let artists: Vec<&str> = t["artists"].as_array().map(|a| a.iter().filter_map(|x| x["name"].as_str()).collect()).unwrap_or_default();
    Some(Track {
        source: Source::Spotify(id.to_string()),
        title: t["name"].as_str().unwrap_or("").to_string(),
        artist: artists.join(", "),
        artist_id: t["artists"][0]["id"].as_str().map(|s| format!("sp:artist:{s}")),
        album: t["album"]["name"].as_str().unwrap_or("").to_string(),
        album_id: t["album"]["id"].as_str().map(|s| format!("sp:album:{s}")),
        duration_ms: t["duration_ms"].as_u64().unwrap_or(0),
        thumb: best_image(&t["album"]),
    })
}

fn album_item(a: &Value) -> Option<Item> {
    let artists: Vec<&str> = a["artists"].as_array().map(|x| x.iter().filter_map(|y| y["name"].as_str()).collect()).unwrap_or_default();
    let year = a["release_date"].as_str().unwrap_or("").split('-').next().unwrap_or("").to_string();
    Some(Item {
        kind: ItemKind::Album,
        id: format!("sp:album:{}", a["id"].as_str()?),
        title: a["name"].as_str().unwrap_or("").to_string(),
        subtitle: [year, artists.join(", ")].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" • "),
        thumb: best_image(a),
        track: None,
    })
}

/// The signed-in person's Spotify library.
pub fn library() -> Result<Library, String> {
    let mut lib = Library::default();
    lib.liked = get_all("/me/tracks?limit=50", 2000)?.iter().filter_map(|i| track_of(&i["track"])).collect();
    lib.playlists = get_all("/me/playlists?limit=50", 500)?
        .iter()
        .filter_map(|p| {
            Some(Item {
                kind: ItemKind::Playlist,
                id: format!("sp:playlist:{}", p["id"].as_str()?),
                title: p["name"].as_str().unwrap_or("").to_string(),
                subtitle: format!("Playlist • {}", p["owner"]["display_name"].as_str().unwrap_or("")),
                thumb: best_image(p),
                track: None,
            })
        })
        .collect();
    lib.albums = get_all("/me/albums?limit=50", 500).unwrap_or_default().iter().filter_map(|i| album_item(&i["album"])).collect();
    lib.artists = get_all("/me/following?type=artist&limit=50", 500)
        .unwrap_or_default()
        .iter()
        .filter_map(|a| {
            Some(Item { kind: ItemKind::Artist, id: format!("sp:artist:{}", a["id"].as_str()?), title: a["name"].as_str().unwrap_or("").to_string(), subtitle: "Artist".into(), thumb: best_image(a), track: None })
        })
        .collect();
    lib.top = get_all("/me/top/tracks?limit=50&time_range=short_term", 50).unwrap_or_default().iter().filter_map(track_of).collect();
    lib.recent = get("/me/player/recently-played?limit=50")
        .map(|v| v["items"].as_array().map(|a| a.iter().filter_map(|i| track_of(&i["track"])).collect()).unwrap_or_default())
        .unwrap_or_default();
    Ok(lib)
}

/// Songs of a Spotify playlist, album or artist (`sp:<kind>:<id>`), shaped like a playlist page.
pub fn collection(id: &str) -> Result<PlaylistPage, String> {
    let mut parts = id.splitn(3, ':');
    let (_, kind, sid) = (parts.next(), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let mut page = PlaylistPage { id: id.to_string(), ..Default::default() };
    match kind {
        "playlist" => {
            let v = get(&format!("/playlists/{sid}?fields=name,owner(display_name),images,tracks(total)"))?;
            page.title = v["name"].as_str().unwrap_or("Playlist").to_string();
            page.owner = v["owner"]["display_name"].as_str().unwrap_or("").to_string();
            page.thumb = best_image(&v);
            let items = get_all(&format!("/playlists/{sid}/tracks?limit=100"), 3000).or_else(|_| get_all(&format!("/playlists/{sid}/items?limit=100"), 3000))?;
            page.tracks = items.iter().filter_map(|i| track_of(if i["track"].is_object() { &i["track"] } else { &i["item"] })).collect();
        }
        "album" => {
            let v = get(&format!("/albums/{sid}"))?;
            page.title = v["name"].as_str().unwrap_or("Album").to_string();
            page.owner = v["artists"].as_array().map(|a| a.iter().filter_map(|x| x["name"].as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
            page.detail = v["release_date"].as_str().unwrap_or("").to_string();
            page.thumb = best_image(&v);
            let album_json = serde_json::json!({"name": v["name"], "id": v["id"], "images": v["images"]});
            let mut items = v["tracks"]["items"].as_array().cloned().unwrap_or_default();
            if let Some(next) = v["tracks"]["next"].as_str() {
                items.extend(get_all(next, 1000).unwrap_or_default());
            }
            page.tracks = items
                .iter()
                .filter_map(|t| {
                    let mut t = t.clone();
                    t["album"] = album_json.clone();
                    track_of(&t)
                })
                .collect();
        }
        "artist" => {
            let v = get(&format!("/artists/{sid}"))?;
            page.title = v["name"].as_str().unwrap_or("Artist").to_string();
            page.owner = "Artist".into();
            page.thumb = best_image(&v);
            let top = get(&format!("/artists/{sid}/top-tracks?market=from_token"))?;
            page.tracks = top["tracks"].as_array().map(|a| a.iter().filter_map(track_of).collect()).unwrap_or_default();
        }
        _ => return Err("Unknown Spotify item".into()),
    }
    Ok(page)
}

// ---------------------------------------------------------------- matching on YouTube Music

struct Marker {
    hard: bool,
    words: &'static [&'static str],
}

/// Version markers from `SongPlayer.alternateVersionMarkers` (hard ones reject a match).
const MARKERS: &[Marker] = &[
    Marker { hard: true, words: &["remix", "re mix", "rmx", "club mix", "dance mix", "dub mix", "vip mix"] },
    Marker { hard: true, words: &["alternative", "alternate", "alt version", "demo", "unreleased", "rough mix", "early version"] },
    Marker { hard: true, words: &["sped up", "speed up", "fast version"] },
    Marker { hard: true, words: &["slowed", "slow version"] },
    Marker { hard: true, words: &["nightcore", "daycore"] },
    Marker { hard: true, words: &["live", "concert", "session", "performance"] },
    Marker { hard: true, words: &["acoustic", "unplugged", "piano version", "guitar version"] },
    Marker { hard: true, words: &["cover", "covered by", "tribute"] },
    Marker { hard: true, words: &["karaoke", "minus one"] },
    Marker { hard: true, words: &["instrumental", "no vocals"] },
    Marker { hard: true, words: &["mashup", "mash up", "bootleg", "rework", "flip"] },
    Marker { hard: true, words: &["fan edit", "fanmade", "right version", "edit audio"] },
    Marker { hard: false, words: &["extended mix", "extended version", "12 inch"] },
    Marker { hard: false, words: &["radio edit", "single edit", "edit version"] },
    Marker { hard: false, words: &["remaster", "remastered", "anniversary edition"] },
];

/// Lowercase, drop "(feat. …)", keep letters/digits/spaces (`SongPlayer.normalizedForMatch`).
fn normalized(s: &str) -> String {
    let lower = s.to_lowercase();
    let no_feat = strip_feat(&lower);
    let cleaned: String = no_feat.chars().map(|c| if c.is_alphanumeric() || c.is_whitespace() { c } else { ' ' }).collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Remove "(feat. …)" / "[ft. …]" (`SongPlayer.featSearchPattern`).
pub fn strip_feat(s: &str) -> String {
    let mut out = s.to_string();
    loop {
        let lower = out.to_lowercase();
        let mut found = None;
        for (open, close) in [('(', ')'), ('[', ']')] {
            if let Some(start) = lower.find(open) {
                let inner = lower[start + 1..].trim_start();
                if inner.starts_with("feat.") || inner.starts_with("ft.") || inner.starts_with("feat ") {
                    if let Some(end) = lower[start..].find(close) {
                        found = Some((start, start + end + 1));
                        break;
                    }
                }
            }
        }
        match found {
            Some((a, b)) => out.replace_range(a..b, ""),
            None => break,
        }
    }
    out.trim().to_string()
}

fn markers(text: &str) -> Vec<(usize, bool)> {
    let t = format!(" {} ", normalized(text));
    MARKERS
        .iter()
        .enumerate()
        .filter(|(_, m)| m.words.iter().any(|w| t.contains(&format!(" {w} "))))
        .map(|(i, m)| (i, m.hard))
        .collect()
}

/// Dice coefficient of character pairs (`SongPlayer.bigramSimilarity`).
pub fn bigram(a: &str, b: &str) -> f64 {
    let (a, b) = (normalized(a), normalized(b));
    if a == b {
        return 1.0;
    }
    let pairs = |s: &str| -> std::collections::HashSet<(char, char)> {
        let c: Vec<char> = s.chars().collect();
        c.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let (pa, pb) = (pairs(&a), pairs(&b));
    if pa.is_empty() || pb.is_empty() {
        return 0.0;
    }
    let common = pa.intersection(&pb).count();
    common as f64 * 2.0 / (pa.len() + pb.len()) as f64
}

pub struct Candidate {
    pub video_id: String,
    pub score: f64,
    pub title: f64,
    pub artist: f64,
    pub duration: Option<f64>,
    pub album: Option<f64>,
    pub hard_alternate: bool,
    pub is_video: bool,
}

/// `SongPlayer.ytmusicTransferScore`.
pub fn score(item: &Item, expected: &Track) -> Option<Candidate> {
    let t = item.track.as_ref()?;
    let is_video = item.kind == ItemKind::Video;
    let mut cand_title = t.title.clone();
    if is_video {
        if let Some((_, after)) = t.title.split_once('-') {
            cand_title = after.trim().to_string();
        }
    }
    let title = bigram(&cand_title, &expected.title);
    let mut artist = bigram(&t.artist, &expected.artist);
    if is_video {
        let before = t.title.split('-').next().unwrap_or("");
        artist = artist.max(bigram(before, &expected.artist));
    }
    let d = expected.duration_ms as f64 / 1000.0;
    let duration = if d > 0.0 && t.duration_ms > 0 {
        Some((1.0 - ((t.duration_ms as f64 / 1000.0 - d).abs() * 2.0) / d).clamp(0.0, 1.0))
    } else {
        None
    };
    let album = if !is_video && !expected.album.trim().is_empty() && !t.album.is_empty() { Some(bigram(&t.album, &expected.album)) } else { None };
    let expected_markers: Vec<usize> = markers(&expected.title).into_iter().map(|m| m.0).collect();
    let unexpected: Vec<(usize, bool)> = markers(&format!("{} {}", t.title, t.album)).into_iter().filter(|m| !expected_markers.contains(&m.0)).collect();
    let hard = unexpected.iter().filter(|m| m.1).count();
    let penalty = (unexpected.len() - hard) as f64 * SOFT_MARKER_PENALTY + hard as f64 * HARD_MARKER_PENALTY;
    let mut parts = vec![title, artist];
    if let Some(ds) = duration {
        parts.push(ds * DURATION_WEIGHT);
    }
    if let Some(a) = album {
        parts.push(a);
    }
    let avg = parts.iter().sum::<f64>() / parts.len() as f64;
    let score = (avg * if is_video { 1.0 } else { SONG_SCORE_MULTIPLIER } - penalty).max(0.0);
    Some(Candidate { video_id: item.id.clone(), score, title, artist, duration, album, hard_alternate: hard > 0, is_video })
}

/// `SongPlayer.isAcceptableMatch`.
pub fn acceptable(c: &Candidate) -> bool {
    let close_duration = c.duration.map(|d| d >= CLOSE_DURATION_SCORE).unwrap_or(false);
    let album_ok = c.album.map(|a| a >= MIN_TITLE_SCORE).unwrap_or(false);
    let threshold = match (c.duration.is_some(), c.is_video) {
        (true, true) => THRESHOLD_VIDEO_WITH_DURATION,
        (true, false) => THRESHOLD_SONG_WITH_DURATION,
        (false, true) => THRESHOLD_VIDEO_NO_DURATION,
        (false, false) => THRESHOLD_SONG_NO_DURATION,
    };
    if c.score >= threshold && !c.hard_alternate && c.title >= MIN_TITLE_SCORE {
        return c.artist >= MIN_ARTIST_SCORE || (album_ok && c.artist >= MIN_ARTIST_SCORE_WITH_ALBUM) || (close_duration && c.artist >= MIN_ARTIST_SCORE_WITH_DURATION);
    }
    false
}

// Values read from the original's bytecode (classes5.dex):
// SongPlayer.ytmusicTransferScore @0x014d (1.75), @0x0152 (0.65), @0x0176 (5.0), @0x0093 (2.0)
// SongPlayer.isAcceptableMatch @0x000e (0.94), @0x001e (0.45), @0x0047 (1.55), @0x004f (2.25),
//   @0x005c (0.78), @0x0062 (1.35), @0x008e (0.32), @0x009d (0.18), @0x00ac (0.25)
// SongPlayer.resolveYtPlaybackUnbounded: take(3) song candidates, then take(3) video candidates.
pub const HARD_MARKER_PENALTY: f64 = 1.75;
pub const SOFT_MARKER_PENALTY: f64 = 0.65;
pub const DURATION_WEIGHT: f64 = 5.0;
pub const SONG_SCORE_MULTIPLIER: f64 = 2.0;
pub const CLOSE_DURATION_SCORE: f64 = 0.94;
pub const MIN_TITLE_SCORE: f64 = 0.45;
pub const THRESHOLD_VIDEO_WITH_DURATION: f64 = 1.55;
pub const THRESHOLD_SONG_WITH_DURATION: f64 = 2.25;
pub const THRESHOLD_VIDEO_NO_DURATION: f64 = 0.78;
pub const THRESHOLD_SONG_NO_DURATION: f64 = 1.35;
pub const MIN_ARTIST_SCORE: f64 = 0.32;
pub const MIN_ARTIST_SCORE_WITH_ALBUM: f64 = 0.18;
pub const MIN_ARTIST_SCORE_WITH_DURATION: f64 = 0.25;
pub const MATCH_CANDIDATES_TRIED: usize = 3;

/// The best YouTube Music match for a Spotify track.
pub fn find_on_youtube(t: &Track, allow_videos: bool) -> Result<String, String> {
    candidates(t, false)
        .ok()
        .and_then(|c| c.into_iter().next())
        .map(Ok)
        .unwrap_or_else(|| {
            if allow_videos {
                candidates(t, true)?.into_iter().next().ok_or_else(|| format!("couldn't find “{}” on YouTube Music", t.title))
            } else {
                Err(format!("couldn't find “{}” on YouTube Music", t.title))
            }
        })
}

/// Up to 3 accepted matches, best first, from song results (`videos` = false) or video results.
pub fn candidates(t: &Track, videos: bool) -> Result<Vec<String>, String> {
    let first_artist = t.artist.split(", ").next().unwrap_or(&t.artist);
    let query = [strip_feat(&t.title), first_artist.to_string()].into_iter().filter(|s| !s.trim().is_empty()).collect::<Vec<_>>().join(" ");
    let filter = if videos { crate::ytm::FILTER_VIDEOS } else { crate::ytm::FILTER_SONGS };
    let items = crate::ytm::search(&query, Some(filter))?;
    let mut accepted: Vec<Candidate> = items.iter().filter(|i| i.track.is_some()).filter_map(|it| score(it, t)).filter(acceptable).collect();
    accepted.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    Ok(accepted.into_iter().take(MATCH_CANDIDATES_TRIED).map(|c| c.video_id).collect())
}

