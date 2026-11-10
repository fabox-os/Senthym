//! Telling when two results are the same song, and playing a song from another source when its own
//! source can't deliver it.

use crate::helper;
use crate::model::{Item, ItemKind, Track};
use crate::store::Quality;
use std::path::PathBuf;

fn norm(s: &str) -> String {
    crate::spotify::normalized(s)
}

/// The first (main) artist of a credit line, normalised.
pub fn first_artist(a: &str) -> String {
    let lower = a.to_lowercase();
    let mut cut = lower.len();
    for sep in [",", "&", ";", " x ", " feat", " ft.", " with "] {
        if let Some(i) = lower.find(sep) {
            cut = cut.min(i);
        }
    }
    norm(&lower[..cut])
}

/// (title, artist) pairs a track can be known by. Uploads named "Artist - Title" get a second pair.
fn sigs(t: &Track) -> Vec<(String, String)> {
    let mut v = vec![(norm(&t.title), first_artist(&t.artist))];
    if let Some((l, r)) = t.title.split_once(" - ") {
        v.push((norm(r), first_artist(l)));
    }
    v
}

fn artists_match(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    a == b || (a.len() >= 3 && b.contains(a)) || (b.len() >= 3 && a.contains(b))
}

/// The same recording: same title (versions such as "Acoustic" or "Remix" keep their own title), same
/// main artist, and the same length within three seconds when both lengths are known.
pub fn same_song(a: &Track, b: &Track) -> bool {
    if a.duration_ms > 0 && b.duration_ms > 0 && a.duration_ms.abs_diff(b.duration_ms) > 3_000 {
        return false;
    }
    let lengths_known = a.duration_ms > 0 && b.duration_ms > 0;
    let (sa, sb) = (sigs(a), sigs(b));
    sa.iter().enumerate().any(|(i, (ta, aa))| {
        !ta.is_empty()
            && sb.iter().enumerate().any(|(j, (tb, ab))| {
                // A name read out of an "Artist - Title" upload is a guess (it may be a cover), so it
                // only counts when the lengths are known to agree.
                (i + j == 0 || lengths_known) && ta == tb && artists_match(aa, ab)
            })
    })
}

/// Keep only the first of each song in a list of results (the list's order decides which source wins).
/// Everything that is not a song is left alone.
pub fn dedupe_songs(items: &mut Vec<Item>) {
    let mut kept: Vec<Track> = Vec::new();
    items.retain(|it| {
        let Some(t) = it.track.as_ref().filter(|_| it.kind == ItemKind::Song) else { return true };
        if kept.iter().any(|k| same_song(k, t)) {
            return false;
        }
        kept.push(t.clone());
        true
    });
}

/// Words that mark another version; a match may only carry one when the wanted song has it too.
const VERSION_WORDS: [&str; 20] = ["rework", "flip", "vip", "parody", "remix", "live", "cover", "karaoke", "instrumental", "nightcore", "sped", "slowed", "reverb", "acoustic", "mashup", "bootleg", "8d", "edit", "remake", "tribute"];

/// The SoundCloud uploads that are this song, best first (at most four). Each one's title holds the song's
/// title and its title or uploader the artist, it is no other version, and its length fits.
pub fn soundcloud_matches(t: &Track) -> Vec<String> {
    let want = norm(&t.title);
    let artist = first_artist(&t.artist);
    if want.is_empty() {
        return vec![];
    }
    let query = format!("{} {}", crate::spotify::strip_feat(&t.title), t.artist.split(", ").next().unwrap_or(&t.artist));
    let Ok(items) = crate::providers::soundcloud_search(query.trim()) else { return vec![] };
    let has_word = |s: &str, w: &str| s.split_whitespace().any(|x| x == w);
    let mut found: Vec<(u64, String)> = Vec::new();
    for it in items {
        let Some(c) = it.track else { continue };
        let ct = norm(&c.title);
        if !format!(" {ct} ").contains(&format!(" {want} ")) {
            continue;
        }
        if !artist.is_empty() && !ct.contains(&artist) && !norm(&c.artist).contains(&artist) {
            continue;
        }
        if VERSION_WORDS.iter().any(|w| has_word(&ct, w) && !has_word(&want, w)) {
            continue;
        }
        let diff = if t.duration_ms > 0 && c.duration_ms > 0 { t.duration_ms.abs_diff(c.duration_ms) } else { 0 };
        if diff > (t.duration_ms * 6 / 100).max(8_000) {
            continue;
        }
        found.push((diff, it.id));
    }
    // Closest length first (the sort keeps SoundCloud's own order among equals).
    found.sort_by_key(|(d, _)| *d / 2_000);
    found.into_iter().map(|(_, url)| url).take(4).collect()
}

/// A YouTube song's audio. When the quick way fails, three things are tried at the same time and the
/// first that works is played, so a broken source costs no waiting: the full download, the song's copy
/// on SoundCloud, and another upload of the song on YouTube Music. The name says where the sound came
/// from when it wasn't the song's own source.
pub fn fetch_song(t: &Track, video_id: &str, quality: Quality) -> (Result<PathBuf, String>, Option<&'static str>) {
    match helper::fetch_quick(video_id, quality) {
        Ok(p) => return (Ok(p), None),
        Err(e) => eprintln!("[senthym] quick fetch of {video_id} failed ({e}); trying other sources"),
    }
    let (tx, rx) = std::sync::mpsc::channel::<(Option<&'static str>, Result<PathBuf, String>)>();
    let (tx1, id) = (tx.clone(), video_id.to_string());
    std::thread::spawn(move || {
        let _ = tx1.send((None, helper::fetch_full(&id, quality)));
    });
    let (tx2, track) = (tx.clone(), t.clone());
    std::thread::spawn(move || {
        let _ = tx2.send((Some("SoundCloud"), from_soundcloud(&track, quality)));
    });
    let (track, id) = (t.clone(), video_id.to_string());
    std::thread::spawn(move || {
        let _ = tx.send((Some("another upload"), other_upload(&track, &id, quality)));
    });
    let mut err = None;
    for _ in 0..3 {
        match rx.recv() {
            Ok((from, Ok(p))) => return (Ok(p), from),
            Ok((None, Err(e))) => err = Some(e),
            Ok((Some(name), Err(e))) => eprintln!("[senthym] fallback ({name}): {e}"),
            Err(_) => break,
        }
    }
    (Err(err.unwrap_or_else(|| "this song couldn't be fetched".into())), None)
}

/// The song's sound from SoundCloud: the best matches in turn (label uploads there are often locked).
pub fn from_soundcloud(t: &Track, quality: Quality) -> Result<PathBuf, String> {
    let mut last = "not found on SoundCloud".to_string();
    for url in soundcloud_matches(t) {
        match helper::fetch_url(&url, quality) {
            Ok(p) => {
                eprintln!("[senthym] “{}” from SoundCloud: {url}", t.title);
                return Ok(p);
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Another upload of the same song on YouTube Music (never the id that just failed).
fn other_upload(t: &Track, failed_id: &str, quality: Quality) -> Result<PathBuf, String> {
    let mut last = "no other upload found".to_string();
    for videos in [false, true] {
        let ids = crate::spotify::candidates(t, videos).unwrap_or_default();
        for id in ids.into_iter().filter(|i| i != failed_id).take(2) {
            match helper::fetch_quick(&id, quality) {
                Ok(p) => {
                    eprintln!("[senthym] “{}” from another upload: {id}", t.title);
                    return Ok(p);
                }
                Err(e) => last = e,
            }
        }
    }
    Err(last)
}

/// A song from a link (SoundCloud) that won't play: its YouTube Music match instead.
pub fn from_youtube(t: &Track, quality: Quality) -> Result<PathBuf, String> {
    let mut c = t.clone();
    // Uploads are often named "Artist - Title".
    if let Some((l, r)) = t.title.split_once(" - ") {
        c.artist = l.trim().to_string();
        c.title = r.trim().to_string();
    }
    let ids = crate::spotify::candidates(&c, false)?;
    let mut last = "not found on YouTube Music".to_string();
    for id in ids.into_iter().take(2) {
        match helper::fetch(&id, quality) {
            Ok(p) => return Ok(p),
            Err(e) => last = e,
        }
    }
    Err(last)
}
