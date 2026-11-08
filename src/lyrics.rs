//! Song lyrics from LRCLIB, with the same matching rules as the original `LyricsApi`,
//! and translation through Google's free translate endpoint (`LyricsViewModel.translate`).

use std::time::Duration;

const BASE: &str = "https://lrclib.net/api";
pub const DURATION_TOLERANCE_S: i64 = 10;
pub const UNSYNCED_PENALTY: i64 = 1000;
pub const MISS_RETRY_MS: u64 = 120_000;

#[derive(Clone, Debug)]
pub struct Line {
    pub time_ms: u64,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct Lyrics {
    pub lines: Vec<Line>,
    pub synced: bool,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .user_agent("Senthym desktop music player (https://lrclib.net)")
        .build()
}

fn enc(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

fn get(url: &str) -> Option<String> {
    agent().get(url).set("Accept", "application/json").call().ok()?.into_string().ok()
}

/// Lowercase letters and digits only.
fn match_key(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Drop " - …" suffixes and (feat. …), (remaster…) style tags.
pub fn clean_title(title: &str) -> String {
    let mut t = title.split(" - ").next().unwrap_or(title).to_string();
    let tags = ["feat", "ft.", "with ", "remaster", "live", "version", "edit", "mono", "stereo", "deluxe", "bonus"];
    loop {
        let mut changed = false;
        for (open, close) in [('(', ')'), ('[', ']')] {
            if let Some(start) = t.find(open) {
                if let Some(rel_end) = t[start..].find(close) {
                    let inner = t[start + 1..start + rel_end].to_lowercase();
                    if tags.iter().any(|k| inner.contains(k)) {
                        t.replace_range(start..start + rel_end + 1, "");
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    t.trim().to_string()
}

fn overlaps(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && (a.contains(b) || b.contains(a))
}

pub fn is_plausible(cand_title: &str, cand_artist: &str, cand_dur: i64, title: &str, artist: &str, dur: i64) -> bool {
    if overlaps(&match_key(cand_artist), &match_key(artist)) && overlaps(&match_key(&clean_title(cand_title)), &match_key(title)) {
        return dur <= 0 || cand_dur <= 0 || (cand_dur - dur).abs() <= DURATION_TOLERANCE_S;
    }
    false
}

pub fn parse_lrc(lrc: &str) -> Vec<Line> {
    let mut out = vec![];
    for raw in lrc.lines() {
        let mut rest = raw;
        let mut times = vec![];
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else { break };
            let tag = &stripped[..end];
            let Some((m, s)) = tag.split_once(':') else { break };
            let (sec, frac) = match s.split_once(['.', ':']) {
                Some((a, b)) => (a, b),
                None => (s, ""),
            };
            let (Ok(m), Ok(sec)) = (m.parse::<u64>(), sec.parse::<u64>()) else { break };
            let frac_ms = match frac.len() {
                0 => 0,
                1 => frac.parse::<u64>().unwrap_or(0) * 100,
                2 => frac.parse::<u64>().unwrap_or(0) * 10,
                _ => frac[..3].parse::<u64>().unwrap_or(0),
            };
            times.push(m * 60_000 + sec * 1000 + frac_ms);
            rest = &stripped[end + 1..];
        }
        let text = rest.trim().to_string();
        for t in times {
            out.push(Line { time_ms: t, text: text.clone() });
        }
    }
    out.sort_by_key(|l| l.time_ms);
    out
}

fn parse(o: &serde_json::Value) -> Option<Lyrics> {
    if let Some(s) = o["syncedLyrics"].as_str().filter(|s| !s.trim().is_empty()) {
        let lines = parse_lrc(s);
        if !lines.is_empty() {
            return Some(Lyrics { lines, synced: true });
        }
    }
    let plain = o["plainLyrics"].as_str().filter(|s| !s.trim().is_empty())?;
    Some(Lyrics { lines: plain.lines().map(|l| Line { time_ms: 0, text: l.trim().to_string() }).collect(), synced: false })
}

fn get_exact(title: &str, artist: &str, album: &str, dur: i64) -> Option<Lyrics> {
    let mut url = format!("{BASE}/get?track_name={}&artist_name={}", enc(title), enc(artist));
    if !album.trim().is_empty() {
        url.push_str(&format!("&album_name={}", enc(album)));
    }
    if dur > 0 {
        url.push_str(&format!("&duration={dur}"));
    }
    let v: serde_json::Value = serde_json::from_str(&get(&url)?).ok()?;
    parse(&v)
}

fn search_url(url: &str, title: &str, artist: &str, dur: i64) -> Option<Lyrics> {
    let arr: Vec<serde_json::Value> = serde_json::from_str(&get(url)?).ok()?;
    let mut best: Option<(i64, &serde_json::Value)> = None;
    for o in &arr {
        let cd = o["duration"].as_f64().unwrap_or(0.0) as i64;
        let ct = o["trackName"].as_str().unwrap_or("");
        let ca = o["artistName"].as_str().unwrap_or("");
        if is_plausible(ct, ca, cd, title, artist, dur) {
            let synced = o["syncedLyrics"].as_str().map(|s| !s.trim().is_empty()).unwrap_or(false);
            let diff = if dur > 0 && cd > 0 { (cd - dur).abs() } else { 0 };
            let score = diff + if synced { 0 } else { UNSYNCED_PENALTY };
            if best.map(|b| score < b.0).unwrap_or(true) {
                best = Some((score, o));
            }
        }
    }
    best.and_then(|(_, o)| parse(o))
}

/// Exact lookup, then search by title+artist, then by title only.
pub fn fetch(title: &str, artist: &str, album: &str, duration_ms: u64) -> Option<Lyrics> {
    let t = clean_title(title);
    let a = artist.split(',').next().unwrap_or(artist).split(" & ").next().unwrap_or(artist).trim().to_string();
    let d = (duration_ms / 1000) as i64;
    get_exact(&t, &a, album, d)
        .or_else(|| search_url(&format!("{BASE}/search?track_name={}&artist_name={}", enc(&t), enc(&a)), &t, &a, d))
        .or_else(|| search_url(&format!("{BASE}/search?track_name={}", enc(&t)), &t, &a, d))
}

/// Translate every line at once; returns one translation per input line.
pub fn translate(lines: &[String], lang: &str) -> Result<Vec<String>, String> {
    let joined = lines.join("\n");
    let url = format!(
        "https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl={}&dt=t&q={}",
        enc(lang),
        enc(&joined)
    );
    let text = agent().get(&url).call().map_err(|e| e.to_string())?.into_string().map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let mut whole = String::new();
    if let Some(parts) = v[0].as_array() {
        for p in parts {
            if let Some(s) = p[0].as_str() {
                whole.push_str(s);
            }
        }
    }
    let mut out: Vec<String> = whole.split('\n').map(|s| s.trim().to_string()).collect();
    out.resize(lines.len(), String::new());
    Ok(out)
}

pub const LANGUAGES: [(&str, &str); 16] = [
    ("en", "English"),
    ("es", "Spanish"),
    ("pt", "Portuguese"),
    ("fr", "French"),
    ("de", "German"),
    ("it", "Italian"),
    ("nl", "Dutch"),
    ("pl", "Polish"),
    ("tr", "Turkish"),
    ("ru", "Russian"),
    ("uk", "Ukrainian"),
    ("ar", "Arabic"),
    ("hi", "Hindi"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("zh-CN", "Chinese"),
];
