//! Importing music files from this computer (the original's `LocalImport`).

use crate::model::{Source, Track};
use std::path::Path;

pub const EXTENSIONS: [&str; 10] = ["mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "aif", "aiff", "wma"];

pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Read one file's tags into a track (falls back to the file name).
pub fn read_track(path: &Path) -> Track {
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::tag::Accessor;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Unknown").to_string();
    let mut t = Track {
        source: Source::Local(path.to_string_lossy().into_owned()),
        title: stem.clone(),
        artist: String::new(),
        artist_id: None,
        album: String::new(),
        album_id: None,
        duration_ms: 0,
        thumb: None,
    };
    if let Ok(f) = lofty::read_from_path(path) {
        t.duration_ms = f.properties().duration().as_millis() as u64;
        if let Some(tag) = f.primary_tag().or_else(|| f.first_tag()) {
            if let Some(v) = tag.title() {
                if !v.trim().is_empty() {
                    t.title = v.to_string();
                }
            }
            if let Some(v) = tag.artist() {
                t.artist = v.to_string();
            }
            if let Some(v) = tag.album() {
                t.album = v.to_string();
            }
        }
    }
    if t.artist.is_empty() {
        if let Some((a, title)) = stem.split_once(" - ") {
            t.artist = a.trim().to_string();
            if t.title == stem {
                t.title = title.trim().to_string();
            }
        }
    }
    t
}

/// Every audio file under a folder (follows subfolders, at most 20000 files).
pub fn scan_folder(dir: &Path) -> Vec<Track> {
    let mut out = vec![];
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if !p.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with('.')).unwrap_or(false) {
                    stack.push(p);
                }
            } else if is_audio(&p) {
                out.push(read_track(&p));
                if out.len() >= 20000 {
                    return out;
                }
            }
        }
    }
    out.sort_by(|a, b| a.artist.to_lowercase().cmp(&b.artist.to_lowercase()).then(a.album.cmp(&b.album)).then(a.title.cmp(&b.title)));
    out
}

/// Embedded cover art of a file, if it has one.
pub fn cover_bytes(path: &str) -> Option<Vec<u8>> {
    use lofty::file::TaggedFileExt;
    let f = lofty::read_from_path(path).ok()?;
    let tag = f.primary_tag().or_else(|| f.first_tag())?;
    tag.pictures().first().map(|p| p.data().to_vec())
}
