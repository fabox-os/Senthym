//! Spotify pages: sign-in, Liked Songs and Spotify playlists / albums / artists.

use super::screens::{action_row, header, track_list, Header};
use super::widgets::*;
use crate::app::{Action, App, Loadable, Route};
use crate::model::*;
use egui::{vec2, Color32, RichText, Ui};

pub const SPOTIFY_GREEN: Color32 = Color32::from_rgb(0x89, 0xCF, 0xF0);

fn step(ui: &mut Ui, n: usize, text: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("{n}.")).font(bold(15.0)).color(SPOTIFY_GREEN));
        ui.label(RichText::new(text).size(14.0).color(TEXT));
    });
    ui.add_space(4.0);
}

/// The sign-in page (the original's SpotifyLoginScreen, done with Spotify's official sign-in).
pub fn login(ui: &mut Ui, app: &mut App) {
    ui.add_space(20.0);
    ui.label(RichText::new("Log in to Spotify").font(bold(36.0)).color(TEXT));
    ui.add_space(6.0);
    if crate::spotify::logged_in() {
        let name = app.store.sp_user.as_ref().map(|u| u.name.clone()).unwrap_or_else(|| "Spotify user".into());
        ui.label(RichText::new(format!("Connected as {name}.")).size(16.0).color(SPOTIFY_GREEN));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(RichText::new("Open Liked Songs").color(Color32::BLACK)).fill(SPOTIFY_GREEN)).clicked() {
                app.actions.push(Action::Navigate(Route::SpotifyLiked));
            }
            if ui.button(if app.spotify_syncing { "Refreshing…" } else { "Refresh library" }).clicked() {
                app.spotify_sync();
            }
            if ui.button("Log out").clicked() {
                app.spotify_logout();
            }
        });
        return;
    }
    ui.label(RichText::new("Bring your Spotify library here: Liked Songs, playlists, saved albums and the artists you follow. Songs play from their match on YouTube Music.").size(14.0).color(SUB));
    ui.add_space(16.0);
    egui::Frame::none().fill(CARD).rounding(10.0).inner_margin(egui::Margin::same(18.0)).show(ui, |ui| {
        ui.set_max_width(760.0);
        ui.label(RichText::new("One-time setup (about 2 minutes)").font(bold(18.0)).color(TEXT));
        ui.add_space(8.0);
        ui.label(RichText::new("Spotify only lets apps sign in with their own key, so you make a free one for yourself:").size(13.5).color(SUB));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            step(ui, 1, "Open the Spotify developer dashboard and log in with your Spotify account.");
            if ui.link("Open dashboard").clicked() {
                crate::spotify::open_in_browser("https://developer.spotify.com/dashboard");
            }
        });
        step(ui, 2, "Click “Create app”. Any name and description will do.");
        ui.horizontal(|ui| {
            step(ui, 3, "Under “Redirect URIs” add exactly:");
            ui.label(RichText::new(crate::spotify::REDIRECT_URI).monospace().color(SPOTIFY_GREEN));
            if ui.small_button("Copy").clicked() {
                app.actions.push(Action::Share(crate::spotify::REDIRECT_URI.to_string()));
            }
        });
        step(ui, 4, "Tick “Web API”, save, then copy the “Client ID” from the app's settings and paste it below.");
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Client ID").size(14.0).color(TEXT));
            ui.add(egui::TextEdit::singleline(&mut app.spotify_client_id).hint_text("32 letters and numbers").desired_width(320.0).font(egui::TextStyle::Monospace));
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let ready = app.spotify_client_id.trim().len() >= 20 && !app.spotify_busy;
            let b = ui.add_enabled(ready, egui::Button::new(RichText::new("  Log in with Spotify  ").font(bold(15.0)).color(Color32::BLACK)).fill(SPOTIFY_GREEN).min_size(vec2(0.0, 40.0)).rounding(20.0));
            if b.clicked() {
                app.spotify_login();
            }
            if app.spotify_busy {
                ui.add(egui::Spinner::new().color(SPOTIFY_GREEN));
                ui.label(RichText::new("Approve the request in your browser…").color(SUB));
            }
        });
        if let Some(e) = &app.spotify_error {
            ui.add_space(8.0);
            ui.label(RichText::new(e).color(Color32::from_rgb(0xE5, 0x73, 0x73)));
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Your sign-in stays on this computer. Spotify Premium isn't needed.").size(12.0).color(SUB));
    });
}

fn sp_liked_item(app: &App) -> Item {
    Item { kind: ItemKind::Playlist, id: "sp-liked".into(), title: "Liked Songs".into(), subtitle: format!("Spotify • {} songs", app.store.sp_liked.len()), thumb: None, track: None }
}

pub fn liked(ui: &mut Ui, app: &mut App) {
    if !crate::spotify::logged_in() {
        return login(ui, app);
    }
    let tracks = app.store.sp_liked.clone();
    let owner = app.store.sp_user.as_ref().map(|u| u.name.clone()).unwrap_or_default();
    header(ui, app, Header { kind: "Spotify playlist", title: "Liked Songs", line: format!("{owner} • {} songs", tracks.len()), cover: None, round: false, special: Some("liked") });
    let item = sp_liked_item(app);
    action_row(ui, app, &tracks, item.clone(), |ui, app| {
        if app.spotify_syncing {
            ui.add(egui::Spinner::new().color(SPOTIFY_GREEN));
        } else if outline_button(ui, "Refresh").clicked() {
            app.spotify_sync();
        }
    });
    if tracks.is_empty() {
        ui.label(RichText::new(if app.spotify_syncing { "Loading your Spotify library…" } else { "No liked songs on Spotify yet." }).color(SUB));
        return;
    }
    track_list(ui, app, &tracks, true, true, item, |_| RowContext::Plain);
}

pub fn collection(ui: &mut Ui, app: &mut App, id: &str) {
    if !crate::spotify::logged_in() {
        return login(ui, app);
    }
    app.ensure_spotify_collection(id);
    let page = match app.playlists.get(id) {
        Some(Loadable::Ready(p)) => p.clone(),
        Some(Loadable::Failed(e)) => {
            let e = e.clone();
            if error_box(ui, &format!("Couldn't load this from Spotify. {e}")) {
                app.playlists.remove(id);
            }
            return;
        }
        _ => return spinner_text(ui, "Loading from Spotify…"),
    };
    let kind = if id.starts_with("sp:album:") {
        "Album"
    } else if id.starts_with("sp:artist:") {
        "Artist"
    } else {
        "Playlist"
    };
    let total: u64 = page.tracks.iter().map(|t| t.duration_ms).sum();
    let line = [page.owner.clone(), page.detail.clone(), format!("{} songs, {}", page.tracks.len(), format_ms(total))].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" • ");
    let item_kind = match kind {
        "Album" => ItemKind::Album,
        "Artist" => ItemKind::Artist,
        _ => ItemKind::Playlist,
    };
    header(ui, app, Header { kind: &format!("Spotify {}", kind.to_lowercase()), title: &page.title, line, cover: page.thumb.clone(), round: kind == "Artist", special: None });
    let item = Item { kind: item_kind, id: id.to_string(), title: page.title.clone(), subtitle: format!("{kind} • Spotify"), thumb: page.thumb.clone(), track: None };
    let name = page.title.clone();
    let is_artist = kind == "Artist";
    action_row(ui, app, &page.tracks, item.clone(), move |ui, app| {
        if is_artist && outline_button(ui, "More on YouTube Music").clicked() {
            app.search_text = name.clone();
            app.search_query = name.clone();
            app.search_tab = crate::app::SearchTab::Artists;
            app.actions.push(Action::Search(name.clone()));
        }
    });
    if page.tracks.is_empty() {
        ui.label(RichText::new("No songs here.").color(SUB));
        return;
    }
    track_list(ui, app, &page.tracks, true, true, item, |_| RowContext::Plain);
}

