//! Extra music sources, all usable without any developer key:
//! - SoundCloud: public tracks, searched and played through the yt-dlp helper.
//! - Deezer: its free public catalogue (search, charts, albums, playlists, artists); songs play
//!   from their YouTube Music match, like the original plays Spotify songs. Deezer's own full
//!   streams need its paid, protected files, which this app doesn't unlock.
//! - Google account (YouTube Music): the sign-in already saved in the person's browser is read by
//!   yt-dlp; with it, YouTube Music requests are made as that account, so its liked songs,
//!   playlists, albums and artists can be saved into the app's own library.

use crate::model::*;
use crate::store::data_dir;
use serde_json::Value;
use std::sync::Mutex;
use std::time::Duration;

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).build()
}

// ---------------------------------------------------------------- SoundCloud

/// SoundCloud Go+ songs only give a 30-second preview to free listeners; those are left out.
pub const SOUNDCLOUD_PREVIEW_SECONDS: f64 = 30.5;

pub fn soundcloud_search(query: &str) -> Result<Vec<Item>, String> {
    let bin = crate::helper::ensure()?;
    let out = crate::sys::cmd(bin)
        .args(["-q", "--no-warnings", "--flat-playlist", "-J", &format!("scsearch25:{query}")])
        .output()
        .map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|_| "SoundCloud didn't answer".to_string())?;
    let mut items = vec![];
    for e in v["entries"].as_array().cloned().unwrap_or_default() {
        let dur = e["duration"].as_f64().unwrap_or(0.0);
        if dur > 0.0 && dur < SOUNDCLOUD_PREVIEW_SECONDS {
            continue;
        }
        let url = e["webpage_url"].as_str().or(e["url"].as_str()).unwrap_or("").to_string();
        if url.is_empty() {
            continue;
        }
        let thumb = e["thumbnails"].as_array().and_then(|a| {
            a.iter().find(|t| t["id"].as_str() == Some("t500x500")).or_else(|| a.last()).and_then(|t| t["url"].as_str()).map(String::from)
        });
        let title = e["title"].as_str().unwrap_or("").to_string();
        let artist = e["uploader"].as_str().unwrap_or("").to_string();
        let track = Track {
            source: Source::Url(url.clone()),
            title: title.clone(),
            artist: artist.clone(),
            artist_id: None,
            album: "SoundCloud".into(),
            album_id: None,
            duration_ms: (dur * 1000.0) as u64,
            thumb: thumb.clone(),
        };
        items.push(Item { kind: ItemKind::Song, id: url, title, subtitle: format!("SoundCloud • {artist}"), thumb, track: Some(track) });
    }
    Ok(items)
}

// ---------------------------------------------------------------- Deezer (public catalogue)

const DEEZER: &str = "https://api.deezer.com";

fn deezer_get(path: &str) -> Result<Value, String> {
    let v: Value = agent().get(&format!("{DEEZER}{path}")).call().map_err(|e| format!("Couldn't reach Deezer ({e})"))?.into_json().map_err(|e| e.to_string())?;
    if let Some(msg) = v["error"]["message"].as_str() {
        return Err(format!("Deezer: {msg}"));
    }
    Ok(v)
}

fn dz_track(t: &Value, album: Option<&Value>) -> Option<Track> {
    let id = t["id"].as_u64()?;
    let alb = album.unwrap_or(&t["album"]);
    Some(Track {
        source: Source::Deezer(id.to_string()),
        title: t["title"].as_str().unwrap_or("").to_string(),
        artist: t["artist"]["name"].as_str().unwrap_or("").to_string(),
        artist_id: t["artist"]["id"].as_u64().map(|a| format!("dz:artist:{a}")),
        album: alb["title"].as_str().unwrap_or("").to_string(),
        album_id: alb["id"].as_u64().map(|a| format!("dz:album:{a}")),
        duration_ms: t["duration"].as_u64().unwrap_or(0) * 1000,
        thumb: alb["cover_medium"].as_str().or(alb["cover_big"].as_str()).map(String::from),
    })
}

fn dz_list(v: &Value, kind: ItemKind) -> Vec<Item> {
    v["data"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let id = x["id"].as_u64()?;
                    let (prefix, title, sub, thumb) = match kind {
                        ItemKind::Album => ("album", x["title"].as_str(), format!("Album • {}", x["artist"]["name"].as_str().unwrap_or("")), x["cover_medium"].as_str()),
                        ItemKind::Artist => ("artist", x["name"].as_str(), "Artist".to_string(), x["picture_medium"].as_str()),
                        _ => ("playlist", x["title"].as_str(), format!("Playlist • {} songs", x["nb_tracks"].as_u64().unwrap_or(0)), x["picture_medium"].as_str()),
                    };
                    Some(Item { kind: kind.clone(), id: format!("dz:{prefix}:{id}"), title: title.unwrap_or("").to_string(), subtitle: format!("Deezer • {sub}"), thumb: thumb.map(String::from), track: None })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Songs, then albums, artists and playlists from Deezer's catalogue.
pub fn deezer_search(query: &str) -> Result<Vec<Item>, String> {
    let q = urlencoding::encode(query);
    let tracks = deezer_get(&format!("/search?q={q}&limit=25"))?;
    let mut items: Vec<Item> = tracks["data"].as_array().map(|a| a.iter().filter_map(|t| dz_track(t, None)).map(|t| {
        let mut it = Item::from_track(&t);
        it.subtitle = format!("Deezer • {}", t.artist);
        it
    }).collect()).unwrap_or_default();
    if let Ok(v) = deezer_get(&format!("/search/album?q={q}&limit=10")) {
        items.extend(dz_list(&v, ItemKind::Album));
    }
    if let Ok(v) = deezer_get(&format!("/search/artist?q={q}&limit=6")) {
        items.extend(dz_list(&v, ItemKind::Artist));
    }
    if let Ok(v) = deezer_get(&format!("/search/playlist?q={q}&limit=10")) {
        items.extend(dz_list(&v, ItemKind::Playlist));
    }
    Ok(items)
}

/// Deezer's chart for the home page.
pub fn deezer_charts() -> Result<Vec<Section>, String> {
    let mut out = vec![];
    let v = deezer_get("/chart/0/tracks?limit=20")?;
    let items: Vec<Item> = v["data"].as_array().map(|a| a.iter().filter_map(|t| dz_track(t, None)).map(|t| Item::from_track(&t)).collect()).unwrap_or_default();
    out.push(Section { title: "Top songs on Deezer".into(), items });
    if let Ok(v) = deezer_get("/chart/0/playlists?limit=12") {
        out.push(Section { title: "Deezer playlists".into(), items: dz_list(&v, ItemKind::Playlist) });
    }
    if let Ok(v) = deezer_get("/chart/0/albums?limit=12") {
        out.push(Section { title: "Popular albums on Deezer".into(), items: dz_list(&v, ItemKind::Album) });
    }
    Ok(out)
}

/// A Deezer album, playlist or artist (`dz:<kind>:<id>`) as a playlist page.
pub fn deezer_collection(id: &str) -> Result<PlaylistPage, String> {
    let mut parts = id.splitn(3, ':');
    let (_, kind, did) = (parts.next(), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let mut page = PlaylistPage { id: id.to_string(), ..Default::default() };
    match kind {
        "album" => {
            let v = deezer_get(&format!("/album/{did}"))?;
            page.title = v["title"].as_str().unwrap_or("Album").to_string();
            page.owner = v["artist"]["name"].as_str().unwrap_or("").to_string();
            page.detail = v["release_date"].as_str().unwrap_or("").to_string();
            page.thumb = v["cover_big"].as_str().map(String::from);
            page.tracks = v["tracks"]["data"].as_array().map(|a| a.iter().filter_map(|t| dz_track(t, Some(&v))).collect()).unwrap_or_default();
        }
        "playlist" => {
            let v = deezer_get(&format!("/playlist/{did}"))?;
            page.title = v["title"].as_str().unwrap_or("Playlist").to_string();
            page.owner = v["creator"]["name"].as_str().unwrap_or("").to_string();
            page.thumb = v["picture_big"].as_str().map(String::from);
            page.tracks = v["tracks"]["data"].as_array().map(|a| a.iter().filter_map(|t| dz_track(t, None)).collect()).unwrap_or_default();
        }
        "artist" => {
            let v = deezer_get(&format!("/artist/{did}"))?;
            page.title = v["name"].as_str().unwrap_or("Artist").to_string();
            page.owner = "Artist".into();
            page.thumb = v["picture_big"].as_str().map(String::from);
            let top = deezer_get(&format!("/artist/{did}/top?limit=50"))?;
            page.tracks = top["data"].as_array().map(|a| a.iter().filter_map(|t| dz_track(t, None)).collect()).unwrap_or_default();
        }
        _ => return Err("Unknown Deezer item".into()),
    }
    Ok(page)
}

// ---------------------------------------------------------------- Google account (YouTube Music)

struct GoogleSession {
    cookie_header: String,
    sapisid: String,
}

static SESSION: Mutex<Option<GoogleSession>> = Mutex::new(None);

fn cookie_file() -> std::path::PathBuf {
    data_dir().join("youtube-cookies.txt")
}

/// Read the YouTube sign-in from a browser (through yt-dlp) and keep it for YouTube Music requests.
pub fn connect_google(browser: &str) -> Result<(), String> {
    let bin = crate::helper::ensure()?;
    // yt-dlp refuses a cookie file that already exists but is empty ("does not look like a Netscape
    // format cookies file"), which is what a previous disconnect or failed attempt used to leave
    // behind. So write to a fresh file and only move it into place once it has the sign-in in it.
    let path = cookie_file();
    let fresh = path.with_extension("new");
    let _ = std::fs::remove_file(&fresh);
    let out = crate::sys::cmd(bin)
        .args(["--cookies-from-browser", browser, "--cookies"])
        .arg(&fresh)
        .args(["-q", "--no-warnings", "--skip-download", "--flat-playlist", "--playlist-items", "1", "https://music.youtube.com/playlist?list=LM"])
        .output()
        .map_err(|e| e.to_string())?;
    let has_cookies = std::fs::metadata(&fresh).map(|m| m.len() > 0).unwrap_or(false);
    if !has_cookies {
        let _ = std::fs::remove_file(&fresh);
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err.lines().filter(|l| l.contains("ERROR")).last().unwrap_or("couldn't read the browser's sign-in").replace("ERROR: ", "");
        return Err(crate::helper::browser_help(browser, &line));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&fresh, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&fresh, &path).map_err(|e| e.to_string())?;
    load_google()?;
    validate_google()
}

/// The original's `YouTube.validateLogin`: ask the account menu (WEB_REMIX) and check the reply
/// has the active-account header, or an account name that isn't the "Sign in" prompt.
pub fn validate_google() -> Result<(), String> {
    let v = crate::ytm::account_menu_text()?;
    let signed_in = v.contains("activeAccountHeaderRenderer") || (v.contains("accountName") && !v.contains("Sign in"));
    if signed_in {
        Ok(())
    } else {
        disconnect_google();
        Err("YouTube says this browser isn't signed in. Sign in to music.youtube.com there, then connect again.".into())
    }
}

/// Load a previously saved sign-in; Err when there is none or it has no Google session.
pub fn load_google() -> Result<(), String> {
    let text = std::fs::read_to_string(cookie_file()).map_err(|_| "Not signed in with Google".to_string())?;
    let mut pairs: Vec<(String, String)> = vec![];
    let mut sapisid = String::new();
    for line in text.lines() {
        let line = line.strip_prefix("#HttpOnly_").unwrap_or(line);
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 7 || !f[0].ends_with("youtube.com") {
            continue;
        }
        let (name, value) = (f[5].to_string(), f[6].to_string());
        if name == "SAPISID" || (sapisid.is_empty() && name == "__Secure-3PAPISID") {
            sapisid = value.clone();
        }
        if !pairs.iter().any(|(n, _)| n == &name) {
            pairs.push((name, value));
        }
    }
    if sapisid.is_empty() {
        *SESSION.lock().unwrap() = None;
        return Err("No Google sign-in found in that browser. Sign in to music.youtube.com there first, then try again.".into());
    }
    let cookie_header = pairs.iter().map(|(n, v)| format!("{n}={v}")).collect::<Vec<_>>().join("; ");
    *SESSION.lock().unwrap() = Some(GoogleSession { cookie_header, sapisid });
    Ok(())
}

pub fn google_connected() -> bool {
    SESSION.lock().unwrap().is_some()
}

pub fn disconnect_google() {
    *SESSION.lock().unwrap() = None;
    // Remove the file (an empty one would make the next sign-in attempt fail).
    let _ = std::fs::remove_file(cookie_file());
}

/// Headers that make a YouTube Music request count as the signed-in account (SAPISIDHASH).
pub fn google_headers() -> Option<(String, String)> {
    use sha1::Digest;
    let s = SESSION.lock().unwrap();
    let s = s.as_ref()?;
    let ts = chrono::Utc::now().timestamp();
    let origin = "https://music.youtube.com";
    let hash = sha1::Sha1::digest(format!("{ts} {} {origin}", s.sapisid).as_bytes());
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    Some((s.cookie_header.clone(), format!("SAPISIDHASH {ts}_{hex}")))
}

pub struct GoogleLibrary {
    pub liked: Vec<Track>,
    pub playlists: Vec<Item>,
    pub albums: Vec<Item>,
    pub artists: Vec<Item>,
}

/// The signed-in account's YouTube Music library.
pub fn google_library() -> Result<GoogleLibrary, String> {
    if !google_connected() {
        return Err("Not signed in with Google".into());
    }
    let liked = crate::ytm::playlist("VLLM").map(|p| p.tracks).unwrap_or_default();
    let grid = |id: &str| -> Vec<Item> { crate::ytm::category(id, None).map(|s| s.into_iter().flat_map(|x| x.items).collect()).unwrap_or_default() };
    let playlists: Vec<Item> = grid("FEmusic_liked_playlists").into_iter().filter(|i| i.kind == ItemKind::Playlist && i.id != "VLLM" && !i.id.is_empty()).collect();
    let albums: Vec<Item> = grid("FEmusic_liked_albums").into_iter().filter(|i| i.kind == ItemKind::Album).collect();
    let artists: Vec<Item> = grid("FEmusic_library_corporate_artists").into_iter().filter(|i| i.kind == ItemKind::Artist).collect();
    if liked.is_empty() && playlists.is_empty() && albums.is_empty() && artists.is_empty() {
        return Err("YouTube Music didn't return a library for this account. Check you're signed in to music.youtube.com in that browser.".into());
    }
    Ok(GoogleLibrary { liked, playlists, albums, artists })
}
