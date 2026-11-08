//! Data shared by every screen: songs, albums, artists, playlists and feed sections.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Source {
    /// A YouTube Music song or video, by its 11-character id.
    YouTube(String),
    /// A file on this computer.
    Local(String),
    /// A song from the person's Spotify library; it plays from its YouTube Music match.
    Spotify(String),
    /// A Deezer catalogue track (public, no key); it plays from its YouTube Music match.
    Deezer(String),
    /// A link yt-dlp can play directly (SoundCloud and other sites it understands).
    Url(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    pub source: Source,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub artist_id: Option<String>,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub album_id: Option<String>,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub thumb: Option<String>,
}

impl Track {
    /// A stable key for likes, queues and history.
    pub fn key(&self) -> String {
        match &self.source {
            Source::YouTube(id) => format!("yt:{id}"),
            Source::Local(p) => format!("file:{p}"),
            Source::Spotify(id) => format!("sp:{id}"),
            Source::Deezer(id) => format!("dz:{id}"),
            Source::Url(u) => format!("url:{u}"),
        }
    }
    /// Plays from the internet (YouTube Music, directly or through a Spotify match).
    pub fn is_online(&self) -> bool {
        !matches!(self.source, Source::Local(_))
    }
    pub fn video_id(&self) -> Option<&str> {
        match &self.source {
            Source::YouTube(id) => Some(id),
            _ => None,
        }
    }
    pub fn share_link(&self) -> String {
        match &self.source {
            Source::YouTube(id) => format!("https://music.youtube.com/watch?v={id}"),
            Source::Local(p) => p.clone(),
            Source::Spotify(id) => format!("https://open.spotify.com/track/{id}"),
            Source::Deezer(id) => format!("https://www.deezer.com/track/{id}"),
            Source::Url(u) => u.clone(),
        }
    }
    /// Large cover art (YouTube thumbnails accept a size suffix).
    pub fn big_thumb(&self) -> Option<String> {
        self.thumb.as_ref().map(|t| upscale_thumb(t, 544))
    }
}

/// Ask Google's image servers for a bigger version of a thumbnail.
/// A wide, high-resolution banner picture. Google's picture servers crop to the faces when asked with `-p`
/// (smart crop), so the artist's face stays in the frame.
pub fn banner_thumb(url: &str) -> String {
    if url.contains("googleusercontent.com") || url.contains("ggpht.com") {
        if let Some(pos) = url.rfind('=') {
            return format!("{}=w1920-h1080-p-l90-rj", &url[..pos]);
        }
    }
    url.to_string()
}

pub fn upscale_thumb(url: &str, size: u32) -> String {
    if url.contains("googleusercontent.com") || url.contains("ggpht.com") {
        if let Some(pos) = url.rfind('=') {
            return format!("{}=w{size}-h{size}-l90-rj", &url[..pos]);
        }
    }
    if url.contains("i.ytimg.com") {
        return url.replace("sddefault", "hqdefault");
    }
    url.to_string()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemKind {
    Song,
    Video,
    Album,
    Artist,
    Playlist,
    Episode,
    Podcast,
}

impl ItemKind {
    pub fn label(&self) -> &'static str {
        match self {
            ItemKind::Song => "Song",
            ItemKind::Video => "Video",
            ItemKind::Album => "Album",
            ItemKind::Artist => "Artist",
            ItemKind::Playlist => "Playlist",
            ItemKind::Episode => "Episode",
            ItemKind::Podcast => "Podcast",
        }
    }
}

/// Anything shown as a card or a row: a song, an album, an artist or a playlist.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub kind: ItemKind,
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub thumb: Option<String>,
    #[serde(default)]
    pub track: Option<Track>,
}

impl Item {
    pub fn from_track(t: &Track) -> Item {
        Item {
            kind: ItemKind::Song,
            id: t.key(),
            title: t.title.clone(),
            subtitle: t.artist.clone(),
            thumb: t.thumb.clone(),
            track: Some(t.clone()),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Section {
    pub title: String,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, Default)]
pub struct AlbumPage {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub artist_id: Option<String>,
    pub subtitle: String,
    pub detail: String,
    pub thumb: Option<String>,
    pub tracks: Vec<Track>,
    pub more: Vec<Section>,
}

#[derive(Clone, Debug, Default)]
pub struct ArtistPage {
    pub id: String,
    pub name: String,
    pub listeners: String,
    pub description: String,
    pub thumb: Option<String>,
    pub top_songs: Vec<Track>,
    pub sections: Vec<Section>,
    pub radio_playlist: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct PlaylistPage {
    pub id: String,
    pub title: String,
    pub owner: String,
    pub detail: String,
    pub thumb: Option<String>,
    pub tracks: Vec<Track>,
    /// More songs are still being fetched in the background.
    pub loading_more: bool,
}

#[derive(Clone, Debug)]
pub struct Category {
    pub title: String,
    pub browse_id: String,
    pub params: Option<String>,
    pub color: u32,
}

pub fn format_ms(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

pub fn parse_duration(text: &str) -> Option<u64> {
    let parts: Vec<&str> = text.trim().split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let mut total = 0u64;
    for p in &parts {
        if p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        total = total * 60 + p.parse::<u64>().ok()?;
    }
    Some(total * 1000)
}
