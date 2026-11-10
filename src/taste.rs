//! Following the person's taste, the way Spotify's autoplay does. The songs they chose
//! themselves are the anchors of their current taste; the next songs are blended from the
//! radios around those anchors, with the newest choice weighing most, so listening to Kygo
//! keeps tropical house coming, and switching styles swings the blend with it. Skipping
//! auto-added songs early steers the blend away from what they don't want right now.

use crate::fallback::same_song;
use crate::model::Track;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How many of the person's own choices shape the taste.
const ANCHORS: usize = 6;
/// How many anchors seed one blend (newest first, distinct artists).
const SEEDS: usize = 3;
/// Shares of the blend per seed, newest taste first.
const SHARES: [usize; SEEDS] = [3, 2, 1];
/// Pressing next before this much of a song was heard counts as an early skip.
pub const SKIP_MS: u64 = 45_000;
/// This many early skips put the artist aside for a while.
const SKIPS_ENOUGH: u32 = 2;
/// How long an artist skipped that often stays out of the blend.
const SKIP_FOR: Duration = Duration::from_secs(45 * 60);
/// This many auto-added songs skipped in a row mean the queued direction is wrong.
pub const STREAK_ENOUGH: u32 = 2;

/// The first (main) artist of a track, normalised, as the taste's unit of style.
fn artist_of(t: &Track) -> String {
    crate::fallback::first_artist(&t.artist)
}

#[derive(Default)]
pub struct Taste {
    /// Songs the person chose themselves, newest first.
    anchors: Vec<Track>,
    /// Main artists whose songs were skipped early: how often, and when last.
    skips: HashMap<String, (u32, Instant)>,
    /// Auto-added songs skipped early in a row since the last choice.
    pub streak: u32,
}

impl Taste {
    /// Taste saved from an earlier run.
    pub fn restore(anchors: Vec<Track>) -> Taste {
        Taste { anchors, ..Taste::default() }
    }

    /// The anchors, newest first (what gets saved).
    pub fn anchors(&self) -> &[Track] {
        &self.anchors
    }

    /// A song the person chose themself: it becomes the newest anchor, and its artist is
    /// welcome again whatever was skipped before.
    pub fn chose(&mut self, t: &Track) {
        self.streak = 0;
        self.skips.remove(&artist_of(t));
        self.anchors.retain(|a| !same_song(a, t));
        self.anchors.insert(0, t.clone());
        self.anchors.truncate(ANCHORS);
    }

    /// A song skipped early; `auto` when the radio queued it rather than the person.
    pub fn skipped(&mut self, t: &Track, auto: bool) {
        if auto {
            self.streak += 1;
        }
        let e = self.skips.entry(artist_of(t)).or_insert((0, Instant::now()));
        e.0 += 1;
        e.1 = Instant::now();
    }

    /// The song's artist was skipped early enough times just now that the blend leaves it out.
    pub fn put_aside(&self, t: &Track) -> bool {
        self.skips.get(&artist_of(t)).map(|(n, at)| *n >= SKIPS_ENOUGH && at.elapsed() < SKIP_FOR).unwrap_or(false)
    }

    /// Up to three seeds for the next blend: `first` (the playing song) and then the newest
    /// choices, one per artist, online songs only, nothing that was just skipped away.
    pub fn seeds(&self, first: Option<&Track>) -> Vec<Track> {
        let mut out: Vec<Track> = Vec::new();
        for t in first.into_iter().chain(self.anchors.iter()) {
            if out.len() >= SEEDS {
                break;
            }
            if t.is_online() && !self.put_aside(t) && !out.iter().any(|o| artist_of(o) == artist_of(t)) {
                out.push(t.clone());
            }
        }
        out
    }
}

/// Blend the radios around each seed into one list of `want` songs. The lists come newest
/// taste first and give songs in rounds of 3:2:1, so the current style leads without taking
/// everything over, and the same recording never appears twice.
pub fn blend(lists: &[Vec<Track>], want: usize) -> Vec<Track> {
    let mut at = vec![0usize; lists.len()];
    let mut out: Vec<Track> = Vec::new();
    loop {
        let mut took = false;
        for (li, list) in lists.iter().enumerate() {
            let share = SHARES.get(li).copied().unwrap_or(1);
            for _ in 0..share {
                while at[li] < list.len() {
                    let t = &list[at[li]];
                    at[li] += 1;
                    if !out.iter().any(|o| same_song(o, t)) {
                        out.push(t.clone());
                        took = true;
                        break;
                    }
                }
            }
        }
        if out.len() >= want || !took {
            break;
        }
    }
    out.truncate(want);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Source;

    fn song(title: &str, artist: &str) -> Track {
        Track {
            source: Source::YouTube(format!("{title}-{artist}")),
            title: title.into(),
            artist: artist.into(),
            artist_id: None,
            album: String::new(),
            album_id: None,
            duration_ms: 180_000,
            thumb: None,
        }
    }

    #[test]
    fn newest_taste_leads_the_blend() {
        let kygo: Vec<Track> = (0..6).map(|i| song(&format!("k{i}"), "Kygo")).collect();
        let rock: Vec<Track> = (0..6).map(|i| song(&format!("r{i}"), "AC/DC")).collect();
        let out = blend(&[kygo, rock], 6);
        // First round: three from the newest taste, two from the older one.
        let kygo_first_5 = out.iter().take(5).filter(|t| t.artist == "Kygo").count();
        assert_eq!(kygo_first_5, 3);
        assert_eq!(out.len(), 6);
    }

    #[test]
    fn blend_never_repeats_a_song() {
        let a = vec![song("same", "Kygo"), song("one", "Kygo")];
        let b = vec![song("same", "Kygo"), song("two", "Kygo")];
        let out = blend(&[a, b], 10);
        assert_eq!(out.iter().filter(|t| t.title == "same").count(), 1);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn choosing_adapts_and_clears_skips() {
        let mut taste = Taste::default();
        let k = song("Firestone", "Kygo");
        let r = song("Back in Black", "AC/DC");
        taste.chose(&k);
        taste.skipped(&r, true);
        taste.skipped(&r, true);
        assert!(taste.put_aside(&r)); // skipped twice: out of the blend
        assert_eq!(taste.streak, 2);
        taste.chose(&r); // but choosing it yourself brings it right back
        assert!(!taste.put_aside(&r));
        assert_eq!(taste.streak, 0);
        // Newest choice seeds first; one seed per artist.
        let seeds = taste.seeds(None);
        assert_eq!(seeds[0].artist, "AC/DC");
        assert_eq!(seeds[1].artist, "Kygo");
        taste.chose(&song("Stole the Show", "Kygo"));
        assert_eq!(taste.seeds(None).len(), 2);
    }
}
