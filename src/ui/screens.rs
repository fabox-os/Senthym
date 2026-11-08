//! Pages: Home, Search, Library, albums, artists, playlists and genres.

use super::pages;
use super::widgets::*;
use crate::app::{Action, App, Loadable, Route, SearchTab};
use crate::model::*;
use egui::{pos2, vec2, Align2, Color32, Rect, RichText, Sense, Ui};

pub fn page(ui: &mut Ui, app: &mut App) {
    let route = app.route.clone();
    let tint = page_tint(app, &route);
    let full = ui.max_rect();
    if let Some(c) = tint {
        gradient(ui, Rect::from_min_size(full.min, vec2(full.width(), 340.0)), c, PANEL);
    }
    ui.add_space(6.0);
    let scroll_id = format!("{:?}", route);
    egui::ScrollArea::vertical().id_source(scroll_id).auto_shrink([false, false]).show(ui, |ui| {
        egui::Frame::none().inner_margin(egui::Margin { left: 22.0, right: 22.0, top: 4.0, bottom: 30.0 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            match route {
                Route::Home => home(ui, app),
                Route::Search => search(ui, app),
                Route::Library => library(ui, app),
                Route::Album(id) => album(ui, app, &id),
                Route::Artist(id) => artist(ui, app, &id),
                Route::Playlist(id) => playlist(ui, app, &id),
                Route::UserPlaylist(id) => user_playlist(ui, app, &id),
                Route::Liked => liked(ui, app),
                Route::LocalFiles => pages::local_files(ui, app),
                Route::Downloads => pages::downloads(ui, app),
                Route::History => pages::history(ui, app),
                Route::Category { title, browse_id, params } => category(ui, app, &title, &browse_id, params),
                Route::Settings => pages::settings(ui, app),
                Route::Queue => pages::queue(ui, app),
                Route::Lyrics => pages::lyrics_page(ui, app),
                Route::Player => pages::now_playing(ui, app),
                Route::SpotifyLogin => super::spotify_pages::login(ui, app),
                Route::SpotifyLiked => super::spotify_pages::liked(ui, app),
                Route::SpotifyCollection(id) => super::spotify_pages::collection(ui, app, &id),
            }
        });
    });
}

pub fn gradient(ui: &Ui, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    ui.painter().add(egui::Shape::mesh(mesh));
}

pub fn darker(c: Color32, f: f32) -> Color32 {
    Color32::from_rgb((c.r() as f32 * f) as u8, (c.g() as f32 * f) as u8, (c.b() as f32 * f) as u8)
}

/// The page's top colour comes from its cover (dark-vibrant swatch, like the original).
fn page_tint(app: &mut App, route: &Route) -> Option<Color32> {
    let url = match route {
        Route::Album(id) => match app.albums.get(id) {
            Some(Loadable::Ready(a)) => a.thumb.clone(),
            _ => None,
        },
        Route::Artist(id) => match app.artists.get(id) {
            Some(Loadable::Ready(a)) => a.thumb.clone(),
            _ => None,
        },
        Route::SpotifyLiked => return Some(Color32::from_rgb(0x50, 0x38, 0xA0)),
        Route::SpotifyCollection(id) => match app.playlists.get(id) {
            Some(Loadable::Ready(p)) => p.thumb.clone(),
            _ => None,
        },
        Route::Playlist(id) => match app.playlists.get(id) {
            Some(Loadable::Ready(p)) => p.thumb.clone(),
            _ => None,
        },
        Route::Player | Route::Lyrics => app.current.as_ref().and_then(track_cover_key),
        Route::Liked => return Some(Color32::from_rgb(0x50, 0x38, 0xA0)),
        Route::LocalFiles => return Some(Color32::from_rgb(0x1E, 0x5C, 0x4A)),
        Route::Downloads => return Some(Color32::from_rgb(0x1E, 0x32, 0x64)),
        Route::Home => return Some(Color32::from_rgb(0x3A, 0x30, 0x6E)),
        _ => None,
    }?;
    let img = app.images.get(&url)?;
    img.dark_vibrant.or(img.muted).map(|c| darker(c, 0.9))
}

/// The search pill in the top bar. Typing or focusing it opens the Search page.
pub fn search_box(ui: &mut Ui, app: &mut App, width: f32) {
    let (r, pill_resp) = ui.allocate_exact_size(vec2(width, 44.0), Sense::hover());
    let hot = pill_resp.hovered() || ui.memory(|m| m.focused().is_some()) && app.route == Route::Search;
    ui.painter().rect_filled(r, 22.0, Color32::from_rgb(0x1F, 0x1F, 0x1F));
    if hot {
        ui.painter().rect_stroke(r, 22.0, egui::Stroke::new(1.0_f32, if app.route == Route::Search { TEXT } else { Color32::from_white_alpha(60) }));
    }
    paint_icon(ui.painter(), Rect::from_center_size(pos2(r.left() + 24.0, r.center().y), vec2(22.0, 22.0)), Icon::Search, SUB);
    let edit_rect = Rect::from_min_max(pos2(r.left() + 46.0, r.top() + 12.0), pos2(r.right() - 84.0, r.bottom() - 8.0));
    let before = app.search_text.clone();
    let resp = ui.put(edit_rect, egui::TextEdit::singleline(&mut app.search_text).hint_text("What do you want to play?").frame(false).font(reg(15.0)));
    if app.route == Route::Search && app.search_query.is_empty() && app.search_text.is_empty() && ui.memory(|m| m.focused().is_none()) {
        resp.request_focus();
    }
    if app.search_text != before {
        app.typed_search();
    }
    if ui.ctx().data(|d| d.get_temp::<bool>(egui::Id::new("focus_search"))).unwrap_or(false) {
        resp.request_focus();
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("focus_search"), false));
    }
    let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let mut handled_enter = false;
    // Browse button: opens the Browse all page (genres and moods).
    {
        let sep_x = r.right() - 48.0;
        ui.painter().line_segment([pos2(sep_x, r.top() + 11.0), pos2(sep_x, r.bottom() - 11.0)], egui::Stroke::new(1.0_f32, Color32::from_white_alpha(50)));
        let br = Rect::from_center_size(pos2(r.right() - 26.0, r.center().y), vec2(30.0, 30.0));
        let b = ui.interact(br, ui.id().with("browse_btn"), Sense::click()).on_hover_text("Browse");
        let on = app.route == Route::Search && app.search_query.is_empty();
        let col = if on || b.hovered() { TEXT } else { SUB };
        let st = egui::Stroke::new(1.8_f32, col);
        let c = br.center();
        ui.painter().rect_stroke(Rect::from_center_size(c + vec2(0.0, 2.5), vec2(16.0, 11.0)), 2.5, st);
        ui.painter().line_segment([c + vec2(-6.0, -5.0), c + vec2(6.0, -5.0)], st);
        ui.painter().line_segment([c + vec2(-3.0, -8.5), c + vec2(3.0, -8.5)], st);
        ui.painter().line_segment([c + vec2(-3.0, 2.5), c + vec2(3.0, 2.5)], st);
        if b.clicked() {
            app.search_text.clear();
            app.search_query.clear();
            app.suggestions.clear();
            app.suggest_open = false;
            app.actions.push(Action::Navigate(Route::Search));
        }
    }
    if !app.search_text.is_empty() {
        let cr = Rect::from_center_size(pos2(r.right() - 66.0, r.center().y), vec2(20.0, 20.0));
        let c = ui.interact(cr, ui.id().with("clear_search"), Sense::click());
        paint_icon(ui.painter(), cr.shrink(3.0), Icon::Close, if c.hovered() { TEXT } else { SUB });
        if c.clicked() {
            app.search_text.clear();
            app.search_query.clear();
            app.suggestions.clear();
        }
    }
    // The list opens when the box is typed in or focused and stays open until a search runs,
    // Escape is pressed, or the person clicks outside the box and the list. Keeping it open
    // (rather than tied to focus) lets a click on a suggestion land.
    if resp.changed() || resp.gained_focus() {
        app.suggest_open = true;
    }
    if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.suggest_open = false;
    }
    let rect_key = egui::Id::new("sugg_rect");
    let popup_rect: Option<Rect> = ui.data(|d| d.get_temp::<Option<Rect>>(rect_key)).flatten();
    let press_in_popup = ui.input(|i| i.pointer.interact_pos()).map(|p| popup_rect.map(|r| r.contains(p)).unwrap_or(false)).unwrap_or(false);
    if ui.input(|i| i.pointer.any_pressed()) && !resp.hovered() && !app.suggest_hovered && !press_in_popup {
        app.suggest_open = false;
    }
    let open = app.suggest_open && !app.suggestions.is_empty() && app.search_text.trim() != app.search_query;
    app.suggest_hovered = false;
    ui.data_mut(|d| d.insert_temp(rect_key, None::<Rect>));
    if open {
        let typed = app.search_text.trim().to_string();
        // Matching artists, playlists, albums and songs for what's typed (fetched once per text).
        app.prefetch_search(&typed, SearchTab::Top);
        let found: Vec<Item> = match app.search_results.get(&(typed.clone(), SearchTab::Top)) {
            Some(Loadable::Ready(items)) => items.clone(),
            _ => vec![],
        };
        let mut matches: Vec<Item> = vec![];
        for kind in [ItemKind::Artist, ItemKind::Playlist, ItemKind::Album, ItemKind::Song] {
            for it in found.iter().filter(|i| i.kind == kind) {
                if matches.len() < 4 && (kind != ItemKind::Song || matches.len() < 4) {
                    matches.push(it.clone());
                }
                if kind != ItemKind::Song && matches.iter().filter(|m| m.kind == kind).count() >= 2 {
                    break;
                }
            }
        }
        let sugg: Vec<String> = app.suggestions.iter().take(4).cloned().collect();
        let total = sugg.len() + matches.len();
        // Keyboard selection (Up / Down), reset when the text changes.
        let sel_key = egui::Id::new("sugg_sel");
        let mut sel: Option<usize> = ui.data(|d| d.get_temp::<Option<usize>>(sel_key)).flatten();
        if resp.changed() {
            sel = None;
        }
        if resp.has_focus() || enter {
            let (down, up) = ui.input(|i| (i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::ArrowUp)));
            if total > 0 && down {
                sel = Some(match sel { None => 0, Some(i) => (i + 1) % total });
            }
            if total > 0 && up {
                sel = Some(match sel { None => total - 1, Some(0) => total - 1, Some(i) => i - 1 });
            }
        }
        let mut activate: Option<usize> = None;
        if enter {
            if let Some(i) = sel {
                activate = Some(i);
                handled_enter = true;
            }
        }
        let area = egui::Area::new(egui::Id::new("suggestions"))
            .fixed_pos(pos2(r.left(), r.bottom() + 6.0))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).fill(Color32::from_rgb(0x28, 0x28, 0x28)).rounding(10.0).inner_margin(egui::Margin::symmetric(0.0, 8.0)).show(ui, |ui| {
                    ui.set_width(r.width() - 4.0);
                    // Key hints with boxed keys.
                    let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
                    let key_box = |ui: &Ui, rect: Rect, label: &str| {
                        ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0_f32, SUB));
                        ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, reg(12.0), SUB);
                    };
                    key_box(ui, Rect::from_center_size(pos2(hr.left() + 26.0, hr.center().y), vec2(22.0, 22.0)), "↑");
                    key_box(ui, Rect::from_center_size(pos2(hr.left() + 52.0, hr.center().y), vec2(22.0, 22.0)), "↓");
                    ui.painter().text(pos2(hr.left() + 70.0, hr.center().y), Align2::LEFT_CENTER, "Navigate", reg(12.0), SUB);
                    ui.painter().text(pos2(hr.right() - 14.0, hr.center().y), Align2::RIGHT_CENTER, "Search", reg(12.0), SUB);
                    key_box(ui, Rect::from_center_size(pos2(hr.right() - 98.0, hr.center().y), vec2(46.0, 22.0)), "Enter");
                    ui.add_space(2.0);
                    for (idx, s) in sugg.iter().enumerate() {
                        let (sr, sresp) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
                        if sresp.hovered() || sel == Some(idx) {
                            ui.painter().rect_filled(sr.shrink2(vec2(6.0, 0.0)), 6.0, Color32::from_white_alpha(16));
                        }
                        let c = pos2(sr.left() + 38.0, sr.center().y);
                        ui.painter().circle_filled(c, 20.0, Color32::from_white_alpha(10));
                        paint_icon(ui.painter(), Rect::from_center_size(c, vec2(20.0, 20.0)), Icon::Search, SUB);
                        // The typed part is bold, the rest normal.
                        let lower = s.to_lowercase();
                        let cut = if lower.starts_with(&typed.to_lowercase()) { typed.chars().count().min(s.chars().count()) } else { 0 };
                        let head: String = s.chars().take(cut).collect();
                        let tail: String = s.chars().skip(cut).collect();
                        let x = sr.left() + 72.0;
                        let hw = ui.fonts(|f| f.layout_no_wrap(head.clone(), bold(15.0), TEXT)).size().x;
                        ui.painter().text(pos2(x, sr.center().y), Align2::LEFT_CENTER, &head, bold(15.0), TEXT);
                        ui.painter().text(pos2(x + hw, sr.center().y), Align2::LEFT_CENTER, &tail, reg(15.0), SUB);
                        if sresp.is_pointer_button_down_on() || sresp.clicked() {
                            activate = Some(idx);
                        }
                    }
                    for (j, it) in matches.iter().enumerate() {
                        let idx = sugg.len() + j;
                        let (ir, iresp) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::click());
                        if iresp.hovered() || sel == Some(idx) {
                            ui.painter().rect_filled(ir.shrink2(vec2(6.0, 0.0)), 6.0, Color32::from_white_alpha(16));
                        }
                        let cover = Rect::from_min_size(pos2(ir.left() + 14.0, ir.top() + 8.0), vec2(48.0, 48.0));
                        let round = if it.kind == ItemKind::Artist { 24.0 } else { 4.0 };
                        let key = it.thumb.clone().or_else(|| it.track.as_ref().and_then(track_cover_key));
                        paint_cover(ui, &mut app.images, key.as_deref(), cover, round, &it.title);
                        let x = ir.left() + 76.0;
                        let w = ir.width() - 76.0 - 130.0;
                        text_trunc(ui, pos2(x, ir.center().y - 2.0), &it.title, bold(15.0), TEXT, w, Align2::LEFT_BOTTOM);
                        let sub = match it.kind {
                            ItemKind::Artist => "Artist".to_string(),
                            ItemKind::Song => format!("Song • {}", it.subtitle),
                            _ => it.subtitle.clone(),
                        };
                        text_trunc(ui, pos2(x, ir.center().y + 2.0), &sub, reg(13.0), SUB, w, Align2::LEFT_TOP);
                        if matches!(it.kind, ItemKind::Artist | ItemKind::Album | ItemKind::Playlist) {
                            let saved = app.store.is_saved(&it.kind, &it.id);
                            if it.kind == ItemKind::Artist {
                                let pill_r = Rect::from_center_size(pos2(ir.right() - 78.0, ir.center().y), vec2(108.0, 34.0));
                                let pr = ui.interact(pill_r, ui.id().with(("sugg_follow", idx)), Sense::click());
                                ui.painter().rect_stroke(pill_r, 17.0, egui::Stroke::new(1.0_f32, if pr.hovered() { TEXT } else { SUB }));
                                ui.painter().text(pill_r.center(), Align2::CENTER_CENTER, if saved { "Following" } else { "Follow" }, bold(13.0), TEXT);
                                if pr.clicked() {
                                    app.actions.push(Action::ToggleSaved(it.clone()));
                                    continue;
                                }
                            } else if saved {
                                let c = pos2(ir.right() - 30.0, ir.center().y);
                                ui.painter().circle_filled(c, 10.0, ACCENT);
                                ui.painter().add(egui::Shape::line(vec![c + vec2(-4.0, 0.0), c + vec2(-1.0, 3.5), c + vec2(4.0, -3.0)], egui::Stroke::new(2.0_f32, Color32::BLACK)));
                            }
                        }
                        if iresp.is_pointer_button_down_on() || iresp.clicked() {
                            activate = Some(idx);
                        }
                    }
                });
            });
        app.suggest_hovered = area.response.hovered();
        ui.data_mut(|d| d.insert_temp(rect_key, Some(area.response.rect)));
        ui.data_mut(|d| d.insert_temp(sel_key, sel));
        if let Some(i) = activate {
            if i < sugg.len() {
                app.actions.push(Action::Search(sugg[i].clone()));
            } else {
                app.actions.push(Action::RememberSearch(matches[i - sugg.len()].clone()));
                app.actions.push(Action::OpenItem(matches[i - sugg.len()].clone()));
                app.suggest_open = false;
                app.suggestions.clear();
            }
            ui.data_mut(|d| d.insert_temp(sel_key, None::<usize>));
        }
    }
    // Recent searches: what was picked from search before, shown when the empty box is in use.
    let text_empty = app.search_text.trim().is_empty();
    let has_recents = !app.store.recent_search_items.is_empty() || !app.store.recent_searches.is_empty();
    if text_empty && app.suggest_open && has_recents {
        app.resolve_recent_queries();
        let items = app.store.recent_search_items.clone();
        let queries = app.store.recent_searches.clone();
        let mut open_item: Option<Item> = None;
        let mut run_query: Option<String> = None;
        let mut remove_item: Option<String> = None;
        let mut remove_query: Option<String> = None;
        let mut clear = false;
        let area = egui::Area::new(egui::Id::new("recent_searches"))
            .fixed_pos(pos2(r.left(), r.bottom() + 6.0))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).fill(Color32::from_rgb(0x28, 0x28, 0x28)).rounding(10.0).inner_margin(egui::Margin::symmetric(0.0, 10.0)).show(ui, |ui| {
                    ui.set_width(r.width() - 4.0);
                    egui::ScrollArea::vertical().id_source("recent_scroll").max_height(520.0).auto_shrink([false, true]).show(ui, |ui| {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.add_space(20.0);
                            ui.label(RichText::new("Recent searches").font(bold(18.0)).color(TEXT));
                        });
                        ui.add_space(6.0);
                        for it in &items {
                            let (ir, iresp) = ui.allocate_exact_size(vec2(ui.available_width(), 72.0), Sense::click());
                            let hov = row_hovered(ui, ir);
                            if hov {
                                ui.painter().rect_filled(ir.shrink2(vec2(8.0, 0.0)), 6.0, Color32::from_white_alpha(16));
                            }
                            let cover = Rect::from_min_size(pos2(ir.left() + 20.0, ir.top() + 6.0), vec2(60.0, 60.0));
                            let round = if it.kind == ItemKind::Artist { 30.0 } else { 4.0 };
                            let key = it.thumb.clone().or_else(|| it.track.as_ref().and_then(track_cover_key));
                            paint_cover(ui, &mut app.images, key.as_deref(), cover, round, &it.title);
                            let x = ir.left() + 94.0;
                            let w = ir.width() - 94.0 - 56.0;
                            text_trunc(ui, pos2(x, ir.center().y - 2.0), &it.title, reg(17.0), TEXT, w, Align2::LEFT_BOTTOM);
                            let sub = match it.kind {
                                ItemKind::Artist => "Artist".to_string(),
                                ItemKind::Video => format!("▶ Music video • {}", it.subtitle.trim_start_matches("Video • ")),
                                ItemKind::Song => format!("Song • {}", it.subtitle),
                                _ => if it.subtitle.is_empty() { it.kind.label().to_string() } else { it.subtitle.clone() },
                            };
                            text_trunc(ui, pos2(x, ir.center().y + 3.0), &sub, reg(14.0), SUB, w, Align2::LEFT_TOP);
                            // Remove from the list.
                            let xr = Rect::from_center_size(pos2(ir.right() - 30.0, ir.center().y), vec2(30.0, 30.0));
                            let xresp = ui.interact(xr, ui.id().with(("recent_x", &it.id)), Sense::click());
                            if hov {
                                let c = xr.center();
                                let col = if xresp.hovered() { TEXT } else { SUB };
                                ui.painter().line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], egui::Stroke::new(1.6_f32, col));
                                ui.painter().line_segment([c + vec2(-5.0, 5.0), c + vec2(5.0, -5.0)], egui::Stroke::new(1.6_f32, col));
                            }
                            if xresp.clicked() {
                                remove_item = Some(it.id.clone());
                            } else if iresp.clicked() {
                                open_item = Some(it.clone());
                            }
                        }
                        for q in &queries {
                            let (qr, qresp) = ui.allocate_exact_size(vec2(ui.available_width(), 72.0), Sense::click());
                            let hov = row_hovered(ui, qr);
                            if hov {
                                ui.painter().rect_filled(qr.shrink2(vec2(8.0, 0.0)), 6.0, Color32::from_white_alpha(16));
                            }
                            // Not resolved to a result yet: a cover-shaped placeholder with the first letter.
                            let cover = Rect::from_min_size(pos2(qr.left() + 20.0, qr.top() + 6.0), vec2(60.0, 60.0));
                            paint_cover(ui, &mut app.images, None, cover, 4.0, q);
                            text_trunc(ui, pos2(qr.left() + 94.0, qr.center().y - 2.0), q, reg(17.0), TEXT, qr.width() - 94.0 - 56.0, Align2::LEFT_BOTTOM);
                            text_trunc(ui, pos2(qr.left() + 94.0, qr.center().y + 3.0), "Search", reg(14.0), SUB, qr.width() - 94.0 - 56.0, Align2::LEFT_TOP);
                            let xr = Rect::from_center_size(pos2(qr.right() - 30.0, qr.center().y), vec2(30.0, 30.0));
                            let xresp = ui.interact(xr, ui.id().with(("recent_qx", q)), Sense::click());
                            if hov {
                                let c = xr.center();
                                let col = if xresp.hovered() { TEXT } else { SUB };
                                ui.painter().line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], egui::Stroke::new(1.6_f32, col));
                                ui.painter().line_segment([c + vec2(-5.0, 5.0), c + vec2(5.0, -5.0)], egui::Stroke::new(1.6_f32, col));
                            }
                            if xresp.clicked() {
                                remove_query = Some(q.clone());
                            } else if qresp.clicked() {
                                run_query = Some(q.clone());
                            }
                        }
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.add_space(22.0);
                            if outline_button(ui, "Clear recent searches").clicked() {
                                clear = true;
                            }
                        });
                        ui.add_space(4.0);
                    });
                });
            });
        app.suggest_hovered = area.response.hovered();
        ui.data_mut(|d| d.insert_temp(rect_key, Some(area.response.rect)));
        if let Some(it) = open_item {
            app.actions.push(Action::RememberSearch(it.clone()));
            app.actions.push(Action::OpenItem(it));
            app.suggest_open = false;
        }
        if let Some(q) = run_query {
            app.actions.push(Action::Search(q));
        }
        if let Some(id) = remove_item {
            app.actions.push(Action::RemoveSearchItem(id));
        }
        if let Some(q) = remove_query {
            app.actions.push(Action::RemoveRecentSearch(q));
        }
        if clear {
            app.actions.push(Action::ClearRecentSearches);
        }
    }
    if enter && !handled_enter {
        let q = app.search_text.clone();
        app.actions.push(Action::Search(q));
    }
}

// ---------------------------------------------------------------- home

fn home(ui: &mut Ui, app: &mut App) {
    // Filter pills: All / Music / Podcasts.
    let fkey = egui::Id::new("home_filter");
    let mut f: u8 = ui.data(|d| d.get_temp(fkey)).unwrap_or(0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        for (i, label) in ["All", "Music", "Podcasts"].iter().enumerate() {
            if pill(ui, label, f == i as u8).clicked() {
                f = i as u8;
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(fkey, f));
    ui.add_space(14.0);
    if f == 2 {
        ui.add_space(30.0);
        ui.label(RichText::new("No podcasts yet").font(bold(20.0)).color(TEXT));
        ui.label(RichText::new("Senthym plays music from YouTube Music, Deezer, Spotify and your own files.").color(SUB));
        return;
    }

    // Quick access: 2 rows x 4. Liked Songs and Local files first, then recent albums/playlists.
    let mut shortcuts: Vec<Item> = vec![App::liked_item()];
    shortcuts.push(Item { kind: ItemKind::Playlist, id: "local".into(), title: "Local files".into(), subtitle: String::new(), thumb: None, track: None });
    shortcuts.extend(app.store.recent_items.iter().filter(|i| i.id != "liked" && i.id != "local" && i.title != "Liked Songs").take(6).cloned());
    shortcuts.truncate(8);
    let w = ui.available_width();
    let cols = if w > 900.0 { 4 } else { 2 };
    let cw = (w - 10.0 * (cols as f32 - 1.0)) / cols as f32;
    for row in shortcuts.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            for it in row {
                shortcut_tile(ui, app, it, cw);
            }
        });
        ui.add_space(6.0);
    }

    // Made For <name>: the personalised feed (YouTube Music home, else Deezer).
    let name = app.store.sp_user.as_ref().map(|u| u.name.clone()).unwrap_or_else(|| "You".into());
    let mut made_for: Vec<Item> = vec![];
    if let Loadable::Ready(sections) = &app.home {
        if let Some(sec) = sections.iter().find(|s| !(s.items.iter().all(|it| it.track.is_some()) && s.items.len() > 8) && !s.items.is_empty()) {
            made_for = sec.items.clone();
        }
    }
    if made_for.is_empty() {
        if let Loadable::Ready(sections) = &app.deezer_home {
            if let Some(sec) = sections.iter().find(|s| !s.items.is_empty()) {
                made_for = sec.items.clone();
            }
        }
    }
    if made_for.is_empty() {
        match &app.home {
            Loadable::Loading => spinner_text(ui, "Loading your feed…"),
            Loadable::Failed(e) => {
                let e = e.clone();
                if error_box(ui, &format!("Couldn't load the home feed. {e}")) {
                    app.load_home();
                }
            }
            _ => {}
        }
    }
    shelf_labeled(ui, app, Some("Made For"), &name, &made_for, "made_for");

    // Your top mixes: most played songs from the listening history.
    let mut counts: Vec<(usize, Track)> = vec![];
    for h in &app.store.history {
        let k = h.track.key();
        if let Some(e) = counts.iter_mut().find(|(_, t)| t.key() == k) {
            e.0 += 1;
        } else {
            counts.push((1, h.track.clone()));
        }
    }
    counts.sort_by(|a, b| b.0.cmp(&a.0));
    let mut top: Vec<Item> = counts.iter().take(12).map(|(_, t)| Item::from_track(t)).collect();
    if top.is_empty() {
        top = app.store.sp_top.iter().take(12).map(Item::from_track).collect();
    }
    shelf(ui, app, "Your top mixes", &top, "top_mixes");

    // Recents: Liked Songs, recent albums/playlists, then recently played songs.
    let mut recents: Vec<Item> = vec![App::liked_item()];
    recents.extend(app.store.recent_items.iter().filter(|i| i.id != "liked").take(11).cloned());
    for h in &app.store.history {
        if recents.len() >= 14 {
            break;
        }
        if !recents.iter().any(|x| x.id == h.track.key()) {
            recents.push(Item::from_track(&h.track));
        }
    }
    shelf(ui, app, "Recents", &recents, "recents");

    // Recommended Stations: a radio from each recently played artist.
    let mut seeds: Vec<Track> = vec![];
    for h in &app.store.history {
        if seeds.len() >= 10 {
            break;
        }
        if h.track.is_online() && !seeds.iter().any(|t| t.artist == h.track.artist) {
            seeds.push(h.track.clone());
        }
    }
    shelf_with(ui, app, Some("Non-stop music based on your favorite songs and artists."), "Recommended Stations", seeds.len(), "stations", |ui, app, i, w| {
        if radio_card(ui, app, &seeds[i], w) {
            app.actions.push(Action::StartRadio(seeds[i].clone()));
        }
    });
}

fn shortcut_tile(ui: &mut Ui, app: &mut App, it: &Item, cw: f32) {
    let (r, resp) = ui.allocate_exact_size(vec2(cw, 56.0), Sense::click());
    let hovered = ui.rect_contains_pointer(r);
    hover_prefetch(ui, app, &format!("tile_{}_{}", it.id, r.min.x as i32), hovered, item_route(it));
    ui.painter().rect_filled(r, 6.0, if hovered { Color32::from_white_alpha(40) } else { Color32::from_white_alpha(20) });
    let cover = Rect::from_min_size(r.min, vec2(56.0, 56.0));
    match it.id.as_str() {
        "liked" => liked_cover(ui, cover, 4.0),
        "local" => simple_cover(ui, cover, Color32::from_rgb(0x3A, 0x8D, 0xB5), "♫"),
        _ => paint_cover(ui, &mut app.images, it.thumb.as_deref(), cover, 4.0, &it.title),
    }
    text_trunc(ui, pos2(cover.right() + 12.0, r.center().y), &it.title, bold(14.0), TEXT, r.width() - 120.0, Align2::LEFT_CENTER);
    let mut played = false;
    let pr = Rect::from_center_size(pos2(r.right() - 26.0, r.center().y), vec2(36.0, 36.0));
    // The position makes the ID unique per tile (a repeated id is what drew the red labels).
    let pbtn = ui.interact(pr, ui.id().with(("sc_play", &it.id, r.min.x as i32, r.min.y as i32)), Sense::click());
    if hovered {
        ui.painter().circle_filled(pr.center(), 18.0, ACCENT);
        paint_icon(ui.painter(), pr.shrink(11.0), Icon::Play, Color32::BLACK);
        if pbtn.clicked() {
            played = true;
            match it.id.as_str() {
                "liked" => app.actions.push(Action::PlayTracks { tracks: app.store.liked.clone(), start: 0, shuffle: false, context: Some(App::liked_item()) }),
                "local" => app.actions.push(Action::PlayTracks { tracks: app.store.local_tracks.clone(), start: 0, shuffle: false, context: None }),
                _ => app.actions.push(Action::PlayItem(it.clone(), false)),
            }
        }
    }
    if resp.clicked() && !played {
        match it.id.as_str() {
            "liked" => app.actions.push(Action::Navigate(Route::Liked)),
            "local" => app.actions.push(Action::Navigate(Route::LocalFiles)),
            _ => app.actions.push(Action::OpenItem(it.clone())),
        }
    }
}

// ---------------------------------------------------------------- search

fn search(ui: &mut Ui, app: &mut App) {
    let q = app.search_query.clone();
    if q.is_empty() {
        browse_all(ui, app);
        return;
    }
    ui.horizontal(|ui| {
        for tab in SearchTab::ALL {
            if pill(ui, tab.label(), app.search_tab == tab).clicked() {
                app.search_tab = tab;
            }
        }
    });
    ui.add_space(10.0);
    let tab = app.search_tab;
    app.run_search(&q, tab);
    // Once the first tab is in, the other tabs are fetched too, so switching tabs is instant.
    if matches!(app.search_results.get(&(q.clone(), tab)), Some(Loadable::Ready(_))) {
        for t in [SearchTab::Songs, SearchTab::Albums, SearchTab::Artists, SearchTab::Playlists] {
            if t != tab {
                app.prefetch_search(&q, t);
            }
        }
    }
    let key = (q.clone(), tab);
    let items = match app.search_results.get(&key) {
        None | Some(Loadable::Loading) => return spinner_text(ui, "Searching…"),
        Some(Loadable::Failed(e)) => {
            let e = e.clone();
            if error_box(ui, &e) {
                app.search_results.remove(&key);
            }
            return;
        }
        Some(Loadable::Ready(items)) => items.clone(),
    };
    if items.is_empty() {
        ui.label(RichText::new(format!("No results found for “{q}”")).color(SUB));
        return;
    }
    // The best songs of the results are the likely next click: warm them up.
    for t in items.iter().filter_map(|i| i.track.as_ref()).take(3) {
        app.actions.push(Action::Prefetch(t.clone()));
    }
    match tab {
        SearchTab::Albums | SearchTab::Artists | SearchTab::Playlists => card_grid(ui, app, &items),
        SearchTab::Top => {
            let tracks: Vec<Track> = items.iter().filter_map(|i| i.track.clone()).collect();
            if let Some(first) = items.first() {
                section_title(ui, "Top result");
                top_result(ui, app, first);
            }
            if !tracks.is_empty() {
                section_title(ui, "Songs");
                for t in tracks.iter().take(6) {
                    if track_row(ui, app, t, RowOpts { number: None, show_cover: true, show_album: true, ctx: RowContext::Plain }) {
                        app.actions.push(Action::OpenItem(Item::from_track(t)));
                    }
                }
            }
            let others: Vec<Item> = items.iter().skip(1).filter(|i| i.track.is_none()).cloned().collect();
            for (kind, title) in [(ItemKind::Artist, "Artists"), (ItemKind::Album, "Albums"), (ItemKind::Playlist, "Playlists")] {
                let list: Vec<Item> = others.iter().filter(|i| i.kind == kind).cloned().collect();
                shelf(ui, app, title, &list, &format!("top_{title}"));
            }
        }
        _ => {
            let tracks: Vec<Track> = items.iter().filter_map(|i| i.track.clone()).collect();
            for (i, t) in tracks.iter().enumerate() {
                if track_row(ui, app, t, RowOpts { number: Some(i + 1), show_cover: true, show_album: true, ctx: RowContext::Plain }) {
                    app.actions.push(Action::OpenItem(Item::from_track(t)));
                }
            }
        }
    }
}

fn browse_all(ui: &mut Ui, app: &mut App) {
    if !app.store.recent_searches.is_empty() {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Recent searches").font(bold(20.0)).color(TEXT));
            if ui.button("Clear").clicked() {
                app.actions.push(Action::ClearRecentSearches);
            }
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for s in app.store.recent_searches.clone() {
                let r = pill(ui, &s, false);
                if r.clicked() {
                    app.actions.push(Action::Search(s.clone()));
                }
                r.context_menu(|ui| {
                    if ui.button("Remove").clicked() {
                        app.actions.push(Action::RemoveRecentSearch(s.clone()));
                        ui.close_menu();
                    }
                });
            }
        });
        ui.add_space(14.0);
    }
    section_title(ui, "Browse all");
    let cats = match &app.categories {
        Loadable::Loading => return spinner_text(ui, "Loading…"),
        Loadable::Failed(e) => {
            let e = e.clone();
            if error_box(ui, &format!("Couldn't load genres. {e}")) {
                app.load_categories();
            }
            return;
        }
        Loadable::Ready(c) => c.clone(),
    };
    let w = ui.available_width();
    let per = ((w / 210.0).floor() as usize).clamp(2, 8);
    let gap = 12.0;
    let cw = (w - gap * (per as f32 - 1.0)) / per as f32;
    let ch = cw * 0.55;
    for row in cats.chunks(per) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for c in row {
                let (r, resp) = ui.allocate_exact_size(vec2(cw, ch), Sense::click());
                let [_, rr, g, b] = c.color.to_be_bytes();
                let col = Color32::from_rgb(rr, g, b);
                ui.painter().rect_filled(r, 8.0, if resp.hovered() { col } else { darker(col, 0.8) });
                // Tilted cover behind the title, bottom-right, cut off by the tile's edge.
                if ui.is_rect_visible(r) {
                    app.ensure_browse_cover(&c.title);
                    if let Some(Some(url)) = app.browse_covers.get(&c.title).cloned() {
                        if let Some(img) = app.images.get(&url) {
                            let id = img.tex.id();
                            let side = ch * 0.62;
                            let ctr = pos2(r.right() - cw * 0.14, r.top() + ch * 0.60);
                            let (sn, cs) = 25.0f32.to_radians().sin_cos();
                            let h = side / 2.0;
                            let pt = |dx: f32, dy: f32| pos2(ctr.x + dx * cs - dy * sn, ctr.y + dx * sn + dy * cs);
                            let mut mesh = egui::Mesh::with_texture(id);
                            let uvs = [pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0), pos2(0.0, 1.0)];
                            for (p, uv) in [pt(-h, -h), pt(h, -h), pt(h, h), pt(-h, h)].iter().zip(uvs.iter()) {
                                mesh.vertices.push(egui::epaint::Vertex { pos: *p, uv: *uv, color: Color32::WHITE });
                            }
                            mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
                            ui.painter().with_clip_rect(r).add(egui::Shape::mesh(mesh));
                        }
                    }
                }
                let mut job = egui::text::LayoutJob::simple(c.title.clone(), bold(17.0), TEXT, r.width() - 24.0);
                job.wrap.max_rows = 2;
                let galley = ui.fonts(|f| f.layout_job(job));
                ui.painter().galley(r.min + vec2(14.0, 12.0), galley, TEXT);
                if resp.clicked() {
                    app.actions.push(Action::Navigate(Route::Category { title: c.title.clone(), browse_id: c.browse_id.clone(), params: c.params.clone() }));
                }
            }
        });
        ui.add_space(12.0);
    }
}



fn top_result(ui: &mut Ui, app: &mut App, it: &Item) {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width().min(460.0), 220.0), Sense::click());
    ui.painter().rect_filled(r, 8.0, if resp.hovered() { CARD_HOVER } else { CARD });
    let cover = Rect::from_min_size(r.min + vec2(20.0, 20.0), vec2(96.0, 96.0));
    let round = if it.kind == ItemKind::Artist { 48.0 } else { 6.0 };
    let key = it.thumb.clone().or_else(|| it.track.as_ref().and_then(track_cover_key));
    paint_cover(ui, &mut app.images, key.as_deref(), cover, round, &it.title);
    text_trunc(ui, pos2(r.left() + 20.0, cover.bottom() + 20.0), &it.title, bold(28.0), TEXT, r.width() - 40.0, Align2::LEFT_TOP);
    let label = it.kind.label();
    let rest = it.subtitle.trim_start_matches(label).trim_start_matches(" • ");
    let sub = if rest.is_empty() { label.to_string() } else { format!("{label} • {rest}") };
    text_trunc(ui, pos2(r.left() + 20.0, cover.bottom() + 62.0), &sub, reg(14.0), SUB, r.width() - 110.0, Align2::LEFT_TOP);
    let pr = Rect::from_center_size(pos2(r.right() - 44.0, r.bottom() - 40.0), vec2(48.0, 48.0));
    ui.painter().circle_filled(pr.center(), 24.0, ACCENT);
    paint_icon(ui.painter(), pr.shrink(14.0), Icon::Play, Color32::BLACK);
    if ui.interact(pr, ui.id().with("top_play"), Sense::click()).clicked() {
        app.actions.push(Action::PlayItem(it.clone(), false));
    } else if resp.clicked() {
        app.actions.push(Action::OpenItem(it.clone()));
    }
}

// ---------------------------------------------------------------- library

fn special_cover(ui: &Ui, app: &mut App, it: &Item, rect: Rect) {
    match it.id.as_str() {
        "liked" => liked_cover(ui, rect, 6.0),
        "local" => simple_cover(ui, rect, Color32::from_rgb(0x3A, 0x8D, 0xB5), "♫"),
        "history" => {
            ui.painter().rect_filled(rect, 6.0, Color32::from_rgb(0x50, 0x3A, 0x8C));
            paint_icon(ui.painter(), rect.shrink(rect.width() * 0.28), Icon::Timer, TEXT);
        }
        _ => paint_cover(ui, &mut app.images, it.thumb.as_deref(), rect, if it.kind == ItemKind::Artist { rect.width() / 2.0 } else { 4.0 }, &it.title),
    }
}

fn open_library_item(app: &mut App, it: &Item) {
    match it.id.as_str() {
        "liked" => app.actions.push(Action::Navigate(Route::Liked)),
        "local" => app.actions.push(Action::Navigate(Route::LocalFiles)),
        "history" => app.actions.push(Action::Navigate(Route::History)),
        id if id.starts_with("local-") => app.actions.push(Action::Navigate(Route::UserPlaylist(id.to_string()))),
        _ => app.actions.push(Action::OpenItem(it.clone())),
    }
}

fn play_library_item(app: &mut App, it: &Item) {
    match it.id.as_str() {
        "liked" => app.actions.push(Action::PlayTracks { tracks: app.store.liked.clone(), start: 0, shuffle: false, context: Some(App::liked_item()) }),
        "local" => app.actions.push(Action::PlayTracks { tracks: app.store.local_tracks.clone(), start: 0, shuffle: false, context: None }),
        "history" => app.actions.push(Action::Navigate(Route::History)),
        id if id.starts_with("local-") => {
            if let Some(pl) = app.store.playlists.iter().find(|x| x.id == id) {
                app.actions.push(Action::PlayTracks { tracks: pl.tracks.clone(), start: 0, shuffle: false, context: Some(it.clone()) });
            }
        }
        _ => app.actions.push(Action::PlayItem(it.clone(), false)),
    }
}

fn library_item_menu(ui: &mut Ui, app: &mut App, it: &Item) {
    if ui.button("Play").clicked() {
        play_library_item(app, it);
        ui.close_menu();
    }
    if it.id.starts_with("local-") {
        if ui.button("Rename").clicked() {
            app.ui_rename = Some((it.id.clone(), it.title.clone()));
            ui.close_menu();
        }
        if ui.button("Delete playlist").clicked() {
            app.actions.push(Action::DeletePlaylist(it.id.clone()));
            ui.close_menu();
        }
    } else if !["liked", "local", "history"].contains(&it.id.as_str())
        && ui.button(if it.kind == ItemKind::Artist { "Unfollow" } else { "Remove from Library" }).clicked()
    {
        app.actions.push(Action::ToggleSaved(it.clone()));
        ui.close_menu();
    }
}

fn library(ui: &mut Ui, app: &mut App) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Your Library").font(bold(28.0)).color(TEXT));
        ui.add_space(20.0);
        let grid = app.store.settings.library_grid;
        if ui.button(if grid { "Show as list" } else { "Show as grid" }).clicked() {
            app.store.settings.library_grid = !grid;
            app.actions.push(Action::SaveSettings);
        }
        if ui.button("+ New playlist").clicked() {
            app.ui_new_playlist = Some((String::new(), None));
        }
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        for (kind, label) in [(None, "All"), (Some(ItemKind::Playlist), "Playlists"), (Some(ItemKind::Album), "Albums"), (Some(ItemKind::Artist), "Artists")] {
            if pill(ui, label, app.library_filter == kind).clicked() {
                app.library_filter = kind;
            }
        }
        ui.add_space(12.0);
        ui.add(egui::TextEdit::singleline(&mut app.library_query).hint_text("Search library").desired_width(220.0));
    });
    ui.add_space(10.0);
    let q = app.library_query.to_lowercase();
    let matches = |s: &str| q.is_empty() || s.to_lowercase().contains(&q);
    let f = app.library_filter.clone();
    let mut items: Vec<Item> = vec![];
    if f.is_none() || f == Some(ItemKind::Playlist) {
        let mut liked = App::liked_item();
        liked.subtitle = format!("Playlist • {} songs", app.store.liked.len());
        items.push(liked);
        for p in &app.store.playlists {
            items.push(Item {
                kind: ItemKind::Playlist,
                id: p.id.clone(),
                title: p.name.clone(),
                subtitle: format!("Playlist • {} songs", p.tracks.len()),
                thumb: p.tracks.first().and_then(|t| t.thumb.clone()),
                track: None,
            });
        }
        items.extend(app.store.saved_playlists.iter().cloned());
    }
    if f.is_none() || f == Some(ItemKind::Album) {
        items.extend(app.store.saved_albums.iter().cloned());
    }
    if f.is_none() || f == Some(ItemKind::Artist) {
        items.extend(app.store.followed_artists.iter().cloned());
    }
    if f.is_none() {
        items.push(Item { kind: ItemKind::Playlist, id: "local".into(), title: "Local files".into(), subtitle: "Music imported from this computer".into(), thumb: None, track: None });
        items.push(Item { kind: ItemKind::Playlist, id: "history".into(), title: "Listening history".into(), subtitle: "Your plays and stats".into(), thumb: None, track: None });
    }
    items.retain(|i| matches(&i.title) || matches(&i.subtitle));
    if items.is_empty() {
        ui.label(RichText::new("Library is Empty").font(bold(18.0)));
        ui.label(RichText::new("Save albums, follow artists and create playlists to see them here.").color(SUB));
        return;
    }
    if app.store.settings.library_grid {
        let w = ui.available_width();
        let per = ((w / 190.0).floor() as usize).max(2);
        let cw = w / per as f32 - 4.0;
        for chunk in items.chunks(per) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for it in chunk {
                    let (r, resp) = ui.allocate_exact_size(vec2(cw, cw + 64.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, 8.0, CARD_HOVER);
                    }
                    let img = Rect::from_min_size(r.min + vec2(10.0, 10.0), vec2(cw - 20.0, cw - 20.0));
                    special_cover(ui, app, it, img);
                    text_trunc(ui, pos2(r.left() + 10.0, img.bottom() + 10.0), &it.title, bold(14.0), TEXT, cw - 20.0, Align2::LEFT_TOP);
                    text_trunc(ui, pos2(r.left() + 10.0, img.bottom() + 30.0), &it.subtitle, reg(12.5), SUB, cw - 20.0, Align2::LEFT_TOP);
                    if resp.clicked() {
                        open_library_item(app, it);
                    }
                    resp.context_menu(|ui| library_item_menu(ui, app, it));
                }
            });
        }
    } else {
        for it in &items {
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, 6.0, Color32::from_white_alpha(14));
            }
            let cover = Rect::from_min_size(pos2(r.left() + 8.0, r.top() + 6.0), vec2(52.0, 52.0));
            special_cover(ui, app, it, cover);
            text_trunc(ui, pos2(cover.right() + 14.0, r.center().y - 1.0), &it.title, reg(15.0), TEXT, r.width() - 120.0, Align2::LEFT_BOTTOM);
            text_trunc(ui, pos2(cover.right() + 14.0, r.center().y + 3.0), &it.subtitle, reg(13.0), SUB, r.width() - 120.0, Align2::LEFT_TOP);
            if resp.clicked() {
                open_library_item(app, it);
            }
            resp.context_menu(|ui| library_item_menu(ui, app, it));
        }
    }
}

// ---------------------------------------------------------------- collection pieces

pub struct Header<'a> {
    pub kind: &'a str,
    pub title: &'a str,
    pub line: String,
    pub cover: Option<String>,
    pub round: bool,
    pub special: Option<&'a str>,
}

pub fn header(ui: &mut Ui, app: &mut App, h: Header) -> Rect {
    ui.add_space(20.0);
    let mut cover_rect = Rect::NOTHING;
    ui.horizontal(|ui| {
        let (cr, _) = ui.allocate_exact_size(vec2(220.0, 220.0), Sense::hover());
        cover_rect = cr;
        match h.special {
            Some("liked") => liked_cover(ui, cr, 6.0),
            Some("local") => simple_cover(ui, cr, Color32::from_rgb(0x3A, 0x8D, 0xB5), "♫"),
            Some("downloads") => {
                ui.painter().rect_filled(cr, 6.0, Color32::from_rgb(0x1E, 0x32, 0x64));
                paint_icon(ui.painter(), cr.shrink(70.0), Icon::Download, TEXT);
            }
            _ => paint_cover(ui, &mut app.images, h.cover.as_deref(), cr, if h.round { 110.0 } else { 6.0 }, h.title),
        }
        ui.add_space(20.0);
        ui.vertical(|ui| {
            ui.add_space(70.0);
            ui.label(RichText::new(h.kind).size(14.0).color(TEXT));
            let size = if h.title.chars().count() > 30 { 34.0 } else { 56.0 };
            ui.add(egui::Label::new(RichText::new(h.title).font(bold(size)).color(TEXT)).truncate());
            ui.label(RichText::new(h.line).size(14.0).color(TEXT));
        });
    });
    ui.add_space(18.0);
    cover_rect
}

pub fn action_row(ui: &mut Ui, app: &mut App, tracks: &[Track], context: Item, extra: impl FnOnce(&mut Ui, &mut App)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        let playing_this = !tracks.is_empty() && app.playing_from == context.title && app.status.playing;
        if play_button(ui, 56.0, playing_this).clicked() {
            if playing_this {
                app.actions.push(Action::TogglePlay);
            } else if !tracks.is_empty() {
                if app.shuffle {
                    app.shuffle = false;
                }
                app.actions.push(Action::PlayTracks { tracks: tracks.to_vec(), start: 0, shuffle: false, context: Some(context.clone()) });
            }
        }
        if icon_button(ui, Icon::Shuffle, 40.0, SUB, "Shuffle play").clicked() && !tracks.is_empty() {
            app.actions.push(Action::PlayTracks { tracks: tracks.to_vec(), start: 0, shuffle: true, context: Some(context.clone()) });
        }
        let downloadable: Vec<Track> = tracks.iter().filter(|t| t.video_id().is_some()).cloned().collect();
        if !downloadable.is_empty() {
            let all_done = downloadable.iter().all(|t| app.store.download_for(t).is_some());
            let busy = downloadable.iter().any(|t| app.downloading.iter().any(|d| d.key() == t.key()));
            let tip = if all_done { "Downloaded" } else if busy { "Downloading…" } else { "Download" };
            if icon_button(ui, if all_done { Icon::Downloaded } else { Icon::Download }, 40.0, SUB, tip).clicked() && !all_done {
                app.actions.push(Action::DownloadMany(downloadable));
            }
        }
        extra(ui, app);
    });
    ui.add_space(12.0);
}

fn save_button(ui: &mut Ui, app: &mut App, it: &Item) {
    let saved = app.store.is_saved(&it.kind, &it.id);
    let (r, resp) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::click());
    let c = r.center();
    if saved {
        // Saved: green circle with a check, like the original.
        ui.painter().circle_filled(c, 12.0, ACCENT);
        ui.painter().add(egui::Shape::line(vec![c + vec2(-5.0, 0.0), c + vec2(-1.5, 4.0), c + vec2(5.5, -4.0)], egui::Stroke::new(2.2_f32, Color32::BLACK)));
    } else {
        let col = if resp.hovered() { TEXT } else { SUB };
        ui.painter().circle_stroke(c, 12.0, egui::Stroke::new(1.8_f32, col));
        ui.painter().line_segment([c + vec2(-6.0, 0.0), c + vec2(6.0, 0.0)], egui::Stroke::new(1.8_f32, col));
        ui.painter().line_segment([c + vec2(0.0, -6.0), c + vec2(0.0, 6.0)], egui::Stroke::new(1.8_f32, col));
    }
    if resp.on_hover_text(if saved { "Remove from Library" } else { "Save to Library" }).clicked() {
        app.actions.push(Action::ToggleSaved(it.clone()));
    }
}

pub fn track_list(ui: &mut Ui, app: &mut App, tracks: &[Track], numbered: bool, covers: bool, context: Item, row_ctx: impl Fn(usize) -> RowContext<'static>) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    if numbered {
        ui.painter().text(pos2(r.left() + 22.0, r.center().y), Align2::CENTER_CENTER, "#", reg(13.0), SUB);
    }
    ui.painter().text(pos2(r.left() + if numbered { 56.0 } else { 12.0 }, r.center().y), Align2::LEFT_CENTER, "Title", reg(13.0), SUB);
    ui.painter().text(pos2(r.right() - 62.0, r.center().y), Align2::RIGHT_CENTER, "Time", reg(13.0), SUB);
    ui.painter().line_segment([pos2(r.left(), r.bottom()), pos2(r.right(), r.bottom())], egui::Stroke::new(1.0_f32, Color32::from_white_alpha(20)));
    ui.add_space(6.0);
    // The first songs of an opened list are likely to be played: warm them up.
    for t in tracks.iter().take(2) {
        app.actions.push(Action::Prefetch(t.clone()));
    }
    // Songs can be hidden in any playlist, Liked Songs included.
    let scoped = context.kind == ItemKind::Playlist;
    app.hide_scope = if scoped { Some((context.id.clone(), context.title.clone())) } else { None };
    for (i, t) in tracks.iter().enumerate() {
        if track_row(ui, app, t, RowOpts { number: if numbered { Some(i + 1) } else { None }, show_cover: covers, show_album: covers, ctx: row_ctx(i) }) {
            app.actions.push(Action::PlayTracks { tracks: tracks.to_vec(), start: i, shuffle: false, context: Some(context.clone()) });
        }
    }
    app.hide_scope = None;
}

// ---------------------------------------------------------------- album / artist / playlist

/// What a page looks like for the moment before its songs arrive: the title and cover of the card that
/// was clicked (when known) and grey placeholder rows, so opening feels instant.
fn loading_shell(ui: &mut Ui, app: &mut App, id: &str, kind: &str) {
    let hint = app.page_hints.get(id).cloned();
    ui.add_space(20.0);
    ui.horizontal(|ui| {
        let (cr, _) = ui.allocate_exact_size(vec2(220.0, 220.0), Sense::hover());
        let round = kind == "Artist";
        match &hint {
            Some(h) if h.thumb.is_some() => paint_cover(ui, &mut app.images, h.thumb.as_deref(), cr, if round { 110.0 } else { 6.0 }, &h.title),
            _ => {
                ui.painter().rect_filled(cr, if round { 110.0 } else { 6.0 }, Color32::from_white_alpha(16));
            }
        }
        ui.add_space(20.0);
        ui.vertical(|ui| {
            ui.add_space(70.0);
            ui.label(RichText::new(kind).size(14.0).color(TEXT));
            let title = hint.as_ref().map(|h| h.title.clone()).unwrap_or_default();
            if title.is_empty() {
                let (r, _) = ui.allocate_exact_size(vec2(280.0, 44.0), Sense::hover());
                ui.painter().rect_filled(r.shrink2(vec2(0.0, 6.0)), 6.0, Color32::from_white_alpha(14));
            } else {
                ui.add(egui::Label::new(RichText::new(title).font(bold(if hint.as_ref().map(|h| h.title.chars().count()).unwrap_or(0) > 30 { 34.0 } else { 56.0 })).color(TEXT)).truncate());
            }
        });
    });
    ui.add_space(24.0);
    for _ in 0..8 {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::hover());
        ui.painter().rect_filled(Rect::from_min_size(r.min + vec2(12.0, 8.0), vec2(40.0, 40.0)), 3.0, Color32::from_white_alpha(10));
        ui.painter().rect_filled(Rect::from_min_size(r.min + vec2(64.0, 14.0), vec2(220.0, 12.0)), 4.0, Color32::from_white_alpha(12));
        ui.painter().rect_filled(Rect::from_min_size(r.min + vec2(64.0, 34.0), vec2(120.0, 10.0)), 4.0, Color32::from_white_alpha(8));
    }
}

fn album(ui: &mut Ui, app: &mut App, id: &str) {
    app.ensure_album(id);
    let page = match app.albums.get(id) {
        Some(Loadable::Ready(p)) => p.clone(),
        Some(Loadable::Failed(e)) => {
            let e = e.clone();
            if error_box(ui, &format!("Couldn't load this album. {e}")) {
                app.albums.remove(id);
            }
            return;
        }
        _ => return loading_shell(ui, app, id, "Album"),
    };
    let total: u64 = page.tracks.iter().map(|t| t.duration_ms).sum();
    let kind = page.subtitle.split(" • ").next().filter(|s| !s.is_empty()).unwrap_or("Album").to_string();
    let year = page.subtitle.split(" • ").nth(1).unwrap_or("").to_string();
    let detail = if page.detail.is_empty() { format!("{} songs, {}", page.tracks.len(), format_ms(total)) } else { page.detail.clone() };
    let line = [page.artist.clone(), year, detail].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" • ");
    header(ui, app, Header { kind: &kind, title: &page.title, line, cover: page.thumb.clone(), round: false, special: None });
    let item = Item { kind: ItemKind::Album, id: page.id.clone(), title: page.title.clone(), subtitle: format!("{} • {}", kind, page.artist), thumb: page.thumb.clone(), track: None };
    let it2 = item.clone();
    let artist_id = page.artist_id.clone();
    action_row(ui, app, &page.tracks, item.clone(), move |ui, app| {
        save_button(ui, app, &it2);
        if let Some(aid) = artist_id {
            if outline_button(ui, "Go to artist").clicked() {
                app.actions.push(Action::Navigate(Route::Artist(aid)));
            }
        }
    });
    track_list(ui, app, &page.tracks, true, false, item, |_| RowContext::Plain);
    for (i, s) in page.more.iter().enumerate() {
        shelf(ui, app, &s.title, &s.items, &format!("album_more_{i}"));
    }
}

/// Full-width banner: the artist's picture cropped to fill, a dark fade, the name large at the bottom-left.
fn artist_hero(ui: &mut Ui, app: &mut App, name: &str, listeners: &str, thumb: Option<&str>) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 380.0), Sense::hover());
    let hero = Rect::from_min_max(pos2(rect.left() - 22.0, rect.top() - 10.0), pos2(rect.right() + 22.0, rect.bottom()));
    ui.painter().rect_filled(hero, 0.0, Color32::from_rgb(0x20, 0x20, 0x20));
    let url = thumb.map(banner_thumb);
    let tex = url.as_deref().and_then(|u| app.images.get(u)).map(|l| (l.tex.id(), l.tex.size_vec2()));
    if let Some((id, ts)) = tex {
        let target = hero.size();
        let sc = (target.x / ts.x).max(target.y / ts.y);
        let vis = target / sc;
        let min = pos2((ts.x - vis.x) / 2.0 / ts.x, (ts.y - vis.y) * 0.42 / ts.y);
        let uv = Rect::from_min_size(min, vec2(vis.x / ts.x, vis.y / ts.y));
        ui.painter().image(id, hero, uv, Color32::WHITE);
    }
    let fade = Rect::from_min_max(pos2(hero.left(), hero.top() + hero.height() * 0.35), hero.max);
    gradient(ui, fade, Color32::from_black_alpha(0), Color32::from_black_alpha(215));
    text_trunc(ui, pos2(rect.left(), rect.bottom() - 52.0), name, bold(84.0), TEXT, w - 40.0, Align2::LEFT_BOTTOM);
    if !listeners.is_empty() {
        text_trunc(ui, pos2(rect.left() + 2.0, rect.bottom() - 18.0), listeners, reg(15.0), TEXT, w - 40.0, Align2::LEFT_BOTTOM);
    }
    ui.add_space(8.0);
}

#[derive(Clone, Copy, PartialEq)]
enum MenuIcon {
    Unfollow,
    Follow,
    Block,
    Radio,
    Report,
    Share,
    CopyLink,
    Embed,
}

fn paint_menu_icon(ui: &Ui, c: egui::Pos2, icon: MenuIcon) {
    let p = ui.painter();
    let st = egui::Stroke::new(1.7_f32, SUB);
    match icon {
        MenuIcon::Unfollow => {
            let g = egui::Stroke::new(2.0_f32, ACCENT);
            p.line_segment([c + vec2(-6.0, -6.0), c + vec2(6.0, 6.0)], g);
            p.line_segment([c + vec2(-6.0, 6.0), c + vec2(6.0, -6.0)], g);
        }
        MenuIcon::Follow => {
            p.line_segment([c + vec2(-7.0, 0.0), c + vec2(7.0, 0.0)], st);
            p.line_segment([c + vec2(0.0, -7.0), c + vec2(0.0, 7.0)], st);
        }
        MenuIcon::Block => {
            p.circle_stroke(c, 8.0, st);
            p.line_segment([c + vec2(-5.6, -5.6), c + vec2(5.6, 5.6)], st);
        }
        MenuIcon::Radio => paint_icon(p, Rect::from_center_size(c, vec2(20.0, 20.0)), Icon::Radio, SUB),
        MenuIcon::Report => {
            p.add(egui::Shape::closed_line(vec![c + vec2(0.0, -8.0), c + vec2(8.0, 7.0), c + vec2(-8.0, 7.0)], st));
            p.line_segment([c + vec2(0.0, -2.5), c + vec2(0.0, 2.0)], st);
            p.circle_filled(c + vec2(0.0, 4.6), 1.0, SUB);
        }
        MenuIcon::Share => {
            p.line_segment([c + vec2(0.0, -8.0), c + vec2(0.0, 3.0)], st);
            p.line_segment([c + vec2(0.0, -8.0), c + vec2(-3.5, -4.5)], st);
            p.line_segment([c + vec2(0.0, -8.0), c + vec2(3.5, -4.5)], st);
            p.add(egui::Shape::line(vec![c + vec2(-4.5, -2.0), c + vec2(-7.5, -2.0), c + vec2(-7.5, 8.0), c + vec2(7.5, 8.0), c + vec2(7.5, -2.0), c + vec2(4.5, -2.0)], st));
        }
        MenuIcon::CopyLink => {
            p.rect_stroke(Rect::from_center_size(c + vec2(2.5, -2.5), vec2(11.0, 11.0)), 1.5, st);
            p.add(egui::Shape::line(vec![c + vec2(-2.0, -5.0), c + vec2(-7.5, -5.0), c + vec2(-7.5, 7.0), c + vec2(4.5, 7.0), c + vec2(4.5, 3.0)], st));
        }
        MenuIcon::Embed => {
            p.rect_stroke(Rect::from_center_size(c, vec2(18.0, 15.0)), 2.0, st);
            p.add(egui::Shape::line(vec![c + vec2(-2.0, -3.5), c + vec2(-5.0, 0.0), c + vec2(-2.0, 3.5)], st));
            p.add(egui::Shape::line(vec![c + vec2(2.0, -3.5), c + vec2(5.0, 0.0), c + vec2(2.0, 3.5)], st));
        }
    }
}

/// One row of the artist menu. Returns the response.
fn menu_row(ui: &mut Ui, icon: MenuIcon, label: &str, trailing: Option<&str>) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, 6.0, Color32::from_white_alpha(18));
    }
    paint_menu_icon(ui, pos2(r.left() + 26.0, r.center().y), icon);
    ui.painter().text(pos2(r.left() + 52.0, r.center().y), Align2::LEFT_CENTER, label, reg(15.0), TEXT);
    let c = pos2(r.right() - 20.0, r.center().y);
    match trailing {
        Some("link") => {
            let st = egui::Stroke::new(1.6_f32, SUB);
            ui.painter().add(egui::Shape::line(vec![c + vec2(-3.0, -8.0), c + vec2(-8.0, -8.0), c + vec2(-8.0, 8.0), c + vec2(8.0, 8.0), c + vec2(8.0, 3.0)], st));
            ui.painter().line_segment([c + vec2(-1.0, 1.0), c + vec2(8.0, -8.0)], st);
            ui.painter().line_segment([c + vec2(2.0, -8.0), c + vec2(8.0, -8.0)], st);
            ui.painter().line_segment([c + vec2(8.0, -8.0), c + vec2(8.0, -2.0)], st);
        }
        Some("arrow") => {
            ui.painter().add(egui::Shape::convex_polygon(vec![c + vec2(-3.0, -6.0), c + vec2(5.0, 0.0), c + vec2(-3.0, 6.0)], TEXT, egui::Stroke::NONE));
        }
        _ => {}
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The "⋯" button on an artist page and its menu: follow, don't play, artist radio, report, share.
fn artist_menu(ui: &mut Ui, app: &mut App, it: &Item, seed: Option<Track>, embed_src: Option<String>) {
    let (br, bresp) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::click());
    let open_key = egui::Id::new(("artist_menu_open", &it.id));
    let sub_key = egui::Id::new(("artist_menu_sub", &it.id));
    let rect_key = egui::Id::new(("artist_menu_rect", &it.id));
    let mut open: bool = ui.data(|d| d.get_temp(open_key)).unwrap_or(false);
    let dots = if open || bresp.hovered() { TEXT } else { SUB };
    for k in -1..=1 {
        ui.painter().circle_filled(br.center() + vec2(k as f32 * 7.0, 0.0), 2.0, dots);
    }
    if bresp.clicked() {
        open = !open;
    }
    if !open {
        ui.data_mut(|d| d.insert_temp(open_key, false));
        return;
    }
    let following = app.store.is_saved(&ItemKind::Artist, &it.id);
    let blocked = app.store.artist_blocked(&it.id, &it.title);
    let prev_rects: Option<(Rect, Option<Rect>)> = ui.data(|d| d.get_temp(rect_key));
    let mut share_open: bool = ui.data(|d| d.get_temp(sub_key)).unwrap_or(false);
    let mut act: Option<Action> = None;
    let mut close = false;
    let mut hover_share = false;
    let mut hover_other = false;
    let area = egui::Area::new(open_key).order(egui::Order::Foreground).fixed_pos(pos2(br.left() - 6.0, br.bottom() + 6.0)).show(ui.ctx(), |ui| {
        egui::Frame::none().fill(Color32::from_rgb(0x28, 0x28, 0x28)).rounding(8.0).inner_margin(egui::Margin::same(8.0)).shadow(egui::epaint::Shadow { offset: vec2(0.0, 6.0), blur: 16.0, spread: 0.0, color: Color32::from_black_alpha(140) }).show(ui, |ui| {
            ui.set_width(250.0);
            let r = menu_row(ui, if following { MenuIcon::Unfollow } else { MenuIcon::Follow }, if following { "Unfollow" } else { "Follow" }, None);
            hover_other |= r.hovered();
            if r.clicked() {
                act = Some(Action::ToggleSaved(it.clone()));
                close = true;
            }
            let r = menu_row(ui, MenuIcon::Block, if blocked { "Play this artist again" } else { "Don't play this artist" }, None);
            hover_other |= r.hovered();
            if r.clicked() {
                act = Some(Action::ToggleBlockArtist(it.id.clone(), it.title.clone()));
                close = true;
            }
            let r = menu_row(ui, MenuIcon::Radio, "Go to artist radio", None);
            hover_other |= r.hovered();
            if r.clicked() {
                act = Some(match &seed {
                    Some(s) => Action::StartRadio(s.clone()),
                    None => Action::Toast("There's no song to start a radio from".into()),
                });
                close = true;
            }
            let r = menu_row(ui, MenuIcon::Report, "Report", Some("link"));
            hover_other |= r.hovered();
            if r.clicked() {
                act = Some(Action::OpenUrl("https://support.google.com/youtube/answer/2802027".into()));
                close = true;
            }
            let r = menu_row(ui, MenuIcon::Share, "Share", Some("arrow"));
            hover_share = r.hovered() || r.clicked();
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("artist_menu_share_row"), r.rect));
        });
    });
    // Share opens on hover and stays while the pointer is on it or on its submenu.
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let in_sub = match (pointer, prev_rects.and_then(|p| p.1)) {
        (Some(p), Some(sr)) => sr.expand(4.0).contains(p),
        _ => false,
    };
    if hover_share {
        share_open = true;
    } else if hover_other && !in_sub {
        share_open = false;
    }
    let mut sub_rect: Option<Rect> = None;
    if share_open {
        let row: Rect = ui.ctx().data(|d| d.get_temp(egui::Id::new("artist_menu_share_row"))).unwrap_or(area.response.rect);
        let sub = egui::Area::new(sub_key).order(egui::Order::Foreground).fixed_pos(pos2(area.response.rect.right() + 4.0, row.top() - 8.0)).show(ui.ctx(), |ui| {
            egui::Frame::none().fill(Color32::from_rgb(0x28, 0x28, 0x28)).rounding(8.0).inner_margin(egui::Margin::same(8.0)).shadow(egui::epaint::Shadow { offset: vec2(0.0, 6.0), blur: 16.0, spread: 0.0, color: Color32::from_black_alpha(140) }).show(ui, |ui| {
                ui.set_width(220.0);
                let r = menu_row(ui, MenuIcon::CopyLink, "Copy link to artist", None);
                if r.clicked() {
                    let link = if it.id.starts_with("UC") { format!("https://music.youtube.com/channel/{}", it.id) } else { format!("https://music.youtube.com/browse/{}", it.id) };
                    act = Some(Action::CopyText(link, "Link copied".into()));
                    close = true;
                }
                let r = menu_row(ui, MenuIcon::Embed, "Embed artist", None);
                if r.clicked() {
                    act = Some(match &embed_src {
                        Some(src) => Action::CopyText(
                            format!("<iframe width=\"560\" height=\"315\" src=\"{src}\" title=\"{}\" frameborder=\"0\" allow=\"autoplay; encrypted-media\" allowfullscreen></iframe>", it.title.replace('"', "&quot;")),
                            "Embed code copied".into(),
                        ),
                        None => Action::Toast("Nothing to embed for this artist".into()),
                    });
                    close = true;
                }
            });
        });
        sub_rect = Some(sub.response.rect);
    }
    ui.data_mut(|d| {
        d.insert_temp(rect_key, (area.response.rect, sub_rect));
        d.insert_temp(sub_key, share_open && !close);
    });
    // Closing: a click outside the button and both menus, or Escape.
    let pressed = ui.input(|i| i.pointer.any_pressed());
    if pressed {
        if let Some(p) = pointer {
            let inside = area.response.rect.contains(p) || sub_rect.map(|r| r.contains(p)).unwrap_or(false) || br.contains(p);
            if !inside {
                close = true;
            }
        }
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    if let Some(a) = act {
        app.actions.push(a);
    }
    ui.data_mut(|d| d.insert_temp(open_key, !close));
}

fn artist(ui: &mut Ui, app: &mut App, id: &str) {
    app.ensure_artist(id);
    let page = match app.artists.get(id) {
        Some(Loadable::Ready(p)) => p.clone(),
        Some(Loadable::Failed(e)) => {
            let e = e.clone();
            if error_box(ui, &format!("Couldn't load this artist. {e}")) {
                app.artists.remove(id);
            }
            return;
        }
        _ => return loading_shell(ui, app, id, "Artist"),
    };
    artist_hero(ui, app, &page.name, &page.listeners, page.thumb.as_deref());
    let item = Item { kind: ItemKind::Artist, id: page.id.clone(), title: page.name.clone(), subtitle: "Artist".into(), thumb: page.thumb.clone(), track: None };
    let it2 = item.clone();
    let seed = page.top_songs.first().cloned();
    // What "Embed artist" copies: a player for the artist's radio, or for their top song.
    let embed_src = page
        .radio_playlist
        .as_ref()
        .map(|p| format!("https://www.youtube.com/embed/videoseries?list={p}"))
        .or_else(|| page.top_songs.first().and_then(|t| t.video_id()).map(|v| format!("https://www.youtube.com/embed/{v}")));
    action_row(ui, app, &page.top_songs, item.clone(), move |ui, app| {
        let following = app.store.is_saved(&ItemKind::Artist, &it2.id);
        if outline_button(ui, if following { "Following" } else { "Follow" }).clicked() {
            app.actions.push(Action::ToggleSaved(it2.clone()));
        }
        artist_menu(ui, app, &it2, seed, embed_src);
    });
    if !page.top_songs.is_empty() {
        section_title(ui, "Popular");
        let top: Vec<Track> = page.top_songs.iter().take(10).cloned().collect();
        for t in top.iter().take(2) {
            app.actions.push(Action::Prefetch(t.clone()));
        }
        for (i, t) in top.iter().enumerate() {
            if track_row(ui, app, t, RowOpts { number: Some(i + 1), show_cover: true, show_album: true, ctx: RowContext::Plain }) {
                app.actions.push(Action::PlayTracks { tracks: top.clone(), start: i, shuffle: false, context: Some(item.clone()) });
            }
        }
    }
    for (i, s) in page.sections.iter().enumerate() {
        shelf(ui, app, &s.title, &s.items, &format!("artist_{i}"));
    }
    if !page.description.is_empty() {
        section_title(ui, "About");
        egui::Frame::none().fill(CARD).rounding(8.0).inner_margin(egui::Margin::same(16.0)).show(ui, |ui| {
            ui.set_max_width(760.0);
            ui.label(RichText::new(&page.description).color(SUB).size(14.0));
        });
    }
}

fn playlist(ui: &mut Ui, app: &mut App, id: &str) {
    app.ensure_playlist(id);
    let page = match app.playlists.get(id) {
        Some(Loadable::Ready(p)) => p.clone(),
        Some(Loadable::Failed(e)) => {
            let e = e.clone();
            if error_box(ui, &format!("Couldn't load this playlist. {e}")) {
                app.playlists.remove(id);
            }
            return;
        }
        _ => return loading_shell(ui, app, id, "Playlist"),
    };
    let total: u64 = page.tracks.iter().map(|t| t.duration_ms).sum();
    let owner = if page.owner.is_empty() { String::new() } else { format!("{} • ", page.owner) };
    let line = format!("{owner}{} songs, {}", page.tracks.len(), format_ms(total));
    header(ui, app, Header { kind: "Playlist", title: &page.title, line, cover: page.thumb.clone(), round: false, special: None });
    let item = Item { kind: ItemKind::Playlist, id: page.id.clone(), title: page.title.clone(), subtitle: format!("Playlist • {}", page.owner), thumb: page.thumb.clone(), track: None };
    let mut tracks = page.tracks.clone();
    sort_tracks(&mut tracks, app.playlist_sort);
    let it2 = item.clone();
    action_row(ui, app, &tracks, item.clone(), move |ui, app| {
        save_button(ui, app, &it2);
        sort_menu(ui, app);
    });
    track_list(ui, app, &tracks, true, true, item, |_| RowContext::Plain);
    if page.loading_more {
        spinner_text(ui, "Loading more songs…");
    }
}

fn sort_tracks(tracks: &mut [Track], sort: usize) {
    match sort {
        1 => tracks.sort_by_key(|t| t.title.to_lowercase()),
        2 => tracks.sort_by_key(|t| std::cmp::Reverse(t.title.to_lowercase())),
        _ => {}
    }
}

fn sort_menu(ui: &mut Ui, app: &mut App) {
    let labels = ["Custom order", "Title A-Z", "Title Z-A"];
    egui::ComboBox::from_id_source("sort").selected_text(labels[app.playlist_sort.min(2)]).show_ui(ui, |ui| {
        for (i, l) in labels.iter().enumerate() {
            ui.selectable_value(&mut app.playlist_sort, i, *l);
        }
    });
}

fn user_playlist(ui: &mut Ui, app: &mut App, id: &str) {
    let Some(p) = app.store.playlists.iter().find(|p| p.id == id).cloned() else {
        ui.label("This playlist was deleted.");
        return;
    };
    let total: u64 = p.tracks.iter().map(|t| t.duration_ms).sum();
    let cover = playlist_cover_key(&p);
    let stats = format!("{} songs, {}", p.tracks.len(), format_ms(total));
    let line = if p.description.is_empty() { stats } else { format!("{} • {}", p.description, stats) };
    let cr = header(ui, app, Header { kind: "Playlist", title: &p.name, line, cover, round: false, special: None });
    // The cover is a button: it opens "Edit details", with a pencil on hover.
    let cresp = ui.interact(cr, ui.id().with(("playlist_cover", &p.id)), Sense::click());
    if cresp.hovered() {
        ui.painter().rect_filled(cr, 6.0, Color32::from_black_alpha(150));
        ui.painter().text(cr.center() - vec2(0.0, 14.0), Align2::CENTER_CENTER, "✎", bold(40.0), TEXT);
        ui.painter().text(cr.center() + vec2(0.0, 26.0), Align2::CENTER_CENTER, "Choose photo", bold(15.0), TEXT);
    }
    if cresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        app.ui_edit_playlist = Some(crate::app::EditPlaylist { pid: p.id.clone(), name: p.name.clone(), desc: p.description.clone(), new_cover: None, remove_cover: false });
    }
    let item = Item { kind: ItemKind::Playlist, id: p.id.clone(), title: p.name.clone(), subtitle: "Playlist".into(), thumb: p.tracks.first().and_then(|t| t.thumb.clone()), track: None };
    let pid = p.id.clone();
    let name = p.name.clone();
    let desc = p.description.clone();
    action_row(ui, app, &p.tracks, item.clone(), move |ui, app| {
        if outline_button(ui, "Edit details").clicked() {
            app.ui_edit_playlist = Some(crate::app::EditPlaylist { pid: pid.clone(), name: name.clone(), desc: desc.clone(), new_cover: None, remove_cover: false });
        }
        if outline_button(ui, "Delete").clicked() {
            app.actions.push(Action::DeletePlaylist(pid.clone()));
        }
    });
    if p.tracks.is_empty() {
        ui.label(RichText::new("Let's find something for your playlist").font(bold(18.0)));
        ui.label(RichText::new("Right-click any song and choose “Add to playlist”.").color(SUB));
        return;
    }
    let pid: &'static str = Box::leak(p.id.clone().into_boxed_str());
    track_list(ui, app, &p.tracks, true, true, item, move |i| RowContext::UserPlaylist(pid, i));
}

fn liked(ui: &mut Ui, app: &mut App) {
    let tracks = app.store.liked.clone();
    header(ui, app, Header { kind: "Playlist", title: "Liked Songs", line: format!("{} songs", tracks.len()), cover: None, round: false, special: Some("liked") });
    action_row(ui, app, &tracks, App::liked_item(), |_, _| {});
    if tracks.is_empty() {
        ui.label(RichText::new("Songs you like will appear here").font(bold(18.0)));
        ui.label(RichText::new("Save songs by clicking the heart icon.").color(SUB));
        return;
    }
    track_list(ui, app, &tracks, true, true, App::liked_item(), |_| RowContext::Plain);
}

fn category(ui: &mut Ui, app: &mut App, title: &str, browse_id: &str, params: Option<String>) {
    ui.label(RichText::new(title).font(bold(40.0)).color(TEXT));
    ui.add_space(8.0);
    app.ensure_category(browse_id, params.clone());
    let key = format!("{browse_id}|{}", params.clone().unwrap_or_default());
    match app.category_pages.get(&key) {
        Some(Loadable::Ready(sections)) => {
            let sections = sections.clone();
            if sections.is_empty() {
                ui.label(RichText::new("Nothing here yet").color(SUB));
            }
            for s in &sections {
                section_title(ui, &s.title);
                card_grid(ui, app, &s.items.iter().take(12).cloned().collect::<Vec<_>>());
            }
        }
        Some(Loadable::Failed(e)) => {
            let e = e.clone();
            if error_box(ui, &format!("Couldn't load {title}. {e}")) {
                app.retry_category(browse_id, params);
            }
        }
        _ => spinner_text(ui, "Loading…"),
    }
}
