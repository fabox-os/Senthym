//! Pages: local files, downloads, history, queue, lyrics, now playing and settings.

use super::screens::{action_row, header, track_list, Header};
use super::widgets::*;
use crate::app::{Action, App, Loadable, LyricsState, Route};
use crate::model::*;
use crate::store::{AltStream, Quality};
use egui::{pos2, vec2, Align2, Color32, Rect, RichText, Sense, Ui};

// ---------------------------------------------------------------- local files

pub fn local_files(ui: &mut Ui, app: &mut App) {
    let tracks = app.store.local_tracks.clone();
    let total: u64 = tracks.iter().map(|t| t.duration_ms).sum();
    header(ui, app, Header {
        kind: "Your music, in lossless",
        title: "Local files",
        line: format!("{} songs · {} h {} min · on this computer", tracks.len(), total / 3_600_000, (total / 60_000) % 60),
        cover: None,
        round: false,
        special: Some("local"),
    });
    let item = Item { kind: ItemKind::Playlist, id: "local".into(), title: "Local files".into(), subtitle: String::new(), thumb: None, track: None };
    action_row(ui, app, &tracks, item.clone(), |ui, app| {
        if outline_button(ui, "Add folder").clicked() {
            if let Some(dir) = rfd::FileDialog::new().set_title("Choose a music folder").pick_folder() {
                app.actions.push(Action::ImportFolder(dir.to_string_lossy().into_owned()));
            }
        }
        if outline_button(ui, "Add songs").clicked() {
            if let Some(files) = rfd::FileDialog::new().set_title("Choose music files").add_filter("Audio", &crate::local::EXTENSIONS).pick_files() {
                app.actions.push(Action::ImportFiles(files.iter().map(|f| f.to_string_lossy().into_owned()).collect()));
            }
        }
        if app.local_scanning {
            ui.add(egui::Spinner::new().color(ACCENT));
            ui.label(RichText::new("Reading your music…").color(SUB));
        }
    });
    // A typed folder path works even when the system has no file picker.
    ui.horizontal(|ui| {
        ui.label(RichText::new("Or type a folder:").color(SUB));
        ui.add(egui::TextEdit::singleline(&mut app.import_path).desired_width(360.0).hint_text("/home/you/Music"));
        if ui.button("Import").clicked() {
            app.actions.push(Action::ImportFolder(app.import_path.clone()));
        }
    });
    ui.add_space(8.0);
    if tracks.is_empty() {
        ui.add_space(20.0);
        ui.label(RichText::new("Add music").font(bold(20.0)));
        ui.label(RichText::new("Import FLAC, MP3, WAV and more from your computer. They play offline, with your albums and artists, next to your streaming music.").color(SUB));
        return;
    }
    ui.horizontal(|ui| {
        for (i, l) in ["Songs", "Albums", "Artists"].iter().enumerate() {
            if pill(ui, l, app.local_tab == i).clicked() {
                app.local_tab = i;
            }
        }
        ui.add_space(12.0);
        ui.add(egui::TextEdit::singleline(&mut app.local_query).hint_text("Search in local files").desired_width(240.0));
    });
    ui.add_space(8.0);
    let q = app.local_query.to_lowercase();
    let filtered: Vec<Track> = tracks
        .into_iter()
        .filter(|t| q.is_empty() || t.title.to_lowercase().contains(&q) || t.artist.to_lowercase().contains(&q) || t.album.to_lowercase().contains(&q))
        .collect();
    if app.local_tab == 0 {
        track_list(ui, app, &filtered, true, true, item, |_| RowContext::Local);
        return;
    }
    let by_album = app.local_tab == 1;
    let mut groups: Vec<(String, String, Vec<Track>)> = vec![];
    for t in &filtered {
        let key = if by_album {
            if t.album.is_empty() { "Unknown album".to_string() } else { t.album.clone() }
        } else if t.artist.is_empty() {
            "Unknown artist".to_string()
        } else {
            t.artist.clone()
        };
        match groups.iter_mut().find(|g| g.0 == key) {
            Some(g) => g.2.push(t.clone()),
            None => groups.push((key, if by_album { t.artist.clone() } else { String::new() }, vec![t.clone()])),
        }
    }
    groups.sort_by_key(|g| g.0.to_lowercase());
    for (name, artist, list) in groups {
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(r, 6.0, Color32::from_white_alpha(14));
        }
        let cover = Rect::from_min_size(pos2(r.left() + 8.0, r.top() + 6.0), vec2(52.0, 52.0));
        let key = list.first().and_then(track_cover_key);
        paint_cover(ui, &mut app.images, key.as_deref(), cover, if by_album { 4.0 } else { 26.0 }, &name);
        text_trunc(ui, pos2(cover.right() + 14.0, r.center().y - 1.0), &name, reg(15.0), TEXT, r.width() - 140.0, Align2::LEFT_BOTTOM);
        let sub = if by_album { format!("{} • {} songs", artist, list.len()) } else { format!("{} songs", list.len()) };
        text_trunc(ui, pos2(cover.right() + 14.0, r.center().y + 3.0), &sub, reg(13.0), SUB, r.width() - 140.0, Align2::LEFT_TOP);
        if resp.on_hover_text("Play").clicked() {
            app.actions.push(Action::PlayTracks {
                tracks: list.clone(),
                start: 0,
                shuffle: false,
                context: Some(Item { kind: ItemKind::Album, id: format!("local:{name}"), title: name.clone(), subtitle: String::new(), thumb: None, track: None }),
            });
        }
    }
}

// ---------------------------------------------------------------- downloads

pub fn downloads(ui: &mut Ui, app: &mut App) {
    let list: Vec<Track> = app.store.downloads.iter().filter(|d| std::path::Path::new(&d.path).exists()).map(|d| d.track.clone()).collect();
    let line = format!("{} downloaded • saved in {}", list.len(), app.store.settings.download_dir);
    header(ui, app, Header { kind: "Playlist", title: "Downloads", line, cover: None, round: false, special: Some("downloads") });
    let item = Item { kind: ItemKind::Playlist, id: "downloads".into(), title: "Downloads".into(), subtitle: String::new(), thumb: None, track: None };
    action_row(ui, app, &list, item.clone(), |_, _| {});
    if !app.downloading.is_empty() {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().color(ACCENT));
            ui.label(RichText::new(format!("Downloading {} …", app.downloading.len())).color(SUB));
            if app.downloading.len() > 1 && ui.button(format!("Cancel all downloads ({})", app.downloading.len() - 1)).clicked() {
                app.actions.push(Action::CancelDownloads);
            }
        });
        for t in app.downloading.clone() {
            ui.label(RichText::new(format!("   • {} — {}", t.title, t.artist)).color(SUB));
        }
        ui.add_space(10.0);
    }
    if list.is_empty() && app.downloading.is_empty() {
        ui.label(RichText::new("Nothing downloaded yet").font(bold(18.0)));
        ui.label(RichText::new("Right-click a song and choose Download, or use the download button on an album or playlist.").color(SUB));
        return;
    }
    track_list(ui, app, &list, true, true, item, |_| RowContext::Plain);
}

// ---------------------------------------------------------------- history

pub fn history(ui: &mut Ui, app: &mut App) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Listening history").font(bold(32.0)).color(TEXT));
        ui.add_space(20.0);
        if !app.store.history.is_empty() && ui.button("Clear all").clicked() {
            app.actions.push(Action::ClearHistory);
        }
    });
    ui.label(RichText::new(format!("{} plays logged", app.store.history.len())).color(SUB));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if pill(ui, "History", !app.history_stats).clicked() {
            app.history_stats = false;
        }
        if pill(ui, "Stats", app.history_stats).clicked() {
            app.history_stats = true;
        }
    });
    ui.add_space(10.0);
    if app.store.history.is_empty() {
        ui.label(RichText::new("Songs you play will show up here.").color(SUB));
        return;
    }
    if app.history_stats {
        stats(ui, app);
        return;
    }
    let entries = app.store.history.clone();
    let all: Vec<Track> = entries.iter().map(|h| h.track.clone()).collect();
    let mut last_day = String::new();
    let row_w = ui.available_width() - 44.0 - 28.0 - 24.0;
    for (i, h) in entries.iter().enumerate() {
        let dt = chrono::DateTime::from_timestamp_millis(h.ts).map(|d| d.with_timezone(&chrono::Local));
        let day = dt.map(|d| d.format("%A, %B %-d").to_string()).unwrap_or_default();
        if day != last_day {
            ui.add_space(8.0);
            ui.label(RichText::new(&day).font(bold(16.0)).color(TEXT));
            last_day = day;
        }
        let time = dt.map(|d| d.format("%H:%M").to_string()).unwrap_or_default();
        let clicked = ui
            .horizontal(|ui| {
                ui.add_sized(vec2(44.0, 56.0), egui::Label::new(RichText::new(time).color(SUB).size(12.0)));
                let w = row_w;
                let r = ui.allocate_ui(vec2(w, 56.0), |ui| {
                    ui.set_width(w);
                    track_row(ui, app, &h.track, RowOpts { number: None, show_cover: true, show_album: true, ctx: RowContext::Plain })
                });
                if icon_button(ui, Icon::Close, 28.0, DIM, "Remove").clicked() {
                    app.actions.push(Action::RemoveHistory(h.ts));
                }
                r.inner
            })
            .inner;
        if clicked {
            app.actions.push(Action::PlayTracks { tracks: all.clone(), start: i, shuffle: false, context: None });
        }
    }
}

fn stats(ui: &mut Ui, app: &mut App) {
    let mut tracks: Vec<(Track, usize)> = vec![];
    let mut artists: Vec<(String, usize)> = vec![];
    for h in &app.store.history {
        match tracks.iter_mut().find(|x| x.0.key() == h.track.key()) {
            Some(x) => x.1 += 1,
            None => tracks.push((h.track.clone(), 1)),
        }
        for a in h.track.artist.split(", ").filter(|a| !a.is_empty()) {
            match artists.iter_mut().find(|x| x.0 == a) {
                Some(x) => x.1 += 1,
                None => artists.push((a.to_string(), 1)),
            }
        }
    }
    tracks.sort_by(|a, b| b.1.cmp(&a.1));
    artists.sort_by(|a, b| b.1.cmp(&a.1));
    section_title(ui, "Top tracks");
    let top: Vec<Track> = tracks.iter().take(10).map(|t| t.0.clone()).collect();
    let row_w = ui.available_width() - 110.0;
    for (i, (t, n)) in tracks.iter().take(10).enumerate() {
        let clicked = ui
            .horizontal(|ui| {
                let w = row_w;
                let r = ui.allocate_ui(vec2(w, 56.0), |ui| {
                    ui.set_width(w);
                    track_row(ui, app, t, RowOpts { number: Some(i + 1), show_cover: true, show_album: false, ctx: RowContext::Plain })
                });
                ui.label(RichText::new(format!("{n} plays")).color(SUB));
                r.inner
            })
            .inner;
        if clicked {
            app.actions.push(Action::PlayTracks { tracks: top.clone(), start: i, shuffle: false, context: None });
        }
    }
    section_title(ui, "Top artists");
    for (i, (a, n)) in artists.iter().take(10).enumerate() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}.", i + 1)).color(SUB).size(15.0));
            let r = ui.add(egui::Label::new(RichText::new(a).size(15.0).color(TEXT)).sense(Sense::click()));
            ui.label(RichText::new(format!("— {n} plays")).color(SUB).size(14.0));
            if r.clicked() {
                let t = Track { source: Source::Local(String::new()), title: String::new(), artist: a.clone(), artist_id: None, album: String::new(), album_id: None, duration_ms: 0, thumb: None };
                app.actions.push(Action::GoArtist(t));
            }
        });
    }
}

// ---------------------------------------------------------------- queue

fn arrow(ui: &Ui, rect: Rect, up: bool, col: Color32) {
    let c = rect.center();
    let s = 6.0;
    let d = if up { -1.0 } else { 1.0 };
    ui.painter().line_segment([pos2(c.x - s, c.y - d * 3.0), pos2(c.x, c.y + d * 3.0)], egui::Stroke::new(1.8_f32, col));
    ui.painter().line_segment([pos2(c.x + s, c.y - d * 3.0), pos2(c.x, c.y + d * 3.0)], egui::Stroke::new(1.8_f32, col));
}

fn small_button(ui: &mut Ui, tip: &str) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    if resp.hovered() {
        ui.painter().circle_filled(r.center(), 14.0, Color32::from_white_alpha(14));
    }
    resp.on_hover_text(tip)
}

pub fn queue(ui: &mut Ui, app: &mut App) {
    ui.label(RichText::new("Queue").font(bold(28.0)).color(TEXT));
    ui.add_space(8.0);
    let Some(cur) = app.current.clone() else {
        ui.label(RichText::new("Nothing playing. Pick a song to start.").color(SUB));
        return;
    };
    ui.label(RichText::new("Now playing").font(bold(16.0)).color(SUB));
    track_row(ui, app, &cur, RowOpts { number: None, show_cover: true, show_album: true, ctx: RowContext::Plain });
    ui.add_space(12.0);
    let from = if app.playing_from.is_empty() { "Next up".to_string() } else { format!("Next from: {}", app.playing_from) };
    ui.label(RichText::new(from).font(bold(16.0)).color(SUB));
    let q = app.queue.clone();
    let start = app.index + 1;
    if start >= q.len() {
        ui.label(RichText::new(if app.store.settings.autoplay_radio { "Similar songs will play next." } else { "The queue ends here." }).color(SUB));
    }
    let row_w = ui.available_width() - 3.0 * 28.0 - 4.0 * 8.0 - 8.0;
    for i in start..q.len() {
        let t = &q[i];
        let clicked = ui
            .horizontal(|ui| {
                let w = row_w;
                let r = ui.allocate_ui(vec2(w, 56.0), |ui| {
                    ui.set_width(w);
                    track_row(ui, app, t, RowOpts { number: Some(i - start + 1), show_cover: true, show_album: true, ctx: RowContext::Queue(i) })
                });
                let up = small_button(ui, "Move up");
                arrow(ui, up.rect, true, if up.hovered() { TEXT } else { DIM });
                if up.clicked() && i > start {
                    app.actions.push(Action::MoveQueue(i, i - 1));
                }
                let down = small_button(ui, "Move down");
                arrow(ui, down.rect, false, if down.hovered() { TEXT } else { DIM });
                if down.clicked() && i + 1 < q.len() {
                    app.actions.push(Action::MoveQueue(i, i + 1));
                }
                if icon_button(ui, Icon::Close, 28.0, DIM, "Remove").clicked() {
                    app.actions.push(Action::RemoveFromQueue(i));
                }
                r.inner
            })
            .inner;
        if clicked {
            app.actions.push(Action::PlayQueueIndex(i));
        }
    }
}

// ---------------------------------------------------------------- lyrics

fn current_line(lines: &[crate::lyrics::Line], pos: u64) -> Option<usize> {
    let mut idx = None;
    for (i, l) in lines.iter().enumerate() {
        if l.time_ms <= pos + 300 {
            idx = Some(i);
        } else {
            break;
        }
    }
    idx
}

pub fn lyrics_page(ui: &mut Ui, app: &mut App) {
    let Some(t) = app.current.clone() else {
        ui.label(RichText::new("Play a song to see its lyrics.").color(SUB));
        return;
    };
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(&t.title).font(bold(28.0)).color(TEXT));
            ui.label(RichText::new(&t.artist).size(15.0).color(SUB));
        });
        ui.add_space(30.0);
        let on = app.show_translation;
        if outline_button(ui, if on { "Stop translating lyrics" } else { "Translate lyrics" }).clicked() {
            app.actions.push(Action::Translate(!on));
        }
    });
    ui.add_space(18.0);
    lyrics_body(ui, app, &t, 30.0, true);
}

pub fn lyrics_body(ui: &mut Ui, app: &mut App, t: &Track, size: f32, follow: bool) {
    let key = t.key();
    app.load_lyrics(t);
    let offset = app.lyric_offset_ms();
    let pos = app.status.position_ms.saturating_sub(offset);
    let l = match app.lyrics.get(&key) {
        Some(LyricsState::Found(l, _)) => l.clone(),
        Some(LyricsState::Missing(_)) => {
            ui.label(RichText::new("No lyrics found for this track").font(bold(20.0)).color(SUB));
            return;
        }
        _ => return spinner_text(ui, "Loading lyrics…"),
    };
    let lang = app.translate_lang();
    let translations = if app.show_translation {
        app.request_translation(&key, l.lines.iter().map(|x| x.text.clone()).collect());
        match app.translations.get(&(key.clone(), lang)) {
            Some(Loadable::Ready(v)) => Some(v.clone()),
            Some(Loadable::Loading) => {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().color(ACCENT));
                    ui.label(RichText::new("Translating…").color(SUB));
                });
                None
            }
            Some(Loadable::Failed(e)) => {
                ui.label(RichText::new(format!("Couldn't translate: {e}")).color(SUB));
                None
            }
            None => None,
        }
    } else {
        None
    };
    let cur = if l.synced { current_line(&l.lines, pos) } else { None };
    for (i, line) in l.lines.iter().enumerate() {
        let text = if line.text.is_empty() { "♪".to_string() } else { line.text.clone() };
        let col = match cur {
            Some(c) if i == c => TEXT,
            Some(c) if i < c => Color32::from_white_alpha(140),
            _ if !l.synced => TEXT,
            _ => Color32::from_rgb(0x6E, 0x6E, 0x6E),
        };
        let r = ui.add(egui::Label::new(RichText::new(text).font(bold(size)).color(col)).sense(Sense::click()));
        if let Some(tr) = translations.as_ref().and_then(|v| v.get(i)).filter(|s| !s.is_empty() && **s != line.text) {
            ui.label(RichText::new(tr).size(size * 0.55).color(Color32::from_white_alpha(170)));
        }
        if l.synced {
            let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
            if r.clicked() {
                app.actions.push(Action::Seek(line.time_ms + offset));
            }
            if follow && Some(i) == cur && app.status.playing {
                r.scroll_to_me(Some(egui::Align::Center));
            }
        }
        ui.add_space(size * 0.25);
    }
    if !l.synced {
        ui.add_space(10.0);
        ui.label(RichText::new("These lyrics aren't synced to the song yet.").color(SUB));
    }
}

// ---------------------------------------------------------------- now playing

pub fn now_playing(ui: &mut Ui, app: &mut App) {
    let Some(t) = app.current.clone() else {
        ui.add_space(40.0);
        ui.label(RichText::new("Nothing playing").font(bold(28.0)));
        ui.label(RichText::new("Search for a song or open your library to start listening.").color(SUB));
        return;
    };
    if super::video_ui::active(app) {
        super::video_ui::video_page(ui, app, &t);
        return;
    }
    let w = ui.available_width();
    let side = (w * 0.42).clamp(260.0, 520.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(side);
            ui.add_space(10.0);
            let from = if app.playing_from.is_empty() { "Now Playing".to_string() } else { format!("PLAYING FROM  {}", app.playing_from) };
            ui.label(RichText::new(from).size(12.0).color(SUB));
            ui.add_space(8.0);
            if super::video_ui::active(app) {
                super::video_ui::video_block(ui, app, side, false);
            } else {
                let (cr, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
                let big = t.big_thumb();
                let have_big = big.as_deref().map(|k| app.images.get(k).is_some()).unwrap_or(false);
                let use_key = if have_big { big } else { track_cover_key(&t) };
                paint_cover(ui, &mut app.images, use_key.as_deref(), cr, 8.0, &t.title);
            }
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(side - 50.0);
                    ui.add(egui::Label::new(RichText::new(&t.title).font(bold(24.0)).color(TEXT)).truncate());
                    let ar = ui.add(egui::Label::new(RichText::new(&t.artist).size(16.0).color(SUB)).sense(Sense::click()).truncate());
                    if ar.clicked() {
                        app.actions.push(Action::GoArtist(t.clone()));
                    }
                });
                let liked = app.store.is_liked(&t);
                if icon_button(ui, if liked { Icon::HeartFilled } else { Icon::Heart }, 36.0, if liked { ACCENT } else { SUB }, "Like").clicked() {
                    app.actions.push(Action::ToggleLike(t.clone()));
                }
            });
            ui.add_space(6.0);
            let source = match (&t.source, app.store.alt_streams.get(&t.key())) {
                (_, Some(AltStream::Local(_))) => "Alternative stream • local file".to_string(),
                (_, Some(AltStream::YouTube(_))) => "Alternative stream • YouTube".to_string(),
                (Source::Local(p), _) => format!("Local file • {}", std::path::Path::new(p).extension().and_then(|e| e.to_str()).unwrap_or("").to_uppercase()),
                (Source::Url(u), _) => format!("Link • {}", u.split('/').nth(2).unwrap_or("web")),
                (Source::Deezer(_), _) => {
                    if app.store.download_for(&t).is_some() {
                        "From Deezer • downloaded".to_string()
                    } else {
                        format!("From Deezer • streamed from YouTube Music • {}", app.store.settings.stream_quality.label())
                    }
                }
                (Source::Spotify(_), _) => {
                    if app.store.download_for(&t).is_some() {
                        "From Spotify • downloaded".to_string()
                    } else {
                        format!("From Spotify • streamed from YouTube Music • {}", app.store.settings.stream_quality.label())
                    }
                }
                (Source::YouTube(_), _) => {
                    if app.store.download_for(&t).is_some() {
                        "Downloaded".to_string()
                    } else {
                        format!("Streamed • {}", app.store.settings.stream_quality.label())
                    }
                }
            };
            ui.label(RichText::new(format!("{source}  •  Listening on {}", app.engine.device_name)).size(12.0).color(SUB));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.menu_button("Alternative stream", |ui| alt_stream_menu(ui, app, &t));
                ui.menu_button("More", |ui| {
                    ui.set_min_width(210.0);
                    song_menu(ui, app, &t, &RowContext::Plain);
                });
                if ui.button("Queue").clicked() {
                    app.actions.push(Action::Navigate(Route::Queue));
                }
            });
        });
        ui.add_space(28.0);
        ui.vertical(|ui| {
            ui.add_space(30.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Lyrics preview").font(bold(20.0)).color(TEXT));
                if ui.button("Show lyrics").clicked() {
                    app.actions.push(Action::Navigate(Route::Lyrics));
                }
            });
            ui.add_space(8.0);
            egui::Frame::none().fill(Color32::from_black_alpha(70)).rounding(10.0).inner_margin(egui::Margin::same(18.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::vertical().id_source("np_lyrics").max_height(side * 0.8).auto_shrink([false, true]).show(ui, |ui| {
                    lyrics_body(ui, app, &t, 20.0, true);
                });
            });
            ui.add_space(14.0);
            let next: Vec<Track> = app.queue.iter().skip(app.index + 1).take(3).cloned().collect();
            if !next.is_empty() {
                ui.label(RichText::new("Next in queue").font(bold(16.0)).color(TEXT));
                for (k, n) in next.iter().enumerate() {
                    if track_row(ui, app, n, RowOpts { number: None, show_cover: true, show_album: false, ctx: RowContext::Queue(app.index + 1 + k) }) {
                        app.actions.push(Action::PlayQueueIndex(app.index + 1 + k));
                    }
                }
            }
        });
    });
}

// ---------------------------------------------------------------- settings

fn settings_row(ui: &mut Ui, title: &str, sub: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - 300.0).max(220.0));
            ui.label(RichText::new(title).size(15.0).color(TEXT));
            if !sub.is_empty() {
                ui.label(RichText::new(sub).size(12.5).color(SUB));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add);
    });
    ui.add_space(10.0);
}

fn quality_picker(ui: &mut Ui, id: &str, q: &mut Quality) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_source(id).selected_text(q.label()).width(170.0).show_ui(ui, |ui| {
        for opt in Quality::ALL {
            if ui.selectable_label(*q == opt, format!("{} — {}", opt.label(), opt.detail())).clicked() {
                *q = opt;
                changed = true;
            }
        }
    });
    changed
}

pub fn settings(ui: &mut Ui, app: &mut App) {
    ui.label(RichText::new("Settings").font(bold(32.0)).color(TEXT));
    ui.add_space(14.0);
    let mut changed = false;
    {
        let s = &mut app.store.settings;
        section_title(ui, "Audio quality");
        settings_row(ui, "Streaming quality", "Used when a song plays from YouTube Music", |ui| {
            changed |= quality_picker(ui, "stream_q", &mut s.stream_quality);
        });
        settings_row(ui, "Download quality", "Used when you download songs", |ui| {
            changed |= quality_picker(ui, "dl_q", &mut s.download_quality);
        });
        ui.label(RichText::new("YouTube Music doesn't offer FLAC, so Lossless plays the High quality stream. Your own FLAC files always play in full quality.").size(12.0).color(SUB));
        ui.add_space(10.0);

        section_title(ui, "Playback");
        settings_row(ui, "Crossfade", "Blend the end of a song into the start of the next", |ui| {
            let mut on = s.crossfade_on();
            if ui.checkbox(&mut on, "").changed() {
                s.crossfade_ms = if on { crate::store::CROSSFADE_DEFAULT_MS } else { 0 };
                changed = true;
            }
        });
        if s.crossfade_on() {
            settings_row(ui, "Crossfade length", "0 to 12 seconds", |ui| {
                let mut secs = s.crossfade_ms as f32 / 1000.0;
                if ui.add(egui::Slider::new(&mut secs, 0.5..=12.0).step_by(0.5).suffix(" s")).changed() {
                    s.crossfade_ms = (secs * 1000.0).round() as u32;
                    changed = true;
                }
            });
            settings_row(ui, "DJ mode", "Sweep filters during the crossfade: the old song fades out its highs while the new one brings in its bass", |ui| {
                changed |= ui.checkbox(&mut s.crossfade_dj, "").changed();
            });
        }
        settings_row(ui, "Preload the next song", "Gets the next song ready while this one plays, so it starts instantly", |ui| {
            changed |= ui.checkbox(&mut s.preload, "").changed();
        });
        settings_row(ui, "Autoplay similar songs", "When your queue ends, keep playing songs like the last one", |ui| {
            changed |= ui.checkbox(&mut s.autoplay_radio, "").changed();
        });

        section_title(ui, "Lyrics");
        let lang_sub = if s.lyric_translate_lang.is_empty() { "Using your computer's language" } else { "" };
        settings_row(ui, "Translate lyrics into", lang_sub, |ui| {
            let current = crate::lyrics::LANGUAGES.iter().find(|l| l.0 == s.lyric_translate_lang).map(|l| l.1).unwrap_or("Computer language");
            egui::ComboBox::from_id_source("lang").selected_text(current).width(180.0).show_ui(ui, |ui| {
                if ui.selectable_label(s.lyric_translate_lang.is_empty(), "Computer language").clicked() {
                    s.lyric_translate_lang.clear();
                    changed = true;
                }
                for (code, name) in crate::lyrics::LANGUAGES {
                    if ui.selectable_label(s.lyric_translate_lang == code, name).clicked() {
                        s.lyric_translate_lang = code.to_string();
                        changed = true;
                    }
                }
            });
        });

        section_title(ui, "Library");
        settings_row(ui, "Show library as a grid", "", |ui| {
            changed |= ui.checkbox(&mut s.library_grid, "").changed();
        });
        let mut dl_dir = s.download_dir.clone();
        settings_row(ui, "Download folder", "Where downloaded songs are saved", |ui| {
            if ui.button("Choose…").clicked() {
                if let Some(d) = rfd::FileDialog::new().pick_folder() {
                    dl_dir = d.to_string_lossy().into_owned();
                }
            }
            ui.add(egui::TextEdit::singleline(&mut dl_dir).desired_width(260.0));
        });
        if dl_dir != s.download_dir {
            s.download_dir = dl_dir;
            changed = true;
        }
    }
    if changed {
        app.actions.push(Action::SaveSettings);
    }

    section_title(ui, "Accounts");
    let g_connected = crate::providers::google_connected();
    let g_sub = if g_connected {
        format!("Connected: {} liked songs, {} playlists, {} albums and {} artists are saved in Senthym.", app.store.gl_liked.len(), app.store.gl_playlists.len(), app.store.gl_albums.len(), app.store.gl_artists.len())
    } else {
        "No API key and no Premium needed. Sign in to music.youtube.com in your browser, pick that browser, then connect.".to_string()
    };
    let browsers = crate::helper::installed_browsers();
    let mut pick = app.store.settings.youtube_browser.clone();
    settings_row(ui, "Google account (YouTube Music)", &g_sub, |ui| {
        if g_connected {
            if ui.button("Sign out").clicked() {
                app.google_disconnect();
            }
            if ui.button(if app.google_syncing { "Refreshing…" } else { "Refresh" }).clicked() {
                app.google_sync();
            }
        } else {
            let label = if app.google_busy { "Connecting…" } else { "Connect" };
            if ui.add_enabled(!pick.is_empty() && !app.google_busy, egui::Button::new(RichText::new(label).color(Color32::BLACK)).fill(ACCENT)).clicked() {
                app.google_connect(&pick.clone());
            }
        }
        let shown = if pick.is_empty() { "Choose browser".to_string() } else { pick.clone() };
        egui::ComboBox::from_id_source("yt_browser").selected_text(shown).width(150.0).show_ui(ui, |ui| {
            for b in ["firefox", "chrome", "chromium", "brave", "edge", "vivaldi", "opera"] {
                let label = if browsers.contains(&b) { b.to_string() } else { format!("{b} (not found)") };
                ui.selectable_value(&mut pick, b.to_string(), label);
            }
        });
    });
    if let Some(e) = app.google_error.clone() {
        ui.label(RichText::new(e).size(12.0).color(Color32::from_rgb(0xE5, 0x73, 0x73)));
    }
    if pick != app.store.settings.youtube_browser {
        crate::helper::set_browser(&pick);
        app.store.settings.youtube_browser = pick;
        app.actions.push(Action::SaveSettings);
    }
    let sp_status = if crate::spotify::logged_in() {
        format!("Signed in as {}. Your Spotify library shows in the sidebar.", app.store.sp_user.as_ref().map(|u| u.name.clone()).unwrap_or_default())
    } else {
        "Optional. Spotify's API needs a free developer key from you (Spotify has no keyless sign-in).".to_string()
    };
    settings_row(ui, "Spotify (optional)", &sp_status, |ui| {
        if crate::spotify::logged_in() {
            if ui.button("Log out").clicked() {
                app.spotify_logout();
            }
        } else if ui.button("Set up").clicked() {
            app.actions.push(Action::Navigate(Route::SpotifyLogin));
        }
    });
    let mut hide = app.store.welcome_dismissed;
    settings_row(ui, "Use without an account", "Hides the sign-in window and the Connect entries in the sidebar. You can still sign in from here at any time.", |ui| {
        if ui.checkbox(&mut hide, "").changed() {
            app.store.welcome_dismissed = hide;
            app.actions.push(Action::SaveSettings);
        }
    });

    section_title(ui, "Updates");
    {
        use crate::app::UpdateState;
        let kind = app.update_kind.label();
        let (status, busy): (String, bool) = match &app.update {
            UpdateState::Idle => (format!("You have version {}. Installed as: {kind}.", crate::update::current()), false),
            UpdateState::Checking => ("Looking for a new version…".into(), true),
            UpdateState::UpToDate => (format!("Senthym {} is the newest version.", crate::update::current()), false),
            UpdateState::Available(r) => (
                match &r.asset {
                    Some(a) => format!("Version {} is available ({:.0} MB, {kind}).", r.version, a.size as f64 / 1_048_576.0),
                    None => format!("Version {} is available. Get it from the releases page.", r.version),
                },
                false,
            ),
            UpdateState::Downloading(r, got) => {
                let total = r.asset.as_ref().map(|a| a.size).unwrap_or(0).max(1);
                let pct = (got.load(std::sync::atomic::Ordering::Relaxed) * 100 / total).min(100);
                (format!("Downloading version {}… {pct}%", r.version), true)
            }
            UpdateState::Done(text) => (text.clone(), false),
            UpdateState::Failed(e) => (format!("Couldn't update: {e}"), false),
        };
        let available = matches!(app.update, UpdateState::Available(_));
        let has_package = matches!(&app.update, UpdateState::Available(r) if r.asset.is_some());
        settings_row(ui, "Senthym updates", &status, |ui| {
            if busy {
                ui.add(egui::Spinner::new().color(ACCENT));
            } else if available {
                if ui.button(if has_package { "Download and install" } else { "Open releases page" }).clicked() {
                    app.actions.push(Action::DownloadUpdate);
                }
            } else if !matches!(app.update, UpdateState::Done(_)) && ui.button("Check for updates").clicked() {
                app.actions.push(Action::CheckUpdate);
            }
        });
        let mut check = app.store.settings.check_updates;
        settings_row(ui, "Check for updates on start", "Asks GitHub for the newest release when Senthym starts and tells you when there is one. Nothing is installed without you pressing the button.", |ui| {
            if ui.checkbox(&mut check, "").changed() {
                app.store.settings.check_updates = check;
                app.actions.push(Action::SaveSettings);
            }
        });
    }

    section_title(ui, "Music source");
    settings_row(ui, "Senthym Music Library", "Search, albums, artists and playlists come from several online music sources, and songs are streamed by the built-in Senthym engine. No key or account needed.", |ui| {
        ui.label(RichText::new("Connected").color(ACCENT));
    });
    let mb = app.cache_bytes as f64 / 1_048_576.0;
    settings_row(ui, "Song cache", &format!("{mb:.0} MB used of {} MB. Songs you played recently start instantly.", crate::helper::CACHE_LIMIT_BYTES / 1_048_576), |ui| {
        if ui.button("Clear cache").clicked() {
            app.actions.push(Action::ClearCache);
        }
    });
    ui.add_space(10.0);
    ui.label(RichText::new("Not included in this desktop version: Deezer, the lossless servers and Soulseek.").size(12.0).color(SUB));
    ui.label(RichText::new(format!("Sound output: {}", app.engine.device_name)).size(12.0).color(SUB));
    ui.add_space(6.0);
    ui.label(RichText::new("Shortcuts: Space play/pause • Ctrl+→ / Ctrl+← next / previous • Ctrl+F search • Alt+← / Alt+→ back / forward").size(12.0).color(SUB));
}
