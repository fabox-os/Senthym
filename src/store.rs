//! Everything the app remembers between runs, saved as JSON in ~/.local/share/senthym.
//! Mirrors the original's SharedPreferences stores (LikedSongs, ListeningHistory,
//! RecentItems, LocalLibrary, Downloads, AlternativeStreams, settings_prefs).

use crate::model::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub const MAX_HISTORY: usize = 500; // ListeningHistoryPrefKt.MAX_ENTRIES
pub const MAX_RECENT_ITEMS: usize = 20; // RecentItemsPrefKt.MAX_ITEMS
pub const MAX_RECENT_SEARCHES: usize = 20;
pub const CROSSFADE_DEFAULT_MS: u32 = 6000;
pub const CROSSFADE_MAX_MS: u32 = 12000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quality {
    Low,
    Normal,
    High,
    Lossless,
}

impl Quality {
    pub const ALL: [Quality; 4] = [Quality::Low, Quality::Normal, Quality::High, Quality::Lossless];
    pub fn label(&self) -> &'static str {
        match self {
            Quality::Low => "Low",
            Quality::Normal => "Normal",
            Quality::High => "High",
            Quality::Lossless => "Lossless",
        }
    }
    pub fn detail(&self) -> &'static str {
        match self {
            Quality::Low => "Data saver — smallest size",
            Quality::Normal => "Balanced for the network",
            Quality::High => "Best compressed quality",
            Quality::Lossless => "FLAC when available, else High",
        }
    }
    /// yt-dlp format choice for this quality (m4a first: it plays without extra tools).
    pub fn ytdlp_format(&self) -> &'static str {
        match self {
            Quality::Low => "worstaudio[ext=m4a]/worstaudio",
            Quality::Normal => "bestaudio[ext=m4a][abr<=140]/bestaudio[ext=m4a]/bestaudio",
            Quality::High | Quality::Lossless => "bestaudio[ext=m4a]/bestaudio",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub stream_quality: Quality,
    pub download_quality: Quality,
    /// Older saves had a separate on/off switch; read once, then dropped.
    #[serde(skip_serializing)]
    pub crossfade_enabled: Option<bool>,
    /// 0 = crossfade off (SettingsPrefKt.isCrossfadeEnabled: getCrossfadeMs > 0).
    pub crossfade_ms: u32,
    pub crossfade_dj: bool,
    pub preload: bool,
    pub video_fallback: bool,
    pub library_grid: bool,
    pub lyric_translate_lang: String,
    pub autoplay_radio: bool,
    /// Look for a newer Senthym release on GitHub when the app starts.
    pub check_updates: bool,
    /// The release version whose "new version" notice was dismissed with "Later".
    pub update_dismissed: String,
    /// Browser whose YouTube sign-in the streaming helper may use ("" = none).
    pub youtube_browser: String,
    pub volume: f32,
    pub download_dir: String,
    /// Play a song's music video (picture) when it has one; false = audio only.
    pub prefer_video: bool,
    /// Music video quality: 0 = automatic (follows the connection and the view), or a fixed height (240..1080).
    pub video_quality: u32,
    /// Set once the quality was reset to Auto (older saves may hold a fixed height nobody chose on purpose).
    pub video_quality_auto_reset: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let music = dirs::audio_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Music"));
        Settings {
            stream_quality: Quality::High,
            download_quality: Quality::High,
            crossfade_enabled: None,
            crossfade_ms: 0,
            crossfade_dj: false,
            preload: true,
            video_fallback: true,
            library_grid: false,
            lyric_translate_lang: String::new(),
            autoplay_radio: true,
            check_updates: true,
            update_dismissed: String::new(),
            youtube_browser: String::new(),
            volume: 0.8,
            download_dir: music.join("Senthym").to_string_lossy().into_owned(),
            prefer_video: true,
            video_quality: 0,
            video_quality_auto_reset: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub ts: i64,
    pub track: Track,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserPlaylist {
    pub id: String,
    pub name: String,
    pub tracks: Vec<Track>,
    pub created: i64,
    /// A picture chosen by the person (a copy kept in the app's folder); None = the first song's cover.
    #[serde(default)]
    pub cover: Option<String>,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DownloadEntry {
    pub track: Track,
    pub path: String,
    pub ts: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum AltStream {
    YouTube(String),
    Local(String),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Store {
    pub settings: Settings,
    pub liked: Vec<Track>,
    pub saved_albums: Vec<Item>,
    pub saved_playlists: Vec<Item>,
    pub followed_artists: Vec<Item>,
    pub playlists: Vec<UserPlaylist>,
    pub history: Vec<HistoryEntry>,
    pub recent_items: Vec<Item>,
    pub recent_searches: Vec<String>,
    /// Things picked from search (artists, albums, playlists, songs, videos), newest first.
    pub recent_search_items: Vec<Item>,
    /// Artists the person chose not to hear ("id:<artist id>" and "name:<lowercase name>").
    pub blocked_artists: Vec<String>,
    /// Songs hidden inside one playlist (playlist id → song keys): they stay in the list, greyed, and are skipped.
    pub hidden_songs: HashMap<String, Vec<String>>,
    pub local_tracks: Vec<Track>,
    pub local_folders: Vec<String>,
    pub downloads: Vec<DownloadEntry>,
    pub alt_streams: HashMap<String, AltStream>,
    /// Spotify library (refreshed after signing in).
    pub sp_user: Option<crate::spotify::User>,
    pub sp_liked: Vec<Track>,
    pub sp_playlists: Vec<Item>,
    pub sp_albums: Vec<Item>,
    pub sp_artists: Vec<Item>,
    pub sp_top: Vec<Track>,
    pub sp_recent: Vec<Track>,
    /// Spotify track id → YouTube Music video id.
    pub sp_matches: HashMap<String, String>,
    /// The person chose to use the app without an account: hides sign-in prompts and entries.
    pub welcome_dismissed: bool,
    /// YouTube Music library of the connected Google account (kept in the app).
    pub gl_liked: Vec<Track>,
    pub gl_playlists: Vec<Item>,
    pub gl_albums: Vec<Item>,
    pub gl_artists: Vec<Item>,
    pub last_queue: Vec<Track>,
    pub last_index: usize,
    pub last_position_ms: u64,
    /// The last few songs the person chose themselves, newest first: what autoplay follows.
    pub taste_anchors: Vec<Track>,
}

/// `SENTHYM_HOME` keeps everything in one folder instead (used for testing).
fn override_dir(sub: &str) -> Option<PathBuf> {
    std::env::var_os("SENTHYM_HOME").map(|h| PathBuf::from(h).join(sub))
}

/// The folders used before the app was renamed (earlier project name). Moved over on the first start.
fn move_legacy(old: PathBuf, new: &PathBuf, fix_paths: bool) {
    if new.exists() || !old.exists() {
        return;
    }
    if let Some(parent) = new.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::rename(&old, new).is_err() {
        return; // another disk or no permission: start fresh rather than risk a half move
    }
    if fix_paths {
        // Saved paths (for example a playlist cover) point into the old folder: point them to the new one.
        let lib = new.join("library.json");
        if let Ok(text) = std::fs::read_to_string(&lib) {
            let (o, n) = (old.to_string_lossy().into_owned(), new.to_string_lossy().into_owned());
            let fixed = text.replace(&o, &n);
            if fixed != text {
                let _ = std::fs::write(&lib, fixed);
            }
        }
    }
}

pub fn data_dir() -> PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    let d = override_dir("data").unwrap_or_else(|| base.join("senthym"));
    ONCE.call_once(|| {
        if override_dir("data").is_none() {
            move_legacy(base.join("sonora"), &d, true);
        }
    });
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn cache_dir() -> PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let base = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
    let d = override_dir("cache").unwrap_or_else(|| base.join("senthym"));
    ONCE.call_once(|| {
        if override_dir("cache").is_none() {
            move_legacy(base.join("sonora"), &d, false);
        }
    });
    let _ = std::fs::create_dir_all(&d);
    d
}

impl Store {
    fn path() -> PathBuf {
        data_dir().join("library.json")
    }

    pub fn load() -> Store {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str::<Store>(&s).ok())
            .map(|mut st| {
                if st.settings.crossfade_enabled.take() == Some(false) {
                    st.settings.crossfade_ms = 0;
                }
                st.settings.crossfade_ms = st.settings.crossfade_ms.min(CROSSFADE_MAX_MS);
                if !st.settings.video_quality_auto_reset {
                    st.settings.video_quality = 0;
                    st.settings.video_quality_auto_reset = true;
                }
                st
            })
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(s) = serde_json::to_string(self) {
            let tmp = Self::path().with_extension("json.tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(tmp, Self::path());
            }
        }
    }

    pub fn is_liked(&self, t: &Track) -> bool {
        let k = t.key();
        self.liked.iter().any(|x| x.key() == k)
    }

    /// Toggle a like; returns true when the song is now liked.
    pub fn toggle_like(&mut self, t: &Track) -> bool {
        let k = t.key();
        if let Some(pos) = self.liked.iter().position(|x| x.key() == k) {
            self.liked.remove(pos);
            false
        } else {
            self.liked.insert(0, t.clone());
            true
        }
    }

    /// History: newest first, a repeat of the newest song isn't logged twice, 500 at most.
    pub fn add_history(&mut self, t: &Track) {
        if t.title.trim().is_empty() {
            return;
        }
        if self.history.first().map(|h| h.track.key() == t.key()).unwrap_or(false) {
            return;
        }
        self.history.insert(0, HistoryEntry { ts: chrono::Local::now().timestamp_millis(), track: t.clone() });
        self.history.truncate(MAX_HISTORY);
    }

    pub fn add_recent_item(&mut self, it: &Item) {
        self.recent_items.retain(|x| x.id != it.id);
        self.recent_items.insert(0, it.clone());
        self.recent_items.truncate(MAX_RECENT_ITEMS);
    }

    pub fn is_blocked(&self, t: &Track) -> bool {
        if self.blocked_artists.is_empty() {
            return false;
        }
        if let Some(id) = &t.artist_id {
            if self.blocked_artists.contains(&format!("id:{id}")) {
                return true;
            }
        }
        t.artist.split(',').any(|n| self.blocked_artists.contains(&format!("name:{}", n.trim().to_lowercase())))
    }

    pub fn artist_blocked(&self, id: &str, name: &str) -> bool {
        self.blocked_artists.contains(&format!("id:{id}")) || self.blocked_artists.contains(&format!("name:{}", name.trim().to_lowercase()))
    }

    pub fn add_search_item(&mut self, it: &Item) {
        if it.id == "liked" || it.id == "local" || it.title.trim().is_empty() {
            return;
        }
        self.recent_search_items.retain(|x| x.id != it.id);
        self.recent_search_items.insert(0, it.clone());
        self.recent_search_items.truncate(50);
    }

    /// Add an older entry at the end of the list (keeps the order of what was searched before).
    pub fn push_search_item_back(&mut self, it: &Item) {
        if it.title.trim().is_empty() || self.recent_search_items.iter().any(|x| x.id == it.id) {
            return;
        }
        self.recent_search_items.push(it.clone());
        self.recent_search_items.truncate(50);
    }

    pub fn add_recent_search(&mut self, q: &str) {
        let q = q.trim();
        if q.is_empty() {
            return;
        }
        self.recent_searches.retain(|x| !x.eq_ignore_ascii_case(q));
        self.recent_searches.insert(0, q.to_string());
        self.recent_searches.truncate(MAX_RECENT_SEARCHES);
    }

    pub fn download_for(&self, t: &Track) -> Option<&DownloadEntry> {
        let k = t.key();
        self.downloads.iter().find(|d| d.track.key() == k && std::path::Path::new(&d.path).exists())
    }

    pub fn is_hidden(&self, pid: &str, key: &str) -> bool {
        self.hidden_songs.get(pid).map(|v| v.iter().any(|k| k == key)).unwrap_or(false)
    }

    /// Hide or show a song inside one playlist.
    pub fn set_hidden(&mut self, pid: &str, key: &str, hide: bool) {
        let list = self.hidden_songs.entry(pid.to_string()).or_default();
        list.retain(|k| k != key);
        if hide {
            list.push(key.to_string());
        }
        if list.is_empty() {
            self.hidden_songs.remove(pid);
        }
    }

    pub fn is_saved(&self, kind: &ItemKind, id: &str) -> bool {
        match kind {
            ItemKind::Album => self.saved_albums.iter().any(|x| x.id == id),
            ItemKind::Artist => self.followed_artists.iter().any(|x| x.id == id),
            ItemKind::Playlist => self.saved_playlists.iter().any(|x| x.id == id),
            _ => false,
        }
    }

    /// Save or remove an album / artist / playlist from the library; true when now saved.
    pub fn toggle_saved(&mut self, it: &Item) -> bool {
        let list = match it.kind {
            ItemKind::Album => &mut self.saved_albums,
            ItemKind::Artist => &mut self.followed_artists,
            ItemKind::Playlist => &mut self.saved_playlists,
            _ => return false,
        };
        if let Some(p) = list.iter().position(|x| x.id == it.id) {
            list.remove(p);
            false
        } else {
            list.insert(0, it.clone());
            true
        }
    }

    pub fn new_playlist(&mut self, name: &str) -> String {
        let id = format!("local-{}", chrono::Local::now().timestamp_millis());
        let name = if name.trim().is_empty() { format!("My Playlist #{}", self.playlists.len() + 1) } else { name.trim().to_string() };
        self.playlists.insert(0, UserPlaylist { id: id.clone(), name, tracks: vec![], created: chrono::Local::now().timestamp_millis(), cover: None, description: String::new() });
        id
    }

    /// Add to a user playlist; false when it was already there.
    pub fn add_to_playlist(&mut self, pid: &str, t: &Track) -> bool {
        if let Some(p) = self.playlists.iter_mut().find(|p| p.id == pid) {
            if p.tracks.iter().any(|x| x.key() == t.key()) {
                return false;
            }
            p.tracks.push(t.clone());
            return true;
        }
        false
    }
}

impl Settings {
    pub fn crossfade_on(&self) -> bool {
        self.crossfade_ms > 0
    }
}
