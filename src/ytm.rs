//! YouTube Music catalogue: search, home feed, albums, artists, playlists, radio and genres.
//!
//! It talks to the same public web endpoints the music.youtube.com site uses (the original
//! app's `com.metrolist.innertube.YouTube` does the same with the WEB_REMIX client).

use crate::model::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const BASE: &str = "https://music.youtube.com/youtubei/v1/";
const CLIENT_NAME: &str = "WEB_REMIX";
const CLIENT_VERSION: &str = "1.20260707.12.00";
const USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";

/// Search filters (the songs and videos codes come from the original app).
pub const FILTER_SONGS: &str = "EgWKAQIIAWoKEAkQBRAKEAMQBA%3D%3D";
pub const FILTER_VIDEOS: &str = "EgWKAQIQAWoKEAkQChAFEAMQBA%3D%3D";
pub const FILTER_ALBUMS: &str = "EgWKAQIYAWoKEAkQAxAEEAkQBQ%3D%3D";
pub const FILTER_ARTISTS: &str = "EgWKAQIgAWoKEAkQAxAEEAkQBQ%3D%3D";
pub const FILTER_PLAYLISTS: &str = "EgeKAQQoAEABagoQAxAEEAoQCRAF";

/// One shared HTTP agent, so connections to YouTube Music are kept open and reused (a new connection
/// for every request costs a few hundred milliseconds of handshake).
fn agent() -> ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT
        .get_or_init(|| ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).user_agent(USER_AGENT).max_idle_connections_per_host(8).build())
        .clone()
}

fn post(endpoint: &str, mut body: Value) -> Result<Value, String> {
    body["context"] = json!({
        "client": {"clientName": CLIENT_NAME, "clientVersion": CLIENT_VERSION, "hl": "en", "gl": "US"}
    });
    let url = format!("{BASE}{endpoint}?prettyPrint=false");
    let mut req = agent()
        .post(&url)
        .set("Content-Type", "application/json")
        .set("Origin", "https://music.youtube.com")
        .set("Referer", "https://music.youtube.com/")
        .set("X-Origin", "https://music.youtube.com");
    // Signed in with a Google account: make the request as that account (no API key needed).
    if let Some((cookies, auth)) = crate::providers::google_headers() {
        req = req.set("Cookie", &cookies).set("Authorization", &auth).set("X-Goog-AuthUser", "0");
    }
    let resp = req
        .send_string(&body.to_string())
        .map_err(|e| format!("Couldn't reach YouTube Music ({e})"))?;
    let text = resp.into_string().map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Like (or remove the like of) a song on the signed-in YouTube Music account, so likes follow the
/// account to every computer.
pub fn rate(video_id: &str, like: bool) -> Result<(), String> {
    let endpoint = if like { "like/like" } else { "like/removelike" };
    post(endpoint, json!({"target": {"videoId": video_id}})).map(|_| ())
}

// ---------------------------------------------------------------- audio versions

/// What kind each video id is (MUSIC_VIDEO_TYPE_ATV = audio version, _OMV = official music video, ...).
fn kind_map() -> &'static Mutex<HashMap<String, String>> {
    static M: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

/// For a video id, the id of the audio version found earlier.
fn audio_map() -> &'static Mutex<HashMap<String, String>> {
    static M: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

fn find_str(v: &Value, key: &str) -> Option<String> {
    match v {
        Value::Object(m) => {
            if let Some(s) = m.get(key).and_then(|x| x.as_str()) {
                return Some(s.to_string());
            }
            m.values().find_map(|x| find_str(x, key))
        }
        Value::Array(a) => a.iter().find_map(|x| find_str(x, key)),
        _ => None,
    }
}

/// The kind of a video id. Rows seen in lists already say; otherwise one quick question to YouTube Music.
pub fn video_type(id: &str) -> Option<String> {
    if let Some(k) = kind_map().lock().unwrap().get(id) {
        return Some(k.clone());
    }
    let v = post("next", json!({"videoId": id, "isAudioOnly": true})).ok()?;
    let t = first(&v, "playlistPanelVideoRenderer").and_then(|p| find_str(p, "musicVideoType"))?;
    kind_map().lock().unwrap().insert(id.to_string(), t.clone());
    Some(t)
}

/// True for the song's audio version ("ATV"), false for a music video or upload.
fn is_audio_version(id: &str) -> Option<bool> {
    video_type(id).map(|t| t == "MUSIC_VIDEO_TYPE_ATV")
}

/// Titles of the videos seen in lists (lowercase), to tell official videos from lyric videos and the like.
fn title_map() -> &'static Mutex<HashMap<String, String>> {
    static M: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Artist lines of the videos seen in lists (lowercase), to check a video is by the song's artist.
fn artist_map() -> &'static Mutex<HashMap<String, String>> {
    static M: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Not a music video at all: a lyric video, visualizer, audio upload, cover, re-upload with subtitles...
/// Words the song's own title has are allowed (a remix's official video is a remix video).
fn has_bad_words(video_title: &str, song_title: &str) -> bool {
    const BAD: [&str; 20] = [
        "lyric", "lyrics", "visualizer", "visualiser", "official audio", "(audio)", "[audio]", "cover", "karaoke", "instrumental",
        "8d", "slowed", "sped up", "nightcore", "reaction", "letra", "sub español", "subtitula", "legendado", "tradu",
    ];
    let v = video_title.to_lowercase();
    let own = song_title.to_lowercase();
    BAD.iter().any(|w| v.contains(w) && !own.contains(w))
}

/// The video is another recording of the song. Performance films (acoustic, live, sessions) are real
/// clips and may stand in for the video when the song has no studio video (`allow_performance`);
/// a remix or mashup never may.
fn is_other_cut(video_title: &str, song_title: &str, allow_performance: bool) -> bool {
    const PERFORMANCE: [&str; 6] = ["acoustic", "live", "session", "sessions", "unplugged", "stripped"];
    const ALTERNATE: [&str; 3] = ["remix", "mashup", "megamix"];
    let words = |t: &str| t.to_lowercase().split(|c: char| !c.is_alphanumeric()).map(str::to_string).collect::<Vec<_>>();
    let (vw, ow) = (words(video_title), words(song_title));
    let hit = |w: &str| vw.iter().any(|x| x == w) && !ow.iter().any(|x| x == w);
    ALTERNATE.iter().any(|w| hit(w)) || (!allow_performance && PERFORMANCE.iter().any(|w| hit(w)))
}

/// The title says it is the official video ("(Official Video)", "Official Music Video", "Official 4K Video"...).
/// Labels also upload lyric videos under the song's bare title with the same "music video" type, so a bare
/// title is not trusted.
fn says_official_video(video_title: &str) -> bool {
    let v = video_title.to_lowercase();
    if v.contains("audio") || v.contains("lyric") {
        return false;
    }
    // Performance films ("Acoustic Version", "Live", "Sessions") are real clips, never lyric uploads.
    const FILMS: [&str; 8] = ["acoustic", "live", "session", "performance", "unplugged", "piano version", "stripped", "behind the"];
    (v.contains("official") && (v.contains("video") || v.contains(" mv") || v.contains("clip"))) || FILMS.iter().any(|w| v.contains(w))
}

/// The title a video was uploaded with and the name of its channel (YouTube's public oEmbed
/// answer; no sign-in needed). Both lowercase.
fn real_meta(id: &str) -> Option<(String, String)> {
    static M: OnceLock<Mutex<HashMap<String, Option<(String, String)>>>> = OnceLock::new();
    let m = M.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(v) = m.lock().unwrap().get(id) {
        return v.clone();
    }
    let url = format!("https://www.youtube.com/oembed?format=json&url=https://www.youtube.com/watch?v={id}");
    let got = agent().get(&url).timeout(Duration::from_secs(6)).call().ok().and_then(|r| r.into_json::<Value>().ok()).and_then(|v| {
        v["title"].as_str().map(|t| (t.to_lowercase(), v["author_name"].as_str().unwrap_or("").to_lowercase()))
    });
    m.lock().unwrap().insert(id.to_string(), got.clone());
    got
}

fn real_title(id: &str) -> Option<String> {
    real_meta(id).map(|(t, _)| t)
}

fn video_map() -> &'static Mutex<HashMap<String, Option<String>>> {
    static M: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

fn clean_artist(a: &str) -> String {
    a.split(',').map(|x| x.trim().trim_end_matches("VEVO").trim().to_string()).filter(|x| !x.is_empty()).collect::<Vec<_>>().join(", ")
}

/// A music video kind: an official music video (OMV) or a video published on a channel (UGC),
/// as opposed to a song's plain audio version (ATV).
fn is_video_kind(kind: Option<&str>) -> bool {
    matches!(kind, Some("MUSIC_VIDEO_TYPE_OMV" | "MUSIC_VIDEO_TYPE_UGC" | "MUSIC_VIDEO_TYPE_OFFICIAL_SOURCE_MUSIC"))
}

/// Letters outside plain a-z folded to their closest ASCII spelling, so a name written with its
/// own letters (an official "VÆB" upload) still matches a re-upload or search query that typed
/// it in plain ASCII ("VAEB"), and the other way round. Only widens what counts as the same
/// name; the artist and title must still both be confirmed, so this never admits a wrong video.
fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'æ' | 'ӕ' => "ae".to_string(),
            'œ' => "oe".to_string(),
            'ø' => "o".to_string(),
            'ß' => "ss".to_string(),
            'þ' => "th".to_string(),
            'ð' => "d".to_string(),
            _ => c.to_string(),
        })
        .collect::<String>()
        .chars()
        .map(|c| {
            // Strip the rest (accents, umlauts, rings...) to their base letter via decomposition.
            unicode_base(c)
        })
        .collect()
}

/// A character's base letter: strip a combining accent by decomposing it (NFD) and keeping only
/// the first, non-combining part. Plain ASCII letters pass through unchanged.
fn unicode_base(c: char) -> char {
    if c.is_ascii() {
        return c;
    }
    // A small, direct table covers the common Latin letters without pulling in a Unicode crate.
    const MAP: &[(&str, char)] = &[
        ("àáâãäåā", 'a'),
        ("èéêëē", 'e'),
        ("ìíîïī", 'i'),
        ("òóôõöøō", 'o'),
        ("ùúûüū", 'u'),
        ("ýÿ", 'y'),
        ("ñ", 'n'),
        ("ç", 'c'),
        ("ś", 's'),
        ("ź", 'z'),
    ];
    MAP.iter().find(|(from, _)| from.chars().any(|f| f == c)).map(|(_, to)| *to).unwrap_or(c)
}

/// True when one of the names in `artist` appears in `hay`, as words or run together the way
/// channel names are ("KygoOfficialVEVO", "Sigrid UK"), letters folded so a different spelling
/// of the same name ("VÆB" / "VAEB") still counts as the same.
fn names_match(hay: &str, artist: &str) -> bool {
    let hay = fold(&crate::spotify::normalized(hay));
    let spaced = format!(" {hay} ");
    let packed = hay.replace(' ', ""); // channel names run the words together
    artist.split([',', '&']).any(|a| {
        let n = fold(&crate::spotify::normalized(a));
        match n.chars().count() {
            0 => false,
            1..=2 => spaced.contains(&format!(" {n} ")),
            _ => spaced.contains(&format!(" {n} ")) || packed.contains(&n.replace(' ', "")),
        }
    })
}

/// True when one of the song's credited artists is named in what is known about the video: its
/// artist line, its titles or its channel name. A video whose artist can't be confirmed is never
/// used, so a song never gets another artist's video. The list data decides when it can; YouTube
/// is only asked (once, remembered) when it can't, so the common case costs nothing.
fn artist_matches(id: &str, artist: &str) -> bool {
    let quick = format!(
        "{} {}",
        artist_map().lock().unwrap().get(id).cloned().unwrap_or_default(),
        title_map().lock().unwrap().get(id).cloned().unwrap_or_default()
    );
    if names_match(&quick, artist) {
        return true;
    }
    match real_meta(id) {
        Some((rt, by)) => names_match(&format!("{rt} {by}"), artist),
        None => false,
    }
}

/// True when the video's title is this song: the song's title appears in the list or upload title
/// (punctuation ignored, letters folded), or the titles match closely once an "Artist -" or
/// "Artist:" prefix is dropped. The list title decides when it can; the upload title is only
/// fetched when it can't.
fn title_matches(id: &str, song_title: &str) -> bool {
    let want = fold(&crate::spotify::normalized(&clean_title(song_title)));
    if want.is_empty() {
        return false;
    }
    let one = |v: &str| {
        if v.is_empty() {
            return false;
        }
        if fold(&crate::spotify::normalized(&clean_title(v))).contains(&want) {
            return true;
        }
        let after = v.split_once(['-', ':']).map(|(_, a)| a.trim()).unwrap_or(v);
        crate::spotify::bigram(&fold(&clean_title(after)), &fold(&clean_title(song_title))) >= 0.75
    };
    if one(&title_map().lock().unwrap().get(id).cloned().unwrap_or_default()) {
        return true;
    }
    real_meta(id).map(|(rt, _)| one(&rt)).unwrap_or(false)
}

/// The official music video of a song, if it has one: the given id when it already is a video,
/// else the best matching official video found by search, else an official performance film
/// (live, acoustic, sessions) by the same artist. Every pick must name the song's artist in its
/// artist line, upload title or channel name, and carry the song's title, so a wrong video can
/// never be shown.
pub fn video_id_for(t: &Track, id: &str) -> Option<String> {
    if let Some(v) = video_map().lock().unwrap().get(id) {
        return v.clone();
    }
    let found = find_video_for(t, id);
    video_map().lock().unwrap().insert(id.to_string(), found.clone());
    found
}

fn find_video_for(t: &Track, id: &str) -> Option<String> {
    let kind_of = |x: &str| kind_map().lock().unwrap().get(x).cloned();
    let title_of = |x: &str| title_map().lock().unwrap().get(x).cloned().unwrap_or_default();
    // Something marked a music video ("Official Video", or a performance film), on either the
    // list title or the real upload title, and never a lyric / visualizer / audio upload.
    let looks_like_video = |x: &str, song_title: &str| {
        let lt = title_of(x);
        if has_bad_words(&lt, song_title) {
            return false;
        }
        if says_official_video(&lt) {
            return true;
        }
        // The list title doesn't decide: the real upload title does (fetched once, remembered).
        match real_title(x) {
            Some(rt) => !has_bad_words(&rt, song_title) && says_official_video(&rt),
            None => false,
        }
    };

    // The id being played is itself a music video (a plain song whose only upload is one, such
    // as a lyric video with no separate audio track): shown only when it looks like a real video.
    // `app.rs` bypasses this check instead, directly, when the person picked a video item
    // themselves (a search result, a related video, a playlist row) — there the choice is
    // explicit and any video-kind upload is shown as is, with no second-guessing.
    if is_video_kind(video_type(id).as_deref()) && looks_like_video(id, &t.title) {
        return Some(id.to_string());
    }

    let mut c = t.clone();
    c.title = clean_title(&t.title);
    c.artist = clean_artist(&t.artist);

    // The pool: the scored matches, then an explicit "official video" search.
    let mut pool: Vec<String> = crate::spotify::candidates(&c, true).unwrap_or_default();
    let q = format!("{} {} official video", c.title, c.artist.split(',').next().unwrap_or(""));
    if let Ok(items) = search(&q, Some(FILTER_VIDEOS)) {
        for i in items {
            if i.track.is_some() && !pool.contains(&i.id) {
                pool.push(i.id);
            }
        }
    }
    // Only music videos, label (OMV) videos first, and not too many of either kind: a check can
    // cost one (remembered) question to YouTube, and the right video sits near the front of its
    // kind when it exists. Capping before sorting would let one kind push the other out entirely.
    pool.retain(|x| is_video_kind(kind_of(x).as_deref()));
    let (mut omv, mut ugc): (Vec<String>, Vec<String>) = pool.into_iter().partition(|x| kind_of(x).as_deref() == Some("MUSIC_VIDEO_TYPE_OMV"));
    omv.truncate(6);
    ugc.truncate(6);
    omv.extend(ugc);
    let pool = omv;
    // Cheap checks first; the later checks may ask YouTube for the upload title (once, remembered).
    let verified = |x: &str, allow_performance: bool| {
        !is_other_cut(&title_of(x), &c.title, allow_performance)
            && looks_like_video(x, &c.title)
            && title_matches(x, &c.title)
            && artist_matches(x, &c.artist)
            && real_title(x).map(|rt| !is_other_cut(&rt, &c.title, allow_performance)).unwrap_or(true)
    };
    // 1. The song's own official studio music video.
    let mut picks: Vec<&String> = pool.iter().filter(|x| kind_of(x).as_deref() == Some("MUSIC_VIDEO_TYPE_OMV") && verified(x, false)).collect();
    picks.sort_by_key(|x| {
        let tt = title_of(x);
        !(tt.contains("official") && tt.contains("video"))
    });
    if let Some(p) = picks.first() {
        return Some((*p).clone());
    }
    // 2. No studio video: an official performance film (live, acoustic, sessions) of the same
    // song by the same artist; the pool already has official music videos first.
    pool.iter().find(|x| verified(x, true)).cloned()
}

/// Title without "(Official Video)", "[Lyric Video]", "- Official Audio" and the like.
fn clean_title(title: &str) -> String {
    const WORDS: [&str; 10] = ["official", "video", "audio", "lyric", "visuali", "clip", "4k", "hd", "m/v", "mv"];
    let mut out = title.to_string();
    for (open, close) in [('(', ')'), ('[', ']')] {
        loop {
            let lower = out.to_ascii_lowercase();
            let Some(start) = lower.find(open) else { break };
            let Some(end) = lower[start..].find(close) else { break };
            let inner = &lower[start..start + end];
            if WORDS.iter().any(|w| inner.contains(w)) {
                out.replace_range(start..start + end + 1, "");
            } else {
                // Keep this group; look for later ones by cutting scanning point.
                let rest = out[start + end + 1..].to_string();
                let head = out[..start + end + 1].to_string();
                let cleaned_rest = clean_title(&rest);
                return format!("{head}{cleaned_rest}").trim().to_string();
            }
        }
    }
    if let Some(i) = out.to_ascii_lowercase().rfind(" - official") {
        out.truncate(i);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The id to actually play for a song: its audio version when the given id is a music video or upload.
/// Falls back to the given id whenever no good match is found, so nothing that played stops playing.
pub fn audio_id_for(t: &Track, id: &str) -> String {
    if let Some(a) = audio_map().lock().unwrap().get(id) {
        return a.clone();
    }
    if is_audio_version(id) != Some(false) {
        return id.to_string();
    }
    let mut c = t.clone();
    c.title = clean_title(&t.title);
    c.artist = clean_artist(&t.artist);
    let found = crate::spotify::candidates(&c, false).ok().and_then(|ids| ids.into_iter().find(|x| x != id));
    let out = found.unwrap_or_else(|| id.to_string());
    audio_map().lock().unwrap().insert(id.to_string(), out.clone());
    out
}

// ---------------------------------------------------------------- helpers

fn runs_text(v: &Value) -> String {
    if let Some(s) = v.get("simpleText").and_then(|s| s.as_str()) {
        return s.to_string();
    }
    v.get("runs")
        .and_then(|r| r.as_array())
        .map(|runs| runs.iter().filter_map(|r| r["text"].as_str()).collect::<String>())
        .unwrap_or_default()
}

fn runs(v: &Value) -> Vec<Value> {
    v.get("runs").and_then(|r| r.as_array()).cloned().unwrap_or_default()
}

fn best_thumb(v: &Value) -> Option<String> {
    // Find the first "thumbnails" array below v and take its largest entry.
    fn find(v: &Value) -> Option<&Vec<Value>> {
        match v {
            Value::Object(m) => {
                if let Some(Value::Array(a)) = m.get("thumbnails") {
                    return Some(a);
                }
                m.values().find_map(find)
            }
            Value::Array(a) => a.iter().find_map(find),
            _ => None,
        }
    }
    let arr = find(v)?;
    arr.iter()
        .max_by_key(|t| t["width"].as_u64().unwrap_or(0))
        .and_then(|t| t["url"].as_str())
        .map(|u| if u.starts_with("//") { format!("https:{u}") } else { u.to_string() })
}

fn page_type(endpoint: &Value) -> Option<&str> {
    endpoint["browseEndpoint"]["browseEndpointContextSupportedConfigs"]["browseEndpointContextMusicConfig"]
        ["pageType"]
        .as_str()
}

/// Collect every object stored under `key` anywhere inside `v`, in document order.
fn collect<'a>(v: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(m) => {
            for (k, child) in m {
                if k == key {
                    out.push(child);
                } else {
                    collect(child, key, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|c| collect(c, key, out)),
        _ => {}
    }
}

fn first<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    let mut out = Vec::new();
    collect(v, key, &mut out);
    out.into_iter().next()
}

struct Artists {
    names: String,
    first_id: Option<String>,
    album: String,
    album_id: Option<String>,
    duration_ms: u64,
    kind_hint: Option<ItemKind>,
    other: Vec<String>,
}

/// Split a subtitle column ("Song • Artist & Artist • Album • 3:45") into its parts.
fn split_subtitle(all_runs: &[Value]) -> Artists {
    let mut a = Artists {
        names: String::new(),
        first_id: None,
        album: String::new(),
        album_id: None,
        duration_ms: 0,
        kind_hint: None,
        other: vec![],
    };
    let mut artist_parts: Vec<String> = vec![];
    for r in all_runs {
        let text = r["text"].as_str().unwrap_or("").to_string();
        let ep = &r["navigationEndpoint"];
        match page_type(ep) {
            Some("MUSIC_PAGE_TYPE_ARTIST") | Some("MUSIC_PAGE_TYPE_USER_CHANNEL") => {
                if a.first_id.is_none() {
                    a.first_id = ep["browseEndpoint"]["browseId"].as_str().map(String::from);
                }
                artist_parts.push(text);
            }
            Some("MUSIC_PAGE_TYPE_ALBUM") => {
                a.album = text;
                a.album_id = ep["browseEndpoint"]["browseId"].as_str().map(String::from);
            }
            _ => {
                let t = text.trim();
                if t.is_empty() || t == "•" || t == "&" || t == "," {
                    continue;
                }
                if let Some(d) = parse_duration(t) {
                    a.duration_ms = d;
                    continue;
                }
                match t {
                    "Song" => a.kind_hint = Some(ItemKind::Song),
                    "Video" => a.kind_hint = Some(ItemKind::Video),
                    "Album" | "Single" | "EP" => a.kind_hint = Some(ItemKind::Album),
                    "Artist" => a.kind_hint = Some(ItemKind::Artist),
                    "Playlist" => a.kind_hint = Some(ItemKind::Playlist),
                    "Episode" => a.kind_hint = Some(ItemKind::Episode),
                    "Podcast" => a.kind_hint = Some(ItemKind::Podcast),
                    _ => a.other.push(t.to_string()),
                }
            }
        }
    }
    if artist_parts.is_empty() {
        // Some rows give artists as plain text: take the first plain run that isn't a count.
        if let Some(pos) = a
            .other
            .iter()
            .position(|t| !t.contains("views") && !t.contains("plays") && !t.contains("songs") && !t.chars().all(|c| c.is_ascii_digit()))
        {
            artist_parts.push(a.other.remove(pos));
        }
    }
    a.names = artist_parts.join(", ");
    a
}

/// Turn a `musicResponsiveListItemRenderer` (a row) into an item.
fn parse_row(r: &Value, fallback_album: Option<(&str, &str, Option<&str>)>) -> Option<Item> {
    let cols = r["flexColumns"].as_array()?;
    let col = |i: usize| cols.get(i).map(|c| &c["musicResponsiveListItemFlexColumnRenderer"]["text"]);
    let title_v = col(0)?;
    let title = runs_text(title_v);
    if title.is_empty() {
        return None;
    }
    let mut sub_runs: Vec<Value> = vec![];
    for i in 1..cols.len() {
        if let Some(c) = col(i) {
            if !sub_runs.is_empty() {
                sub_runs.push(json!({"text": " • "}));
            }
            sub_runs.extend(runs(c));
        }
    }
    if let Some(fixed) = r["fixedColumns"].as_array() {
        for f in fixed {
            sub_runs.extend(runs(&f["musicResponsiveListItemFixedColumnRenderer"]["text"]));
        }
    }
    let parts = split_subtitle(&sub_runs);
    let thumb = best_thumb(&r["thumbnail"]);

    let video_id = r["playlistItemData"]["videoId"]
        .as_str()
        .or_else(|| title_v["runs"][0]["navigationEndpoint"]["watchEndpoint"]["videoId"].as_str())
        .or_else(|| {
            first(&r["overlay"], "watchEndpoint").and_then(|w| w["videoId"].as_str())
        })
        .map(String::from);

    if let Some(vid) = video_id {
        if let Some(t) = find_str(r, "musicVideoType") {
            kind_map().lock().unwrap().insert(vid.clone(), t);
            title_map().lock().unwrap().insert(vid.clone(), title.to_lowercase());
            if !parts.names.is_empty() {
                artist_map().lock().unwrap().insert(vid.clone(), parts.names.to_lowercase());
            }
        }
        let mut kind = parts.kind_hint.clone().unwrap_or(ItemKind::Song);
        if kind != ItemKind::Video && kind != ItemKind::Episode {
            kind = ItemKind::Song;
        }
        let (album, album_id, thumb) = match (&parts.album_id, fallback_album) {
            (None, Some((name, id, th))) => (name.to_string(), Some(id.to_string()), thumb.or(th.map(String::from))),
            _ => (parts.album.clone(), parts.album_id.clone(), thumb),
        };
        let artist = if parts.names.is_empty() {
            fallback_album.map(|_| String::new()).unwrap_or_default()
        } else {
            parts.names.clone()
        };
        let track = Track {
            source: Source::YouTube(vid.clone()),
            title: title.clone(),
            artist: artist.clone(),
            artist_id: parts.first_id.clone(),
            album,
            album_id,
            duration_ms: parts.duration_ms,
            thumb: thumb.clone(),
        };
        let subtitle = if kind == ItemKind::Video {
            format!("Video • {artist}")
        } else {
            artist
        };
        return Some(Item { kind, id: vid, title, subtitle, thumb, track: Some(track) });
    }

    let ep = &r["navigationEndpoint"];
    let browse_id = ep["browseEndpoint"]["browseId"].as_str()?.to_string();
    let kind = match page_type(ep) {
        Some("MUSIC_PAGE_TYPE_ALBUM") => ItemKind::Album,
        Some("MUSIC_PAGE_TYPE_ARTIST") | Some("MUSIC_PAGE_TYPE_USER_CHANNEL") => ItemKind::Artist,
        Some("MUSIC_PAGE_TYPE_PLAYLIST") => ItemKind::Playlist,
        Some("MUSIC_PAGE_TYPE_PODCAST_SHOW_DETAIL_PAGE") => ItemKind::Podcast,
        _ => parts.kind_hint.clone()?,
    };
    let mut bits: Vec<String> = vec![];
    if let Some(k) = &parts.kind_hint {
        bits.push(k.label().to_string());
    }
    if !parts.names.is_empty() {
        bits.push(parts.names.clone());
    }
    bits.extend(parts.other.iter().cloned());
    Some(Item { kind, id: browse_id, title, subtitle: bits.join(" • "), thumb, track: None })
}

/// Turn a `musicTwoRowItemRenderer` (a card) into an item.
fn parse_card(r: &Value) -> Option<Item> {
    let title = runs_text(&r["title"]);
    let sub_runs = runs(&r["subtitle"]);
    let parts = split_subtitle(&sub_runs);
    let subtitle = runs_text(&r["subtitle"]);
    let thumb = best_thumb(&r["thumbnailRenderer"]);
    let ep = &r["navigationEndpoint"];
    if let Some(vid) = ep["watchEndpoint"]["videoId"].as_str() {
        let track = Track {
            source: Source::YouTube(vid.to_string()),
            title: title.clone(),
            artist: if parts.names.is_empty() { subtitle.clone() } else { parts.names.clone() },
            artist_id: parts.first_id.clone(),
            album: parts.album.clone(),
            album_id: parts.album_id.clone(),
            duration_ms: parts.duration_ms,
            thumb: thumb.clone(),
        };
        return Some(Item { kind: ItemKind::Song, id: vid.to_string(), title, subtitle, thumb, track: Some(track) });
    }
    let browse_id = ep["browseEndpoint"]["browseId"].as_str()?.to_string();
    let kind = match page_type(ep) {
        Some("MUSIC_PAGE_TYPE_ALBUM") => ItemKind::Album,
        Some("MUSIC_PAGE_TYPE_ARTIST") | Some("MUSIC_PAGE_TYPE_USER_CHANNEL") => ItemKind::Artist,
        Some("MUSIC_PAGE_TYPE_PLAYLIST") => ItemKind::Playlist,
        Some("MUSIC_PAGE_TYPE_PODCAST_SHOW_DETAIL_PAGE") => ItemKind::Podcast,
        _ => return None,
    };
    Some(Item { kind, id: browse_id, title, subtitle, thumb, track: None })
}

fn parse_any_item(container: &Value) -> Option<Item> {
    if let Some(r) = container.get("musicResponsiveListItemRenderer") {
        return parse_row(r, None);
    }
    if let Some(r) = container.get("musicTwoRowItemRenderer") {
        return parse_card(r);
    }
    None
}

fn shelf_title(header: &Value) -> String {
    let mut t = runs_text(&header["musicCarouselShelfBasicHeaderRenderer"]["title"]);
    if t.is_empty() {
        t = runs_text(&header["musicImmersiveCarouselShelfBasicHeaderRenderer"]["title"]);
    }
    t
}

/// Every carousel and shelf on a page, as titled sections.
fn sections_of(root: &Value) -> Vec<Section> {
    let mut out = vec![];
    let mut shelves = Vec::new();
    collect(root, "musicCarouselShelfRenderer", &mut shelves);
    for s in shelves {
        let title = shelf_title(&s["header"]);
        let items: Vec<Item> = s["contents"].as_array().map(|a| a.iter().filter_map(parse_any_item).collect()).unwrap_or_default();
        if !items.is_empty() {
            out.push(Section { title, items });
        }
    }
    out
}

// ---------------------------------------------------------------- public API

pub fn home() -> Result<Vec<Section>, String> {
    let v = post("browse", json!({"browseId": "FEmusic_home"}))?;
    let mut sections = sections_of(&v["contents"]);
    // Follow one continuation for more shelves, like scrolling down on the site.
    if let Some(token) = first(&v["contents"], "nextContinuationData").and_then(|c| c["continuation"].as_str()) {
        if let Ok(more) = post("browse", json!({"continuation": token})) {
            sections.extend(sections_of(&more["continuationContents"]));
        }
    }
    Ok(sections)
}

pub fn suggestions(query: &str) -> Result<Vec<String>, String> {
    let v = post("music/get_search_suggestions", json!({"input": query}))?;
    let mut out = vec![];
    let mut found = Vec::new();
    collect(&v, "searchSuggestionRenderer", &mut found);
    for s in found {
        let t = runs_text(&s["suggestion"]);
        if !t.is_empty() {
            out.push(t);
        }
    }
    out.truncate(8);
    Ok(out)
}

/// Search one category (a filter) or everything (no filter).
/// Raw text of the account menu reply (used to check a sign-in, like the original's validateLogin).
pub fn account_menu_text() -> Result<String, String> {
    let v = post("account/account_menu", json!({}))?;
    Ok(v.to_string())
}

pub fn search(query: &str, filter: Option<&str>) -> Result<Vec<Item>, String> {
    let mut body = json!({"query": query});
    if let Some(f) = filter {
        body["params"] = json!(urlencoding::decode(f).map(|s| s.into_owned()).unwrap_or_else(|_| f.to_string()));
    }
    let v = post("search", body)?;
    let mut items = vec![];
    // The "top result" card first, then every row.
    let mut cards = Vec::new();
    collect(&v["contents"], "musicCardShelfRenderer", &mut cards);
    for c in cards {
        let title = runs_text(&c["title"]);
        let ep = runs(&c["title"]).first().map(|r| r["navigationEndpoint"].clone()).unwrap_or(Value::Null);
        let sub = runs(&c["subtitle"]);
        let parts = split_subtitle(&sub);
        let thumb = best_thumb(&c["thumbnail"]);
        if let Some(vid) = ep["watchEndpoint"]["videoId"].as_str() {
            let t = Track {
                source: Source::YouTube(vid.into()),
                title: title.clone(),
                artist: parts.names.clone(),
                artist_id: parts.first_id.clone(),
                album: parts.album.clone(),
                album_id: parts.album_id.clone(),
                duration_ms: parts.duration_ms,
                thumb: thumb.clone(),
            };
            items.push(Item { kind: ItemKind::Song, id: vid.into(), title, subtitle: parts.names, thumb, track: Some(t) });
        } else if let Some(bid) = ep["browseEndpoint"]["browseId"].as_str() {
            let kind = match page_type(&ep) {
                Some("MUSIC_PAGE_TYPE_ALBUM") => ItemKind::Album,
                Some("MUSIC_PAGE_TYPE_PLAYLIST") => ItemKind::Playlist,
                _ => ItemKind::Artist,
            };
            items.push(Item { kind, id: bid.into(), title, subtitle: runs_text(&c["subtitle"]), thumb, track: None });
        }
    }
    let mut rows = Vec::new();
    collect(&v["contents"], "musicResponsiveListItemRenderer", &mut rows);
    for r in rows {
        if let Some(it) = parse_row(r, None) {
            if !items.iter().any(|x| x.id == it.id) {
                items.push(it);
            }
        }
    }
    Ok(items)
}

pub fn album(browse_id: &str) -> Result<AlbumPage, String> {
    let v = post("browse", json!({"browseId": browse_id}))?;
    let mut page = AlbumPage { id: browse_id.to_string(), ..Default::default() };
    if let Some(h) = first(&v["contents"], "musicResponsiveHeaderRenderer") {
        page.title = runs_text(&h["title"]);
        page.subtitle = runs_text(&h["subtitle"]);
        page.detail = runs_text(&h["secondSubtitle"]);
        page.thumb = best_thumb(&h["thumbnail"]);
        let strap = runs(&h["straplineTextOne"]);
        page.artist = runs_text(&h["straplineTextOne"]);
        page.artist_id = strap.iter().find_map(|r| r["navigationEndpoint"]["browseEndpoint"]["browseId"].as_str()).map(String::from);
    } else if let Some(h) = first(&v["header"], "title") {
        page.title = runs_text(h);
        page.thumb = best_thumb(&v["header"]);
    }
    let album_ref = (page.title.clone(), page.id.clone(), page.thumb.clone());
    let mut shelves = Vec::new();
    collect(&v["contents"], "musicShelfRenderer", &mut shelves);
    for s in shelves {
        if let Some(rows) = s["contents"].as_array() {
            for row in rows {
                if let Some(r) = row.get("musicResponsiveListItemRenderer") {
                    if let Some(mut it) = parse_row(r, Some((&album_ref.0, &album_ref.1, album_ref.2.as_deref()))) {
                        if let Some(t) = it.track.as_mut() {
                            if t.artist.is_empty() {
                                t.artist = page.artist.clone();
                                t.artist_id = page.artist_id.clone();
                            }
                            if t.thumb.is_none() {
                                t.thumb = page.thumb.clone();
                            }
                            page.tracks.push(t.clone());
                        }
                    }
                }
            }
        }
    }
    page.more = sections_of(&v["contents"]);
    Ok(page)
}

pub fn artist(browse_id: &str) -> Result<ArtistPage, String> {
    let v = post("browse", json!({"browseId": browse_id}))?;
    let mut page = ArtistPage { id: browse_id.to_string(), ..Default::default() };
    let header = v["header"]
        .get("musicImmersiveHeaderRenderer")
        .or_else(|| v["header"].get("musicVisualHeaderRenderer"))
        .cloned()
        .unwrap_or(Value::Null);
    page.name = runs_text(&header["title"]);
    page.listeners = runs_text(&header["monthlyListenerCount"]);
    page.description = runs_text(&header["description"]);
    page.thumb = best_thumb(&header["thumbnail"]);
    page.radio_playlist = first(&header["startRadioButton"], "watchEndpoint")
        .and_then(|w| w["playlistId"].as_str())
        .map(String::from);
    let mut shelves = Vec::new();
    collect(&v["contents"], "musicShelfRenderer", &mut shelves);
    if let Some(s) = shelves.first() {
        if let Some(rows) = s["contents"].as_array() {
            for row in rows {
                if let Some(it) = parse_any_item(row) {
                    if let Some(mut t) = it.track {
                        if t.artist.is_empty() {
                            t.artist = page.name.clone();
                        }
                        page.top_songs.push(t);
                    }
                }
            }
        }
    }
    page.sections = sections_of(&v["contents"]);
    Ok(page)
}

pub fn playlist(browse_id: &str) -> Result<PlaylistPage, String> {
    playlist_progressive(browse_id, &mut |_| {})
}

/// Like `playlist`, but hands over the page as soon as the first batch of songs is in and again after
/// every further batch, so the page can open at once while a long playlist keeps loading.
pub fn playlist_progressive(browse_id: &str, emit: &mut dyn FnMut(&PlaylistPage)) -> Result<PlaylistPage, String> {
    let id = if browse_id.starts_with("VL") || browse_id.starts_with("MPRE") || browse_id.starts_with("FE") {
        browse_id.to_string()
    } else {
        format!("VL{browse_id}")
    };
    let v = post("browse", json!({"browseId": id}))?;
    let mut page = PlaylistPage { id: browse_id.to_string(), ..Default::default() };
    if let Some(h) = first(&v["contents"], "musicResponsiveHeaderRenderer").or_else(|| first(&v["header"], "musicDetailHeaderRenderer")) {
        page.title = runs_text(&h["title"]);
        page.owner = runs_text(&h["straplineTextOne"]);
        if page.owner.is_empty() {
            page.owner = runs_text(&h["subtitle"]);
        }
        page.detail = runs_text(&h["secondSubtitle"]);
        page.thumb = best_thumb(&h["thumbnail"]);
    }
    let mut rows = Vec::new();
    collect(&v["contents"], "musicResponsiveListItemRenderer", &mut rows);
    let mut cont = first(&v["contents"], "continuationCommand").and_then(|c| c["token"].as_str()).map(String::from);
    let push_rows = |rows: Vec<&Value>, page: &mut PlaylistPage| {
        for r in rows {
            if let Some(it) = parse_row(r, None) {
                if let Some(t) = it.track {
                    if !page.tracks.iter().any(|x| x.source == t.source) {
                        page.tracks.push(t);
                    }
                }
            }
        }
    };
    push_rows(rows, &mut page);
    if page.title.is_empty() {
        page.title = "Playlist".into();
    }
    page.loading_more = cont.is_some();
    if page.loading_more {
        emit(&page);
    }
    // Load up to 4 more pages of a long playlist.
    for n in 0..4 {
        let Some(token) = cont.take() else { break };
        let Ok(more) = post("browse", json!({"continuation": token})) else { break };
        let mut rows = Vec::new();
        collect(&more, "musicResponsiveListItemRenderer", &mut rows);
        if rows.is_empty() {
            break;
        }
        push_rows(rows, &mut page);
        cont = first(&more, "continuationCommand").and_then(|c| c["token"].as_str()).map(String::from);
        page.loading_more = cont.is_some() && n < 3;
        if page.loading_more {
            emit(&page);
        }
    }
    page.loading_more = false;
    Ok(page)
}

/// Songs like this one: the "radio" YouTube Music builds from a song.
pub fn radio(video_id: &str, playlist_id: Option<&str>) -> Result<Vec<Track>, String> {
    let pl = playlist_id.map(String::from).unwrap_or_else(|| format!("RDAMVM{video_id}"));
    let mut body = json!({"playlistId": pl, "isAudioOnly": true});
    if !video_id.is_empty() {
        body["videoId"] = json!(video_id);
    }
    let v = post("next", body)?;
    let mut panel = Vec::new();
    collect(&v, "playlistPanelVideoRenderer", &mut panel);
    let mut out = vec![];
    for p in panel {
        let Some(vid) = p["videoId"].as_str() else { continue };
        let parts = split_subtitle(&runs(&p["longBylineText"]));
        let duration_ms = parse_duration(&runs_text(&p["lengthText"])).unwrap_or(parts.duration_ms);
        out.push(Track {
            source: Source::YouTube(vid.into()),
            title: runs_text(&p["title"]),
            artist: if parts.names.is_empty() { runs_text(&p["shortBylineText"]) } else { parts.names },
            artist_id: parts.first_id,
            album: parts.album,
            album_id: parts.album_id,
            duration_ms,
            thumb: best_thumb(&p["thumbnail"]),
        });
    }
    Ok(out)
}

/// The "Browse all" tiles (moods and genres).
pub fn categories() -> Result<Vec<Category>, String> {
    let v = post("browse", json!({"browseId": "FEmusic_moods_and_genres"}))?;
    let mut buttons = Vec::new();
    collect(&v, "musicNavigationButtonRenderer", &mut buttons);
    let mut out = vec![];
    for b in buttons {
        let ep = &b["clickCommand"]["browseEndpoint"];
        let Some(id) = ep["browseId"].as_str() else { continue };
        out.push(Category {
            title: runs_text(&b["buttonText"]),
            browse_id: id.to_string(),
            params: ep["params"].as_str().map(String::from),
            color: b["solid"]["leftStripeColor"].as_u64().unwrap_or(0xff2a2a2a) as u32,
        });
    }
    Ok(out)
}

pub fn category(browse_id: &str, params: Option<&str>) -> Result<Vec<Section>, String> {
    let mut body = json!({"browseId": browse_id});
    if let Some(p) = params {
        body["params"] = json!(p);
    }
    let v = post("browse", body)?;
    let mut sections = sections_of(&v["contents"]);
    let mut grids = Vec::new();
    collect(&v["contents"], "gridRenderer", &mut grids);
    for g in grids {
        let title = runs_text(&g["header"]["gridHeaderRenderer"]["title"]);
        let items: Vec<Item> = g["items"].as_array().map(|a| a.iter().filter_map(parse_any_item).collect()).unwrap_or_default();
        if !items.is_empty() {
            sections.push(Section { title, items });
        }
    }
    Ok(sections)
}

/// Pull an 11-character video id out of a YouTube link (or accept a bare id).
pub fn video_id_from_link(text: &str) -> Option<String> {
    let t = text.trim();
    let valid = |s: &str| s.len() == 11 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if valid(t) {
        return Some(t.to_string());
    }
    for marker in ["v=", "youtu.be/", "/shorts/", "/embed/", "/live/"] {
        if let Some(pos) = t.find(marker) {
            let rest = &t[pos + marker.len()..];
            let id: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect();
            if valid(&id) {
                return Some(id);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // These encode the video-matching rules, so a wrong video can never come back:
    // a song must never show a lyric video, a visualizer, a re-upload or another artist's video,
    // and official performance films must stay acceptable as a last resort.

    #[test]
    fn lyric_and_visualizer_uploads_are_bad() {
        assert!(has_bad_words("kygo & sigrid - the feeling (lyrics)", "The Feeling"));
        assert!(has_bad_words("the feeling (visualizer)", "The Feeling"));
        assert!(has_bad_words("kygo - the feeling (official audio)", "The Feeling"));
        assert!(has_bad_words("the feeling (sub español)", "The Feeling"));
        assert!(!has_bad_words("kygo, sigrid - the feeling (official video)", "The Feeling"));
        // Words of the song's own title stay allowed.
        assert!(!has_bad_words("lyric pichler - my song (official video)", "My Song (feat. Lyric Pichler)".to_lowercase().as_str()));
    }

    #[test]
    fn performance_films_allowed_only_as_films() {
        let v = "kygo, sigrid - the feeling (live from the troll's tongue)";
        assert!(is_other_cut(v, "the feeling", false)); // never as the studio video
        assert!(!is_other_cut(v, "the feeling", true)); // fine as a performance film
        assert!(says_official_video(v)); // a live film counts as a real clip
    }

    #[test]
    fn remixes_never_cross_over() {
        // A remix is never the plain song's video, film tier included...
        assert!(is_other_cut("the feeling (oliver nelson remix)", "the feeling", true));
        // ...but a remix song may use its own remix video.
        assert!(!is_other_cut("the feeling (oliver nelson remix)", "the feeling (oliver nelson remix)", true));
    }

    #[test]
    fn official_video_titles_recognized() {
        assert!(says_official_video("kygo - it ain't me (official video)"));
        assert!(says_official_video("artist - song (official music video)"));
        assert!(!says_official_video("artist - song (official lyric video)"));
        assert!(!says_official_video("artist - song (official audio)"));
        assert!(!says_official_video("artist - song")); // a bare title is not trusted
    }

    #[test]
    fn clean_title_strips_decorations() {
        assert_eq!(clean_title("The Feeling (Official Video)"), "The Feeling");
        assert_eq!(clean_title("The Feeling (Oliver Nelson Remix)"), "The Feeling (Oliver Nelson Remix)");
        assert_eq!(clean_title("Song [Lyric Video] - Official"), "Song");
    }

    #[test]
    fn fold_matches_different_spellings_of_the_same_name() {
        // An ASCII re-upload ("VAEB") still counts as the official act's name ("VÆB"), and back.
        assert_eq!(fold("VÆB"), fold("VAEB"));
        assert_eq!(fold("Björk"), fold("Bjork"));
        assert!(names_match("vaeb official", "VÆB"));
        assert!(names_match("VÆB UK", "VAEB"));
        // Different names still never match.
        assert!(!names_match("some other band", "VÆB"));
    }

    #[test]
    fn title_matches_colon_and_folded_titles() {
        // "Artist: Title [tag]" (a colon, not a dash) still carries the song's title, letters folded.
        title_map().lock().unwrap().insert("fold-test-colon".into(), "væb: róa [axmo remix] (audio)".into());
        assert!(title_matches("fold-test-colon", "RÓA (AXMO Remix)"));
        title_map().lock().unwrap().insert("fold-test-dash".into(), "vaeb - roa axmo remix".into());
        assert!(title_matches("fold-test-dash", "RÓA (AXMO Remix)"));
    }
}
