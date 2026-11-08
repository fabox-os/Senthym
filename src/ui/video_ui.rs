//! Music video in the Now playing panel, its mini player and full-screen view.

use super::widgets::*;
use crate::app::{Action, App, Route};
use egui::{pos2, vec2, Align2, Color32, Rect, Sense, Stroke, Ui};

const PIP_KEY: &str = "video_pip";
const FS_KEY: &str = "video_fs";

/// A music video is showing (or loading) for the current song.
pub fn active(app: &App) -> bool {
    app.store.settings.prefer_video && same_song(app)
}

/// The video session belongs to the song that is playing now (never a leftover from the one before).
fn same_song(app: &App) -> bool {
    match (&app.video, &app.current) {
        (Some(v), Some(t)) => v.track_key == t.key(),
        _ => false,
    }
}

/// The video is off but the current song has one that can be switched to.
pub fn available_off(app: &App) -> bool {
    if app.store.settings.prefer_video {
        return false;
    }
    app.current.as_ref().map(|t| matches!(app.video_ids.get(&t.key()), Some(Some(_)))).unwrap_or(false)
}

fn texture(app: &App) -> Option<egui::TextureId> {
    if !same_song(app) {
        return None;
    }
    app.video.as_ref().and_then(|v| v.tex.as_ref()).map(|t| t.id())
}

fn draw_picture(ui: &mut Ui, app: &mut App, rect: Rect, rounding: f32) {
    ui.painter().rect_filled(rect, rounding, Color32::BLACK);
    match texture(app) {
        Some(id) => {
            egui::Image::new((id, rect.size())).rounding(rounding).paint_at(ui, rect);
        }
        None => {
            // While the picture loads, show the song's cover (dimmed) so the area is never an empty box.
            if let Some(t) = app.current.clone() {
                let side = rect.height();
                let sq = Rect::from_center_size(rect.center(), vec2(side, side));
                let key = t.big_thumb().filter(|k| app.images.get(k).is_some()).or_else(|| track_cover_key(&t));
                paint_cover(ui, &mut app.images, key.as_deref(), sq, 0.0, &t.title);
                ui.painter().rect_filled(rect, rounding, Color32::from_black_alpha(120));
            }
            if app.video.as_ref().map(|v| v.error.is_some()).unwrap_or(false) {
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, "Couldn't load the video", reg(13.0), TEXT);
            } else {
                spinner_arc(ui, rect.center(), 12.0, TEXT);
            }
        }
    }
}

fn fmt_pos(app: &App) -> (u64, u64) {
    let dur = if app.status.duration_ms > 0 { app.status.duration_ms } else { app.current.as_ref().map(|t| t.duration_ms).unwrap_or(0) };
    (app.status.position_ms, dur)
}

/// A thin seek bar over the picture. Click or drag to seek.
fn seek_bar(ui: &mut Ui, app: &mut App, track: Rect, id: &str) {
    let hit = Rect::from_center_size(track.center(), vec2(track.width(), 18.0));
    let resp = ui.interact(hit, ui.id().with(id), Sense::click_and_drag());
    let (pos, dur) = fmt_pos(app);
    let active = resp.hovered() || resp.dragged();
    let line = Rect::from_center_size(track.center(), vec2(track.width(), if active { 5.0 } else { 3.0 }));
    ui.painter().rect_filled(line, 2.0, Color32::from_white_alpha(80));
    let mut frac = if dur > 0 { (pos as f32 / dur as f32).clamp(0.0, 1.0) } else { 0.0 };
    if let Some(p) = resp.interact_pointer_pos() {
        if resp.dragged() || resp.clicked() {
            frac = ((p.x - track.left()) / track.width()).clamp(0.0, 1.0);
            if dur > 0 {
                app.actions.push(Action::Seek((frac as f64 * dur as f64) as u64));
            }
        }
    }
    ui.painter().rect_filled(Rect::from_min_max(line.min, pos2(line.left() + line.width() * frac, line.bottom())), 2.0, TEXT);
    if active {
        ui.painter().circle_filled(pos2(line.left() + line.width() * frac, line.center().y), 6.0, TEXT);
    }
}

fn round_btn(ui: &mut Ui, c: egui::Pos2, id: &str, tip: &str, dark: bool) -> egui::Response {
    let r = Rect::from_center_size(c, vec2(34.0, 34.0));
    let resp = ui.interact(r, ui.id().with(id), Sense::click());
    if dark || resp.hovered() {
        ui.painter().circle_filled(c, 17.0, Color32::from_black_alpha(if dark { 150 } else { 90 }));
    }
    resp.on_hover_text(tip)
}

/// The quality menu: the quality in use, "Auto" with its note, then the fixed qualities.
fn quality_menu(ui: &mut Ui, app: &mut App) {
    ui.set_min_width(250.0);
    let cur = app.store.settings.video_quality;
    let eff = app.video.as_ref().map(|v| v.height).unwrap_or(0);
    let head = if eff > 0 { format!("Video quality • {eff}p") } else { "Video quality".to_string() };
    ui.label(egui::RichText::new(head).font(bold(12.0)).color(TEXT));
    ui.add_space(6.0);
    let rows: [(u32, &str, Option<&str>); 6] = [
        (0, "Auto", Some("Video quality adjusts based on your connection")),
        (1080, "1080p (HD)", None),
        (720, "720p", None),
        (480, "480p", None),
        (360, "360p", None),
        (240, "240p", None),
    ];
    for (q, name, note) in rows {
        let h = if note.is_some() { 58.0 } else { 40.0 };
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(r, 6.0, Color32::from_white_alpha(14));
        }
        let selected = q == cur;
        let col = if selected { ACCENT } else { TEXT };
        let ty = if note.is_some() { r.top() + 16.0 } else { r.center().y };
        ui.painter().text(pos2(r.left() + 8.0, ty), Align2::LEFT_CENTER, name, reg(15.0), col);
        if let Some(n) = note {
            let mut job = egui::text::LayoutJob::simple(n.to_string(), reg(12.0), SUB, r.width() - 56.0);
            job.wrap.max_rows = 2;
            let g = ui.fonts(|f| f.layout_job(job));
            ui.painter().galley(pos2(r.left() + 8.0, r.top() + 28.0), g, SUB);
        }
        if selected {
            let c = pos2(r.right() - 18.0, if note.is_some() { r.top() + 22.0 } else { r.center().y });
            ui.painter().add(egui::Shape::line(vec![c + vec2(-5.0, 0.0), c + vec2(-1.5, 4.0), c + vec2(5.5, -4.0)], Stroke::new(2.0_f32, ACCENT)));
        }
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !selected {
            app.actions.push(Action::SetVideoHeight(q));
        }
    }
}

/// A small "HD" mark on the gear when the picture is 720p or better.
fn hd_badge(ui: &Ui, c: egui::Pos2, app: &App) {
    let eff = app.video.as_ref().map(|v| v.height).unwrap_or(0);
    if eff >= 720 {
        let r = Rect::from_center_size(c + vec2(9.0, -9.0), vec2(16.0, 10.0));
        ui.painter().rect_filled(r, 2.0, TEXT);
        ui.painter().text(r.center(), Align2::CENTER_CENTER, "HD", bold(7.5), Color32::BLACK);
    }
}

fn paint_gear(ui: &Ui, c: egui::Pos2) {
    let st = Stroke::new(1.8_f32, TEXT);
    ui.painter().circle_stroke(c, 6.0, st);
    ui.painter().circle_stroke(c, 2.2, st);
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::TAU / 8.0;
        let d = vec2(a.cos(), a.sin());
        ui.painter().line_segment([c + d * 6.5, c + d * 9.0], Stroke::new(2.4_f32, TEXT));
    }
}

fn paint_pip(ui: &Ui, c: egui::Pos2) {
    let st = Stroke::new(1.8_f32, TEXT);
    ui.painter().rect_stroke(Rect::from_center_size(c, vec2(18.0, 13.0)), 2.0, st);
    ui.painter().rect_filled(Rect::from_min_size(c + vec2(0.0, 0.5), vec2(7.0, 5.0)), 1.0, TEXT);
}

fn paint_fullscreen(ui: &Ui, c: egui::Pos2, exit: bool) {
    let st = Stroke::new(1.8_f32, TEXT);
    let h = 8.0;
    let l = 4.0;
    for (sx, sy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let corner = c + vec2(sx * h, sy * h);
        let (a, b) = if exit { (corner - vec2(sx * l, 0.0), corner - vec2(0.0, sy * l)) } else { (corner - vec2(sx * l, 0.0), corner - vec2(0.0, sy * l)) };
        let _ = exit;
        ui.painter().line_segment([corner, a], st);
        ui.painter().line_segment([corner, b], st);
    }
}

/// The picture with its hover controls, and the "Switch to audio" button under it.
pub fn video_block(ui: &mut Ui, app: &mut App, w: f32, expand_btn: bool) {
    let h = (w * 9.0 / 16.0).round();
    let (r, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    draw_picture(ui, app, r, 8.0);
    let pop_id = ui.id().with("video_quality_popup");
    let popup_open = ui.memory(|m| m.is_popup_open(pop_id));
    let have_picture = texture(app).is_some();
    if have_picture && (ui.rect_contains_pointer(r) || popup_open) {
        super::screens::gradient(ui, Rect::from_min_size(r.min, vec2(w, 64.0)), Color32::from_black_alpha(150), Color32::from_black_alpha(0));
        super::screens::gradient(ui, Rect::from_min_max(pos2(r.left(), r.bottom() - 50.0), r.max), Color32::from_black_alpha(0), Color32::from_black_alpha(170));
        let y = r.top() + 28.0;
        let mut x = r.right() - 26.0;
        if expand_btn {
            let c = pos2(x, y);
            let b = round_btn(ui, c, "vid_expand", "Expand Now Playing view", true);
            paint_resize_icon(ui, c);
            if b.clicked() {
                app.actions.push(Action::Navigate(Route::Player));
            }
            x -= 40.0;
        }
        let c = pos2(x, y);
        let b = round_btn(ui, c, "vid_fs", "Full screen", false);
        paint_fullscreen(ui, c, false);
        if b.clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(FS_KEY), true));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        }
        x -= 40.0;
        let c = pos2(x, y);
        let pip_on: bool = ui.ctx().data(|d| d.get_temp(egui::Id::new(PIP_KEY))).unwrap_or(false);
        let b = round_btn(ui, c, "vid_pip", if pip_on { "Close mini player" } else { "Mini player" }, false);
        paint_pip(ui, c);
        if b.clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(PIP_KEY), !pip_on));
        }
        x -= 40.0;
        let c = pos2(x, y);
        let gear = round_btn(ui, c, "vid_gear", "Quality", false);
        paint_gear(ui, c);
        hd_badge(ui, c, app);
        if gear.clicked() {
            ui.memory_mut(|m| m.toggle_popup(pop_id));
        }
        egui::popup::popup_above_or_below_widget(ui, pop_id, &gear, egui::AboveOrBelow::Below, egui::PopupCloseBehavior::CloseOnClick, |ui| {
    quality_menu(ui, app);
        });
        seek_bar(ui, app, Rect::from_min_max(pos2(r.left() + 12.0, r.bottom() - 14.0), pos2(r.right() - 12.0, r.bottom() - 10.0)), "vid_seek");
    }
    ui.add_space(14.0);
    if switch_pill(ui, "Switch to audio") {
        app.actions.push(Action::SetVideoMode(false));
    }
}

fn paint_resize_icon(ui: &Ui, c: egui::Pos2) {
    let st = Stroke::new(1.7_f32, TEXT);
    let h = 6.0;
    let s = 3.5;
    let a = c + vec2(h, -h);
    let b = c + vec2(-h, h);
    ui.painter().line_segment([a, b], st);
    ui.painter().line_segment([a, a + vec2(-s, 0.0)], st);
    ui.painter().line_segment([a, a + vec2(0.0, s)], st);
    ui.painter().line_segment([b, b + vec2(s, 0.0)], st);
    ui.painter().line_segment([b, b + vec2(0.0, -s)], st);
}

/// The pill button under the picture / cover ("Switch to audio", "Switch to video").
pub fn switch_pill(ui: &mut Ui, label: &str) -> bool {
    let tw = ui.fonts(|f| f.layout_no_wrap(label.to_string(), bold(15.0), TEXT)).size().x;
    let (r, resp) = ui.allocate_exact_size(vec2(tw + 70.0, 44.0), Sense::click());
    ui.painter().rect_filled(r, 22.0, if resp.hovered() { Color32::from_rgb(0x34, 0x34, 0x34) } else { Color32::from_rgb(0x2A, 0x2A, 0x2A) });
    ui.painter().text(pos2(r.left() + 24.0, r.center().y), Align2::CENTER_CENTER, "♫", bold(18.0), TEXT);
    ui.painter().text(pos2(r.left() + 44.0, r.center().y), Align2::LEFT_CENTER, label, bold(15.0), TEXT);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// The floating mini player (bottom-right) and the full-screen view; called every frame.
pub fn overlays(ctx: &egui::Context, app: &mut App) {
    let pip: bool = ctx.data(|d| d.get_temp(egui::Id::new(PIP_KEY))).unwrap_or(false);
    let fs: bool = ctx.data(|d| d.get_temp(egui::Id::new(FS_KEY))).unwrap_or(false);
    if (pip || fs) && !active(app) {
        // The song changed to one without a picture: close both.
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new(PIP_KEY), false);
            d.insert_temp(egui::Id::new(FS_KEY), false);
        });
        if fs {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        }
        return;
    }
    if fs {
        fullscreen(ctx, app);
    } else if pip {
        mini_player(ctx, app);
    }
}

fn mini_player(ctx: &egui::Context, app: &mut App) {
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("video_mini"))
        .order(egui::Order::Foreground)
        .movable(true)
        .default_pos(pos2(screen.right() - 372.0, screen.bottom() - 290.0))
        .show(ctx, |ui| {
            egui::Frame::none().fill(Color32::BLACK).rounding(8.0).stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(40))).shadow(egui::epaint::Shadow { offset: vec2(0.0, 6.0), blur: 18.0, spread: 0.0, color: Color32::from_black_alpha(160) }).show(ui, |ui| {
                let (r, _) = ui.allocate_exact_size(vec2(340.0, 191.0), Sense::hover());
                draw_picture(ui, app, r, 8.0);
                if ui.rect_contains_pointer(r) {
                    super::screens::gradient(ui, Rect::from_min_size(r.min, vec2(r.width(), 44.0)), Color32::from_black_alpha(150), Color32::from_black_alpha(0));
                    let c = pos2(r.right() - 22.0, r.top() + 22.0);
                    let b = round_btn(ui, c, "mini_close", "Close mini player", false);
                    ui.painter().text(c, Align2::CENTER_CENTER, "✕", bold(14.0), TEXT);
                    if b.clicked() {
                        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(PIP_KEY), false));
                    }
                    let c = pos2(r.right() - 62.0, r.top() + 22.0);
                    let b = round_btn(ui, c, "mini_fs", "Full screen", false);
                    paint_fullscreen(ui, c, false);
                    if b.clicked() {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::new(PIP_KEY), false);
                            d.insert_temp(egui::Id::new(FS_KEY), true);
                        });
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
                    }
                    // Play / pause in the middle.
                    let pc = r.center();
                    let pr = Rect::from_center_size(pc, vec2(52.0, 52.0));
                    let pb = ui.interact(pr, ui.id().with("mini_play"), Sense::click());
                    ui.painter().circle_filled(pc, 24.0, Color32::from_black_alpha(150));
                    paint_icon(ui.painter(), pr.shrink(15.0), if app.status.playing { Icon::Pause } else { Icon::Play }, TEXT);
                    if pb.clicked() {
                        app.actions.push(Action::TogglePlay);
                    }
                    seek_bar(ui, app, Rect::from_min_max(pos2(r.left() + 10.0, r.bottom() - 14.0), pos2(r.right() - 10.0, r.bottom() - 10.0)), "mini_seek");
                }
            });
        });
}

fn fullscreen(ctx: &egui::Context, app: &mut App) {
    let screen = ctx.screen_rect();
    let exit = |ctx: &egui::Context| {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(FS_KEY), false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
    };
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        exit(ctx);
        return;
    }
    egui::Area::new(egui::Id::new("video_full")).order(egui::Order::Tooltip).fixed_pos(screen.min).show(ctx, |ui| {
        let (r, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
        ui.painter().rect_filled(r, 0.0, Color32::BLACK);
        // Fit 16:9 inside the screen.
        let (mut w, mut h) = (r.width(), r.width() * 9.0 / 16.0);
        if h > r.height() {
            h = r.height();
            w = h * 16.0 / 9.0;
        }
        let pic = Rect::from_center_size(r.center(), vec2(w, h));
        draw_picture(ui, app, pic, 0.0);
        if resp.clicked() {
            app.actions.push(Action::TogglePlay);
        }
        // Controls show while the pointer has moved in the last couple of seconds.
        let now = ui.input(|i| i.time);
        let moved = ui.input(|i| i.pointer.delta() != vec2(0.0, 0.0) || i.pointer.any_pressed());
        let tkey = egui::Id::new("video_fs_moved");
        if moved {
            ui.ctx().data_mut(|d| d.insert_temp(tkey, now));
        }
        let last: f64 = ui.ctx().data(|d| d.get_temp(tkey)).unwrap_or(now);
        if now - last < 2.5 {
            super::screens::gradient(ui, Rect::from_min_max(pos2(r.left(), r.bottom() - 120.0), r.max), Color32::from_black_alpha(0), Color32::from_black_alpha(200));
            let (pos, dur) = fmt_pos(app);
            let by = r.bottom() - 52.0;
            let pc = pos2(r.left() + 48.0, by);
            let pr = Rect::from_center_size(pc, vec2(44.0, 44.0));
            let pb = ui.interact(pr, ui.id().with("full_play"), Sense::click());
            ui.painter().circle_filled(pc, 20.0, TEXT);
            paint_icon(ui.painter(), pr.shrink(14.0), if app.status.playing { Icon::Pause } else { Icon::Play }, Color32::BLACK);
            if pb.clicked() {
                app.actions.push(Action::TogglePlay);
            }
            ui.painter().text(pos2(r.left() + 92.0, by), Align2::LEFT_CENTER, crate::model::format_ms(pos), reg(14.0), TEXT);
            ui.painter().text(pos2(r.right() - 120.0, by), Align2::RIGHT_CENTER, crate::model::format_ms(dur), reg(14.0), TEXT);
            seek_bar(ui, app, Rect::from_min_max(pos2(r.left() + 160.0, by - 2.0), pos2(r.right() - 200.0, by + 2.0)), "full_seek");
            let ec = pos2(r.right() - 60.0, by);
            let eb = round_btn(ui, ec, "full_exit", "Exit full screen (Esc)", false);
            paint_fullscreen(ui, ec, true);
            if eb.clicked() {
                exit(ui.ctx());
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
    });
}

/// The expanded Now playing page for a music video: the picture fills the page, with the title top-left,
/// controls top-right and the related music videos starting at the bottom edge.
pub fn video_page(ui: &mut Ui, app: &mut App, t: &crate::model::Track) {
    app.ensure_now_related(t);
    let w = ui.available_width();
    let h = (ui.clip_rect().height() - 28.0).max(320.0);
    // The picture runs on behind the related row, so scrolling down never shows a plain background.
    let ready = matches!(app.np_related.get(&t.key()), Some(crate::app::Loadable::Ready(_)));
    let extra = if ready { 175.0 - 22.0 + 24.0 } else { 0.0 };
    let (r, _) = ui.allocate_exact_size(vec2(w, h + extra), Sense::hover());
    let view_bottom = r.top() + h;
    let hero = Rect::from_min_max(pos2(r.left() - 22.0, r.top() - 10.0), pos2(r.right() + 22.0, r.bottom()));
    ui.painter().rect_filled(hero, 0.0, Color32::BLACK);
    match texture(app) {
        Some(id) => {
            // Fill the page: drop black bars baked into the clip, then crop to the page's shape.
            let (bt, bb) = app.video.as_ref().and_then(|v| v.bars).filter(|b| b.0 < 0.99).unwrap_or((0.0, 0.0));
            let (bt, bb) = (if bt > 0.01 { bt } else { 0.0 }, if bb > 0.01 { bb } else { 0.0 });
            let vis_h = (1.0 - bt - bb).max(0.4);
            let pic_aspect = 16.0 / 9.0 / vis_h;
            let (mut u0, mut v0, mut u1, mut v1) = (0.0f32, bt, 1.0f32, 1.0 - bb);
            let page_aspect = hero.width() / hero.height();
            if page_aspect > pic_aspect {
                let vis = pic_aspect / page_aspect;
                let mid = (v0 + v1) / 2.0;
                v0 = mid - vis_h * vis / 2.0;
                v1 = mid + vis_h * vis / 2.0;
            } else {
                let vis = page_aspect / pic_aspect;
                u0 = (1.0 - vis) / 2.0;
                u1 = u0 + vis;
            }
            egui::Image::new((id, hero.size())).uv(Rect::from_min_max(pos2(u0, v0), pos2(u1, v1))).paint_at(ui, hero);
        }
        None => draw_picture(ui, app, hero, 0.0),
    }
    super::screens::gradient(ui, Rect::from_min_size(hero.min, vec2(hero.width(), 90.0)), Color32::from_black_alpha(170), Color32::from_black_alpha(0));
    super::screens::gradient(ui, Rect::from_min_max(pos2(hero.left(), view_bottom - 190.0), hero.max), Color32::from_black_alpha(0), Color32::from_black_alpha(215));
    // Title.
    text_trunc(ui, pos2(hero.left() + 28.0, hero.top() + 36.0), &t.title, bold(18.0), TEXT, hero.width() * 0.5, Align2::LEFT_CENTER);

    // Controls, right to left: collapse, full screen, more, quality, Switch to audio.
    let y = hero.top() + 36.0;
    let mut x = hero.right() - 34.0;
    let c = pos2(x, y);
    let b = round_btn(ui, c, "vp_collapse", "Collapse Now Playing view", false);
    super::paint_resize_pub(ui.painter(), Rect::from_center_size(c, vec2(18.0, 18.0)), TEXT);
    if b.clicked() {
        app.actions.push(if app.back.is_empty() { Action::Navigate(Route::Home) } else { Action::Back });
    }
    x -= 42.0;
    let c = pos2(x, y);
    let b = round_btn(ui, c, "vp_fs", "Full screen", false);
    paint_fullscreen(ui, c, false);
    if b.clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(FS_KEY), true));
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
    }
    x -= 42.0;
    let c = pos2(x, y);
    let more = round_btn(ui, c, "vp_more", "More", false);
    for k in -1..=1 {
        ui.painter().circle_filled(c + vec2(k as f32 * 6.0, 0.0), 1.8, TEXT);
    }
    let more_id = ui.id().with("vp_more_popup");
    if more.clicked() {
        ui.memory_mut(|m| m.toggle_popup(more_id));
    }
    let pip_on: bool = ui.ctx().data(|d| d.get_temp(egui::Id::new(PIP_KEY))).unwrap_or(false);
    egui::popup::popup_above_or_below_widget(ui, more_id, &more, egui::AboveOrBelow::Below, egui::PopupCloseBehavior::CloseOnClick, |ui| {
        ui.set_min_width(150.0);
        if ui.button(if pip_on { "Close mini player" } else { "Mini player" }).clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(PIP_KEY), !pip_on));
        }
    });
    x -= 42.0;
    let c = pos2(x, y);
    let gear = round_btn(ui, c, "vp_gear", "Quality", false);
    paint_gear(ui, c);
    hd_badge(ui, c, app);
    let gear_id = ui.id().with("vp_quality_popup");
    if gear.clicked() {
        ui.memory_mut(|m| m.toggle_popup(gear_id));
    }
    egui::popup::popup_above_or_below_widget(ui, gear_id, &gear, egui::AboveOrBelow::Below, egui::PopupCloseBehavior::CloseOnClick, |ui| {
    quality_menu(ui, app);
    });
    x -= 22.0;
    let tw = ui.fonts(|f| f.layout_no_wrap("Switch to audio".to_string(), bold(14.0), TEXT)).size().x;
    let pill = Rect::from_min_max(pos2(x - tw - 52.0, y - 18.0), pos2(x, y + 18.0));
    let pr = ui.interact(pill, ui.id().with("vp_switch"), Sense::click());
    ui.painter().rect_filled(pill, 18.0, Color32::from_black_alpha(if pr.hovered() { 190 } else { 140 }));
    ui.painter().text(pos2(pill.left() + 20.0, y), Align2::CENTER_CENTER, "♫", bold(16.0), TEXT);
    ui.painter().text(pos2(pill.left() + 38.0, y), Align2::LEFT_CENTER, "Switch to audio", bold(14.0), TEXT);
    if pr.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        app.actions.push(Action::SetVideoMode(false));
    }

    // Related music videos: laid over the bottom of the picture like Spotify (heading, then the row
    // peeking in); scrolling down reveals the whole row.
    let left = hero.left() + hero.width() * 0.2;
    let row_top = view_bottom - 22.0;
    ui.painter().text(pos2(left, row_top - 40.0), Align2::LEFT_CENTER, "Related music videos", bold(22.0), TEXT);
    if let Some(crate::app::Loadable::Ready(vids)) = app.np_related.get(&t.key()).cloned() {
        for (i, v) in vids.iter().enumerate() {
            let cr = Rect::from_min_size(pos2(left + i as f32 * 256.0, row_top), vec2(240.0, 175.0));
            if cr.left() > hero.right() + 240.0 {
                break;
            }
            let resp = ui.interact(cr, ui.id().with(("np_rel", i)), Sense::click());
            let thumb = Rect::from_min_size(cr.min, vec2(240.0, 135.0));
            let key = v.thumb.clone().or_else(|| v.track.as_ref().and_then(track_cover_key));
            paint_cover(ui, &mut app.images, key.as_deref(), thumb, 6.0, &v.title);
            text_trunc(ui, pos2(cr.left(), thumb.bottom() + 8.0), &v.title, reg(14.0), TEXT, 240.0, Align2::LEFT_TOP);
            text_trunc(ui, pos2(cr.left(), thumb.bottom() + 28.0), &v.subtitle, reg(13.0), SUB, 240.0, Align2::LEFT_TOP);
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                app.actions.push(Action::OpenItem(v.clone()));
            }
        }
    }
}
