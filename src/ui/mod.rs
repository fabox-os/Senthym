//! The window: sidebar, page area, player bar, dialogs and messages.

pub mod pages;
pub mod screens;
pub mod spotify_pages;
pub mod video_ui;
pub mod widgets;

use crate::app::{Action, App, Loadable, Route};
use crate::model::*;
use crate::store::AltStream;
use egui::{pos2, vec2, Align2, Color32, Rect, Sense, Stroke};
use widgets::*;

pub fn draw(ctx: &egui::Context, app: &mut App) {
    draw_inner(ctx, app);
    video_ui::overlays(ctx, app);
    // Playing a song from anywhere opens the Now playing panel on the right.
    let plays = app.actions.iter().any(|a| match a {
        Action::PlayTracks { .. } | Action::PlayItem(..) => true,
        Action::OpenItem(it) => it.track.is_some() || matches!(it.kind, ItemKind::Song | ItemKind::Video),
        _ => false,
    });
    if plays {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("np_open"), true));
        ctx.request_repaint();
    }
}

fn draw_inner(ctx: &egui::Context, app: &mut App) {
    shortcuts(ctx, app);
    if crate::sys::CUSTOM_FRAME {
        window_edges(ctx);
    }
    player_bar(ctx, app);
    top_bar(ctx, app);
    // The expanded library fills the window until you pick something or navigate elsewhere.
    let full_key = egui::Id::new("lib_full");
    let route_key = egui::Id::new("lib_full_route");
    let route_now = format!("{:?}", app.route);
    let last_route: String = ctx.data(|d| d.get_temp(route_key)).unwrap_or_default();
    if last_route != route_now {
        ctx.data_mut(|d| {
            d.insert_temp(route_key, route_now.clone());
            d.insert_temp(full_key, false);
        });
    }
    if ctx.data(|d| d.get_temp::<bool>(full_key)).unwrap_or(false) {
        library_full(ctx, app);
        dialogs(ctx, app);
        if app.show_welcome {
            welcome(ctx, app);
        }
        toasts(ctx, app);
    undo_toast(ctx, app);
    update_notice(ctx, app);
        return;
    }
    sidebar(ctx, app);
    if ctx.data(|d| d.get_temp::<bool>(egui::Id::new("np_open"))).unwrap_or(false) && app.current.is_some() && app.route != Route::Player {
        now_panel(ctx, app);
    }
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin { left: 4.0, right: 8.0, top: 0.0, bottom: 8.0 }))
        .show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            ui.painter().rect_filled(rect, 8.0, PANEL);
            ui.allocate_ui_at_rect(rect, |ui| {
                ui.set_clip_rect(rect);
                screens::page(ui, app);
            });
        });
    dialogs(ctx, app);
    if app.show_welcome {
        welcome(ctx, app);
    }
    toasts(ctx, app);
}

fn shortcuts(ctx: &egui::Context, app: &mut App) {
    let typing = ctx.wants_keyboard_input();
    // Space toggles play once. It is taken out of the input and any focused button lets go, so a button that
    // was clicked earlier (with the keyboard focus still on it) can't toggle a second time and undo the pause.
    if !typing && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space)) {
        app.actions.push(Action::TogglePlay);
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
    }
    ctx.input(|i| {
        if i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft) {
            app.actions.push(Action::Back);
        }
        if i.modifiers.alt && i.key_pressed(egui::Key::ArrowRight) {
            app.actions.push(Action::Forward);
        }
        if i.modifiers.ctrl && i.key_pressed(egui::Key::ArrowRight) {
            app.actions.push(Action::Next);
        }
        if i.modifiers.ctrl && i.key_pressed(egui::Key::ArrowLeft) {
            app.actions.push(Action::Prev);
        }
        if i.modifiers.ctrl && (i.key_pressed(egui::Key::F) || i.key_pressed(egui::Key::K)) {
            app.actions.push(Action::Navigate(Route::Search));
        }
        if i.modifiers.ctrl && i.key_pressed(egui::Key::L) {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("focus_search"), true));
        }
    });
}

/// The bar across the top: history arrows, Home, the search pill, and account/settings.
fn top_bar(ctx: &egui::Context, app: &mut App) {
    egui::TopBottomPanel::top("topbar")
        .exact_height(60.0)
        .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin::symmetric(12.0, 8.0)))
        .show_separator_line(false)
        .show(ctx, |ui| {
            let full = ui.available_rect_before_wrap();
            let cy = full.center().y;
            // The empty part of the bar moves the window (double-click: maximize / restore). It comes first,
            // so every button drawn after it is on top and keeps its own clicks.
            let drag = ui.interact(Rect::from_min_max(pos2(ctx.screen_rect().left(), full.top() - 8.0), pos2(ctx.screen_rect().right(), full.bottom() + 8.0)), ui.id().with("window_drag"), Sense::click_and_drag());
            if drag.drag_started_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                let max = ctx.input(|i| i.viewport().maximized).unwrap_or(false);
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
            }
            // Left: back / forward.
            let mut x = full.left() + 4.0;
            for (icon, enabled, back, tip) in [(Icon::Back, !app.back.is_empty(), true, "Go back"), (Icon::Forward, !app.forward.is_empty(), false, "Go forward")] {
                let r = Rect::from_center_size(pos2(x + 16.0, cy), vec2(32.0, 32.0));
                let resp = ui.interact(r, ui.id().with(("nav", back)), Sense::click());
                ui.painter().circle_filled(r.center(), 16.0, Color32::from_white_alpha(if resp.hovered() && enabled { 28 } else { 14 }));
                paint_icon(ui.painter(), r.shrink(9.0), icon, if enabled { TEXT } else { DIM });
                if enabled && resp.on_hover_text(tip).clicked() {
                    app.actions.push(if back { Action::Back } else { Action::Forward });
                }
                x += 38.0;
            }
            // Centre: Home button and search pill.
            let search_w = (full.width() - 560.0).clamp(260.0, 500.0);
            let start = full.center().x - (48.0 + 10.0 + search_w) / 2.0;
            let home_r = Rect::from_center_size(pos2(start + 24.0, cy), vec2(48.0, 48.0));
            let hr = ui.interact(home_r, ui.id().with("home_btn"), Sense::click());
            let active = app.route == Route::Home;
            ui.painter().circle_filled(home_r.center(), 24.0, if hr.hovered() { Color32::from_rgb(0x2A, 0x2A, 0x2A) } else { Color32::from_rgb(0x1F, 0x1F, 0x1F) });
            paint_icon(ui.painter(), home_r.shrink(12.0), Icon::Home, if active || hr.hovered() { TEXT } else { SUB });
            if hr.on_hover_text("Home").clicked() {
                app.actions.push(Action::Navigate(Route::Home));
            }
            let search_rect = Rect::from_min_size(pos2(start + 58.0, cy - 22.0), vec2(search_w, 44.0));
            ui.allocate_ui_at_rect(search_rect, |ui| screens::search_box(ui, app, search_w));
            // Right: streaming helper status, settings, account.
            let pad = if crate::sys::CUSTOM_FRAME { 132.0 } else { 16.0 };
            let right = Rect::from_min_max(pos2(full.right() - 360.0 - pad, full.top()), pos2(full.right() - pad, full.bottom()));
            ui.allocate_ui_at_rect(right, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let initial = app.store.sp_user.as_ref().and_then(|u| u.name.chars().next()).map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "♪".into());
                    let (r, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
                    ui.painter().circle_filled(r.center(), 17.0, Color32::from_rgb(0x2A, 0x2A, 0x2A));
                    ui.painter().circle_filled(r.center(), 14.0, if app.route == Route::Settings { ACCENT } else { Color32::from_rgb(0x50, 0x3A, 0x8C) });
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, initial, bold(14.0), if app.route == Route::Settings { Color32::BLACK } else { TEXT });
                    if resp.on_hover_text("Settings").clicked() {
                        app.actions.push(Action::Navigate(Route::Settings));
                    }
                    if app.helper_checked && app.helper_version.is_none() && !app.helper_busy {
                        let b = ui.add(egui::Button::new(egui::RichText::new("Set up streaming").color(Color32::BLACK)).fill(ACCENT));
                        if b.on_hover_text("Sets up the Senthym streaming engine that plays songs from YouTube Music").clicked() {
                            app.actions.push(Action::UpdateHelper);
                        }
                    } else if app.helper_busy {
                        ui.add(egui::Spinner::new().color(ACCENT));
                        ui.label(egui::RichText::new("Setting up streaming…").color(SUB));
                    }
                });
            });
            if crate::sys::CUSTOM_FRAME {
                window_controls(ui, ctx, cy);
            }
        });
}

/// Minimize, maximize / restore and close, at the right end of the top bar.
fn window_controls(ui: &mut egui::Ui, ctx: &egui::Context, cy: f32) {
    let maximized = ctx.input(|i| i.viewport().maximized).unwrap_or(false);
    let right = ctx.screen_rect().right() - 10.0;
    for (n, kind) in [0usize, 1, 2].into_iter().enumerate() {
        let c = pos2(right - 20.0 - (2 - n) as f32 * 40.0, cy);
        let r = Rect::from_center_size(c, vec2(36.0, 30.0));
        let resp = ui.interact(r, ui.id().with(("win_btn", kind)), Sense::click());
        let close = kind == 2;
        if resp.hovered() {
            let bg = if close { Color32::from_rgb(0xE8, 0x11, 0x23) } else { Color32::from_white_alpha(26) };
            ui.painter().rect_filled(r, 8.0, bg);
        }
        let col = if resp.hovered() { Color32::WHITE } else { Color32::from_gray(0xC8) };
        let st = Stroke::new(1.4_f32, col);
        let p = ui.painter();
        match kind {
            0 => {
                p.line_segment([c + vec2(-5.0, 2.0), c + vec2(5.0, 2.0)], st);
            }
            1 if !maximized => {
                p.rect_stroke(Rect::from_center_size(c, vec2(10.0, 10.0)), 2.0, st);
            }
            1 => {
                p.rect_stroke(Rect::from_center_size(c + vec2(-1.0, 1.0), vec2(8.0, 8.0)), 2.0, st);
                p.line_segment([c + vec2(-1.0, -4.5), c + vec2(4.5, -4.5)], st);
                p.line_segment([c + vec2(4.5, -4.5), c + vec2(4.5, 1.0)], st);
            }
            _ => {
                p.line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], st);
                p.line_segment([c + vec2(-5.0, 5.0), c + vec2(5.0, -5.0)], st);
            }
        }
        let tip = match kind {
            0 => "Minimize",
            1 if maximized => "Restore",
            1 => "Maximize",
            _ => "Close",
        };
        if resp.on_hover_text(tip).clicked() {
            ctx.send_viewport_cmd(match kind {
                0 => egui::ViewportCommand::Minimized(true),
                1 => egui::ViewportCommand::Maximized(!maximized),
                _ => egui::ViewportCommand::Close,
            });
        }
    }
}

/// The window has no system frame, so its edges and corners resize it (a thin invisible border).
fn window_edges(ctx: &egui::Context) {
    use egui::viewport::ResizeDirection as D;
    let (maximized, full) = ctx.input(|i| (i.viewport().maximized.unwrap_or(false), i.viewport().fullscreen.unwrap_or(false)));
    if maximized || full {
        return;
    }
    let Some(p) = ctx.input(|i| i.pointer.hover_pos()) else { return };
    let r = ctx.screen_rect();
    let m = 5.0;
    let (l, rt, t, b) = (p.x - r.left() < m, r.right() - p.x < m, p.y - r.top() < m, r.bottom() - p.y < m);
    let (dir, icon) = match (l, rt, t, b) {
        (true, _, true, _) => (D::NorthWest, egui::CursorIcon::ResizeNwSe),
        (_, true, true, _) => (D::NorthEast, egui::CursorIcon::ResizeNeSw),
        (true, _, _, true) => (D::SouthWest, egui::CursorIcon::ResizeNeSw),
        (_, true, _, true) => (D::SouthEast, egui::CursorIcon::ResizeNwSe),
        (true, ..) => (D::West, egui::CursorIcon::ResizeHorizontal),
        (_, true, ..) => (D::East, egui::CursorIcon::ResizeHorizontal),
        (_, _, true, _) => (D::North, egui::CursorIcon::ResizeVertical),
        (_, _, _, true) => (D::South, egui::CursorIcon::ResizeVertical),
        _ => return,
    };
    ctx.set_cursor_icon(icon);
    if ctx.input(|i| i.pointer.primary_pressed()) {
        ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
    }
}

#[derive(Clone, Copy, PartialEq)]
enum LibFilter {
    All,
    Playlists,
    Albums,
    Artists,
}

enum EntryCover {
    Kind(SidebarKind),
    Url(Option<String>, bool),
}

struct Entry {
    cover: EntryCover,
    title: String,
    sub: String,
    route: Route,
    /// Which filter pill keeps this row (None: only shown with no filter).
    group: Option<LibFilter>,
}

fn kind_group(k: &ItemKind) -> Option<LibFilter> {
    Some(match k {
        ItemKind::Album => LibFilter::Albums,
        ItemKind::Artist => LibFilter::Artists,
        _ => LibFilter::Playlists,
    })
}

fn library_entries(app: &App) -> Vec<Entry> {
    let mut v: Vec<Entry> = vec![];
    let st = &app.store;
    v.push(Entry { cover: EntryCover::Kind(SidebarKind::Liked), title: "Liked Songs".into(), sub: format!("Playlist • {} songs", st.liked.len()), route: Route::Liked, group: Some(LibFilter::Playlists) });
    v.push(Entry { cover: EntryCover::Kind(SidebarKind::Local), title: "Local files".into(), sub: format!("Music on this computer • {}", st.local_tracks.len()), route: Route::LocalFiles, group: Some(LibFilter::Playlists) });
    v.push(Entry { cover: EntryCover::Kind(SidebarKind::Downloads), title: "Downloads".into(), sub: format!("{} downloaded", st.downloads.len()), route: Route::Downloads, group: None });
    v.push(Entry { cover: EntryCover::Kind(SidebarKind::History), title: "Listening history".into(), sub: "Your plays and stats".into(), route: Route::History, group: None });
    // The playlists you made come first (newest first), so a new one is right where you look.
    for p in st.playlists.iter().rev() {
        v.push(Entry { cover: EntryCover::Url(playlist_cover_key(p), false), title: p.name.clone(), sub: format!("Playlist • {} songs", p.tracks.len()), route: Route::UserPlaylist(p.id.clone()), group: Some(LibFilter::Playlists) });
    }
    // What you follow or saved yourself comes before the long synced lists, so it is never pushed out of sight.
    for it in st.followed_artists.iter() {
        v.push(Entry { cover: EntryCover::Url(it.thumb.clone(), true), title: it.title.clone(), sub: "Artist".into(), route: Route::Artist(it.id.clone()), group: Some(LibFilter::Artists) });
    }
    for it in st.saved_playlists.iter().chain(st.saved_albums.iter()) {
        let route = if it.kind == ItemKind::Album { Route::Album(it.id.clone()) } else { Route::Playlist(it.id.clone()) };
        let sub = format!("{} • {}", it.kind.label(), it.subtitle.split(" • ").last().unwrap_or(""));
        v.push(Entry { cover: EntryCover::Url(it.thumb.clone(), false), title: it.title.clone(), sub, route, group: kind_group(&it.kind) });
    }
    // Google (YouTube Music) library, saved in the app.
    if crate::providers::google_connected() {
        v.push(Entry { cover: EntryCover::Kind(SidebarKind::Liked), title: "Liked Songs".into(), sub: format!("YouTube Music • {} songs", st.gl_liked.len()), route: Route::Playlist("LM".into()), group: Some(LibFilter::Playlists) });
        for it in st.gl_playlists.iter().chain(st.gl_albums.iter()).chain(st.gl_artists.iter()).filter(|g| !st.followed_artists.iter().chain(st.saved_albums.iter()).chain(st.saved_playlists.iter()).any(|x| x.id == g.id)) {
            let route = match it.kind {
                ItemKind::Album => Route::Album(it.id.clone()),
                ItemKind::Artist => Route::Artist(it.id.clone()),
                _ => Route::Playlist(it.id.clone()),
            };
            v.push(Entry { cover: EntryCover::Url(it.thumb.clone(), it.kind == ItemKind::Artist), title: it.title.clone(), sub: it.subtitle.clone(), route, group: kind_group(&it.kind) });
        }
    }
    // Spotify library, or a way to connect it (hidden when the person chose to go without accounts).
    if crate::spotify::logged_in() {
        v.push(Entry { cover: EntryCover::Kind(SidebarKind::SpotifyLiked), title: "Liked Songs".into(), sub: format!("Spotify • {} songs", st.sp_liked.len()), route: Route::SpotifyLiked, group: Some(LibFilter::Playlists) });
        for it in st.sp_playlists.iter().chain(st.sp_albums.iter()).chain(st.sp_artists.iter()) {
            v.push(Entry { cover: EntryCover::Url(it.thumb.clone(), it.kind == ItemKind::Artist), title: it.title.clone(), sub: it.subtitle.clone(), route: Route::SpotifyCollection(it.id.clone()), group: kind_group(&it.kind) });
        }
    } else if !st.welcome_dismissed {
        v.push(Entry { cover: EntryCover::Kind(SidebarKind::SpotifyConnect), title: "Connect Spotify".into(), sub: "Optional, needs a free key".into(), route: Route::SpotifyLogin, group: None });
    }
    v
}

/// The Now playing panel on the right: cover, title, like button, related music videos, "In this video".
fn now_panel(ctx: &egui::Context, app: &mut App) {
    let Some(t) = app.current.clone() else { return };
    app.ensure_now_related(&t);
    let related = app.np_related.get(&t.key()).cloned();
    egui::SidePanel::right("now_panel")
        .exact_width(364.0)
        .resizable(false)
        .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin { left: 4.0, right: 8.0, top: 0.0, bottom: 8.0 }))
        .show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            ui.painter().rect_filled(rect, 8.0, PANEL);
            ui.allocate_ui_at_rect(rect.shrink2(vec2(16.0, 16.0)), |ui| {
                ui.set_clip_rect(rect);
                let from = if app.playing_from.is_empty() { format!("{} Popular", t.artist.split(',').next().unwrap_or("")) } else { app.playing_from.clone() };
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(egui::RichText::new(from).font(bold(15.0)).color(TEXT)).truncate());
                });
                ui.add_space(8.0);
                egui::ScrollArea::vertical().id_source("now_panel_scroll").auto_shrink([false, false]).show(ui, |ui| {
                    let w = ui.available_width();
                    // Cover.
                    if video_ui::active(app) {
                        video_ui::video_block(ui, app, w, true);
                    } else {
                        let (cr, cresp) = ui.allocate_exact_size(vec2(w, w), Sense::click());
                        let key = track_cover_key(&t);
                        paint_cover(ui, &mut app.images, key.as_deref(), cr, 8.0, &t.title);
                        if cresp.on_hover_text("Open Now playing").clicked() {
                            app.actions.push(Action::Navigate(Route::Player));
                        }
                        // Video off: find out whether this song has one to switch to.
                        if !app.store.settings.prefer_video {
                            app.ensure_video_lookup(&t);
                        }
                        if video_ui::available_off(app) {
                            ui.add_space(12.0);
                            if video_ui::switch_pill(ui, "Switch to video") {
                                app.actions.push(Action::SetVideoMode(true));
                            }
                        }
                    }
                    ui.add_space(14.0);
                    // Title, artist and the add/like button.
                    let liked = app.store.is_liked(&t);
                    let (tr, _) = ui.allocate_exact_size(vec2(w, 56.0), Sense::hover());
                    let title_r = text_trunc(ui, pos2(tr.left(), tr.top()), &t.title, bold(22.0), TEXT, w - 44.0, Align2::LEFT_TOP);
                    if ui.interact(title_r, ui.id().with("np_t"), Sense::click()).clicked() {
                        app.actions.push(Action::GoAlbum(t.clone()));
                    }
                    let ar = text_trunc(ui, pos2(tr.left(), tr.top() + 32.0), &t.artist, reg(15.0), SUB, w - 44.0, Align2::LEFT_TOP);
                    if ui.interact(ar, ui.id().with("np_a"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        app.actions.push(Action::GoArtist(t.clone()));
                    }
                    if plus_button(ui, pos2(tr.right() - 14.0, tr.top() + 18.0), liked, "np_like") {
                        app.actions.push(Action::ToggleLike(t.clone()));
                    }
                    ui.add_space(14.0);

                    // Related music videos.
                    match related {
                        Some(Loadable::Ready(vids)) if !vids.is_empty() => {
                            ui.label(egui::RichText::new("Related music videos").font(bold(15.0)).color(TEXT));
                            ui.add_space(8.0);
                            let tkey = egui::Id::new("np_rel_target");
                            let pending: Option<f32> = ui.data(|d| d.get_temp::<Option<f32>>(tkey)).flatten();
                            ui.data_mut(|d| d.insert_temp(tkey, None::<f32>));
                            let mut area = egui::ScrollArea::horizontal().id_source("np_related_scroll").auto_shrink([false, true]);
                            if let Some(x) = pending {
                                area = area.horizontal_scroll_offset(x);
                            }
                            let out = area.show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 12.0;
                                    for v in &vids {
                                        let (r, resp) = ui.allocate_exact_size(vec2(172.0, 150.0), Sense::click());
                                        let thumb = Rect::from_min_size(r.min, vec2(172.0, 97.0));
                                        let key = v.thumb.clone().or_else(|| v.track.as_ref().and_then(track_cover_key));
                                        paint_cover(ui, &mut app.images, key.as_deref(), thumb, 4.0, &v.title);
                                        text_trunc(ui, pos2(r.left(), thumb.bottom() + 8.0), &v.title, reg(14.0), TEXT, 172.0, Align2::LEFT_TOP);
                                        text_trunc(ui, pos2(r.left(), thumb.bottom() + 28.0), &v.subtitle, reg(13.0), SUB, 172.0, Align2::LEFT_TOP);
                                        if resp.clicked() {
                                            app.actions.push(Action::OpenItem(v.clone()));
                                        }
                                    }
                                });
                            });
                            // Round arrows over the row to page through more videos.
                            let view = out.inner_rect;
                            let off = out.state.offset.x;
                            let max = (out.content_size.x - view.width()).max(0.0);
                            let cy = view.top() + 48.0;
                            for (right, show) in [(true, off < max - 1.0), (false, off > 1.0)] {
                                if !show {
                                    continue;
                                }
                                let c = pos2(if right { view.right() - 26.0 } else { view.left() + 26.0 }, cy);
                                let br = Rect::from_center_size(c, vec2(40.0, 40.0));
                                let resp = ui.interact(br, ui.id().with(("np_rel_arrow", right)), Sense::click());
                                ui.painter().circle_filled(c, if resp.hovered() { 20.0 } else { 19.0 }, Color32::from_black_alpha(190));
                                let d = if right { 1.0 } else { -1.0 };
                                let st = Stroke::new(2.2_f32, TEXT);
                                ui.painter().line_segment([c + vec2(-3.0 * d, -6.0), c + vec2(3.0 * d, 0.0)], st);
                                ui.painter().line_segment([c + vec2(3.0 * d, 0.0), c + vec2(-3.0 * d, 6.0)], st);
                                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    let step = 2.0 * (172.0 + 12.0);
                                    let target = if right { (off + step).min(max) } else { (off - step).max(0.0) };
                                    ui.data_mut(|d| d.insert_temp(tkey, Some(target)));
                                    ui.ctx().request_repaint();
                                }
                            }
                            ui.add_space(14.0);
                        }
                        Some(Loadable::Loading) => {
                            ui.label(egui::RichText::new("Related music videos").font(bold(15.0)).color(TEXT));
                            spinner_text(ui, "Loading…");
                        }
                        _ => {}
                    }

                    // In this video.
                    egui::Frame::none().fill(Color32::from_rgb(0x1E, 0x1E, 0x1E)).rounding(8.0).inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(egui::RichText::new("In this video").font(bold(15.0)).color(TEXT));
                        ui.add_space(8.0);
                        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, 6.0, Color32::from_white_alpha(12));
                        }
                        let cover = Rect::from_min_size(r.min + vec2(4.0, 4.0), vec2(48.0, 48.0));
                        paint_cover(ui, &mut app.images, key_of(&t).as_deref(), cover, 4.0, &t.title);
                        let x = cover.right() + 12.0;
                        text_trunc(ui, pos2(x, r.center().y - 1.0), &t.title, reg(15.0), TEXT, r.right() - x - 40.0, Align2::LEFT_BOTTOM);
                        text_trunc(ui, pos2(x, r.center().y + 2.0), &format!("Song • {}", t.artist), reg(13.0), SUB, r.right() - x - 40.0, Align2::LEFT_TOP);
                        if plus_button(ui, pos2(r.right() - 16.0, r.center().y), liked, "np_like2") {
                            app.actions.push(Action::ToggleLike(t.clone()));
                        }
                    });
                    // About the artist.
                    if let Some(aid) = t.artist_id.clone() {
                        app.ensure_artist(&aid);
                        if let Some(Loadable::Ready(page)) = app.artists.get(&aid).cloned() {
                            ui.add_space(12.0);
                            let w = ui.available_width();
                            let img_h = w * 0.66;
                            let (card, cresp) = ui.allocate_exact_size(vec2(w, img_h + 150.0), Sense::click());
                            let bg = Color32::from_rgb(0x1E, 0x1E, 0x1E);
                            ui.painter().rect_filled(card, 8.0, bg);
                            let img = Rect::from_min_size(card.min, vec2(w, img_h));
                            let url = page.thumb.as_deref().map(|u| upscale_thumb(u, 800));
                            let tex = url.as_deref().and_then(|u| app.images.get(u)).map(|l| (l.tex.id(), l.tex.size_vec2()));
                            if let Some((id, ts)) = tex {
                                let sc = (img.width() / ts.x).max(img.height() / ts.y);
                                let vis = img.size() / sc;
                                let uv = Rect::from_min_size(pos2((ts.x - vis.x) / 2.0 / ts.x, (ts.y - vis.y) * 0.2 / ts.y), vec2(vis.x / ts.x, vis.y / ts.y));
                                egui::Image::new((id, img.size())).uv(uv).rounding(egui::Rounding { nw: 8.0, ne: 8.0, sw: 0.0, se: 0.0 }).paint_at(ui, img);
                            }
                            screens::gradient(ui, Rect::from_min_size(img.min, vec2(w, 70.0)), Color32::from_black_alpha(150), Color32::from_black_alpha(0));
                            ui.painter().text(img.min + vec2(20.0, 22.0), Align2::LEFT_CENTER, "About the artist", bold(15.0), TEXT);
                            let y = img.bottom();
                            text_trunc(ui, pos2(card.left() + 20.0, y + 20.0), &page.name, bold(17.0), TEXT, w - 40.0, Align2::LEFT_TOP);
                            if !page.listeners.is_empty() {
                                text_trunc(ui, pos2(card.left() + 20.0, y + 52.0), &page.listeners, reg(14.0), SUB, w - 150.0, Align2::LEFT_TOP);
                            }
                            let item = Item { kind: ItemKind::Artist, id: page.id.clone(), title: page.name.clone(), subtitle: "Artist".into(), thumb: page.thumb.clone(), track: None };
                            let following = app.store.is_saved(&ItemKind::Artist, &page.id);
                            if outline_pill(ui, pos2(card.right() - 62.0, y + 62.0), if following { "Following" } else { "Follow" }, "about_follow") {
                                app.actions.push(Action::ToggleSaved(item));
                            } else if cresp.clicked() {
                                app.actions.push(Action::Navigate(Route::Artist(page.id.clone())));
                            }
                            if !page.description.is_empty() {
                                let mut job = egui::text::LayoutJob::simple(page.description.clone(), reg(13.5), SUB, w - 40.0);
                                job.wrap.max_rows = 3;
                                job.wrap.overflow_character = Some('…');
                                let galley = ui.fonts(|f| f.layout_job(job));
                                ui.painter().galley(pos2(card.left() + 20.0, y + 88.0), galley, SUB);
                            }
                        }
                    }

                    // Credits: every artist on the song.
                    let names: Vec<String> = t.artist.split(',').map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect();
                    if !names.is_empty() {
                        ui.add_space(12.0);
                        let skey = egui::Id::new("np_credits_all");
                        let mut all: bool = ui.data(|d| d.get_temp(skey)).unwrap_or(false);
                        egui::Frame::none().fill(Color32::from_rgb(0x1E, 0x1E, 0x1E)).rounding(8.0).inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Credits").font(bold(15.0)).color(TEXT));
                                if names.len() > 3 {
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let r = ui.add(egui::Label::new(egui::RichText::new(if all { "Show less" } else { "Show all" }).font(bold(13.0)).color(SUB)).sense(Sense::click()));
                                        if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                            all = !all;
                                        }
                                    });
                                }
                            });
                            ui.add_space(6.0);
                            let shown = if all { names.len() } else { names.len().min(3) };
                            for (i, name) in names.iter().take(shown).enumerate() {
                                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::hover());
                                text_trunc(ui, pos2(r.left(), r.center().y - 2.0), name, reg(15.0), TEXT, r.width() - 110.0, Align2::LEFT_BOTTOM);
                                text_trunc(ui, pos2(r.left(), r.center().y + 2.0), if i == 0 { "Main Artist" } else { "Artist" }, reg(13.0), SUB, r.width() - 110.0, Align2::LEFT_TOP);
                                let known = if i == 0 { t.artist_id.clone() } else { None };
                                let low = name.to_lowercase();
                                let following = known.as_ref().map_or(false, |id| app.store.is_saved(&ItemKind::Artist, id))
                                    || app.store.followed_artists.iter().any(|a| a.title.to_lowercase() == low);
                                if outline_pill(ui, pos2(r.right() - 46.0, r.center().y), if following { "Following" } else { "Follow" }, &format!("cred_follow_{i}")) {
                                    match known {
                                        Some(aid) => {
                                            let thumb = match app.artists.get(&aid) {
                                                Some(Loadable::Ready(p)) => p.thumb.clone(),
                                                _ => None,
                                            };
                                            app.actions.push(Action::ToggleSaved(Item { kind: ItemKind::Artist, id: aid, title: name.clone(), subtitle: "Artist".into(), thumb, track: None }));
                                        }
                                        None => app.actions.push(Action::FollowByName(name.clone())),
                                    }
                                }
                            }
                        });
                        ui.data_mut(|d| d.insert_temp(skey, all));
                    }
                    ui.add_space(12.0);
                });
            });
        });
}

/// A small outlined pill button (Follow / Following) centred at `c`. True when clicked.
fn outline_pill(ui: &mut egui::Ui, c: egui::Pos2, label: &str, id: &str) -> bool {
    let r = Rect::from_center_size(c, vec2(92.0, 32.0));
    let resp = ui.interact(r, ui.id().with(id), Sense::click());
    ui.painter().rect_stroke(r, 16.0, Stroke::new(1.0_f32, if resp.hovered() { TEXT } else { SUB }));
    ui.painter().text(r.center(), Align2::CENTER_CENTER, label, bold(13.0), TEXT);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn key_of(t: &Track) -> Option<String> {
    track_cover_key(t)
}

/// A round "+" button that turns into a green check once the song is liked. True when clicked.
fn plus_button(ui: &mut egui::Ui, c: egui::Pos2, liked: bool, id: &str) -> bool {
    let r = Rect::from_center_size(c, vec2(32.0, 32.0));
    let resp = ui.interact(r, ui.id().with(id), Sense::click());
    if liked {
        ui.painter().circle_filled(c, 11.0, ACCENT);
        ui.painter().add(egui::Shape::line(vec![c + vec2(-4.5, 0.0), c + vec2(-1.5, 3.5), c + vec2(4.5, -3.5)], Stroke::new(2.0_f32, Color32::BLACK)));
    } else {
        let col = if resp.hovered() { TEXT } else { SUB };
        ui.painter().circle_stroke(c, 11.0, Stroke::new(1.6_f32, col));
        ui.painter().line_segment([c + vec2(-5.0, 0.0), c + vec2(5.0, 0.0)], Stroke::new(1.6_f32, col));
        ui.painter().line_segment([c + vec2(0.0, -5.0), c + vec2(0.0, 5.0)], Stroke::new(1.6_f32, col));
    }
    resp.on_hover_text(if liked { "Remove from Liked Songs" } else { "Add to Liked Songs" }).clicked()
}

/// The shrink arrows, for other UI files.
pub fn paint_resize_pub(p: &egui::Painter, rect: Rect, col: Color32) {
    paint_resize(p, rect, col, false);
}

/// Two diagonal arrows: pointing out (expand) or in (shrink).
fn paint_resize(p: &egui::Painter, rect: Rect, col: Color32, expand: bool) {
    let c = rect.center();
    let h = rect.width() * 0.5;
    let st = Stroke::new(1.6_f32, col);
    let s = h * 0.55;
    let a = c + vec2(h, -h);
    let b = c + vec2(-h, h);
    if expand {
        p.line_segment([a, b], st);
        p.line_segment([a, a + vec2(-s, 0.0)], st);
        p.line_segment([a, a + vec2(0.0, s)], st);
        p.line_segment([b, b + vec2(s, 0.0)], st);
        p.line_segment([b, b + vec2(0.0, -s)], st);
    } else {
        let d = h * 0.2;
        let ai = c + vec2(d, -d);
        let bi = c + vec2(-d, d);
        p.line_segment([a, ai], st);
        p.line_segment([b, bi], st);
        p.line_segment([ai, ai + vec2(s, 0.0)], st);
        p.line_segment([ai, ai + vec2(0.0, -s)], st);
        p.line_segment([bi, bi + vec2(-s, 0.0)], st);
        p.line_segment([bi, bi + vec2(0.0, s)], st);
    }
}

/// The library as a full-window grid of covers (the expanded view).
fn library_full(ctx: &egui::Context, app: &mut App) {
    let full_key = egui::Id::new("lib_full");
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin { left: 8.0, right: 8.0, top: 0.0, bottom: 8.0 }))
        .show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            ui.painter().rect_filled(rect, 8.0, PANEL);
            ui.allocate_ui_at_rect(rect.shrink2(vec2(16.0, 14.0)), |ui| {
                ui.set_clip_rect(rect);
                // Header.
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Your Library").font(bold(17.0)).color(TEXT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (er, eresp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                        if eresp.hovered() {
                            ui.painter().circle_filled(er.center(), 16.0, Color32::from_white_alpha(14));
                        }
                        paint_resize(ui.painter(), er.shrink(7.0), if eresp.hovered() { TEXT } else { SUB }, false);
                        if eresp.on_hover_text("Collapse Your Library").clicked() {
                            ui.ctx().data_mut(|d| d.insert_temp(full_key, false));
                        }
                        let (cr, cresp) = ui.allocate_exact_size(vec2(84.0, 34.0), Sense::click());
                        ui.painter().rect_filled(cr, 17.0, if cresp.hovered() { Color32::from_rgb(0x3A, 0x3A, 0x3A) } else { Color32::from_rgb(0x2A, 0x2A, 0x2A) });
                        let tw = ui.fonts(|f| f.layout_no_wrap("Create".to_string(), bold(13.0), TEXT)).size().x;
                        let gx = cr.center().x - (22.0 + tw) / 2.0;
                        paint_icon(ui.painter(), Rect::from_center_size(pos2(gx + 8.0, cr.center().y), vec2(16.0, 16.0)), Icon::Plus, TEXT);
                        ui.painter().text(pos2(gx + 22.0, cr.center().y), Align2::LEFT_CENTER, "Create", bold(13.0), TEXT);
                        if cresp.on_hover_text("Create playlist").clicked() {
                            app.ui_new_playlist = Some((String::new(), None));
                        }
                    });
                });
                ui.add_space(8.0);

                // Filter pills + find in library.
                let fkey = egui::Id::new("lib_filter");
                let qkey = egui::Id::new("lib_search_text");
                let mut filter: LibFilter = ui.data(|d| d.get_temp(fkey)).unwrap_or(LibFilter::All);
                let mut query: String = ui.data(|d| d.get_temp(qkey)).unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for (f, label) in [(LibFilter::Playlists, "Playlists"), (LibFilter::Albums, "Albums"), (LibFilter::Artists, "Artists")] {
                        if pill(ui, label, filter == f).clicked() {
                            filter = if filter == f { LibFilter::All } else { f };
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add(egui::TextEdit::singleline(&mut query).hint_text("Search in Your Library").desired_width(220.0));
                    });
                });
                ui.data_mut(|d| {
                    d.insert_temp(fkey, filter);
                    d.insert_temp(qkey, query.clone());
                });
                ui.add_space(14.0);

                let q = query.trim().to_lowercase();
                let entries: Vec<Entry> = library_entries(app)
                    .into_iter()
                    .filter(|e| filter == LibFilter::All || e.group == Some(filter))
                    .filter(|e| q.is_empty() || e.title.to_lowercase().contains(&q))
                    .collect();
                let gap = 12.0;
                let w = ui.available_width();
                let cols = (((w + gap) / (150.0 + gap)).floor() as usize).max(1);
                let cell = (w - gap * (cols as f32 - 1.0)) / cols as f32;
                egui::ScrollArea::vertical().id_source("library_full").auto_shrink([false, false]).show(ui, |ui| {
                    if entries.is_empty() {
                        ui.label(egui::RichText::new("Nothing here yet").color(SUB));
                    }
                    for chunk in entries.chunks(cols) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = gap;
                            for e in chunk {
                                let (r, resp) = ui.allocate_exact_size(vec2(cell, cell + 50.0), Sense::click());
                                hover_prefetch(ui, app, &format!("full_{}", e.title), resp.hovered(), Some(e.route.clone()));
                                if ui.rect_contains_pointer(r) {
                                    ui.painter().rect_filled(r, 8.0, Color32::from_white_alpha(16));
                                }
                                let pad = 8.0;
                                let cover = Rect::from_min_size(r.min + vec2(pad, pad), vec2(cell - pad * 2.0, cell - pad * 2.0));
                                match &e.cover {
                                    EntryCover::Kind(k) => paint_kind_cover(ui, cover, *k),
                                    EntryCover::Url(url, round) => paint_cover(ui, &mut app.images, url.as_deref(), cover, if *round { cover.width() / 2.0 } else { 6.0 }, &e.title),
                                }
                                text_trunc(ui, pos2(r.left() + pad, cover.bottom() + 8.0), &e.title, reg(14.0), TEXT, cell - pad * 2.0, Align2::LEFT_TOP);
                                text_trunc(ui, pos2(r.left() + pad, cover.bottom() + 27.0), &e.sub, reg(12.5), SUB, cell - pad * 2.0, Align2::LEFT_TOP);
                                if resp.clicked() {
                                    app.actions.push(Action::Navigate(e.route.clone()));
                                    ui.ctx().data_mut(|d| d.insert_temp(full_key, false));
                                }
                            }
                        });
                        ui.add_space(6.0);
                    }
                });
            });
        });
}

fn sidebar(ctx: &egui::Context, app: &mut App) {
    let ckey = egui::Id::new("lib_collapsed");
    let collapsed: bool = ctx.data(|d| d.get_temp(ckey)).unwrap_or(true);
    egui::SidePanel::left("sidebar")
        .exact_width(if collapsed { 72.0 } else { 320.0 })
        .resizable(false)
        .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin { left: 8.0, right: 4.0, top: 0.0, bottom: 8.0 }))
        .show(ctx, |ui| {
            let lib = ui.available_rect_before_wrap();
            ui.painter().rect_filled(lib, 8.0, PANEL);
            if collapsed {
                ui.allocate_ui_at_rect(lib.shrink2(vec2(6.0, 10.0)), |ui| {
                    ui.vertical_centered(|ui| {
                        if icon_button(ui, Icon::Library, 40.0, SUB, "Expand Your Library").clicked() {
                            ui.ctx().data_mut(|d| d.insert_temp(ckey, false));
                        }
                        let (cr, cresp) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
                        ui.painter().circle_filled(cr.center(), 18.0, if cresp.hovered() { Color32::from_rgb(0x3A, 0x3A, 0x3A) } else { Color32::from_rgb(0x2A, 0x2A, 0x2A) });
                        paint_icon(ui.painter(), cr.shrink(10.0), Icon::Plus, TEXT);
                        if cresp.on_hover_text("Create playlist").clicked() {
                            app.ui_new_playlist = Some((String::new(), None));
                        }
                        ui.add_space(6.0);
                        let entries = library_entries(app);
                        egui::ScrollArea::vertical().id_source("sidebar_lib_collapsed").auto_shrink([false, false]).show(ui, |ui| {
                            for e in entries {
                                let (r, resp) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::click());
                                hover_prefetch(ui, app, &format!("rail_{}", e.title), resp.hovered(), Some(e.route.clone()));
                                if resp.hovered() || app.route == e.route {
                                    ui.painter().rect_filled(r, 6.0, Color32::from_white_alpha(if app.route == e.route { 22 } else { 12 }));
                                }
                                if playing_here(app, &e.route) {
                                    // A green dot marks the library item that is playing.
                                    ui.painter().circle_filled(pos2(r.right() - 5.0, r.top() + 7.0), 3.5, ACCENT);
                                }
                                let cover = Rect::from_center_size(r.center(), vec2(44.0, 44.0));
                                match &e.cover {
                                    EntryCover::Kind(k) => paint_kind_cover(ui, cover, *k),
                                    EntryCover::Url(url, round) => paint_cover(ui, &mut app.images, url.as_deref(), cover, if *round { 24.0 } else { 4.0 }, &e.title),
                                }
                                if resp.hovered() {
                                    rail_card(ui.ctx(), pos2(r.right() + 12.0, r.center().y - 26.0), &e.title, &e.sub, playing_here(app, &e.route));
                                }
                                if resp.clicked() {
                                    app.actions.push(Action::Navigate(e.route.clone()));
                                }
                            }
                        });
                    });
                });
                return;
            }
            ui.allocate_ui_at_rect(lib.shrink2(vec2(8.0, 10.0)), |ui| {
                // Header: Your Library + Create.
                ui.horizontal(|ui| {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width() - 136.0, 36.0), Sense::click());
                    let col = if resp.hovered() || app.route == Route::Library { TEXT } else { SUB };
                    paint_icon(ui.painter(), Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(24.0, 24.0)), Icon::Library, col);
                    ui.painter().text(pos2(r.left() + 48.0, r.center().y), Align2::LEFT_CENTER, "Your Library", bold(15.0), col);
                    let icon_hit = ui.interact(Rect::from_min_size(r.min, vec2(40.0, r.height())), ui.id().with("lib_collapse"), Sense::click()).on_hover_text("Collapse Your Library");
                    if icon_hit.clicked() {
                        ui.ctx().data_mut(|d| d.insert_temp(ckey, true));
                    } else if resp.clicked() {
                        app.actions.push(Action::Navigate(Route::Library));
                    }
                    let (cr, cresp) = ui.allocate_exact_size(vec2(84.0, 34.0), Sense::click());
                    ui.painter().rect_filled(cr, 17.0, if cresp.hovered() { Color32::from_rgb(0x3A, 0x3A, 0x3A) } else { Color32::from_rgb(0x2A, 0x2A, 0x2A) });
                    let tw = ui.fonts(|f| f.layout_no_wrap("Create".to_string(), bold(13.0), TEXT)).size().x;
                    let group = 16.0 + 6.0 + tw;
                    let gx = cr.center().x - group / 2.0;
                    paint_icon(ui.painter(), Rect::from_center_size(pos2(gx + 8.0, cr.center().y), vec2(16.0, 16.0)), Icon::Plus, TEXT);
                    ui.painter().text(pos2(gx + 22.0, cr.center().y), Align2::LEFT_CENTER, "Create", bold(13.0), TEXT);
                    if cresp.on_hover_text("Create playlist").clicked() {
                        app.ui_new_playlist = Some((String::new(), None));
                    }
                    let (er, eresp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                    if eresp.hovered() {
                        ui.painter().circle_filled(er.center(), 16.0, Color32::from_white_alpha(14));
                    }
                    paint_resize(ui.painter(), er.shrink(7.0), if eresp.hovered() { TEXT } else { SUB }, true);
                    if eresp.on_hover_text("Expand Your Library").clicked() {
                        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("lib_full"), true));
                    }
                });
                ui.add_space(6.0);

                // Filter pills.
                let fkey = egui::Id::new("lib_filter");
                let mut filter: LibFilter = ui.data(|d| d.get_temp(fkey)).unwrap_or(LibFilter::All);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for (f, label) in [(LibFilter::Playlists, "Playlists"), (LibFilter::Albums, "Albums"), (LibFilter::Artists, "Artists")] {
                        if pill(ui, label, filter == f).clicked() {
                            filter = if filter == f { LibFilter::All } else { f };
                        }
                    }
                });
                ui.data_mut(|d| d.insert_temp(fkey, filter));
                ui.add_space(4.0);

                // Find in library.
                let okey = egui::Id::new("lib_search_open");
                let qkey = egui::Id::new("lib_search_text");
                let mut open: bool = ui.data(|d| d.get_temp(okey)).unwrap_or(false);
                let mut query: String = ui.data(|d| d.get_temp(qkey)).unwrap_or_default();
                ui.horizontal(|ui| {
                    if icon_button(ui, Icon::Search, 30.0, if open { TEXT } else { SUB }, "Search in Your Library").clicked() {
                        open = !open;
                        if !open {
                            query.clear();
                        }
                    }
                    if open {
                        let r = ui.add(egui::TextEdit::singleline(&mut query).hint_text("Search in Your Library").desired_width(ui.available_width() - 4.0));
                        if r.gained_focus() {
                            ui.memory_mut(|m| m.request_focus(r.id));
                        }
                    }
                });
                ui.data_mut(|d| {
                    d.insert_temp(okey, open);
                    d.insert_temp(qkey, query.clone());
                });
                ui.add_space(2.0);

                let q = query.trim().to_lowercase();
                let entries: Vec<Entry> = library_entries(app)
                    .into_iter()
                    .filter(|e| filter == LibFilter::All || e.group == Some(filter))
                    .filter(|e| q.is_empty() || e.title.to_lowercase().contains(&q))
                    .collect();
                egui::ScrollArea::vertical().id_source("sidebar_lib").auto_shrink([false, false]).show(ui, |ui| {
                    if entries.is_empty() {
                        ui.add_space(12.0);
                        ui.label(egui::RichText::new("Nothing here yet").color(SUB));
                    }
                    for e in entries {
                        match e.cover {
                            EntryCover::Kind(k) => sidebar_entry(ui, app, k, &e.title, &e.sub, e.route),
                            EntryCover::Url(url, round) => sidebar_item(ui, app, url.as_deref(), &e.title, &e.sub, round, e.route),
                        }
                    }
                });
            });
        });
}

/// True when this library row is the album / playlist / artist the queue is playing from.
fn route_matches_ctx(r: &Route, it: &Item) -> bool {
    match r {
        Route::Liked => it.id == "liked",
        Route::SpotifyLiked => it.id == "sp-liked",
        Route::Playlist(i) | Route::Album(i) | Route::Artist(i) | Route::UserPlaylist(i) | Route::SpotifyCollection(i) => *i == it.id,
        _ => false,
    }
}

fn playing_here(app: &App, r: &Route) -> bool {
    app.current.is_some() && app.playing_ctx.as_ref().map(|c| route_matches_ctx(r, c)).unwrap_or(false)
}

/// The card shown next to a rail cover: name, kind, and a green speaker while it plays.
fn rail_card(ctx: &egui::Context, pos: egui::Pos2, title: &str, sub: &str, playing: bool) {
    egui::Area::new(egui::Id::new("rail_tip")).order(egui::Order::Tooltip).fixed_pos(pos).interactable(false).show(ctx, |ui| {
        egui::Frame::none().fill(Color32::from_rgb(0x28, 0x28, 0x28)).rounding(8.0).inner_margin(egui::Margin::symmetric(12.0, 9.0)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.add(egui::Label::new(egui::RichText::new(title).font(bold(14.0)).color(if playing { ACCENT } else { TEXT })).extend());
                    ui.add(egui::Label::new(egui::RichText::new(sub).size(12.0).color(SUB)).extend());
                });
                if playing {
                    ui.add_space(16.0);
                    let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                    paint_icon(ui.painter(), r, Icon::Volume, ACCENT);
                }
            });
        });
    });
}

#[derive(Clone, Copy)]
enum SidebarKind {
    SpotifyLiked,
    SpotifyConnect,
    Liked,
    Local,
    Downloads,
    History,
}

fn paint_kind_cover(ui: &mut egui::Ui, cover: Rect, kind: SidebarKind) {
    match kind {
        SidebarKind::SpotifyLiked => liked_cover(ui, cover, 4.0),
        SidebarKind::SpotifyConnect => {
            ui.painter().circle_filled(cover.center(), 24.0, spotify_pages::SPOTIFY_GREEN);
            for (i, w) in [(0, 26.0), (1, 21.0), (2, 16.0)] {
                let y = cover.center().y - 7.0 + i as f32 * 7.0;
                ui.painter().line_segment([pos2(cover.center().x - w / 2.0, y), pos2(cover.center().x + w / 2.0, y)], Stroke::new(2.6_f32, Color32::BLACK));
            }
        }
        SidebarKind::Liked => liked_cover(ui, cover, 4.0),
        SidebarKind::Local => simple_cover(ui, cover, Color32::from_rgb(0x3A, 0x8D, 0xB5), "♫"),
        SidebarKind::Downloads => {
            ui.painter().rect_filled(cover, 4.0, Color32::from_rgb(0x1E, 0x32, 0x64));
            paint_icon(ui.painter(), cover.shrink(12.0), Icon::Download, TEXT);
        }
        SidebarKind::History => {
            ui.painter().rect_filled(cover, 4.0, Color32::from_rgb(0x50, 0x3A, 0x8C));
            paint_icon(ui.painter(), cover.shrink(12.0), Icon::Timer, TEXT);
        }
    }
}

fn sidebar_entry(ui: &mut egui::Ui, app: &mut App, kind: SidebarKind, title: &str, sub: &str, route: Route) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 60.0), Sense::click());
    if resp.hovered() || app.route == route {
        ui.painter().rect_filled(rect, 6.0, Color32::from_white_alpha(if app.route == route { 22 } else { 12 }));
    }
    let cover = Rect::from_min_size(pos2(rect.left() + 8.0, rect.top() + 6.0), vec2(48.0, 48.0));
    paint_kind_cover(ui, cover, kind);
    let x = cover.right() + 12.0;
    let w = rect.right() - x - 8.0;
    let playing = playing_here(app, &route);
    let col = if app.route == route || playing { ACCENT } else { TEXT };
    text_trunc(ui, pos2(x, rect.center().y - 1.0), title, reg(15.0), col, w - if playing { 22.0 } else { 0.0 }, Align2::LEFT_BOTTOM);
    text_trunc(ui, pos2(x, rect.center().y + 3.0), sub, reg(13.0), SUB, w, Align2::LEFT_TOP);
    if playing {
        paint_icon(ui.painter(), Rect::from_center_size(pos2(rect.right() - 16.0, rect.center().y), vec2(16.0, 16.0)), Icon::Volume, ACCENT);
    }
    if resp.clicked() {
        app.actions.push(Action::Navigate(route));
    }
}

fn sidebar_item(ui: &mut egui::Ui, app: &mut App, cover_url: Option<&str>, title: &str, sub: &str, round: bool, route: Route) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 60.0), Sense::click());
    hover_prefetch(ui, app, &format!("side_{}_{}", title, rect.top() as i32), resp.hovered(), Some(route.clone()));
    if resp.hovered() || app.route == route {
        ui.painter().rect_filled(rect, 6.0, Color32::from_white_alpha(if app.route == route { 22 } else { 12 }));
    }
    let cover = Rect::from_min_size(pos2(rect.left() + 8.0, rect.top() + 6.0), vec2(48.0, 48.0));
    paint_cover(ui, &mut app.images, cover_url, cover, if round { 24.0 } else { 4.0 }, title);
    let x = cover.right() + 12.0;
    let w = rect.right() - x - 8.0;
    let playing = playing_here(app, &route);
    let col = if app.route == route || playing { ACCENT } else { TEXT };
    text_trunc(ui, pos2(x, rect.center().y - 1.0), title, reg(15.0), col, w - if playing { 22.0 } else { 0.0 }, Align2::LEFT_BOTTOM);
    text_trunc(ui, pos2(x, rect.center().y + 3.0), sub, reg(13.0), SUB, w, Align2::LEFT_TOP);
    if playing {
        paint_icon(ui.painter(), Rect::from_center_size(pos2(rect.right() - 16.0, rect.center().y), vec2(16.0, 16.0)), Icon::Volume, ACCENT);
    }
    if resp.clicked() {
        app.actions.push(Action::Navigate(route));
    }
}

fn player_bar(ctx: &egui::Context, app: &mut App) {
    egui::TopBottomPanel::bottom("player")
        .exact_height(84.0)
        .frame(egui::Frame::none().fill(Color32::BLACK).inner_margin(egui::Margin::symmetric(12.0, 8.0)))
        .show_separator_line(false)
        .show(ctx, |ui| {
            let full = ui.available_rect_before_wrap();
            let left = Rect::from_min_size(full.min, vec2(full.width() * 0.3, full.height()));
            let right = Rect::from_min_max(pos2(full.right() - full.width() * 0.3, full.top()), full.max);
            let mid = Rect::from_min_max(pos2(left.right(), full.top()), pos2(right.left(), full.bottom()));

            // Now playing.
            if let Some(t) = app.current.clone() {
                let cover = Rect::from_min_size(pos2(left.left(), left.center().y - 28.0), vec2(56.0, 56.0));
                let key = track_cover_key(&t);
                paint_cover(ui, &mut app.images, key.as_deref(), cover, 4.0, &t.title);
                let cr = ui.interact(cover, ui.id().with("np_cover"), Sense::click()).on_hover_text("Now playing view");
                if cr.clicked() {
                    app.actions.push(Action::Navigate(Route::Player));
                }
                let x = cover.right() + 14.0;
                let w = left.right() - x - 44.0;
                let tr = text_trunc(ui, pos2(x, left.center().y - 1.0), &t.title, reg(14.0), TEXT, w, Align2::LEFT_BOTTOM);
                let title_resp = ui.interact(tr, ui.id().with("np_title"), Sense::click());
                if title_resp.clicked() {
                    app.actions.push(Action::GoAlbum(t.clone()));
                }
                let ar = text_trunc(ui, pos2(x, left.center().y + 3.0), &t.artist, reg(12.0), SUB, w, Align2::LEFT_TOP);
                if ui.interact(ar, ui.id().with("np_artist"), Sense::click()).clicked() {
                    app.actions.push(Action::GoArtist(t.clone()));
                }
                let liked = app.store.is_liked(&t);
                let hr = Rect::from_center_size(pos2((x + tr.width().max(ar.width()) + 22.0).min(left.right() - 18.0), left.center().y), vec2(28.0, 28.0));
                let h = ui.interact(hr, ui.id().with("np_heart"), Sense::click());
                paint_like_sized(ui.painter(), hr.center(), liked, h.hovered(), 7.0);
                if h.on_hover_text(if liked { "Remove from Liked Songs" } else { "Add to Liked Songs" }).clicked() {
                    app.actions.push(Action::ToggleLike(t.clone()));
                }
            }

            // Controls and progress.
            ui.allocate_ui_at_rect(mid, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(2.0);
                    let controls_w = 5.0 * 40.0;
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() - controls_w) / 2.0);
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let shuffle_col = if app.shuffle { ACCENT } else { SUB };
                        if icon_button(ui, Icon::Shuffle, 34.0, shuffle_col, if app.shuffle { "Shuffle on" } else { "Shuffle" }).clicked() {
                            app.actions.push(Action::ToggleShuffle);
                        }
                        if icon_button(ui, Icon::Prev, 34.0, SUB, "Previous").clicked() {
                            app.actions.push(Action::Prev);
                        }
                        let playing = app.status.playing && app.status.loaded;
                        if app.loading {
                            let (r, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
                            ui.painter().circle_filled(r.center(), 18.0, TEXT);
                            spinner_arc(ui, r.center(), 8.0, Color32::BLACK);
                        } else {
                            let (r, resp) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
                            let rad = if resp.hovered() { 18.5 } else { 17.5 };
                            ui.painter().circle_filled(r.center(), rad, TEXT);
                            paint_icon(ui.painter(), r.shrink(10.0), if playing { Icon::Pause } else { Icon::Play }, Color32::BLACK);
                            if resp.on_hover_text(if playing { "Pause" } else { "Play" }).clicked() {
                                app.actions.push(Action::TogglePlay);
                            }
                        }
                        if icon_button(ui, Icon::Next, 34.0, SUB, "Next").clicked() {
                            app.actions.push(Action::Next);
                        }
                        let rep_col = if app.repeat { ACCENT } else { SUB };
                        let rr = icon_button(ui, Icon::Repeat, 34.0, rep_col, if app.repeat { "Repeat one: on" } else { "Repeat one" });
                        if app.repeat {
                            ui.painter().text(rr.rect.center() + vec2(0.0, 0.5), Align2::CENTER_CENTER, "1", bold(8.0), ACCENT);
                        }
                        if rr.clicked() {
                            app.actions.push(Action::ToggleRepeat);
                        }
                    });
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        let w = ui.available_width().min(620.0);
                        ui.add_space((ui.available_width() - w) / 2.0);
                        let dur = if app.status.duration_ms > 0 { app.status.duration_ms } else { app.current.as_ref().map(|t| t.duration_ms).unwrap_or(0) };
                        let pos = app.status.position_ms;
                        ui.add_sized(vec2(44.0, 16.0), egui::Label::new(egui::RichText::new(format_ms(pos)).size(11.5).color(SUB)));
                        let frac = if dur > 0 { pos as f32 / dur as f32 } else { 0.0 };
                        if let Some(f) = bar(ui, w - 104.0, frac, "seek") {
                            if dur > 0 {
                                app.actions.push(Action::Seek((f as f64 * dur as f64) as u64));
                            }
                        }
                        ui.add_sized(vec2(44.0, 16.0), egui::Label::new(egui::RichText::new(format_ms(dur)).size(11.5).color(SUB)));
                    });
                });
            });

            // Right side: lyrics, queue, timer, volume, now playing.
            ui.allocate_ui_at_rect(right, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    // Keep the volume bar off the window's edge.
                    ui.add_space(28.0);
                    let mut v = app.store.settings.volume;
                    if let Some(f) = bar(ui, 96.0, v, "volume") {
                        v = f;
                        app.actions.push(Action::SetVolume(v));
                    }
                    let vol = icon_button(ui, Icon::Volume, 32.0, SUB, &format!("Volume {}%", (v * 100.0).round()));
                    if vol.clicked() {
                        app.actions.push(Action::SetVolume(if v > 0.0 { 0.0 } else { 0.8 }));
                    }
                    if icon_button(ui, Icon::Queue, 32.0, if app.route == Route::Queue { ACCENT } else { SUB }, "Queue").clicked() {
                        app.actions.push(Action::Navigate(if app.route == Route::Queue { Route::Player } else { Route::Queue }));
                    }
                    let on_lyrics = app.route == Route::Lyrics;
                    if icon_button(ui, Icon::Lyrics, 32.0, if on_lyrics { ACCENT } else { SUB }, if on_lyrics { "Close lyrics" } else { "Lyrics" }).clicked() {
                        // Pressing it again closes the lyrics and returns to the page you were on.
                        if on_lyrics {
                            app.actions.push(if app.back.is_empty() { Action::Navigate(Route::Home) } else { Action::Back });
                        } else {
                            app.actions.push(Action::Navigate(Route::Lyrics));
                        }
                    }
                    {
                        let nkey = egui::Id::new("np_open");
                        let on: bool = ui.ctx().data(|d| d.get_temp(nkey)).unwrap_or(false);
                        let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                        let col = if on { ACCENT } else if resp.hovered() { TEXT } else { SUB };
                        let b = Rect::from_center_size(r.center(), vec2(16.0, 14.0));
                        ui.painter().rect_stroke(b, 2.5, Stroke::new(1.6_f32, col));
                        ui.painter().rect_filled(Rect::from_min_max(pos2(b.right() - 6.0, b.top() + 3.0), pos2(b.right() - 2.5, b.bottom() - 3.0)), 1.0, col);
                        if resp.on_hover_text("Now playing view").clicked() {
                            ui.ctx().data_mut(|d| d.insert_temp(nkey, !on));
                        }
                    }
                });
            });
        });
}

fn dialogs(ctx: &egui::Context, app: &mut App) {
    if let Some((mut name, track)) = app.ui_new_playlist.take() {
        let mut open = true;
        let mut done = false;
        egui::Window::new("New playlist")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(320.0);
                let r = ui.add(egui::TextEdit::singleline(&mut name).hint_text("My Playlist").desired_width(300.0));
                r.request_focus();
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        done = true;
                    }
                    if ui.add(egui::Button::new(egui::RichText::new("Create").color(Color32::BLACK)).fill(ACCENT)).clicked()
                        || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    {
                        app.actions.push(Action::NewPlaylist(name.clone(), track.clone()));
                        done = true;
                    }
                });
            });
        if open && !done {
            app.ui_new_playlist = Some((name, track));
        }
    }
    if let Some((track, mut link)) = app.ui_alt_link.take() {
        let mut open = true;
        let mut done = false;
        egui::Window::new("Alternative stream")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(egui::RichText::new(format!("Play “{}” from another YouTube video.", track.title)).color(SUB));
                let r = ui.add(egui::TextEdit::singleline(&mut link).hint_text("Paste a YouTube video link or video ID").desired_width(360.0));
                let id = crate::ytm::video_id_from_link(&link);
                if !link.trim().is_empty() && id.is_none() {
                    ui.label(egui::RichText::new("That doesn't look like a YouTube link").color(Color32::from_rgb(0xE5, 0x73, 0x73)));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        done = true;
                    }
                    let ok = ui.add_enabled(id.is_some(), egui::Button::new(egui::RichText::new("Use YouTube").color(Color32::BLACK)).fill(ACCENT));
                    if ok.clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && id.is_some()) {
                        app.actions.push(Action::SetAlt(track.clone(), AltStream::YouTube(id.clone().unwrap())));
                        done = true;
                    }
                });
            });
        if open && !done {
            app.ui_alt_link = Some((track, link));
        }
    }
    if let Some(mut ed) = app.ui_edit_playlist.take() {
        let mut open = true;
        let mut save = false;
        // Dim the app behind the window.
        egui::Area::new(egui::Id::new("edit_dim")).order(egui::Order::Foreground).fixed_pos(ctx.screen_rect().min).show(ctx, |ui| {
            let (r, resp) = ui.allocate_exact_size(ctx.screen_rect().size(), Sense::click());
            ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(170));
            if resp.clicked() {
                open = false;
            }
        });
        let current = app.store.playlists.iter().find(|p| p.id == ed.pid).cloned();
        let has_custom = current.as_ref().map(|p| p.cover.is_some()).unwrap_or(false) && !ed.remove_cover;
        let shown_key: Option<String> = if let Some(c) = &ed.new_cover {
            Some(format!("img:{c}"))
        } else if ed.remove_cover {
            current.as_ref().and_then(|p| p.tracks.first().and_then(track_cover_key))
        } else {
            current.as_ref().and_then(playlist_cover_key)
        };
        egui::Area::new(egui::Id::new("edit_playlist")).order(egui::Order::Tooltip).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
            egui::Frame::none().fill(Color32::from_rgb(0x28, 0x28, 0x28)).rounding(10.0).inner_margin(egui::Margin::same(24.0)).show(ui, |ui| {
                ui.set_width(600.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Edit details").font(bold(26.0)).color(TEXT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, 32.0, SUB, "Close").clicked() {
                            open = false;
                        }
                    });
                });
                ui.add_space(16.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 18.0;
                    // The picture, with "Choose photo" over it on hover.
                    let (cr, cresp) = ui.allocate_exact_size(vec2(200.0, 200.0), Sense::click());
                    paint_cover(ui, &mut app.images, shown_key.as_deref(), cr, 6.0, &ed.name);
                    if cresp.hovered() {
                        ui.painter().rect_filled(cr, 6.0, Color32::from_black_alpha(150));
                        ui.painter().text(cr.center() - vec2(0.0, 16.0), Align2::CENTER_CENTER, "✎", bold(42.0), TEXT);
                        ui.painter().text(cr.center() + vec2(0.0, 28.0), Align2::CENTER_CENTER, "Choose photo", bold(16.0), TEXT);
                    }
                    if cresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        if let Some(f) = rfd::FileDialog::new().set_title("Choose a picture").add_filter("Pictures", &["png", "jpg", "jpeg", "webp"]).pick_file() {
                            ed.new_cover = Some(f.to_string_lossy().into_owned());
                            ed.remove_cover = false;
                        }
                    }
                    ui.vertical(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut ed.name).desired_width(340.0).hint_text("Playlist name").margin(egui::vec2(12.0, 10.0)));
                        ui.add_space(8.0);
                        ui.add(egui::TextEdit::multiline(&mut ed.desc).desired_width(340.0).desired_rows(6).hint_text("Add an optional description").margin(egui::vec2(12.0, 10.0)));
                    });
                });
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if (has_custom || ed.new_cover.is_some()) && outline_button(ui, "Remove photo").clicked() {
                        ed.new_cover = None;
                        ed.remove_cover = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let can = !ed.name.trim().is_empty();
                        let (r, resp) = ui.allocate_exact_size(vec2(120.0, 48.0), Sense::click());
                        ui.painter().rect_filled(r, 24.0, if can { if resp.hovered() { Color32::WHITE } else { Color32::from_rgb(0xF0, 0xF0, 0xF0) } } else { Color32::from_rgb(0x55, 0x55, 0x55) });
                        ui.painter().text(r.center(), Align2::CENTER_CENTER, "Save", bold(16.0), Color32::BLACK);
                        if can && resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            save = true;
                        }
                    });
                });
                ui.add_space(10.0);
                ui.label(egui::RichText::new("The picture you choose is copied into Senthym's own folder, so it stays even if the original is moved.").size(12.0).color(SUB));
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
        if save {
            let cover = if let Some(c) = ed.new_cover.clone() { Some(Some(c)) } else if ed.remove_cover { Some(None) } else { None };
            app.actions.push(Action::UpdatePlaylist { pid: ed.pid.clone(), name: ed.name.clone(), desc: ed.desc.clone(), cover });
        } else if open {
            app.ui_edit_playlist = Some(ed);
        }
    }
    if let Some((pid, mut name)) = app.ui_rename.take() {
        let mut open = true;
        let mut done = false;
        egui::Window::new("Rename playlist")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(320.0);
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(300.0));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        done = true;
                    }
                    if ui.add(egui::Button::new(egui::RichText::new("Save").color(Color32::BLACK)).fill(ACCENT)).clicked()
                        || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    {
                        app.actions.push(Action::RenamePlaylist(pid.clone(), name.clone()));
                        done = true;
                    }
                });
            });
        if open && !done {
            app.ui_rename = Some((pid, name));
        }
    }
}

/// "Hidden in <playlist>. Undo" — a light pill with a clickable Undo, shown for a few seconds.
/// A card in the bottom-right corner when a newer Senthym is out (until it is dismissed with "Later").
fn update_notice(ctx: &egui::Context, app: &mut App) {
    use crate::app::UpdateState;
    let (title, sub, can_install) = match &app.update {
        UpdateState::Available(r) if app.store.settings.update_dismissed != r.version => {
            (format!("Senthym {} is available", r.version), "A new version is ready to download.".to_string(), true)
        }
        UpdateState::Downloading(r, got) => {
            let total = r.asset.as_ref().map(|a| a.size).unwrap_or(0).max(1);
            let pct = (got.load(std::sync::atomic::Ordering::Relaxed) * 100 / total).min(100);
            (format!("Downloading Senthym {}", r.version), format!("{pct}%"), false)
        }
        UpdateState::Done(text) => ("Senthym update".to_string(), text.clone(), false),
        _ => return,
    };
    let (mut install, mut later, mut close) = (false, false, false);
    let is_done = matches!(app.update, UpdateState::Done(_));
    egui::Area::new(egui::Id::new("update_notice")).order(egui::Order::Foreground).anchor(Align2::RIGHT_BOTTOM, vec2(-16.0, -100.0)).show(ctx, |ui| {
        egui::Frame::none().fill(Color32::from_rgb(0x28, 0x28, 0x28)).rounding(10.0).inner_margin(egui::Margin::symmetric(16.0, 12.0)).show(ui, |ui| {
            ui.set_max_width(300.0);
            ui.label(egui::RichText::new(title).font(bold(15.0)).color(TEXT));
            ui.label(egui::RichText::new(sub).size(13.0).color(SUB));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if can_install {
                    if ui.add(egui::Button::new(egui::RichText::new("Update").color(Color32::BLACK)).fill(ACCENT)).clicked() {
                        install = true;
                    }
                    if ui.button("Later").clicked() {
                        later = true;
                    }
                } else if is_done && ui.button("OK").clicked() {
                    close = true;
                }
            });
        });
    });
    if install {
        app.actions.push(Action::DownloadUpdate);
    }
    if later {
        app.actions.push(Action::DismissUpdate);
    }
    if close {
        app.update = crate::app::UpdateState::UpToDate;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
}

fn undo_toast(ctx: &egui::Context, app: &mut App) {
    let Some((pid, key, name, at)) = app.undo_hide.clone() else { return };
    if at.elapsed() > std::time::Duration::from_secs(7) {
        app.undo_hide = None;
        return;
    }
    let mut undo = false;
    egui::Area::new(egui::Id::new("undo_toast")).order(egui::Order::Tooltip).anchor(Align2::CENTER_BOTTOM, vec2(0.0, -100.0)).show(ctx, |ui| {
        egui::Frame::none().fill(Color32::WHITE).rounding(8.0).inner_margin(egui::Margin::symmetric(16.0, 11.0)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("Hidden in {name}.")).color(Color32::BLACK).size(14.0));
                let r = ui.add(egui::Label::new(egui::RichText::new("Undo").color(Color32::from_rgb(0x1B, 0x7F, 0xB5)).strong().size(14.0)).sense(Sense::click()));
                if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    undo = true;
                }
            });
        });
    });
    if undo {
        // Rebuild a minimal song from its key: the action only needs the key.
        app.store.set_hidden(&pid, &key, false);
        app.store.save();
        app.undo_hide = None;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
}

fn toasts(ctx: &egui::Context, app: &mut App) {
    let Some((text, _)) = app.toasts.last().cloned() else { return };
    egui::Area::new(egui::Id::new("toast"))
        .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -100.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(Color32::from_rgb(0x2E, 0x77, 0xD0))
                .rounding(8.0)
                .inner_margin(egui::Margin::symmetric(16.0, 10.0))
                .stroke(Stroke::NONE)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(text).color(TEXT).size(14.0));
                });
        });
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
}

/// The start-up window over a dimmed app: connect a Google account (no key, no Premium) or go on
/// without an account. Hidden for good once the person chooses to use the app without accounts.
/// A dropdown of the browsers found on this computer, the system's default one marked, shared by the
/// welcome window and Settings. `pick` is the browser to use (the suggestion when none was chosen yet);
/// true when the person picked another one.
pub fn browser_picker(ui: &mut egui::Ui, id: &str, width: f32, pick: &mut String) -> bool {
    let found = crate::helper::browsers();
    // What is offered: what was found. When nothing was found, every known browser, so the list is never empty.
    let mut offered: Vec<&'static str> = if found.installed.is_empty() { crate::helper::KNOWN_BROWSERS.to_vec() } else { found.installed.clone() };
    // A browser chosen earlier stays in the list even if it can't be found now.
    if let Some(k) = crate::helper::KNOWN_BROWSERS.iter().find(|k| **k == pick.as_str()) {
        if !offered.contains(k) {
            offered.push(k);
        }
    }
    let label = |name: &str| {
        let mut l = crate::helper::browser_label(name).to_string();
        if found.default == Some(name) {
            l.push_str(" (default)");
        } else if !found.installed.is_empty() && !found.installed.contains(&name) {
            l.push_str(" (not found)");
        }
        l
    };
    let shown = if pick.is_empty() { "Choose your browser".to_string() } else { label(pick) };
    let before = pick.clone();
    // Test switch: open the list at once (to check it is drawn above the window it is in).
    if std::env::var_os("SENTHYM_TEST_OPEN_LIST").is_some() {
        let popup = ui.make_persistent_id(egui::Id::new(id)).with("popup");
        ui.memory_mut(|m| m.open_popup(popup));
    }
    egui::ComboBox::from_id_source(id).selected_text(shown).width(width).show_ui(ui, |ui| {
        for name in offered {
            ui.selectable_value(pick, name.to_string(), label(name));
        }
    });
    *pick != before
}

fn welcome(ctx: &egui::Context, app: &mut App) {
    use spotify_pages::SPOTIFY_GREEN;
    let screen = ctx.screen_rect();
    // The dimmed backdrop sits below the card, and the card on the same layer as a dropdown's list (which
    // is drawn on the Foreground layer): on a higher layer the list opened hidden behind the card.
    egui::Area::new(egui::Id::new("welcome_dim")).order(egui::Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
        let (r, _) = ui.allocate_exact_size(screen.size(), Sense::click_and_drag());
        ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(205));
    });
    let size = vec2(860.0f32.min(screen.width() - 40.0), 400.0);
    egui::Area::new(egui::Id::new("welcome_card"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.center() - size / 2.0)
        .show(ctx, |ui| {
            let card = Rect::from_min_size(ui.cursor().min, size);
            ui.painter().rect_filled(card, 10.0, Color32::from_rgb(0x12, 0x12, 0x12));
            let left = Rect::from_min_max(card.min, pos2(card.center().x, card.max.y)).shrink(30.0);
            let right = Rect::from_min_max(pos2(card.center().x, card.min.y), card.max).shrink(30.0);
            ui.painter().line_segment([pos2(card.center().x, card.top() + 40.0), pos2(card.center().x, card.bottom() - 40.0)], Stroke::new(1.0_f32, Color32::from_white_alpha(30)));

            ui.allocate_ui_at_rect(left, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Sign in with Google").font(bold(24.0)).color(TEXT));
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Your YouTube Music library, right here.").size(14.0).color(SUB));
                    ui.add_space(14.0);
                    // The browser the person chose, else the system's default one (or the first found).
                    let mut pick = if app.store.settings.youtube_browser.is_empty() {
                        crate::helper::suggested_browser().map(String::from).unwrap_or_default()
                    } else {
                        app.store.settings.youtube_browser.clone()
                    };
                    if browser_picker(ui, "welcome_browser", 220.0, &mut pick) {
                        app.use_browser(&pick);
                    }
                    ui.add_space(10.0);
                    let can = !pick.is_empty() && !app.google_busy;
                    let label = if app.google_busy { "Connecting…" } else { "Connect Google account" };
                    let b = ui.add_enabled(can, egui::Button::new(egui::RichText::new(label).font(bold(16.0)).color(Color32::BLACK)).fill(SPOTIFY_GREEN).rounding(24.0).min_size(vec2(230.0, 46.0)));
                    if b.clicked() {
                        app.use_browser(&pick);
                        app.google_connect(&pick);
                    }
                    if let Some(e) = &app.google_error {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(e).size(12.0).color(Color32::from_rgb(0xE5, 0x73, 0x73)));
                    }
                    ui.add_space(14.0);
                    if ui.link(egui::RichText::new("Continue without an account").color(SUB)).clicked() {
                        app.show_welcome = false;
                        app.store.welcome_dismissed = true;
                        app.actions.push(Action::SaveSettings);
                    }
                });
            });

            ui.allocate_ui_at_rect(right, |ui| {
                ui.vertical(|ui| {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Your Music. Your Freedom.").font(bold(20.0)).color(TEXT));
                    ui.add_space(8.0);
                    for line in [
                        "1. Sign in to music.youtube.com in your browser (once, as usual).",
                        "2. Pick that browser here and press Connect.",
                        "3. Your liked songs, playlists, albums and artists come into Senthym and are saved in the app.",
                    ] {
                        ui.label(egui::RichText::new(line).size(13.5).color(TEXT));
                        ui.add_space(4.0);
                    }
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Spotify, Deezer and SoundCloud can be added later in Settings. Deezer and SoundCloud need no account at all.").size(12.0).color(SUB));
                });
            });
            ui.allocate_space(size);
        });
}
