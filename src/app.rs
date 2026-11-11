//! App state: navigation, background jobs, the play queue and the player controller.

use crate::audio::engine::{Engine, Status};
use crate::helper;
use crate::images::Images;
use crate::lyrics::{self, Lyrics};
use crate::model::*;
use crate::store::{AltStream, DownloadEntry, Store};
use crate::ytm;
use rand::seq::SliceRandom;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

/// Last-position save interval (12 ticks × 250 ms, SongPlayer$startPositionWatch$1).
pub const POSITION_SAVE_MS: u64 = 12 * 250;

#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    Home,
    Search,
    Library,
    Album(String),
    Artist(String),
    Playlist(String),
    UserPlaylist(String),
    Liked,
    LocalFiles,
    Downloads,
    History,
    Category { title: String, browse_id: String, params: Option<String> },
    Settings,
    Queue,
    Lyrics,
    Player,
    /// A Spotify playlist, album or artist: `sp:<kind>:<id>`.
    SpotifyCollection(String),
    SpotifyLiked,
    SpotifyLogin,
}

#[derive(Clone)]
pub enum Loadable<T> {
    Loading,
    Ready(T),
    Failed(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SearchTab {
    Top,
    Songs,
    Albums,
    Artists,
    Playlists,
    Videos,
}

impl SearchTab {
    pub const ALL: [SearchTab; 6] = [SearchTab::Top, SearchTab::Songs, SearchTab::Albums, SearchTab::Artists, SearchTab::Playlists, SearchTab::Videos];
    pub fn label(&self) -> &'static str {
        match self {
            SearchTab::Top => "All",
            SearchTab::Songs => "Songs",
            SearchTab::Albums => "Albums",
            SearchTab::Artists => "Artists",
            SearchTab::Playlists => "Playlists",
            SearchTab::Videos => "Videos",
        }
    }
    fn filter(&self) -> Option<&'static str> {
        match self {
            SearchTab::Top => None,
            SearchTab::Songs => Some(ytm::FILTER_SONGS),
            SearchTab::Albums => Some(ytm::FILTER_ALBUMS),
            SearchTab::Artists => Some(ytm::FILTER_ARTISTS),
            SearchTab::Playlists => Some(ytm::FILTER_PLAYLISTS),
            SearchTab::Videos => Some(ytm::FILTER_VIDEOS),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResolvePurpose {
    Play(u64),
    Preload,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RadioPurpose {
    /// Start a radio from this song now.
    Start,
    /// Add more songs at the end of the queue.
    Extend,
    /// The queue ran out: add songs and keep playing.
    Continue,
}

enum Msg {
    Home(Result<Vec<Section>, String>),
    Search(String, SearchTab, Result<Vec<Item>, String>),
    /// More results for a search from another source (Deezer, SoundCloud), added when they arrive.
    SearchMore(String, SearchTab, Vec<Item>),
    Suggest(String, Vec<String>),
    BrowseCover(String, Option<String>),
    NowRelated(String, Result<Vec<Item>, String>),
    /// The music video of a song (None: it has none), found by song key.
    VideoFound(String, Option<String>),
    /// A typed search turned into the thing it found first (with its cover).
    RecentResolved(String, Option<Item>),
    /// An artist found by name for a Follow click.
    FollowFound(String, Option<Item>),
    /// A direct address for a music video's picture at a height (by video id).
    VideoAddr(String, u32, Result<String, String>),
    /// The audio version of the current song is ready to switch to.
    SwitchAudio(u64, String, Result<PathBuf, String>),
    Album(String, Result<AlbumPage, String>),
    Artist(String, Result<ArtistPage, String>),
    Playlist(String, Result<PlaylistPage, String>),
    Categories(Result<Vec<Category>, String>),
    Category(String, Result<Vec<Section>, String>),
    Radio(RadioPurpose, Track, Result<Vec<Track>, String>),
    Resolved(String, ResolvePurpose, Result<PathBuf, String>),
    Lyrics(String, Option<Lyrics>),
    Translation(String, String, Result<Vec<String>, String>),
    Downloaded(String, Track, Result<String, String>),
    LocalScan(Vec<Track>),
    Helper(Option<String>),
    HelperUpdate(Result<String, String>),
    /// A look for a newer Senthym release finished (true: the person asked for it).
    UpdateChecked(Result<Option<crate::update::Release>, String>, bool),
    /// The new version's package was downloaded and handed over: (what to tell the person).
    UpdateDone(Result<crate::update::Outcome, String>),
    /// Play an album or playlist once its songs are known.
    PlayCollection(Item, bool, Result<Vec<Track>, String>),
    SpotifyLogin(Result<crate::spotify::User, String>),
    SpotifySync(Result<crate::spotify::Library, String>),
    SpotifyCollection(String, Result<PlaylistPage, String>),
    /// A Spotify song was found on YouTube Music.
    SpotifyMatch(String, String),
    GoogleConnect(Result<(), String>),
    GoogleSync(Result<crate::providers::GoogleLibrary, String>),
    DeezerHome(Result<Vec<Section>, String>),
    /// Missing details (artist, album) of a song, looked up on YouTube Music.
    TrackInfo(String, Track),
    Nothing,
    /// A short message to show (from a background task).
    Note(String),
}

pub enum LyricsState {
    Loading,
    Found(Lyrics, #[allow(dead_code)] Instant),
    Missing(Instant),
}

/// Something the user asked for; collected while drawing and applied after.
pub enum Action {
    Navigate(Route),
    Back,
    Forward,
    PlayTracks { tracks: Vec<Track>, start: usize, shuffle: bool, context: Option<Item> },
    PlayItem(Item, bool),
    /// Warm up a song the person is likely to play next (hovered, or first in an opened list).
    Prefetch(Track),
    /// Like `Prefetch`, but for something the pointer rests on: goes to the front of the line.
    PrefetchHot(Track),
    PrefetchRoute(Route),
    /// Show the music video (true) or play the audio version only (false).
    SetVideoMode(bool),
    SetVideoHeight(u32),
    OpenItem(Item),
    TogglePlay,
    Next,
    Prev,
    Seek(u64),
    SetVolume(f32),
    ToggleShuffle,
    ToggleRepeat,
    PlayQueueIndex(usize),
    AddQueue(Track),
    PlayNext(Track),
    RemoveFromQueue(usize),
    MoveQueue(usize, usize),
    ToggleLike(Track),
    ToggleSaved(Item),
    /// Hide (or show again) a song inside one playlist: id, name, song, hide.
    HideInPlaylist(String, String, Track, bool),
    /// Follow (or unfollow) an artist known only by name: finds the artist page first.
    FollowByName(String),
    GoAlbum(Track),
    GoArtist(Track),
    Share(String),
    StartRadio(Track),
    Download(Track),
    DownloadMany(Vec<Track>),
    RemoveDownload(Track),
    CancelDownloads,
    NewPlaylist(String, Option<Track>),
    AddToPlaylist(String, Track),
    RemoveFromPlaylist(String, usize),
    DeletePlaylist(String),
    RenamePlaylist(String, String),
    /// Save a playlist's details. `cover`: None = keep, Some(None) = back to the first song's cover,
    /// Some(Some(path)) = use this picture file.
    UpdatePlaylist { pid: String, name: String, desc: String, cover: Option<Option<String>> },
    Search(String),
    SetAlt(Track, AltStream),
    ClearAlt(Track),
    ImportFolder(String),
    ImportFiles(Vec<String>),
    RemoveLocal(Track),
    ClearHistory,
    RemoveHistory(i64),
    ClearRecentSearches,
    RemoveRecentSearch(String),
    /// Keep something picked from search in the recent searches.
    RememberSearch(Item),
    RemoveSearchItem(String),
    /// Stop (or resume) playing an artist: (artist id, name).
    ToggleBlockArtist(String, String),
    OpenUrl(String),
    /// Copy text to the clipboard and say so: (text, message).
    CopyText(String, String),
    Translate(bool),
    UpdateHelper,
    CheckUpdate,
    DownloadUpdate,
    DismissUpdate,
    ClearCache,
    Toast(String),
    SaveSettings,
}

/// The music video being shown for the current song.
/// The "Edit details" window of a playlist the person made.
pub struct EditPlaylist {
    pub pid: String,
    pub name: String,
    pub desc: String,
    /// A newly chosen picture file (not copied until Save).
    pub new_cover: Option<String>,
    pub remove_cover: bool,
}

pub struct VideoSession {
    pub track_key: String,
    pub video_id: String,
    pub gen: u64,
    pub url: Option<String>,
    pub stream: Option<crate::video::VideoStream>,
    pub tex: Option<egui::TextureHandle>,
    pub error: Option<String>,
    pub height: u32,
    /// Share of the picture's height taken by black bars baked into the clip (top, bottom); only ever shrinks.
    pub bars: Option<(f32, f32)>,
    restart_at: Instant,
    started: Instant,
    /// A picture at another quality being started next to the one on screen; it takes over with its
    /// first frame, so changing quality never freezes the picture.
    next: Option<NextStream>,
}

pub struct NextStream {
    height: u32,
    stream: Option<crate::video::VideoStream>,
    restart_at: Instant,
    /// When the change was asked for, and how often its picture was started.
    asked: Instant,
    tries: u8,
}

/// Where the "update Senthym" flow is.
#[derive(Clone)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available(crate::update::Release),
    Downloading(crate::update::Release, std::sync::Arc<std::sync::atomic::AtomicU64>),
    Done(String),
    Failed(String),
}

pub struct App {
    pub store: Store,
    pub engine: Engine,
    pub images: Images,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub ctx: egui::Context,

    // navigation
    pub route: Route,
    pub back: Vec<Route>,
    pub forward: Vec<Route>,

    // data
    pub home: Loadable<Vec<Section>>,
    pub categories: Loadable<Vec<Category>>,
    pub albums: HashMap<String, Loadable<AlbumPage>>,
    pub artists: HashMap<String, Loadable<ArtistPage>>,
    pub playlists: HashMap<String, Loadable<PlaylistPage>>,
    pub category_pages: HashMap<String, Loadable<Vec<Section>>>,

    // search
    pub search_text: String,
    pub search_query: String,
    pub search_tab: SearchTab,
    pub search_results: HashMap<(String, SearchTab), Loadable<Vec<Item>>>,
    pub suggestions: Vec<String>,
    /// The pointer is over the suggestion list (keeps it open after the search box loses focus).
    pub suggest_hovered: bool,
    /// The suggestion list is open (set by typing or focusing the box; closed by searching, Escape or an outside click).
    pub suggest_open: bool,
    suggest_for: String,
    last_typed: Instant,
    /// Browse tiles: cover picture per title (None while loading or when nothing was found).
    pub browse_covers: HashMap<String, Option<String>>,
    /// Related music videos for the song in the Now playing panel, by song key.
    pub np_related: HashMap<String, Loadable<Vec<Item>>>,
    /// Music video id per song key (None: the song has no official video).
    pub video_ids: HashMap<String, Option<String>>,
    video_lookups: HashSet<String>,
    pub video: Option<VideoSession>,
    video_gen: u64,
    search_extra: HashMap<(String, SearchTab), Vec<Item>>,
    /// Searches started ahead of time that still lack the extra sources.
    search_partial: HashSet<(String, SearchTab)>,
    recent_resolving: HashSet<String>,
    video_urls: HashMap<(String, u32), (String, Instant)>,
    video_url_pending: HashSet<(String, u32)>,
    video_lagged: Option<(Instant, u32)>,
    /// Last time the picture was looked after and the song position then; a long gap (the window was
    /// hidden) or a jump in position (a seek) is noted, so it isn't mistaken for a slow connection.
    video_tick: Instant,
    video_last_pos: u64,
    video_jump_at: Option<Instant>,
    browse_inflight: usize,

    // player
    pub queue: Vec<Track>,
    unshuffled: Option<Vec<Track>>,
    pub index: usize,
    pub current: Option<Track>,
    pub shuffle: bool,
    pub repeat: bool,
    pub loading: bool,
    play_gen: u64,
    pub status: Status,
    ready: HashMap<String, PathBuf>,
    requested: HashSet<String>,
    /// Songs already tried in the background (a failed one is not retried every frame).
    preload_tried: HashSet<String>,
    prefetch_queue: std::collections::VecDeque<Track>,
    /// What is known about a page before it has loaded (title and cover of the card that was clicked).
    pub page_hints: HashMap<String, Item>,
    fade_target: Option<usize>,
    radio_loading: bool,
    /// What the person is listening to lately; it shapes the songs autoplay adds.
    pub taste: crate::taste::Taste,
    /// Keys of the songs autoplay added (as opposed to the person's own choices).
    auto_added: HashSet<String>,
    pub playing_from: String,
    /// The album / playlist / artist the queue was started from (for the highlighting in the library).
    pub playing_ctx: Option<Item>,
    /// The playlist whose songs are being drawn right now (id, name): lets rows and menus offer "Hide in this playlist".
    pub hide_scope: Option<(String, String)>,
    /// Last hide, for the Undo toast: playlist id, song key, playlist name, when.
    pub undo_hide: Option<(String, String, String, Instant)>,
    fail_streak: u32,
    pending_resume_ms: Option<u64>,
    last_save: Instant,

    // lyrics
    pub lyrics: HashMap<String, LyricsState>,
    pub translations: HashMap<(String, String), Loadable<Vec<String>>>,
    pub show_translation: bool,

    // downloads
    pub downloading: Vec<Track>,
    download_queue: Vec<Track>,
    download_running: bool,

    // misc
    pub toasts: Vec<(String, Instant)>,
    pub helper_version: Option<String>,
    pub helper_checked: bool,
    pub helper_busy: bool,
    pub update: UpdateState,
    pub update_kind: crate::update::Kind,
    pub local_scanning: bool,
    pub cache_bytes: u64,
    pub actions: Vec<Action>,

    // screen state
    pub ui_new_playlist: Option<(String, Option<Track>)>,
    pub ui_alt_link: Option<(Track, String)>,
    pub ui_rename: Option<(String, String)>,
    pub ui_edit_playlist: Option<EditPlaylist>,
    pub library_filter: Option<ItemKind>,
    pub library_query: String,
    pub local_tab: usize,
    pub local_query: String,
    pub import_path: String,
    pub playlist_sort: usize,
    pub history_stats: bool,
    /// `--play <query>`: search for a song at start-up and play it.
    pub autoplay_query: Option<String>,
    // spotify
    pub spotify_client_id: String,
    pub spotify_busy: bool,
    pub spotify_syncing: bool,
    pub spotify_error: Option<String>,
    /// The sign-in window shown over the app at start-up.
    pub show_welcome: bool,
    pub google_busy: bool,
    pub google_syncing: bool,
    pub google_error: Option<String>,
    pub deezer_home: Loadable<Vec<Section>>,
    /// `--test-crossfade`: jump near the end of the first song to try the crossfade.
    pub test_seek_end: bool,
    pub test_wait_fade: bool,
}

impl App {
    pub fn new(ctx: egui::Context, _screenshot_mode: bool) -> App {
        let (tx, rx) = channel();
        let store = Store::load();
        let engine = Engine::new();
        engine.set_volume(store.settings.volume);
        let mut app = App {
            engine,
            images: Images::new(),
            tx,
            rx,
            ctx,
            route: Route::Home,
            back: vec![],
            forward: vec![],
            home: Loadable::Loading,
            categories: Loadable::Loading,
            albums: HashMap::new(),
            artists: HashMap::new(),
            playlists: HashMap::new(),
            category_pages: HashMap::new(),
            search_text: String::new(),
            search_query: String::new(),
            search_tab: SearchTab::Top,
            search_results: HashMap::new(),
            suggestions: vec![],
            suggest_hovered: false,
            suggest_open: false,
            suggest_for: String::new(),
            last_typed: Instant::now(),
            browse_covers: HashMap::new(),
            np_related: HashMap::new(),
            video_ids: HashMap::new(),
            video_lookups: HashSet::new(),
            video: None,
            video_gen: 0,
            search_extra: HashMap::new(),
            search_partial: HashSet::new(),
            recent_resolving: HashSet::new(),
            video_urls: HashMap::new(),
            video_url_pending: HashSet::new(),
            video_lagged: None,
            video_tick: Instant::now(),
            video_last_pos: 0,
            video_jump_at: None,
            browse_inflight: 0,
            queue: vec![],
            unshuffled: None,
            index: 0,
            current: None,
            shuffle: false,
            repeat: false,
            loading: false,
            play_gen: 0,
            status: Status::default(),
            ready: HashMap::new(),
            requested: HashSet::new(),
            preload_tried: HashSet::new(),
            prefetch_queue: std::collections::VecDeque::new(),
            page_hints: HashMap::new(),
            fade_target: None,
            radio_loading: false,
            taste: crate::taste::Taste::default(),
            auto_added: HashSet::new(),
            playing_from: String::new(),
            playing_ctx: None,
            hide_scope: None,
            undo_hide: None,
            fail_streak: 0,
            pending_resume_ms: None,
            last_save: Instant::now(),
            lyrics: HashMap::new(),
            translations: HashMap::new(),
            show_translation: false,
            downloading: vec![],
            download_queue: vec![],
            download_running: false,
            toasts: vec![],
            helper_version: None,
            helper_checked: false,
            helper_busy: false,
            update: UpdateState::Idle,
            update_kind: crate::update::Kind::detect(),
            local_scanning: false,
            cache_bytes: 0,
            actions: vec![],
            ui_new_playlist: None,
            ui_alt_link: None,
            ui_rename: None,
            ui_edit_playlist: None,
            library_filter: None,
            library_query: String::new(),
            local_tab: 0,
            local_query: String::new(),
            import_path: dirs::audio_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
            playlist_sort: 0,
            history_stats: false,
            autoplay_query: None,
            spotify_client_id: crate::spotify::saved_client_id(),
            spotify_busy: false,
            spotify_syncing: false,
            spotify_error: None,
            show_welcome: false,
            google_busy: false,
            google_syncing: false,
            google_error: None,
            deezer_home: Loadable::Loading,
            test_seek_end: false,
            test_wait_fade: false,
            store,
        };
        app.load_home();
        app.load_categories();
        app.check_helper();
        helper::cleanup_temp();
        helper::set_browser(&app.store.settings.youtube_browser);
        crate::spotify::load_auth();
        if crate::spotify::logged_in() {
            app.spotify_sync();
        }
        if crate::providers::load_google().is_ok() {
            app.google_sync();
        }
        if !crate::spotify::logged_in() && !crate::providers::google_connected() {
            app.show_welcome = !app.store.welcome_dismissed;
        }
        app.spawn(|| Msg::DeezerHome(crate::providers::deezer_charts()));
        // Pick the person's taste up where they left it, so autoplay carries on in their style.
        app.taste = crate::taste::Taste::restore(app.store.taste_anchors.clone());
        // Bring back the last queue (paused where it stopped), like the original's PlaybackState.
        if !app.store.last_queue.is_empty() {
            app.queue = app.store.last_queue.clone();
            app.index = app.store.last_index.min(app.queue.len() - 1);
            app.current = app.queue.get(app.index).cloned();
            app.pending_resume_ms = Some(app.store.last_position_ms);
        }
        app
    }

    fn spawn<F: FnOnce() -> Msg + Send + 'static>(&self, f: F) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(f());
            ctx.request_repaint();
        });
    }

    pub fn toast(&mut self, s: impl Into<String>) {
        let s = s.into();
        eprintln!("[senthym] {s}");
        self.toasts.push((s, Instant::now()));
    }

    // ------------------------------------------------------------ loading data

    pub fn load_home(&mut self) {
        self.home = Loadable::Loading;
        self.spawn(|| Msg::Home(ytm::home()));
    }

    /// Music videos related to a song (shown in the Now playing panel).
    pub fn ensure_now_related(&mut self, t: &Track) {
        let key = t.key();
        if self.np_related.contains_key(&key) {
            return;
        }
        if !t.is_online() {
            self.np_related.insert(key, Loadable::Ready(vec![]));
            return;
        }
        self.np_related.insert(key.clone(), Loadable::Loading);
        let q = format!("{} {}", t.artist, t.title);
        let own = t.video_id().map(|s| s.to_string());
        self.spawn(move || {
            let r = ytm::search(&q, Some(ytm::FILTER_VIDEOS)).map(|items| {
                items.into_iter().filter(|i| Some(i.id.as_str()) != own.as_deref() && i.track.is_some()).take(10).collect::<Vec<Item>>()
            });
            Msg::NowRelated(key, r)
        });
    }

    /// Fetch a cover for a Browse tile: the first playlist found for its name (a few at a time).
    pub fn ensure_browse_cover(&mut self, title: &str) {
        if self.browse_covers.contains_key(title) || self.browse_inflight >= 6 {
            return;
        }
        self.browse_covers.insert(title.to_string(), None);
        self.browse_inflight += 1;
        let t = title.to_string();
        self.spawn(move || {
            let thumb = ytm::search(&format!("{t} playlist"), Some(ytm::FILTER_PLAYLISTS))
                .ok()
                .and_then(|items| items.into_iter().find_map(|i| i.thumb));
            Msg::BrowseCover(t, thumb)
        });
    }

    pub fn load_categories(&mut self) {
        self.categories = Loadable::Loading;
        self.spawn(|| Msg::Categories(ytm::categories()));
    }

    pub fn ensure_album(&mut self, id: &str) {
        if !self.albums.contains_key(id) || matches!(self.albums.get(id), Some(Loadable::Failed(_))) {
            self.albums.insert(id.to_string(), Loadable::Loading);
            let id = id.to_string();
            self.spawn(move || Msg::Album(id.clone(), ytm::album(&id)));
        }
    }

    pub fn ensure_artist(&mut self, id: &str) {
        if !self.artists.contains_key(id) || matches!(self.artists.get(id), Some(Loadable::Failed(_))) {
            self.artists.insert(id.to_string(), Loadable::Loading);
            let id = id.to_string();
            self.spawn(move || Msg::Artist(id.clone(), ytm::artist(&id)));
        }
    }

    pub fn ensure_playlist(&mut self, id: &str) {
        if !self.playlists.contains_key(id) || matches!(self.playlists.get(id), Some(Loadable::Failed(_))) {
            self.playlists.insert(id.to_string(), Loadable::Loading);
            let id = id.to_string();
            let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
            std::thread::spawn(move || {
                let (tx2, ctx2, id2) = (tx.clone(), ctx.clone(), id.clone());
                // The page opens with the first songs; the rest streams in.
                let r = ytm::playlist_progressive(&id, &mut |p| {
                    let _ = tx2.send(Msg::Playlist(id2.clone(), Ok(p.clone())));
                    ctx2.request_repaint();
                });
                let _ = tx.send(Msg::Playlist(id, r));
                ctx.request_repaint();
            });
        }
    }

    /// Start loading the page behind a card or row the pointer rests on, so opening it is instant.
    pub fn prefetch_route(&mut self, r: &Route) {
        match r {
            Route::Playlist(id) if !id.starts_with("sp:") && !id.starts_with("dz:") => self.ensure_playlist(id),
            Route::Album(id) if !id.starts_with("sp:") && !id.starts_with("dz:") => self.ensure_album(id),
            Route::Artist(id) if !id.starts_with("sp:") && !id.starts_with("dz:") => self.ensure_artist(id),
            _ => {}
        }
    }

    pub fn ensure_category(&mut self, browse_id: &str, params: Option<String>) {
        let key = format!("{browse_id}|{}", params.clone().unwrap_or_default());
        if !self.category_pages.contains_key(&key) {
            self.category_pages.insert(key.clone(), Loadable::Loading);
            let id = browse_id.to_string();
            self.spawn(move || Msg::Category(key, ytm::category(&id, params.as_deref())));
        }
    }

    pub fn retry_category(&mut self, browse_id: &str, params: Option<String>) {
        let key = format!("{browse_id}|{}", params.clone().unwrap_or_default());
        self.category_pages.remove(&key);
        self.ensure_category(browse_id, params);
    }

    pub fn run_search(&mut self, q: &str, tab: SearchTab) {
        let q = q.trim().to_string();
        if q.is_empty() {
            return;
        }
        let key = (q.clone(), tab);
        if matches!(self.search_results.get(&key), Some(Loadable::Ready(_)) | Some(Loadable::Loading)) {
            // A search started ahead of time (YouTube Music only) now gets the other sources too.
            if self.search_partial.remove(&key) {
                self.spawn_search_extras(&q, tab);
            }
            return;
        }
        self.search_results.insert(key, Loadable::Loading);
        self.spawn_search_extras(&q, tab);
        let filter = tab.filter();
        self.spawn(move || Msg::Search(q.clone(), tab, ytm::search(&q, filter)));
    }

    /// Start a search before it is asked for (as it is typed, or for the other tabs): YouTube Music only,
    /// which is quick. Deezer and SoundCloud are added by `run_search` when the search is really shown.
    pub fn prefetch_search(&mut self, q: &str, tab: SearchTab) {
        let q = q.trim().to_string();
        if q.chars().count() < 2 {
            return;
        }
        let key = (q.clone(), tab);
        if self.search_results.contains_key(&key) {
            return;
        }
        self.search_results.insert(key.clone(), Loadable::Loading);
        self.search_partial.insert(key);
        let filter = tab.filter();
        self.spawn(move || Msg::Search(q.clone(), tab, ytm::search(&q, filter)));
    }

    /// Deezer's and SoundCloud's results for a search, each handed over when it arrives.
    fn spawn_search_extras(&self, q: &str, tab: SearchTab) {
        let wants = move |k: &ItemKind| match tab {
            SearchTab::Top => true,
            SearchTab::Songs => *k == ItemKind::Song,
            SearchTab::Albums => *k == ItemKind::Album,
            SearchTab::Artists => *k == ItemKind::Artist,
            SearchTab::Playlists => *k == ItemKind::Playlist,
            SearchTab::Videos => false,
        };
        if tab != SearchTab::Videos {
            let (tx, ctx, q2) = (self.tx.clone(), self.ctx.clone(), q.to_string());
            std::thread::spawn(move || {
                if let Ok(dz) = crate::providers::deezer_search(&q2) {
                    let items: Vec<Item> = dz.into_iter().filter(|i| wants(&i.kind)).collect();
                    let _ = tx.send(Msg::SearchMore(q2, tab, items));
                    ctx.request_repaint();
                }
            });
        }
        if matches!(tab, SearchTab::Top | SearchTab::Songs) {
            let (tx, ctx, q2) = (self.tx.clone(), self.ctx.clone(), q.to_string());
            std::thread::spawn(move || {
                let items = crate::providers::soundcloud_search(&q2).unwrap_or_default();
                let _ = tx.send(Msg::SearchMore(q2, tab, items));
                ctx.request_repaint();
            });
        }
    }

    pub fn typed_search(&mut self) {
        self.last_typed = Instant::now();
    }

    fn update_suggestions(&mut self) {
        let t = self.search_text.trim().to_string();
        if t.is_empty() {
            self.suggestions.clear();
            self.suggest_for.clear();
            return;
        }
        if t != self.suggest_for && self.last_typed.elapsed() > Duration::from_millis(120) {
            self.suggest_for = t.clone();
            // Look for the results right away, so Enter finds them waiting.
            self.prefetch_search(&t, SearchTab::Top);
            self.spawn(move || Msg::Suggest(t.clone(), ytm::suggestions(&t).unwrap_or_default()));
        }
    }

    // ------------------------------------------------------------ spotify

    pub fn spotify_login(&mut self) {
        if self.spotify_busy {
            return;
        }
        let cid = self.spotify_client_id.trim().to_string();
        self.spotify_busy = true;
        self.spotify_error = None;
        self.spawn(move || Msg::SpotifyLogin(crate::spotify::login(&cid)));
    }

    pub fn spotify_sync(&mut self) {
        if !crate::spotify::logged_in() || self.spotify_syncing {
            return;
        }
        self.spotify_syncing = true;
        self.spawn(|| Msg::SpotifySync(crate::spotify::library()));
    }

    pub fn spotify_logout(&mut self) {
        crate::spotify::log_out();
        self.store.sp_user = None;
        self.store.sp_liked.clear();
        self.store.sp_playlists.clear();
        self.store.sp_albums.clear();
        self.store.sp_artists.clear();
        self.store.sp_top.clear();
        self.store.sp_recent.clear();
        self.store.save();
        self.toast("Logged out of Spotify");
    }

    pub fn ensure_spotify_collection(&mut self, id: &str) {
        if !self.playlists.contains_key(id) || matches!(self.playlists.get(id), Some(Loadable::Failed(_))) {
            self.playlists.insert(id.to_string(), Loadable::Loading);
            let id = id.to_string();
            self.spawn(move || {
                let r = if id.starts_with("dz:") { crate::providers::deezer_collection(&id) } else { crate::spotify::collection(&id) };
                Msg::SpotifyCollection(id.clone(), r)
            });
        }
    }

    /// The YouTube Music id of a track if already known (its own, or its match).
    pub fn known_video_id(&self, t: &Track) -> Option<String> {
        match &t.source {
            Source::YouTube(id) => Some(id.clone()),
            Source::Spotify(_) | Source::Deezer(_) => self.store.sp_matches.get(&t.key()).cloned(),
            Source::Local(_) | Source::Url(_) => None,
        }
    }

    // ------------------------------------------------------------ google account

    /// Read the YouTube sign-in from a browser (no API key, no Premium needed).
    pub fn google_connect(&mut self, browser: &str) {
        if self.google_busy {
            return;
        }
        self.google_busy = true;
        self.google_error = None;
        let b = browser.to_string();
        self.spawn(move || Msg::GoogleConnect(crate::providers::connect_google(&b)));
    }

    /// Remember the browser the sign-in is read from (and use it from now on).
    pub fn use_browser(&mut self, name: &str) {
        if !name.is_empty() && self.store.settings.youtube_browser != name {
            crate::helper::set_browser(name);
            self.store.settings.youtube_browser = name.to_string();
            self.actions.push(Action::SaveSettings);
        }
    }

    pub fn google_sync(&mut self) {
        if !crate::providers::google_connected() || self.google_syncing {
            return;
        }
        self.google_syncing = true;
        self.spawn(|| Msg::GoogleSync(crate::providers::google_library()));
    }

    pub fn google_disconnect(&mut self) {
        crate::providers::disconnect_google();
        self.google_error = None;
        self.store.likes_uploaded = false;
        self.store.gl_liked.clear();
        self.store.gl_playlists.clear();
        self.store.gl_albums.clear();
        self.store.gl_artists.clear();
        self.store.save();
        self.toast("Signed out of the Google account");
    }

    pub fn check_helper(&mut self) {
        self.spawn(|| {
            // Windows ships as a single .exe: fetch ffmpeg once, in the background, if it is missing.
            #[cfg(windows)]
            if let Err(e) = helper::ensure_ffmpeg() {
                eprintln!("[senthym] ffmpeg setup: {e}");
            }
            // Set the engine up now, so sign-in and search work on a fresh install.
            if helper::find().is_none() {
                if let Err(e) = helper::install_latest() {
                    eprintln!("[senthym] engine setup: {e}");
                }
            }
            Msg::Helper(helper::find().map(|(_, v)| v))
        });
        if self.store.settings.check_updates {
            let kind = self.update_kind.clone();
            self.spawn(move || Msg::UpdateChecked(crate::update::check(&kind), false));
        }
    }

    // ------------------------------------------------------------ messages

    fn handle(&mut self, m: Msg) {
        match m {
            Msg::Home(r) => {
                self.home = match r {
                    Ok(s) => Loadable::Ready(s),
                    Err(e) => Loadable::Failed(e),
                }
            }
            Msg::Categories(r) => {
                self.categories = match r {
                    Ok(s) => Loadable::Ready(s),
                    Err(e) => Loadable::Failed(e),
                }
            }
            Msg::Search(q, tab, r) => {
                // Results from the other sources that came in first are added behind the main ones.
                let early = self.search_extra.remove(&(q.clone(), tab)).unwrap_or_default();
                self.search_results.insert((q, tab), match r {
                    Ok(mut v) => {
                        let have: HashSet<String> = v.iter().map(|i| i.id.clone()).collect();
                        v.extend(early.into_iter().filter(|i| !have.contains(&i.id)));
                        crate::fallback::dedupe_songs(&mut v);
                        Loadable::Ready(v)
                    }
                    Err(_) if !early.is_empty() => {
                        let mut early = early;
                        crate::fallback::dedupe_songs(&mut early);
                        Loadable::Ready(early)
                    }
                    Err(e) => Loadable::Failed(e),
                });
            }
            Msg::SearchMore(q, tab, items) => {
                if items.is_empty() {
                    return;
                }
                let key = (q, tab);
                match self.search_results.get_mut(&key) {
                    Some(Loadable::Ready(v)) => {
                        let have: HashSet<String> = v.iter().map(|i| i.id.clone()).collect();
                        v.extend(items.into_iter().filter(|i| !have.contains(&i.id)));
                        crate::fallback::dedupe_songs(v);
                    }
                    Some(Loadable::Failed(_)) => {
                        let mut items = items;
                        crate::fallback::dedupe_songs(&mut items);
                        self.search_results.insert(key, Loadable::Ready(items));
                    }
                    _ => {
                        // The main results are not in yet: keep these until they are.
                        self.search_extra.entry(key).or_default().extend(items);
                    }
                }
            }
            Msg::VideoFound(k, v) => {
                self.video_lookups.remove(&k);
                // The song being played, or about to be, gets its picture's address ready at once.
                if let Some(vid) = v.clone() {
                    let is_current = self.current.as_ref().map(|t| t.key() == k).unwrap_or(false);
                    let soon = is_current || (1..=2).any(|n| self.queue.get(self.index + n).map(|t| t.key() == k).unwrap_or(false));
                    // Even with the video off, the current song's picture address and sound are made ready,
                    // so "Switch to video" starts at once.
                    if soon && (self.store.settings.prefer_video || is_current) {
                        let h = self.start_video_height();
                        self.request_video_url(&vid, h);
                    }
                    if is_current && !self.store.settings.prefer_video {
                        let vkey = format!("vaudio:{}", self.resolve_key(&self.current.clone().unwrap()).unwrap_or_default());
                        if !self.ready.contains_key(&vkey) && self.requested.insert(vkey.clone()) {
                            let quality = self.store.settings.stream_quality;
                            self.spawn(move || Msg::Resolved(vkey, ResolvePurpose::Preload, helper::fetch(&vid, quality)));
                        }
                    }
                }
                self.video_ids.insert(k, v);
            }
            Msg::FollowFound(name, item) => match item {
                Some(it) => {
                    if !self.store.is_saved(&ItemKind::Artist, &it.id) {
                        self.store.toggle_saved(&it);
                        self.store.save();
                    }
                    self.toast("Following");
                }
                None => self.toast(&format!("Couldn't find {name}")),
            },
            Msg::RecentResolved(q, item) => {
                // A failed lookup stays in the "tried" set, so it isn't asked again every frame.
                if let Some(it) = item {
                    self.recent_resolving.remove(&q);
                    self.store.push_search_item_back(&it);
                    self.store.recent_searches.retain(|x| x != &q);
                    self.store.save();
                }
            }
            Msg::VideoAddr(vid, h, r) => {
                self.video_url_pending.remove(&(vid.clone(), h));
                match r {
                    Ok(url) => {
                        // Read the file's index now, so its picture starts quicker when it is asked for.
                        crate::video::warm(&url);
                        self.video_urls.insert((vid, h), (url, Instant::now()));
                        self.attach_video_addr();
                        self.attach_next();
                    }
                    Err(e) => {
                        if let Some(sess) = self.video.as_mut() {
                            if sess.video_id == vid && sess.height == h && sess.stream.is_none() {
                                sess.error = Some(e);
                            } else if sess.next.as_ref().map(|n| n.height == h).unwrap_or(false) {
                                // The other quality isn't available: keep what is playing.
                                sess.next = None;
                                eprintln!("[senthym] no {h}p address for this video: {e}");
                            }
                        }
                    }
                }
            }
            Msg::SwitchAudio(gen, k, r) => {
                if gen == self.play_gen {
                    match r {
                        Ok(p) => self.apply_audio_switch(p, k),
                        Err(e) => self.toast(format!("Couldn't switch to audio. {e}")),
                    }
                }
            }
            Msg::NowRelated(k, r) => {
                self.np_related.insert(k, match r {
                    Ok(v) => Loadable::Ready(v),
                    Err(e) => Loadable::Failed(e),
                });
            }
            Msg::BrowseCover(t, thumb) => {
                self.browse_inflight = self.browse_inflight.saturating_sub(1);
                self.browse_covers.insert(t, thumb);
            }
            Msg::Suggest(q, s) => {
                if q == self.search_text.trim() {
                    self.suggestions = s;
                }
            }
            Msg::Album(id, r) => {
                self.albums.insert(id, match r {
                    Ok(v) => Loadable::Ready(v),
                    Err(e) => Loadable::Failed(e),
                });
            }
            Msg::Artist(id, r) => {
                self.artists.insert(id, match r {
                    Ok(v) => Loadable::Ready(v),
                    Err(e) => Loadable::Failed(e),
                });
            }
            Msg::Playlist(id, r) => {
                self.playlists.insert(id, match r {
                    Ok(v) => Loadable::Ready(v),
                    Err(e) => Loadable::Failed(e),
                });
            }
            Msg::Category(key, r) => {
                self.category_pages.insert(key, match r {
                    Ok(v) => Loadable::Ready(v),
                    Err(e) => Loadable::Failed(e),
                });
            }
            Msg::Radio(purpose, seed, r) => {
                self.radio_loading = false;
                match r {
                    Ok(tracks) => {
                        let known: HashSet<String> = self.queue.iter().map(|t| t.key()).collect();
                        // No repeats: drop what is already in the queue — by id, and also as the same
                        // recording under another upload or source (same title, artist and length) —
                        // and what the list itself carries twice.
                        let mut fresh: Vec<Track> = Vec::new();
                        for t in tracks {
                            if !known.contains(&t.key())
                                && !self.store.is_blocked(&t)
                                && !self.taste.put_aside(&t)
                                && !self.queue.iter().chain(fresh.iter()).any(|q| crate::fallback::same_song(q, &t))
                            {
                                fresh.push(t);
                            }
                        }
                        // These came from autoplay, not from the person: skipping them steers the taste.
                        self.auto_added.extend(fresh.iter().map(|t| t.key()));
                        match purpose {
                            RadioPurpose::Start => {
                                let mut q = vec![seed.clone()];
                                q.extend(fresh.into_iter().filter(|t| t.key() != seed.key()));
                                self.playing_from = format!("{} Radio", seed.title);
                                self.playing_ctx = None;
                                let same = self.current.as_ref().map(|c| c.key() == seed.key()).unwrap_or(false);
                                if same && (self.status.loaded || self.loading) {
                                    // Keep the song going; just line up the radio after it.
                                    self.queue = q;
                                    self.unshuffled = None;
                                    self.index = 0;
                                    self.ready_next_changed();
                                    self.persist_queue();
                                } else {
                                    self.set_queue(q, 0);
                                }
                            }
                            RadioPurpose::Extend => {
                                self.queue.extend(fresh);
                            }
                            RadioPurpose::Continue => {
                                if fresh.is_empty() {
                                    self.engine.set_playing(false);
                                } else {
                                    let next = self.queue.len();
                                    self.queue.extend(fresh);
                                    self.play_index(next);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if purpose != RadioPurpose::Extend {
                            self.toast(format!("Couldn't load similar songs: {e}"));
                        }
                    }
                }
            }
            Msg::Resolved(key, purpose, r) => {
                self.requested.remove(&key);
                let cur_key = self.current.as_ref().and_then(|t| self.resolve_key(t));
                // A Spotify/Deezer song was requested under its own key before it was matched.
                let sp_key = self.current.as_ref().and_then(|t| match &t.source {
                    Source::Spotify(_) | Source::Deezer(_) => Some(t.key()),
                    _ => None,
                });
                let wanted_now = self.loading && (cur_key.as_deref() == Some(key.as_str()) || sp_key.as_deref() == Some(key.as_str()));
                match r {
                    Ok(path) => {
                        self.ready.insert(key.clone(), path.clone());
                        if let Some(k) = cur_key.filter(|k| k.starts_with("yt:") && wanted_now) {
                            self.ready.insert(k, path.clone());
                        }
                        if key.starts_with("sp:") || key.starts_with("dz:") {
                            // Keep it under the matched YouTube id too, so it's found next time.
                            if let Some(vid) = self.store.sp_matches.get(&key).cloned() {
                                self.ready.insert(format!("yt:{vid}"), path.clone());
                            }
                        }
                        if wanted_now {
                            self.start_engine(&path);
                        }
                    }
                    Err(e) => {
                        let _ = purpose;
                        if wanted_now {
                            self.loading = false;
                            self.fail_streak += 1;
                            let title = self.current.as_ref().map(|t| t.title.clone()).unwrap_or_default();
                            self.toast(format!("Couldn't play “{title}”. {e}"));
                            if self.fail_streak < 3 && self.index + 1 < self.queue.len() {
                                self.play_index(self.index + 1);
                            }
                        }
                    }
                }
            }
            Msg::Lyrics(key, l) => {
                self.lyrics.insert(key, match l {
                    Some(l) => LyricsState::Found(l, Instant::now()),
                    None => LyricsState::Missing(Instant::now()),
                });
            }
            Msg::Translation(key, lang, r) => {
                self.translations.insert((key, lang), match r {
                    Ok(v) => Loadable::Ready(v),
                    Err(e) => Loadable::Failed(e),
                });
            }
            Msg::Downloaded(_key, track, r) => {
                self.downloading.retain(|t| t.key() != track.key());
                self.download_running = false;
                match r {
                    Ok(path) => {
                        self.store.downloads.retain(|d| d.track.key() != track.key());
                        self.store.downloads.insert(0, DownloadEntry { track: track.clone(), path, ts: chrono::Local::now().timestamp_millis() });
                        self.store.save();
                        self.toast(format!("Downloaded “{}”", track.title));
                    }
                    Err(e) => self.toast(format!("Download failed: {e}")),
                }
                self.next_download();
            }
            Msg::LocalScan(tracks) => {
                self.local_scanning = false;
                let mut added = 0;
                for t in tracks {
                    if !self.store.local_tracks.iter().any(|x| x.source == t.source) {
                        self.store.local_tracks.push(t);
                        added += 1;
                    }
                }
                self.store.save();
                self.toast(format!("Added {added} songs to Local files"));
            }
            Msg::Helper(v) => {
                self.helper_checked = true;
                self.helper_version = v;
            }
            Msg::UpdateChecked(r, manual) => match r {
                Ok(Some(rel)) => {
                    if !manual && self.store.settings.update_dismissed != rel.version {
                        self.toast(format!("Senthym {} is available. Update it from Settings.", rel.version));
                    }
                    self.update = UpdateState::Available(rel);
                }
                Ok(None) => {
                    self.update = UpdateState::UpToDate;
                    if manual {
                        self.toast("Senthym is up to date");
                    }
                }
                Err(e) => {
                    // A check at start-up that can't reach GitHub is silent; one the person asked for says why.
                    self.update = if manual { UpdateState::Failed(e) } else { UpdateState::Idle };
                }
            },
            Msg::UpdateDone(r) => match r {
                Ok(out) => {
                    self.update = UpdateState::Done(out.text);
                    if let Some((program, args)) = out.relaunch {
                        // Close Senthym (saving everything first) and open the new version a moment later.
                        eprintln!("[senthym] updated; reopening {}", program.display());
                        self.store.last_position_ms = self.status.position_ms;
                        self.store.save();
                        crate::sys::relaunch_later(&program, &args);
                        self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                Err(e) => self.update = UpdateState::Failed(e),
            },
            Msg::HelperUpdate(r) => {
                self.helper_busy = false;
                match r {
                    Ok(v) => {
                        self.toast(format!("Senthym streaming engine is up to date ({v})"));
                        self.helper_version = Some(v);
                    }
                    Err(e) => self.toast(e),
                }
            }
            Msg::SpotifyLogin(r) => {
                self.spotify_busy = false;
                match r {
                    Ok(user) => {
                        self.toast(format!("Signed in to Spotify as {}", user.name));
                        self.show_welcome = false;
                        self.store.sp_user = Some(user);
                        self.store.save();
                        self.spotify_sync();
                        if self.route == Route::SpotifyLogin {
                            self.route = Route::SpotifyLiked;
                        }
                    }
                    Err(e) => {
                        self.toast(e.clone());
                        self.spotify_error = Some(e);
                    }
                }
            }
            Msg::SpotifySync(r) => {
                self.spotify_syncing = false;
                match r {
                    Ok(lib) => {
                        self.store.sp_liked = lib.liked;
                        self.store.sp_playlists = lib.playlists;
                        self.store.sp_albums = lib.albums;
                        self.store.sp_artists = lib.artists;
                        self.store.sp_top = lib.top;
                        self.store.sp_recent = lib.recent;
                        self.store.save();
                    }
                    Err(e) => self.toast(format!("Couldn't load your Spotify library: {e}")),
                }
            }
            Msg::SpotifyCollection(id, r) => {
                self.playlists.insert(id, match r {
                    Ok(v) => Loadable::Ready(v),
                    Err(e) => Loadable::Failed(e),
                });
            }
            Msg::Nothing => {}
            Msg::Note(s) => self.toast(s),
            Msg::GoogleConnect(r) => {
                self.google_busy = false;
                match r {
                    Ok(()) => {
                        self.google_error = None;
                        self.show_welcome = false;
                        self.toast("Google account connected");
                        self.google_sync();
                    }
                    Err(e) => {
                        self.toast(e.clone());
                        self.google_error = Some(e);
                    }
                }
            }
            Msg::GoogleSync(r) => {
                self.google_syncing = false;
                match r {
                    Ok(lib) => {
                        if crate::providers::google_connected() && !lib.liked.is_empty() {
                            let account: std::collections::HashSet<String> = lib.liked.iter().map(|t| t.key()).collect();
                            if !self.store.likes_uploaded {
                                // First sync after signing in: songs liked on this computer go to the account.
                                let ids: Vec<String> = self.store.liked.iter().filter(|t| !account.contains(&t.key())).filter_map(|t| t.video_id().map(String::from)).take(200).collect();
                                self.store.likes_uploaded = true;
                                if !ids.is_empty() {
                                    self.spawn(move || {
                                        let n = ids.iter().filter(|id| crate::ytm::rate(id, true).is_ok()).count();
                                        Msg::Note(format!("{n} liked songs from this computer were added to your YouTube Music account"))
                                    });
                                }
                            } else {
                                // The account is the truth: a song un-liked on another computer goes away here too.
                                self.store.liked.retain(|t| t.video_id().is_none() || account.contains(&t.key()));
                            }
                        }
                        self.store.gl_liked = lib.liked;
                        self.store.gl_playlists = lib.playlists;
                        self.store.gl_albums = lib.albums;
                        self.store.gl_artists = lib.artists;
                        self.store.save();
                    }
                    Err(e) => self.toast(format!("Couldn't load your YouTube Music library: {e}")),
                }
            }
            Msg::DeezerHome(r) => {
                self.deezer_home = match r {
                    Ok(v) => Loadable::Ready(v),
                    Err(e) => Loadable::Failed(e),
                };
            }
            Msg::TrackInfo(key, info) => {
                let fill = |t: &mut Track| {
                    if t.key() == key {
                        if t.artist.is_empty() {
                            t.artist = info.artist.clone();
                            t.artist_id = t.artist_id.clone().or(info.artist_id.clone());
                        }
                        if t.album.is_empty() {
                            t.album = info.album.clone();
                            t.album_id = t.album_id.clone().or(info.album_id.clone());
                        }
                        if t.thumb.is_none() {
                            t.thumb = info.thumb.clone();
                        }
                        if t.duration_ms == 0 {
                            t.duration_ms = info.duration_ms;
                        }
                    }
                };
                self.queue.iter_mut().for_each(fill);
                if let Some(c) = self.current.as_mut() {
                    fill(c);
                }
                for h in self.store.history.iter_mut() {
                    fill(&mut h.track);
                }
                self.store.save();
                // Look the lyrics up again with the artist now known.
                if let Some(c) = self.current.clone().filter(|c| c.key() == key) {
                    self.lyrics.remove(&key);
                    self.load_lyrics(&c);
                }
            }
            Msg::SpotifyMatch(sp, vid) => {
                self.store.sp_matches.insert(sp, vid);
                self.store.save();
            }
            Msg::PlayCollection(item, shuffle, r) => match r {
                Ok(tracks) if !tracks.is_empty() => {
                    self.actions.push(Action::PlayTracks { tracks, start: 0, shuffle, context: Some(item) });
                }
                Ok(_) => self.toast("Nothing to play here"),
                Err(e) => self.toast(e),
            },
        }
    }

    // ------------------------------------------------------------ player

    pub fn set_queue(&mut self, tracks: Vec<Track>, start: usize) {
        // No duplicates, like CurrentSongState.updateQueue.
        let mut seen = HashSet::new();
        let start_key = tracks.get(start).map(|t| t.key());
        let q: Vec<Track> = tracks.into_iter().filter(|t| seen.insert(t.key())).collect();
        let start = start_key.and_then(|k| q.iter().position(|t| t.key() == k)).unwrap_or(0);
        self.queue = q;
        self.unshuffled = None;
        // A queue the person started themselves: nothing in it came from autoplay, and the song
        // they picked is the newest sign of their taste.
        self.auto_added.clear();
        if let Some(t) = self.queue.get(start).cloned() {
            self.note_choice(&t);
        }
        // Never start on a song hidden in the playlist being played.
        let start = self.skip_hidden(start);
        if self.shuffle {
            self.apply_shuffle(start);
        } else {
            self.play_index(start);
        }
    }

    /// Current song first, then the rest in random order; the original order is kept.
    fn apply_shuffle(&mut self, current: usize) {
        if self.queue.is_empty() {
            return;
        }
        self.unshuffled = Some(self.queue.clone());
        let cur = self.queue.remove(current.min(self.queue.len() - 1));
        self.queue.shuffle(&mut rand::thread_rng());
        self.queue.insert(0, cur);
        self.play_index(0);
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        if self.queue.is_empty() {
            return;
        }
        let cur_key = self.current.as_ref().map(|t| t.key());
        if self.shuffle {
            self.unshuffled = Some(self.queue.clone());
            let cur = self.queue.remove(self.index.min(self.queue.len() - 1));
            self.queue.shuffle(&mut rand::thread_rng());
            self.queue.insert(0, cur);
            self.index = 0;
        } else if let Some(orig) = self.unshuffled.take() {
            // Keep songs added meanwhile.
            let mut restored = orig;
            for t in &self.queue {
                if !restored.iter().any(|x| x.key() == t.key()) {
                    restored.push(t.clone());
                }
            }
            self.queue = restored;
            if let Some(k) = cur_key {
                self.index = self.queue.iter().position(|t| t.key() == k).unwrap_or(0);
            }
        }
        self.ready_next_changed();
    }

    fn ready_next_changed(&mut self) {
        self.fade_target = None;
        if self.status.fading {
            self.engine.cancel_fade();
        }
    }

    fn alt_for(&self, t: &Track) -> Option<AltStream> {
        self.store.alt_streams.get(&t.key()).cloned()
    }

    /// The file to play for a track, if it's ready right now.
    fn ready_path(&self, t: &Track) -> Option<PathBuf> {
        match self.alt_for(t) {
            Some(AltStream::Local(p)) => return Some(PathBuf::from(p)),
            Some(AltStream::YouTube(id)) => return self.ready.get(&format!("yt:{id}")).cloned(),
            None => {}
        }
        if let Source::Local(p) = &t.source {
            return Some(PathBuf::from(p));
        }
        if let Some(d) = self.store.download_for(t) {
            return Some(PathBuf::from(&d.path));
        }
        self.resolve_key(t).and_then(|k| self.ready.get(&k).cloned())
    }

    /// The key a track's audio is fetched and kept under.
    fn resolve_key(&self, t: &Track) -> Option<String> {
        match self.alt_for(t) {
            Some(AltStream::YouTube(id)) => return Some(format!("yt:{id}")),
            Some(AltStream::Local(_)) => return None,
            None => {}
        }
        match &t.source {
            Source::Local(_) => None,
            Source::YouTube(id) => Some(format!("yt:{id}")),
            Source::Url(u) => Some(format!("url:{u}")),
            Source::Spotify(_) | Source::Deezer(_) => Some(match self.store.sp_matches.get(&t.key()) {
                Some(v) => format!("yt:{v}"),
                None => t.key(),
            }),
        }
    }

    // ------------------------------------------------------------ music video

    /// Typed searches in the recent list become what they found first (artist, playlist, song…), with its
    /// cover, a few at a time.
    pub fn resolve_recent_queries(&mut self) {
        let queries = self.store.recent_searches.clone();
        for q in queries {
            if self.recent_resolving.len() >= 3 {
                break;
            }
            if self.recent_resolving.contains(&q) {
                continue;
            }
            self.recent_resolving.insert(q.clone());
            self.spawn(move || {
                let item = ytm::search(&q, None).ok().and_then(|items| {
                    items.into_iter().find(|i| {
                        matches!(i.kind, ItemKind::Artist | ItemKind::Album | ItemKind::Playlist | ItemKind::Song | ItemKind::Video)
                            && (i.thumb.is_some() || i.track.as_ref().and_then(|t| t.thumb.clone()).is_some())
                    })
                });
                Msg::RecentResolved(q, item)
            });
        }
    }

    /// How far the playing sound runs behind the studio version that lyrics are timed to. A music video
    /// usually opens with a scene before the song starts, so its sound is longer than the song by about
    /// that much; the lyrics are moved by the same amount. Zero for the audio version.
    pub fn lyric_offset_ms(&self) -> u64 {
        if !self.store.settings.prefer_video || self.video.is_none() {
            return 0;
        }
        let Some(t) = self.current.as_ref() else { return 0 };
        if t.duration_ms == 0 || self.status.duration_ms == 0 {
            return 0;
        }
        let diff = self.status.duration_ms as i64 - t.duration_ms as i64;
        if (2_000..=90_000).contains(&diff) { diff as u64 } else { 0 }
    }

    /// Look up (once) whether a song has an official music video.
    pub fn ensure_video_lookup(&mut self, t: &Track) {
        let key = t.key();
        if self.video_ids.contains_key(&key) || !self.video_lookups.insert(key.clone()) {
            return;
        }
        // A YouTube song's own id, or a Spotify / Deezer song's known YouTube Music match: a video
        // is looked up against whichever catalogue the song actually plays from, cross-checked the
        // same way either way (the song's title and credited artists must both be confirmed).
        let known = self.known_video_id(t);
        let videos = self.store.settings.video_fallback;
        let tr = t.clone();
        let tx = self.tx.clone();
        self.spawn(move || {
            let id = match known {
                Some(id) => Some(id),
                None => match crate::spotify::find_on_youtube(&tr, videos) {
                    Ok(vid) => {
                        let _ = tx.send(Msg::SpotifyMatch(tr.key(), vid.clone()));
                        Some(vid)
                    }
                    Err(_) => None,
                },
            };
            let v = id.and_then(|id| ytm::video_id_for(&tr, &id));
            Msg::VideoFound(key, v)
        });
    }

    /// The best quality the connection can carry, from the recent download speed (never above 1080p).
    fn network_height(&self) -> u32 {
        match helper::measured_kbps() {
            0 => 720,
            k if k >= 12_000 => 1080,
            k if k >= 6_000 => 720,
            k if k >= 3_000 => 480,
            k if k >= 1_500 => 360,
            _ => 240,
        }
    }

    /// The height to use now: the chosen one, or automatic: as much as the connection allows in the
    /// big views (the expanded page and full screen), and a smaller picture in the side panel.
    fn desired_video_height(&self) -> u32 {
        let q = self.store.settings.video_quality;
        if q != 0 {
            return q.clamp(144, 1080);
        }
        let net = self.network_height();
        let fs = self.ctx.data(|d| d.get_temp::<bool>(egui::Id::new("video_fs"))).unwrap_or(false);
        if self.route == Route::Player || fs { net } else { net.min(480) }
    }

    /// Where a picture starts: in automatic mode never above 480p, so it appears quickly; the quality
    /// then climbs to what the view and connection allow.
    fn start_video_height(&self) -> u32 {
        let d = self.desired_video_height();
        if self.store.settings.video_quality == 0 { d.min(480) } else { d }
    }

    /// Ask for a picture address in the background (once per video and height). Addresses stay valid for hours.
    fn request_video_url(&mut self, vid: &str, height: u32) {
        let key = (vid.to_string(), height);
        if self.video_url_pending.contains(&key) {
            return;
        }
        if let Some((_, at)) = self.video_urls.get(&key) {
            if at.elapsed() < Duration::from_secs(3 * 3600) {
                return;
            }
        }
        self.video_url_pending.insert(key);
        let vid = vid.to_string();
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        self.spawn(move || {
            // One lookup gives every quality's address, so a later quality change needs none.
            match helper::video_urls(&vid) {
                Ok(all) => {
                    let mut mine = None;
                    for (h, url) in all {
                        if h == height {
                            mine = Some(url);
                        } else {
                            let _ = tx.send(Msg::VideoAddr(vid.clone(), h, Ok(url)));
                        }
                    }
                    ctx.request_repaint();
                    match mine {
                        Some(url) => Msg::VideoAddr(vid, height, Ok(url)),
                        None => {
                            let r = helper::video_url(&vid, height);
                            Msg::VideoAddr(vid, height, r)
                        }
                    }
                }
                Err(_) => {
                    let r = helper::video_url(&vid, height);
                    Msg::VideoAddr(vid, height, r)
                }
            }
        });
    }

    /// The position the picture follows: where playback will continue when the sound is still
    /// loading (a switch to video, a restored session), else the playing position. Without this,
    /// a picture started during the load would begin at 0 and have to be started over.
    fn video_pos(&self) -> u64 {
        self.pending_resume_ms.unwrap_or(self.status.position_ms)
    }

    /// Start the session's picture when its address is known (or ask for it).
    fn attach_video_addr(&mut self) {
        let pos = self.video_pos();
        let Some(sess) = self.video.as_ref() else { return };
        if sess.stream.is_some() || sess.error.is_some() {
            return;
        }
        let key = (sess.video_id.clone(), sess.height);
        let cached = self.video_urls.get(&key).filter(|(_, at)| at.elapsed() < Duration::from_secs(3 * 3600)).map(|(u, _)| u.clone());
        match cached {
            Some(url) => {
                let probe = url.clone();
                std::thread::spawn(move || helper::probe_speed(&probe));
                if let Some(sess) = self.video.as_mut() {
                    let (w, h) = ((sess.height as usize * 16 / 9) & !1, sess.height as usize);
                    match crate::video::VideoStream::start(&url, pos, w, h) {
                        Ok(st) => sess.stream = Some(st),
                        Err(e) => sess.error = Some(e),
                    }
                    sess.url = Some(url);
                    sess.restart_at = Instant::now();
                }
            }
            None => self.request_video_url(&key.0, key.1),
        }
    }

    /// Change quality without a gap: start the new picture beside the current one.
    fn switch_quality(&mut self, height: u32) {
        let Some(sess) = self.video.as_mut() else { return };
        if sess.height == height && sess.next.is_none() {
            return;
        }
        if sess.next.as_ref().map(|n| n.height == height).unwrap_or(false) {
            return;
        }
        if sess.stream.is_none() {
            // Nothing on screen yet: just start at the new height.
            self.restart_video(height);
            return;
        }
        sess.next = Some(NextStream { height, stream: None, restart_at: Instant::now(), asked: Instant::now(), tries: 0 });
        self.attach_next();
    }

    fn attach_next(&mut self) {
        let pos = self.video_pos();
        let Some(sess) = self.video.as_ref() else { return };
        let Some(n) = sess.next.as_ref() else { return };
        if n.stream.is_some() {
            return;
        }
        let key = (sess.video_id.clone(), n.height);
        let cached = self.video_urls.get(&key).filter(|(_, at)| at.elapsed() < Duration::from_secs(3 * 3600)).map(|(u, _)| u.clone());
        match cached {
            Some(url) => {
                let (w, h) = ((key.1 as usize * 16 / 9) & !1, key.1 as usize);
                let st = crate::video::VideoStream::start(&url, pos, w, h).ok();
                if let Some(n) = self.video.as_mut().and_then(|s| s.next.as_mut()) {
                    n.stream = st;
                    n.restart_at = Instant::now();
                    n.tries += 1;
                }
            }
            None => self.request_video_url(&key.0, key.1),
        }
    }

    /// The quality a change in progress is going to, while its picture is being started.
    pub fn video_switching_to(&self) -> Option<u32> {
        self.video.as_ref().and_then(|s| s.next.as_ref()).map(|n| n.height)
    }

    /// Start the session's picture again at another height, keeping the last frame on screen meanwhile.
    fn restart_video(&mut self, height: u32) {
        self.video_gen += 1;
        let gen = self.video_gen;
        let Some(sess) = self.video.as_mut() else { return };
        sess.gen = gen;
        sess.height = height;
        sess.stream = None;
        sess.url = None;
        sess.error = None;
        sess.restart_at = Instant::now();
        self.attach_video_addr();
    }

    /// The music video's picture follows the audio position (called every frame).
    fn update_video(&mut self) {
        // Was the picture left alone for a while (window hidden, so no frames were drawn), or did the
        // song jump (a seek)? Then being behind says nothing about the connection.
        let now = Instant::now();
        let wall = now.duration_since(self.video_tick).as_millis() as u64;
        let moved = self.video_pos().saturating_sub(self.video_last_pos);
        if wall > 1500 || moved > wall + 1500 {
            self.video_jump_at = Some(now);
            if let Some(n) = self.video.as_mut().and_then(|s| s.next.as_mut()) {
                n.tries = 0;
            }
        }
        // A seek forwards while the picture is running: start the picture at the new position at once,
        // keeping the current frame on screen meanwhile. Left to catch up on its own, the old frames
        // would race across the screen (a fast-forward look) until the lag check restarted it seconds later.
        if moved > wall + 1500 && wall <= 1500 {
            let restart = self.video.as_ref().map(|s| s.stream.is_some() && s.restart_at.elapsed() > Duration::from_millis(250)).unwrap_or(false);
            if restart {
                let h = self.video.as_ref().map(|s| s.height).unwrap_or(0);
                eprintln!("[senthym] seek forwards: video picked up at the new position ({h}p)");
                self.restart_video(h);
            }
        }
        self.video_tick = now;
        self.video_last_pos = self.video_pos();
        let jumped = self.video_jump_at.map(|t| t.elapsed() < Duration::from_secs(8)).unwrap_or(false);
        let cur_key = self.current.as_ref().map(|t| t.key());
        let prefer = self.store.settings.prefer_video;
        if !prefer {
            if let Some(t) = self.current.clone() {
                self.ensure_video_lookup(&t);
            }
        }
        if let Some(sess) = &self.video {
            // Gone when the song changed, the mode is off, or the song's video turned out to be another one.
            let still = cur_key.as_ref().and_then(|k| self.video_ids.get(k)).map(|v| v.as_deref() == Some(sess.video_id.as_str())).unwrap_or(false);
            if Some(&sess.track_key) != cur_key.as_ref() || !prefer || !still {
                self.video = None;
            }
        }
        if self.video.is_none() && prefer {
            if let Some(key) = cur_key.clone() {
                if let Some(Some(vid)) = self.video_ids.get(&key).cloned() {
                    self.video_gen += 1;
                    let gen = self.video_gen;
                    let height = self.start_video_height();
                    self.video = Some(VideoSession { track_key: key.clone(), video_id: vid.clone(), gen, url: None, stream: None, tex: None, error: None, height, bars: None, restart_at: Instant::now(), started: Instant::now(), next: None });
                    self.attach_video_addr();
                }
            }
        }
        // Automatic quality follows the view: bigger in the expanded page / full screen, smaller in the panel.
        if self.store.settings.video_quality == 0 {
            let want = self.desired_video_height();
            let cur = self.video.as_ref().map(|s| (s.height, s.restart_at.elapsed() > Duration::from_millis(1500) && s.next.is_none(), s.url.is_some() || s.error.is_some()));
            if let Some((h, settled, ready)) = cur {
                if h != want && settled && ready && self.video_lagged.map(|(at, _)| at.elapsed() > Duration::from_secs(30)).unwrap_or(true) {
                    self.switch_quality(want);
                }
            }
        }
        let pos = self.video_pos();
        let ctx = self.ctx.clone();
        let playing = self.status.playing;
        let mut lag_to: Option<u32> = None;
        let mut start_failed: Option<String> = None;
        if let Some(sess) = self.video.as_mut() {
            if let Some(stream) = sess.stream.as_mut() {
                match stream.poll(pos) {
                    crate::video::Poll::Frame(bytes) => {
                        let found = black_bars(&bytes, stream.w, stream.h);
                        sess.bars = Some(match sess.bars {
                            Some((t, b)) => (t.min(found.0), b.min(found.1)),
                            None => found,
                        });
                        let img = egui::ColorImage::from_rgba_unmultiplied([stream.w, stream.h], &bytes);
                        match sess.tex.as_mut() {
                            Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                            None => {
                                eprintln!("[senthym] first picture {} ms after the video was found ({}p)", sess.started.elapsed().as_millis(), sess.height);
                                sess.tex = Some(ctx.load_texture("music_video", img, egui::TextureOptions::LINEAR));
                            }
                        }
                    }
                    crate::video::Poll::Restart => {
                        // After a seek: begin again at the audio position (not more often than every 2 s).
                        if sess.restart_at.elapsed() > Duration::from_secs(2) {
                            if let Some(url) = sess.url.clone() {
                                let (w, h) = ((sess.height as usize * 16 / 9) & !1, sess.height as usize);
                                sess.stream = crate::video::VideoStream::start(&url, pos, w, h).ok();
                                sess.restart_at = Instant::now();
                            }
                        }
                    }
                    crate::video::Poll::Lag => {
                        // Can't keep up: step down one quality (when it is automatic) and start again here.
                        if sess.restart_at.elapsed() > Duration::from_secs(3) {
                            let auto = self.store.settings.video_quality == 0;
                            let lower = [1080u32, 720, 480, 360, 240].iter().copied().find(|h| *h < sess.height).unwrap_or(240);
                            lag_to = Some(if auto && !jumped { lower } else { sess.height });
                        }
                    }
                    crate::video::Poll::Wait => {
                        if stream.ended {
                            start_failed = stream.failure();
                        }
                    }
                }
                if playing {
                    ctx.request_repaint_after(Duration::from_millis(16));
                }
            }
            if start_failed.is_some() && sess.error.is_none() {
                sess.error = start_failed.take();
            }
        }
        // A quality change in progress: the new picture runs beside the one on screen and takes over
        // with its first frame, so the picture never freezes or goes black while changing.
        let mut retry_next = false;
        let mut gave_up: Option<u32> = None;
        let next_url = self.video.as_ref().and_then(|s| s.next.as_ref().map(|n| (s.video_id.clone(), n.height))).and_then(|k| self.video_urls.get(&k).map(|(u, _)| u.clone()));
        if let Some(sess) = self.video.as_mut() {
            let mut promote = false;
            if let Some(n) = sess.next.as_mut() {
                let stuck = match n.stream.as_mut() {
                    Some(st) => match st.poll(pos) {
                        crate::video::Poll::Frame(bytes) => {
                            let img = egui::ColorImage::from_rgba_unmultiplied([st.w, st.h], &bytes);
                            match sess.tex.as_mut() {
                                Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                                None => sess.tex = Some(ctx.load_texture("music_video", img, egui::TextureOptions::LINEAR)),
                            }
                            promote = true;
                            false
                        }
                        // The song was sought, or the new picture never got going: start it again here.
                        crate::video::Poll::Restart | crate::video::Poll::Lag => true,
                        // ffmpeg ended without a picture.
                        crate::video::Poll::Wait => st.ended,
                    },
                    // Its address is being looked up (ask again if it doesn't come), or starting it failed.
                    None => {
                        if next_url.is_none() && n.restart_at.elapsed() > Duration::from_secs(3) {
                            n.restart_at = Instant::now();
                            retry_next = true;
                        }
                        next_url.is_some()
                    }
                };
                if stuck && n.restart_at.elapsed() > Duration::from_secs(2) {
                    if n.tries >= 3 {
                        gave_up = Some(n.height);
                    } else {
                        n.stream = None;
                        retry_next = true;
                    }
                }
                // An address that never arrives must not block later changes.
                if next_url.is_none() && n.asked.elapsed() > Duration::from_secs(40) {
                    gave_up = Some(n.height);
                }
            }
            if promote {
                if let Some(n) = sess.next.take() {
                    eprintln!("[senthym] video quality {}p -> {}p in {} ms", sess.height, n.height, n.asked.elapsed().as_millis());
                    sess.stream = n.stream;
                    sess.height = n.height;
                    sess.url = next_url.clone();
                    sess.error = None;
                    sess.restart_at = Instant::now();
                }
            } else if gave_up.is_some() {
                sess.next = None;
            }
            if sess.next.is_some() && playing {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        }
        if retry_next {
            self.attach_next();
        }
        if let Some(h) = gave_up {
            eprintln!("[senthym] video quality change to {h}p didn't start; keeping the current picture");
            if self.store.settings.video_quality != 0 {
                self.toast(format!("Couldn't switch the video to {h}p. Keeping the current quality."));
            } else {
                // Automatic: don't try to climb again for a while.
                let cur = self.video.as_ref().map(|s| s.height).unwrap_or(h);
                self.video_lagged = Some((Instant::now(), cur));
            }
        }
        // The picture couldn't keep up: start it again at the audio position (one step lower when automatic),
        // and don't climb back up for a while.
        if let Some(h) = lag_to {
            if jumped {
                // After a seek or a hidden window: just pick the picture up at the song's position.
                eprintln!("[senthym] video picked up again at {h}p");
            } else {
                eprintln!("[senthym] video can't keep up: restarting at {h}p");
                self.video_lagged = Some((Instant::now(), h));
            }
            self.restart_video(h);
        }
    }

    /// Switch between the music video (with its own sound) and the audio version, at the same position.
    fn set_video_mode(&mut self, on: bool) {
        self.store.settings.prefer_video = on;
        self.store.save();
        let Some(t) = self.current.clone() else { return };
        let pos = self.status.position_ms;
        let key = self.resolve_key(&t);
        // What is stored under the "yt:" keys depends on the mode, so start those over.
        self.ready.retain(|k, _| !k.starts_with("yt:"));
        self.preload_tried.clear();
        self.video = None;
        if on {
            let ready = key.as_ref().and_then(|k| self.ready.get(&format!("vaudio:{k}")).cloned().map(|p| (k.clone(), p)));
            if let Some((k, p)) = ready {
                // The video's sound is already on disk: carry on from the same spot without a restart.
                self.apply_audio_switch(p, k);
            } else {
                let i = self.index;
                self.play_index_from(i, Some(pos));
            }
        } else if let Some(key) = key {
            if let Some(p) = self.ready.get(&format!("audio:{key}")).cloned() {
                self.apply_audio_switch(p, key);
            } else if let Some(id) = key.strip_prefix("yt:").map(String::from) {
                self.toast("Switching to audio…");
                let quality = self.store.settings.stream_quality;
                let gen = self.play_gen;
                self.spawn(move || {
                    let a = ytm::audio_id_for(&t, &id);
                    Msg::SwitchAudio(gen, key, helper::fetch(&a, quality))
                });
            }
        }
    }

    fn apply_audio_switch(&mut self, path: PathBuf, key: String) {
        let Some(t) = self.current.clone() else { return };
        let pos = self.status.position_ms;
        let playing = self.status.playing;
        self.ready.insert(key, path.clone());
        self.engine.load(&path.to_string_lossy(), pos, t.duration_ms, playing);
    }

    /// Fetch a YouTube song in the background so that playing it is instant. At most a few at a time.
    fn prefetch(&mut self, t: &Track, hot: bool) {
        if !matches!(t.source, Source::YouTube(_)) || self.ready_path(t).is_some() {
            return;
        }
        let key = t.key();
        if let Some(rk) = self.resolve_key(t) {
            if self.preload_tried.contains(&rk) || self.requested.contains(&rk) {
                return;
            }
        }
        if let Some(pos) = self.prefetch_queue.iter().position(|q| q.key() == key) {
            if hot && pos > 0 {
                let q = self.prefetch_queue.remove(pos).unwrap();
                self.prefetch_queue.push_front(q);
            }
            return;
        }
        if hot {
            self.prefetch_queue.push_front(t.clone());
        } else {
            self.prefetch_queue.push_back(t.clone());
        }
        // The line is short: what is no longer on screen shouldn't pile up.
        while self.prefetch_queue.len() > 6 {
            self.prefetch_queue.pop_back();
        }
    }

    /// Start queued fetches as the (few) background slots free up.
    fn pump_prefetch(&mut self) {
        // Not while the song the person just chose is still loading: it gets the whole connection.
        if self.loading {
            return;
        }
        while self.requested.len() < 3 {
            let Some(t) = self.prefetch_queue.pop_front() else { break };
            if self.ready_path(&t).is_none() {
                self.request_resolve(&t, ResolvePurpose::Preload);
            }
        }
    }

    fn request_resolve(&mut self, t: &Track, purpose: ResolvePurpose) {
        let Some(key) = self.resolve_key(t) else { return };
        if self.requested.contains(&key) {
            return; // already being fetched; it starts when it arrives
        }
        if purpose == ResolvePurpose::Preload && !self.preload_tried.insert(key.clone()) {
            return; // a background fetch was already tried for this song
        }
        self.requested.insert(key.clone());
        let quality = self.store.settings.stream_quality;
        let videos = self.store.settings.video_fallback;
        let track = t.clone();
        let explicit = self.alt_for(t).is_some();
        let want_video = self.store.settings.prefer_video;
        let ctx = self.ctx.clone();
        let tx = self.tx.clone();
        self.spawn(move || {
            if let Some(id) = key.strip_prefix("yt:") {
                let started = Instant::now();
                let orig = id.to_string();
                // The song's own audio always starts at once — a song never waits on its video to
                // play, whether it has one or not. With the video on, the video is looked up fully
                // in the background and only attaches its picture once found (over this same
                // audio); a link chosen by hand is used as is, with no video lookup at all.
                let (atv_id, atv_res, other_source) = if explicit {
                    (orig.clone(), helper::fetch(&orig, quality), None)
                } else {
                    let a = ytm::audio_id_for(&track, &orig);
                    let (ar, alt) = crate::fallback::fetch_song(&track, &a, quality);
                    (a, ar, alt)
                };
                if !explicit && want_video {
                    let (t2, o2, tx2, ctx2) = (track.clone(), orig.clone(), tx.clone(), ctx.clone());
                    std::thread::spawn(move || {
                        let v = ytm::video_id_for(&t2, &o2);
                        let _ = tx2.send(Msg::VideoFound(t2.key(), v));
                        ctx2.request_repaint();
                    });
                } else if !explicit {
                    let _ = tx.send(Msg::VideoFound(track.key(), None));
                }
                eprintln!("[senthym] got {atv_id} in {} ms ({})", started.elapsed().as_millis(), if atv_res.is_ok() { "ok" } else { "failed" });
                if let (Some(from), true) = (other_source, atv_res.is_ok()) {
                    // Sound from another source never goes with this song's picture.
                    let _ = tx.send(Msg::VideoFound(track.key(), None));
                    if matches!(purpose, ResolvePurpose::Play(_)) {
                        let _ = tx.send(Msg::Note(format!("“{}” wasn't available from its source. Playing it from {from}.", track.title)));
                    }
                }
                return Msg::Resolved(key, purpose, atv_res);
            }
            if let Source::Url(u) = &track.source {
                let mut r = helper::fetch_url(u, quality);
                if r.is_err() {
                    // The link won't play: the same song from YouTube Music instead.
                    if let Ok(p) = crate::fallback::from_youtube(&track, quality) {
                        if matches!(purpose, ResolvePurpose::Play(_)) {
                            let _ = tx.send(Msg::Note(format!("“{}” wasn't available from its source. Playing it from YouTube Music.", track.title)));
                        }
                        r = Ok(p);
                    }
                }
                return Msg::Resolved(key, purpose, r);
            }
            // A Spotify or Deezer song (SongPlayer.resolveYtPlaybackUnbounded): try up to 3
            // accepted song matches, then up to 3 video matches when video fallback is on. The
            // song always starts at once; with the video on, its match is cross-checked for a
            // video fully in the background, the same verified way as for a YouTube song, and
            // only attaches the picture once found — never waits for it.
            let match_key = track.key();
            let mut last_err = format!("couldn't find “{}” on YouTube Music", track.title);
            let rounds: &[bool] = if videos { &[false, true] } else { &[false] };
            for &use_videos in rounds {
                let ids = match crate::spotify::candidates(&track, use_videos) {
                    Ok(ids) => ids,
                    Err(e) => {
                        last_err = e;
                        continue;
                    }
                };
                for vid in ids {
                    match helper::fetch(&vid, quality) {
                        Ok(path) => {
                            let _ = tx.send(Msg::SpotifyMatch(match_key.clone(), vid.clone()));
                            if want_video {
                                let (t2, v2, tx2, ctx2) = (track.clone(), vid.clone(), tx.clone(), ctx.clone());
                                std::thread::spawn(move || {
                                    let v = ytm::video_id_for(&t2, &v2);
                                    let _ = tx2.send(Msg::VideoFound(t2.key(), v));
                                    ctx2.request_repaint();
                                });
                            } else {
                                let _ = tx.send(Msg::VideoFound(match_key, None));
                            }
                            return Msg::Resolved(key, purpose, Ok(path));
                        }
                        Err(e) => last_err = e,
                    }
                }
            }
            // No YouTube Music match plays: the song's SoundCloud copy.
            if let Ok(p) = crate::fallback::from_soundcloud(&track, quality) {
                if matches!(purpose, ResolvePurpose::Play(_)) {
                    let _ = tx.send(Msg::Note(format!("“{}” wasn't available from its source. Playing it from SoundCloud.", track.title)));
                }
                return Msg::Resolved(key, purpose, Ok(p));
            }
            Msg::Resolved(key, purpose, Err(last_err))
        });
    }

    pub fn play_index(&mut self, i: usize) {
        self.play_index_from(i, None)
    }

    pub fn play_index_from(&mut self, i: usize, start_ms: Option<u64>) {
        let Some(t) = self.queue.get(i).cloned() else { return };
        eprintln!("[senthym] play #{i}: {} — {}", t.title, t.artist);
        self.index = i;
        self.current = Some(t.clone());
        self.play_gen += 1;
        self.fade_target = None;
        self.pending_resume_ms = start_ms;
        self.engine.stop();
        self.engine.set_playing(false);
        self.status = Status::default();
        match self.ready_path(&t) {
            Some(p) => self.start_engine(&p),
            None => {
                self.loading = true;
                let gen = self.play_gen;
                self.request_resolve(&t, ResolvePurpose::Play(gen));
            }
        }
        self.load_lyrics(&t);
        self.fill_missing_info(&t);
        self.maybe_extend_radio();
        self.persist_queue();
    }

    /// Some lists leave the artist out; ask YouTube Music for the song's details.
    fn fill_missing_info(&self, t: &Track) {
        if !t.artist.trim().is_empty() {
            return;
        }
        let Some(id) = t.video_id().map(String::from) else { return };
        let key = t.key();
        self.spawn(move || {
            let info = ytm::radio(&id, None).ok().and_then(|list| list.into_iter().find(|x| x.video_id() == Some(id.as_str())));
            match info {
                Some(i) => Msg::TrackInfo(key, i),
                None => Msg::Nothing,
            }
        });
    }

    fn start_engine(&mut self, path: &PathBuf) {
        let Some(t) = self.current.clone() else { return };
        self.loading = false;
        self.fail_streak = 0;
        let start = self.pending_resume_ms.take().unwrap_or(0);
        self.engine.load(&path.to_string_lossy(), start, t.duration_ms, true);
        self.store.add_history(&t);
        self.store.save();
    }

    /// When the queue is nearly done, fetch similar songs (PlayerViewModel.maybeExtendRadio).
    fn maybe_extend_radio(&mut self) {
        if !self.store.settings.autoplay_radio || self.radio_loading || self.queue.len() < 2 || self.index + 2 < self.queue.len() {
            return;
        }
        // Seed from the most recent YouTube song among the last 5.
        let seed = self.queue.iter().rev().take(5).find(|t| t.is_online()).cloned();
        if let Some(seed) = seed {
            self.radio_loading = true;
            self.spawn_radio(RadioPurpose::Extend, seed);
        }
    }

    pub fn start_radio(&mut self, t: &Track) {
        if !t.is_online() {
            self.toast("Radio needs a streaming song");
            return;
        }
        self.toast(format!("Starting radio from “{}”…", t.title));
        self.radio_loading = true;
        self.spawn_radio(RadioPurpose::Start, t.clone());
    }

    /// Similar songs, in the person's current taste: the radios around the playing song and
    /// their last few own choices are fetched side by side and blended, newest taste leading
    /// (see `taste::blend`). With one seed this is the plain radio of that song.
    fn spawn_radio(&self, purpose: RadioPurpose, seed: Track) {
        // The playing song first, then the newest choices behind it — one per artist.
        let seeds = self.taste.seeds(Some(&seed));
        let known: Vec<Option<String>> = seeds.iter().map(|t| self.known_video_id(t)).collect();
        let videos = self.store.settings.video_fallback;
        let tx = self.tx.clone();
        self.spawn(move || {
            // All the radios at the same time, so a blend costs no more waiting than one radio.
            let lists: Vec<Vec<Track>> = std::thread::scope(|sc| {
                let hs: Vec<_> = seeds
                    .iter()
                    .zip(known)
                    .map(|(t, id)| {
                        let tx = tx.clone();
                        sc.spawn(move || {
                            let id = match id {
                                Some(id) => Ok(id),
                                None => {
                                    let r = crate::spotify::find_on_youtube(t, videos);
                                    if let (Ok(vid), Source::Spotify(_) | Source::Deezer(_)) = (&r, &t.source) {
                                        let _ = tx.send(Msg::SpotifyMatch(t.key(), vid.clone()));
                                    }
                                    r
                                }
                            };
                            id.and_then(|id| ytm::radio(&id, None))
                        })
                    })
                    .collect();
                hs.into_iter().map(|h| h.join().ok().and_then(|r| r.ok()).unwrap_or_default()).collect()
            });
            if lists.iter().all(|l| l.is_empty()) {
                return Msg::Radio(purpose, seed, Err("couldn't load similar songs".into()));
            }
            Msg::Radio(purpose, seed, Ok(crate::taste::blend(&lists, 50)))
        });
    }

    pub fn toggle_play(&mut self) {
        if self.current.is_none() {
            return;
        }
        if !self.status.loaded && !self.loading {
            // Restored queue: start the song where it stopped.
            let resume = self.pending_resume_ms;
            self.play_index_from(self.index, resume);
            return;
        }
        self.engine.set_playing(!self.status.playing);
    }

    pub fn next(&mut self) {
        if self.queue.is_empty() {
            return;
        }
        if self.status.fading {
            return; // PlayerViewModel.playNextSongs ignores skips during a crossfade.
        }
        self.note_skip();
        if self.index + 1 >= self.queue.len() && self.store.settings.autoplay_radio {
            self.continue_into_radio();
            return;
        }
        // Several autoplay songs skipped in a row: this direction is wrong. Drop what autoplay
        // queued ahead and ask for a fresh blend around what the person last chose.
        if self.taste.streak >= crate::taste::STREAK_ENOUGH && self.store.settings.autoplay_radio && !self.radio_loading {
            let cur = self.current.as_ref().map(|t| t.key());
            let keep: Vec<Track> = self.queue.iter().take(self.index + 1).cloned().collect();
            let dropped = self.queue.len() - keep.len();
            if dropped > 0 {
                self.queue = keep;
                self.index = cur.and_then(|k| self.queue.iter().position(|t| t.key() == k)).unwrap_or(self.index.min(self.queue.len().saturating_sub(1)));
                self.persist_queue();
            }
            self.taste.streak = 0;
            eprintln!("[senthym] taste: autoplay songs skipped in a row; blending again ({dropped} dropped)");
            self.continue_into_radio();
            return;
        }
        let i = if self.index + 1 < self.queue.len() { self.index + 1 } else { 0 };
        let i = self.skip_blocked(i, true);
        self.play_index(i);
    }

    /// `i`, or the next song after it that isn't hidden in the playlist being played.
    fn skip_hidden(&self, i: usize) -> usize {
        let n = self.queue.len();
        let Some(ctx) = self.playing_ctx.as_ref() else { return i };
        if n == 0 {
            return i;
        }
        let mut j = i.min(n - 1);
        for _ in 0..n {
            if !self.store.is_hidden(&ctx.id, &self.queue[j].key()) {
                return j;
            }
            j = (j + 1) % n;
        }
        i
    }

    /// The nearest song from `i` (in the given direction) that isn't by a blocked artist.
    fn skip_blocked(&self, mut i: usize, forward: bool) -> usize {
        let n = self.queue.len();
        for _ in 0..n {
            let hidden = self.playing_ctx.as_ref().map(|c| self.store.is_hidden(&c.id, &self.queue[i].key())).unwrap_or(false);
            if !self.store.is_blocked(&self.queue[i]) && !hidden {
                return i;
            }
            i = if forward { (i + 1) % n } else { (i + n - 1) % n };
        }
        i
    }

    pub fn prev(&mut self) {
        if self.queue.is_empty() {
            return;
        }
        // PlayerViewModel.playPreviousSong: always the previous song, wrapping to the last.
        let i = if self.index == 0 { self.queue.len() - 1 } else { self.index - 1 };
        let i = self.skip_blocked(i, false);
        self.play_index(i);
    }

    /// The song being left was barely heard: tell the taste, so skipping a style steers away
    /// from it. Only autoplay's own songs count towards the "wrong direction" streak.
    fn note_skip(&mut self) {
        let Some(t) = self.current.clone() else { return };
        if self.status.position_ms >= crate::taste::SKIP_MS || !t.is_online() {
            return;
        }
        let auto = self.auto_added.contains(&t.key());
        self.taste.skipped(&t, auto);
    }

    /// A song the person chose themself: it leads the taste from now on, and it is saved so the
    /// next run starts where they left off.
    pub fn note_choice(&mut self, t: &Track) {
        if !t.is_online() {
            return;
        }
        self.taste.chose(t);
        self.store.taste_anchors = self.taste.anchors().to_vec();
        self.store.save();
    }

    fn continue_into_radio(&mut self) {
        let seed = self.current.clone().filter(|t| t.is_online());
        match seed {
            Some(seed) if !self.radio_loading => {
                self.radio_loading = true;
                self.spawn_radio(RadioPurpose::Continue, seed);
            }
            Some(_) => {}
            None => self.play_index(0),
        }
    }

    fn on_track_end(&mut self) {
        if self.repeat {
            self.engine.seek(0);
            self.engine.set_playing(true);
            return;
        }
        if self.index + 1 < self.queue.len() {
            let i = self.skip_blocked(self.index + 1, true);
            self.play_index(i);
        } else if self.store.settings.autoplay_radio {
            self.continue_into_radio();
        } else {
            self.engine.set_playing(false);
            self.engine.seek(0);
        }
    }

    fn tick_player(&mut self) {
        self.status = self.engine.status();
        if let Some(err) = self.status.error.clone() {
            if self.status.finished {
                self.toast(err);
                self.engine.stop();
                self.status = self.engine.status();
            }
        }
        if self.engine.take_fade_done() {
            eprintln!("[senthym] crossfade finished");
            self.test_wait_fade = false;
            if let Some(i) = self.fade_target.take() {
                self.index = i;
                self.current = self.queue.get(i).cloned();
                if let Some(t) = self.current.clone() {
                    self.store.add_history(&t);
                    self.load_lyrics(&t);
                }
                self.maybe_extend_radio();
                self.persist_queue();
            }
        }
        if self.current.is_some() && self.status.loaded && self.status.finished && !self.status.fading && !self.loading {
            self.on_track_end();
            return;
        }
        let s = self.store.settings.clone();
        let next_track = self.queue.get(self.index + 1).cloned().filter(|t| !self.store.is_blocked(t));
        // Crossfade (SongPlayer.triggerCrossfade).
        if s.crossfade_on()
            && self.status.playing
            && self.status.loaded
            && !self.status.fading
            && !self.repeat
            && self.fade_target.is_none()
            && self.status.duration_ms > 0
        {
            if let Some(next) = next_track.clone() {
                let remaining = self.status.duration_ms.saturating_sub(self.status.position_ms);
                if remaining <= s.crossfade_ms as u64 + 15_000 {
                    if self.ready_path(&next).is_none() {
                        self.request_resolve(&next, ResolvePurpose::Preload);
                    }
                }
                // startPositionWatch$1 @0x01e4: position >= duration - crossfadeMs.
                if remaining <= s.crossfade_ms as u64 {
                    if let Some(path) = self.ready_path(&next) {
                        let effective = (s.crossfade_ms as u64).min(remaining).max(1000);
                        eprintln!("[senthym] crossfade {effective} ms (dj: {}) into {}", s.crossfade_dj, next.title);
                        self.engine.crossfade_to(&path.to_string_lossy(), next.duration_ms, effective, s.crossfade_dj);
                        self.fade_target = Some(self.index + 1);
                    }
                }
            }
        }
        // Preload the next two songs as soon as this one has started, so Next is instant.
        if self.store.settings.preload && self.status.loaded && !self.loading {
            for k in 1..=2 {
                if let Some(next) = self.queue.get(self.index + k).cloned() {
                    if self.ready_path(&next).is_none() && self.requested.len() < 2 {
                        self.request_resolve(&next, ResolvePurpose::Preload);
                    }
                }
            }
        }
        let _ = next_track;
        // startPositionWatch$1: 250 ms ticks, position saved every 12th tick (3 s).
        if self.last_save.elapsed() > Duration::from_millis(POSITION_SAVE_MS) {
            self.last_save = Instant::now();
            if self.status.loaded {
                self.store.last_position_ms = self.status.position_ms;
                self.store.save();
            }
        }
    }

    fn persist_queue(&mut self) {
        self.store.last_queue = self.queue.iter().take(300).cloned().collect();
        self.store.last_index = self.index.min(299);
        self.store.last_position_ms = 0;
        self.store.save();
    }

    // ------------------------------------------------------------ lyrics

    pub fn load_lyrics(&mut self, t: &Track) {
        let key = t.key();
        let retry = match self.lyrics.get(&key) {
            None => true,
            Some(LyricsState::Missing(at)) => at.elapsed() > Duration::from_millis(lyrics::MISS_RETRY_MS),
            _ => false,
        };
        if retry {
            self.lyrics.insert(key.clone(), LyricsState::Loading);
            let (title, artist, album, dur) = (t.title.clone(), t.artist.clone(), t.album.clone(), t.duration_ms);
            self.spawn(move || Msg::Lyrics(key, lyrics::fetch(&title, &artist, &album, dur)));
        }
    }

    pub fn request_translation(&mut self, key: &str, lines: Vec<String>) {
        let lang = self.translate_lang();
        let k = (key.to_string(), lang.clone());
        if self.translations.contains_key(&k) {
            return;
        }
        self.translations.insert(k, Loadable::Loading);
        let key = key.to_string();
        self.spawn(move || {
            let r = lyrics::translate(&lines, &lang);
            Msg::Translation(key, lang, r)
        });
    }

    pub fn translate_lang(&self) -> String {
        let l = self.store.settings.lyric_translate_lang.clone();
        if !l.is_empty() {
            return l;
        }
        std::env::var("LANG").ok().and_then(|v| v.split(['_', '.']).next().map(String::from)).filter(|v| !v.is_empty() && v != "C").unwrap_or_else(|| "en".into())
    }

    // ------------------------------------------------------------ downloads

    fn queue_download(&mut self, t: Track) {
        if !t.is_online() {
            self.toast("This song is already on your computer");
            return;
        }
        if self.store.download_for(&t).is_some() || self.downloading.iter().any(|x| x.key() == t.key()) {
            return;
        }
        self.downloading.push(t.clone());
        self.download_queue.push(t);
        self.next_download();
    }

    fn next_download(&mut self) {
        if self.download_running || self.download_queue.is_empty() {
            return;
        }
        let t = self.download_queue.remove(0);
        self.download_running = true;
        let quality = self.store.settings.download_quality;
        let folder = self.store.settings.download_dir.clone();
        let known = self.known_video_id(&t);
        let videos = self.store.settings.video_fallback;
        let tx = self.tx.clone();
        self.spawn(move || {
            let id = match known {
                Some(id) => Ok(ytm::audio_id_for(&t, &id)),
                None => {
                    let r = crate::spotify::find_on_youtube(&t, videos);
                    if let (Ok(vid), Source::Spotify(_) | Source::Deezer(_)) = (&r, &t.source) {
                        let _ = tx.send(Msg::SpotifyMatch(t.key(), vid.clone()));
                    }
                    r
                }
            };
            let r = id.and_then(|id| helper::download(&t, &id, quality, &folder));
            Msg::Downloaded(t.key(), t, r)
        });
    }

    // ------------------------------------------------------------ actions

    pub fn navigate(&mut self, r: Route) {
        if self.route != r {
            let old = std::mem::replace(&mut self.route, r);
            self.back.push(old);
            self.forward.clear();
        }
    }

    pub fn apply_actions(&mut self) {
        let actions = std::mem::take(&mut self.actions);
        for a in actions {
            self.apply(a);
        }
    }

    fn apply(&mut self, a: Action) {
        match a {
            Action::Navigate(r) => self.navigate(r),
            Action::Back => {
                if let Some(r) = self.back.pop() {
                    let cur = std::mem::replace(&mut self.route, r);
                    self.forward.push(cur);
                }
            }
            Action::Forward => {
                if let Some(r) = self.forward.pop() {
                    let cur = std::mem::replace(&mut self.route, r);
                    self.back.push(cur);
                }
            }
            Action::PlayTracks { tracks, start, shuffle, context } => {
                if tracks.is_empty() {
                    return;
                }
                if self.route == Route::Search {
                    if let Some(t) = tracks.get(start) {
                        self.store.add_search_item(&Item::from_track(t));
                        self.store.save();
                    }
                }
                self.playing_ctx = context.clone();
                if let Some(c) = &context {
                    self.playing_from = c.title.clone();
                    if matches!(c.kind, ItemKind::Album | ItemKind::Playlist | ItemKind::Artist) {
                        self.store.add_recent_item(c);
                    }
                } else {
                    self.playing_from.clear();
                }
                if shuffle {
                    self.shuffle = true;
                    let start = if start == 0 { rand::random::<usize>() % tracks.len() } else { start };
                    self.set_queue(tracks, start);
                } else {
                    self.set_queue(tracks, start);
                }
            }
            Action::PlayItem(item, shuffle) => self.play_item(item, shuffle),
            Action::OpenItem(item) => {
                // Anything opened from the search page is kept in the recent searches.
                if self.route == Route::Search {
                    self.store.add_search_item(&item);
                    self.store.save();
                }
                self.open_item(item)
            }
            Action::Prefetch(t) => self.prefetch(&t, false),
            Action::PrefetchHot(t) => self.prefetch(&t, true),
            Action::PrefetchRoute(r) => self.prefetch_route(&r),
            Action::SetVideoMode(on) => self.set_video_mode(on),
            Action::SetVideoHeight(h) => {
                // 0 = automatic.
                self.store.settings.video_quality = h;
                self.store.save();
                let want = self.desired_video_height();
                self.switch_quality(want);
            }
            Action::TogglePlay => self.toggle_play(),
            Action::Next => self.next(),
            Action::Prev => self.prev(),
            Action::Seek(ms) => {
                if !self.status.loaded && self.current.is_some() {
                    self.pending_resume_ms = Some(ms);
                } else {
                    self.engine.seek(ms);
                }
            }
            Action::SetVolume(v) => {
                self.store.settings.volume = v;
                self.engine.set_volume(v);
            }
            Action::ToggleShuffle => self.toggle_shuffle(),
            Action::ToggleRepeat => self.repeat = !self.repeat,
            Action::PlayQueueIndex(i) => {
                // Picking a song out of the queue is a choice too, even when autoplay queued it.
                if let Some(t) = self.queue.get(i).cloned() {
                    self.auto_added.remove(&t.key());
                    self.note_choice(&t);
                }
                self.play_index(i)
            }
            Action::AddQueue(t) => {
                if self.queue.iter().any(|x| x.key() == t.key()) {
                    self.toast("Already in your queue");
                } else {
                    let empty = self.queue.is_empty();
                    self.queue.push(t);
                    self.toast("Added to queue");
                    if empty {
                        self.play_index(0);
                    }
                    self.persist_queue();
                }
            }
            Action::PlayNext(t) => {
                if self.queue.is_empty() {
                    self.set_queue(vec![t], 0);
                    return;
                }
                let k = t.key();
                let cur_key = self.current.as_ref().map(|c| c.key());
                if cur_key.as_deref() == Some(&k) {
                    return;
                }
                self.queue.retain(|x| x.key() != k);
                let cur = cur_key.and_then(|ck| self.queue.iter().position(|x| x.key() == ck));
                if let Some(c) = cur {
                    self.index = c;
                }
                let at = cur.map(|c| c + 1).unwrap_or(0).min(self.queue.len());
                self.queue.insert(at, t);
                self.ready_next_changed();
                self.toast("Playing next");
                self.persist_queue();
            }
            Action::RemoveFromQueue(i) => {
                if i < self.queue.len() && i != self.index {
                    self.queue.remove(i);
                    if i < self.index {
                        self.index -= 1;
                    }
                    self.ready_next_changed();
                    self.persist_queue();
                }
            }
            Action::MoveQueue(from, to) => {
                if from < self.queue.len() && to < self.queue.len() && from != to {
                    let cur_key = self.current.as_ref().map(|c| c.key());
                    let t = self.queue.remove(from);
                    self.queue.insert(to, t);
                    if let Some(k) = cur_key {
                        self.index = self.queue.iter().position(|x| x.key() == k).unwrap_or(self.index);
                    }
                    self.ready_next_changed();
                    self.persist_queue();
                }
            }
            Action::ToggleLike(t) => {
                let liked = self.store.toggle_like(&t);
                self.store.save();
                if crate::providers::google_connected() {
                    if let Some(id) = t.video_id().map(String::from) {
                        self.spawn(move || match crate::ytm::rate(&id, liked) {
                            Ok(()) => Msg::Nothing,
                            Err(e) => Msg::Note(format!("Couldn't update your YouTube Music likes: {e}")),
                        });
                    }
                }
                self.toast(if liked { "Added to Liked Songs" } else { "Removed from Liked Songs" });
            }
            Action::HideInPlaylist(pid, name, t, hide) => {
                self.store.set_hidden(&pid, &t.key(), hide);
                self.store.save();
                if hide {
                    self.undo_hide = Some((pid, t.key(), name.clone(), Instant::now()));
                    eprintln!("[senthym] Hidden in {name}");
                } else {
                    self.undo_hide = None;
                    self.toast(format!("Shown again in {name}"));
                }
            }
            Action::ToggleSaved(it) => {
                let saved = self.store.toggle_saved(&it);
                self.store.save();
                let msg = match (&it.kind, saved) {
                    (ItemKind::Artist, true) => "Following",
                    (ItemKind::Artist, false) => "Unfollowed",
                    (_, true) => "Added to Library",
                    (_, false) => "Removed from Library",
                };
                self.toast(msg);
            }
            Action::GoAlbum(t) => {
                if let Some(id) = t.album_id.clone() {
                    self.navigate(if id.starts_with("sp:") { Route::SpotifyCollection(id) } else { Route::Album(id) });
                } else if matches!(t.source, Source::Local(_)) {
                    self.navigate(Route::LocalFiles);
                } else {
                    self.toast("No album for this song");
                }
            }
            Action::GoArtist(t) => {
                if let Some(id) = t.artist_id.clone() {
                    self.navigate(if id.starts_with("sp:") { Route::SpotifyCollection(id) } else { Route::Artist(id) });
                } else {
                    let name = t.artist.clone();
                    self.search_text = name.clone();
                    self.search_query = name.clone();
                    self.search_tab = SearchTab::Artists;
                    self.run_search(&name, SearchTab::Artists);
                    self.navigate(Route::Search);
                }
            }
            Action::Share(link) => {
                self.ctx.output_mut(|o| o.copied_text = link);
                self.toast("Link copied");
            }
            Action::StartRadio(t) => self.start_radio(&t),
            Action::OpenUrl(u) => {
                if u.starts_with("https://") {
                    let _ = std::process::Command::new("xdg-open").arg(&u).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
                }
            }
            Action::CopyText(text, msg) => {
                self.ctx.output_mut(|o| o.copied_text = text);
                self.toast(msg);
            }
            Action::ToggleBlockArtist(id, name) => {
                let (a, b) = (format!("id:{id}"), format!("name:{}", name.trim().to_lowercase()));
                if self.store.artist_blocked(&id, &name) {
                    self.store.blocked_artists.retain(|x| x != &a && x != &b);
                    self.toast(format!("{name} can be played again"));
                } else {
                    self.store.blocked_artists.push(a);
                    self.store.blocked_artists.push(b);
                    self.toast(format!("You won't hear {name}"));
                    // If it is playing now, move on.
                    if self.current.as_ref().map(|t| self.store.is_blocked(t)).unwrap_or(false) {
                        self.next();
                    }
                }
                self.store.save();
            }
            Action::Download(t) => self.queue_download(t),
            Action::DownloadMany(ts) => {
                let n = ts.iter().filter(|t| t.video_id().is_some() && self.store.download_for(t).is_none()).count();
                for t in ts {
                    self.queue_download(t);
                }
                if n > 0 {
                    self.toast(format!("Downloading {n} tracks…"));
                }
            }
            Action::RemoveDownload(t) => {
                if let Some(pos) = self.store.downloads.iter().position(|d| d.track.key() == t.key()) {
                    let d = self.store.downloads.remove(pos);
                    let _ = std::fs::remove_file(&d.path);
                    self.store.save();
                    self.toast(format!("Removed “{}”", t.title));
                }
            }
            Action::CancelDownloads => {
                let keep: Vec<String> = self.download_queue.iter().map(|t| t.key()).collect();
                self.download_queue.clear();
                self.downloading.retain(|t| !keep.contains(&t.key()));
            }
            Action::NewPlaylist(name, t) => {
                let id = self.store.new_playlist(&name);
                if let Some(t) = t {
                    self.store.add_to_playlist(&id, &t);
                }
                self.store.save();
                self.toast("Playlist created");
            }
            Action::AddToPlaylist(pid, t) => {
                let ok = self.store.add_to_playlist(&pid, &t);
                self.store.save();
                let name = self.store.playlists.iter().find(|p| p.id == pid).map(|p| p.name.clone()).unwrap_or_default();
                self.toast(if ok { format!("Added to {name}") } else { format!("Already in {name}") });
            }
            Action::RemoveFromPlaylist(pid, i) => {
                if let Some(p) = self.store.playlists.iter_mut().find(|p| p.id == pid) {
                    if i < p.tracks.len() {
                        p.tracks.remove(i);
                    }
                }
                self.store.save();
            }
            Action::DeletePlaylist(pid) => {
                self.store.playlists.retain(|p| p.id != pid);
                self.store.save();
                if self.route == Route::UserPlaylist(pid) {
                    self.route = Route::Library;
                }
            }
            Action::RenamePlaylist(pid, name) => {
                if let Some(p) = self.store.playlists.iter_mut().find(|p| p.id == pid) {
                    if !name.trim().is_empty() {
                        p.name = name.trim().to_string();
                    }
                }
                self.store.save();
            }
            Action::Search(q) => {
                self.search_text = q.clone();
                self.search_query = q.clone();
                self.suggestions.clear();
                self.suggest_open = false;
                self.store.add_recent_search(&q);
                self.store.save();
                let tab = self.search_tab;
                self.run_search(&q, tab);
                if self.route != Route::Search {
                    self.navigate(Route::Search);
                }
            }
            Action::SetAlt(t, alt) => {
                let msg = match &alt {
                    AltStream::YouTube(_) => "Alternative stream set to YouTube",
                    AltStream::Local(_) => "Alternative stream set to local file",
                };
                self.store.alt_streams.insert(t.key(), alt);
                self.store.save();
                self.toast(msg);
                if self.current.as_ref().map(|c| c.key()) == Some(t.key()) {
                    let pos = self.status.position_ms;
                    self.play_index_from(self.index, Some(pos));
                }
            }
            Action::ClearAlt(t) => {
                self.store.alt_streams.remove(&t.key());
                self.store.save();
                self.toast("Alternative stream cleared");
            }
            Action::ImportFolder(path) => {
                let p = PathBuf::from(&path);
                if !p.is_dir() {
                    self.toast("That folder doesn't exist");
                    return;
                }
                if !self.store.local_folders.contains(&path) {
                    self.store.local_folders.push(path.clone());
                }
                self.local_scanning = true;
                self.spawn(move || Msg::LocalScan(crate::local::scan_folder(&p)));
            }
            Action::ImportFiles(files) => {
                self.local_scanning = true;
                self.spawn(move || {
                    Msg::LocalScan(
                        files.iter().map(PathBuf::from).filter(|p| crate::local::is_audio(p)).map(|p| crate::local::read_track(&p)).collect(),
                    )
                });
            }
            Action::RemoveLocal(t) => {
                self.store.local_tracks.retain(|x| x.source != t.source);
                self.store.save();
                self.toast("Removed from library");
            }
            Action::ClearHistory => {
                self.store.history.clear();
                self.store.save();
            }
            Action::RemoveHistory(ts) => {
                self.store.history.retain(|h| h.ts != ts);
                self.store.save();
            }
            Action::FollowByName(name) => {
                let low = name.to_lowercase();
                if let Some(p) = self.store.followed_artists.iter().position(|a| a.title.to_lowercase() == low) {
                    self.store.followed_artists.remove(p);
                    self.store.save();
                    self.toast("Unfollowed");
                } else {
                    let n = name.clone();
                    self.spawn(move || {
                        let item = ytm::search(&n, Some(ytm::FILTER_ARTISTS)).ok().and_then(|items| {
                            let low = n.to_lowercase();
                            let artists: Vec<Item> = items.into_iter().filter(|i| matches!(i.kind, ItemKind::Artist)).collect();
                            artists.iter().find(|i| i.title.to_lowercase() == low).cloned().or_else(|| artists.into_iter().next())
                        });
                        Msg::FollowFound(n, item)
                    });
                }
            }
            Action::RememberSearch(it) => {
                self.store.add_search_item(&it);
                self.store.save();
            }
            Action::UpdatePlaylist { pid, name, desc, cover } => {
                let mut new_cover: Option<Option<String>> = None;
                match cover {
                    Some(Some(src)) => {
                        // Keep our own copy, so the picture stays even if the original is moved or deleted.
                        let dir = crate::store::data_dir().join("covers");
                        let _ = std::fs::create_dir_all(&dir);
                        let ext = std::path::Path::new(&src).extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
                        let dest = dir.join(format!("{}-{}.{}", pid, chrono::Local::now().timestamp_millis(), ext));
                        match std::fs::copy(&src, &dest) {
                            Ok(_) => new_cover = Some(Some(dest.to_string_lossy().into_owned())),
                            Err(e) => self.toast(format!("Couldn't use that picture ({e})")),
                        }
                    }
                    Some(None) => new_cover = Some(None),
                    None => {}
                }
                if let Some(p) = self.store.playlists.iter_mut().find(|p| p.id == pid) {
                    if !name.trim().is_empty() {
                        p.name = name.trim().to_string();
                    }
                    p.description = desc.trim().to_string();
                    if let Some(c) = new_cover {
                        // The old copy is no longer needed.
                        if let Some(old) = &p.cover {
                            if std::path::Path::new(old).starts_with(crate::store::data_dir().join("covers")) {
                                let _ = std::fs::remove_file(old);
                            }
                        }
                        p.cover = c;
                    }
                }
                self.store.save();
                self.toast("Playlist updated");
            }
            Action::RemoveSearchItem(id) => {
                self.store.recent_search_items.retain(|x| x.id != id);
                self.store.save();
            }
            Action::ClearRecentSearches => {
                self.store.recent_searches.clear();
                self.store.recent_search_items.clear();
                self.store.save();
            }
            Action::RemoveRecentSearch(q) => {
                self.store.recent_searches.retain(|x| x != &q);
                self.store.save();
            }
            Action::Translate(on) => self.show_translation = on,
            Action::CheckUpdate => {
                if !matches!(self.update, UpdateState::Checking | UpdateState::Downloading(..)) {
                    self.update = UpdateState::Checking;
                    let kind = self.update_kind.clone();
                    self.spawn(move || Msg::UpdateChecked(crate::update::check(&kind), true));
                }
            }
            Action::DownloadUpdate => {
                if let UpdateState::Available(rel) = self.update.clone() {
                    if rel.asset.is_none() {
                        // No package for how this copy was installed: the releases page.
                        crate::sys::open_url(&rel.page);
                    } else {
                        let got = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
                        self.update = UpdateState::Downloading(rel.clone(), got.clone());
                        let kind = self.update_kind.clone();
                        // Everything is saved before the update starts.
                        self.store.save();
                        self.spawn(move || Msg::UpdateDone(crate::update::download(&rel, got).and_then(|file| crate::update::install(&kind, &file))));
                    }
                }
            }
            Action::DismissUpdate => {
                if let UpdateState::Available(rel) = &self.update {
                    self.store.settings.update_dismissed = rel.version.clone();
                    self.store.save();
                }
            }
            Action::UpdateHelper => {
                if !self.helper_busy {
                    self.helper_busy = true;
                    self.spawn(|| Msg::HelperUpdate(helper::install_latest()));
                }
            }
            Action::ClearCache => {
                helper::clear_cache();
                self.ready.clear();
                self.cache_bytes = 0;
                self.toast("Cache cleared");
            }
            Action::Toast(s) => self.toast(s),
            Action::SaveSettings => self.store.save(),
        }
    }

    fn open_item(&mut self, item: Item) {
        self.page_hints.insert(item.id.clone(), item.clone());
        if item.id.starts_with("dz:") && item.track.is_none() {
            self.navigate(Route::SpotifyCollection(item.id.clone()));
            return;
        }
        if item.id.starts_with("sp:") && item.track.is_none() {
            self.navigate(Route::SpotifyCollection(item.id.clone()));
            return;
        }
        if item.id == "sp-liked" {
            self.navigate(Route::SpotifyLiked);
            return;
        }
        match item.kind {
            ItemKind::Album => self.navigate(Route::Album(item.id.clone())),
            ItemKind::Artist => self.navigate(Route::Artist(item.id.clone())),
            ItemKind::Playlist | ItemKind::Podcast => self.navigate(Route::Playlist(item.id.clone())),
            ItemKind::Song | ItemKind::Video | ItemKind::Episode => {
                if let Some(t) = item.track.clone() {
                    self.actions.push(Action::PlayTracks { tracks: vec![t.clone()], start: 0, shuffle: false, context: None });
                    // Keep going with similar songs, like tapping a song in the original.
                    if self.store.settings.autoplay_radio {
                        self.start_radio(&t);
                    }
                }
            }
        }
    }

    /// Play an album / playlist / artist straight from a card.
    pub fn play_item(&mut self, item: Item, shuffle: bool) {
        if let Some(t) = item.track.clone() {
            self.actions.push(Action::PlayTracks { tracks: vec![t], start: 0, shuffle: false, context: None });
            return;
        }
        if item.id == "sp-liked" {
            self.actions.push(Action::PlayTracks { tracks: self.store.sp_liked.clone(), start: 0, shuffle, context: Some(item) });
            return;
        }
        if item.id.starts_with("dz:") {
            let it = item.clone();
            self.spawn(move || {
                let r = crate::providers::deezer_collection(&it.id).map(|p| p.tracks);
                Msg::PlayCollection(it, shuffle, r)
            });
            return;
        }
        let it = item.clone();
        self.spawn(move || {
            if it.id.starts_with("sp:") {
                let r = crate::spotify::collection(&it.id).map(|p| p.tracks);
                return Msg::PlayCollection(it, shuffle, r);
            }
            let r = match it.kind {
                ItemKind::Album => ytm::album(&it.id).map(|a| a.tracks),
                ItemKind::Artist => ytm::artist(&it.id).map(|a| a.top_songs),
                _ => ytm::playlist(&it.id).map(|p| p.tracks),
            };
            Msg::PlayCollection(it, shuffle, r)
        });
        self.toast(format!("Loading “{}”…", item.title));
    }

    pub fn liked_item() -> Item {
        Item { kind: ItemKind::Playlist, id: "liked".into(), title: "Liked Songs".into(), subtitle: "Playlist".into(), thumb: None, track: None }
    }

    fn run_autoplay_query(&mut self) {
        let Some(q) = self.autoplay_query.clone() else { return };
        self.run_search(&q, SearchTab::Songs);
        match self.search_results.get(&(q.trim().to_string(), SearchTab::Songs)) {
            Some(Loadable::Ready(items)) => {
                if let Some(it) = items.iter().find(|i| i.track.is_some()).cloned() {
                    self.actions.push(Action::OpenItem(it));
                }
                self.autoplay_query = None;
            }
            Some(Loadable::Failed(_)) => self.autoplay_query = None,
            _ => {}
        }
    }

    /// Run once per frame before drawing.
    pub fn update(&mut self) {
        self.pump_prefetch();
        self.update_video();
        self.run_autoplay_query();
        if self.test_seek_end && self.status.playing && self.status.duration_ms > 20_000 && self.status.position_ms > 1500 && self.queue.len() > 1 {
            self.test_seek_end = false;
            let to = self.status.duration_ms - 9000;
            eprintln!("[senthym] test: jumping to {}", format_ms(to));
            self.engine.seek(to);
        }
        while let Ok(m) = self.rx.try_recv() {
            self.handle(m);
        }
        self.apply_actions();
        self.update_suggestions();
        self.tick_player();
        self.toasts.retain(|t| t.1.elapsed() < Duration::from_millis(3200));
        if self.route == Route::Settings && self.cache_bytes == 0 {
            self.cache_bytes = helper::cache_size().max(1);
        }
    }
}

/// Share of the height (top, bottom) taken by solid black rows, sampled sparsely; capped at 30% each.
fn black_bars(rgba: &[u8], w: usize, h: usize) -> (f32, f32) {
    if w < 16 || h < 16 || rgba.len() < w * h * 4 {
        return (0.0, 0.0);
    }
    let black = |y: usize| -> bool {
        (0..24).all(|i| {
            let x = (i * (w - 1)) / 23;
            let p = (y * w + x) * 4;
            rgba[p].max(rgba[p + 1]).max(rgba[p + 2]) < 28
        })
    };
    let cap = (h as f32 * 0.3) as usize;
    let mut top = 0;
    while top < cap && black(top) {
        top += 1;
    }
    let mut bot = 0;
    while bot < cap && black(h - 1 - bot) {
        bot += 1;
    }
    // A fully dark frame says nothing about bars.
    if top >= cap && bot >= cap {
        return (1.0, 1.0);
    }
    (top as f32 / h as f32, bot as f32 / h as f32)
}
