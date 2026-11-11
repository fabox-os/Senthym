//! Shared drawing pieces: colours, fonts, covers, icons, song rows and cards.

use crate::app::{Action, App};
use crate::images::Images;
use crate::model::*;
use crate::store::AltStream;
use egui::{pos2, vec2, Align2, Color32, FontFamily, FontId, Rect, Response, Rounding, Sense, Stroke, Ui};

pub const BG: Color32 = Color32::from_rgb(0x00, 0x00, 0x00); // outer frame, like the desktop Spotify layout
pub const PANEL: Color32 = Color32::from_rgb(0x12, 0x12, 0x12);
pub const CARD: Color32 = Color32::from_rgb(0x1A, 0x1A, 0x1A);
pub const CARD_HOVER: Color32 = Color32::from_rgb(0x2A, 0x2A, 0x2A); // ColorKt.GridBackground
pub const ACCENT: Color32 = Color32::from_rgb(0x89, 0xCF, 0xF0);
pub const TEXT: Color32 = Color32::WHITE;
pub const SUB: Color32 = Color32::from_rgb(0xB3, 0xB3, 0xB3);
pub const DIM: Color32 = Color32::from_rgb(0x6A, 0x6A, 0x6A);

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}
pub fn reg(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn setup_style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // The fonts come with the program (Noto Sans, and DejaVu Sans for symbols such as ♪), so the text
    // looks the same on Linux, Windows and macOS instead of using whatever the system has.
    fonts.font_data.insert("ui".into(), egui::FontData::from_static(include_bytes!("../../assets/fonts/NotoSans-Regular.ttf")));
    fonts.font_data.insert("ui-bold".into(), egui::FontData::from_static(include_bytes!("../../assets/fonts/NotoSans-Bold.ttf")));
    fonts.font_data.insert("dejavu".into(), egui::FontData::from_static(include_bytes!("../../assets/fonts/DejaVuSans.ttf")));
    let mut prop = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut bold_family = prop.clone();
    prop.insert(0, "ui".into());
    prop.push("dejavu".into());
    bold_family.insert(0, "ui-bold".into());
    bold_family.push("dejavu".into());
    // Scripts and marks these fonts lack (the 【 】 brackets and Chinese, Japanese, Korean text in song
    // titles, more symbols): every system font below that exists is added as a fallback, in this order.
    let system = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\YuGothR.ttc",
        "C:\\Windows\\Fonts\\malgun.ttf",
        "C:\\Windows\\Fonts\\msgothic.ttc",
        "C:\\Windows\\Fonts\\simsun.ttc",
        "C:\\Windows\\Fonts\\seguisym.ttf",
        "C:\\Windows\\Fonts\\segoeui.ttf",
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    ];
    for (i, path) in system.iter().enumerate() {
        if let Ok(b) = std::fs::read(path) {
            let name = format!("system{i}");
            fonts.font_data.insert(name.clone(), egui::FontData::from_owned(b));
            prop.push(name.clone());
            bold_family.push(name);
        }
    }
    fonts.families.insert(FontFamily::Proportional, prop);
    fonts.families.insert(FontFamily::Name("bold".into()), bold_family);
    ctx.set_fonts(fonts);

    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = Color32::from_rgb(0x28, 0x28, 0x28);
    style.visuals.extreme_bg_color = Color32::from_rgb(0x24, 0x24, 0x24);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, SUB);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(0x2A, 0x2A, 0x2A);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x2A, 0x2A, 0x2A);
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x3A, 0x3A, 0x3A);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x3A, 0x3A, 0x3A);
    style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(0x44, 0x44, 0x44);
    style.visuals.selection.bg_fill = Color32::from_rgb(0x3E, 0x7F, 0xA3);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.inactive.rounding = Rounding::same(16.0);
    style.visuals.widgets.hovered.rounding = Rounding::same(16.0);
    style.visuals.widgets.active.rounding = Rounding::same(16.0);
    style.visuals.menu_rounding = Rounding::same(6.0);
    style.visuals.window_rounding = Rounding::same(10.0);
    style.spacing.item_spacing = vec2(8.0, 6.0);
    style.spacing.button_padding = vec2(12.0, 6.0);
    style.spacing.interact_size.y = 26.0;
    style.text_styles.insert(egui::TextStyle::Body, reg(14.0));
    style.text_styles.insert(egui::TextStyle::Button, reg(14.0));
    style.text_styles.insert(egui::TextStyle::Small, reg(12.0));
    style.text_styles.insert(egui::TextStyle::Heading, bold(24.0));
    ctx.set_style(style);
}

/// Text on one line, cut with "…" when too wide.
pub fn text_trunc(ui: &Ui, pos: egui::Pos2, text: &str, font: FontId, color: Color32, max_w: f32, align: Align2) -> Rect {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = egui::text::TextWrapping { max_width: max_w.max(10.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    let galley = ui.fonts(|f| f.layout_job(job));
    let rect = align.anchor_size(pos, galley.size());
    ui.painter().galley(rect.min, galley, color);
    rect
}

fn hash_color(s: &str) -> Color32 {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    let palette = [
        Color32::from_rgb(0x50, 0x9B, 0xF5),
        Color32::from_rgb(0xE8, 0x62, 0x2C),
        Color32::from_rgb(0x8D, 0x67, 0xAB),
        Color32::from_rgb(0x3A, 0x8D, 0xB5),
        Color32::from_rgb(0xE1, 0x33, 0x00),
        Color32::from_rgb(0x1E, 0x32, 0x64),
        Color32::from_rgb(0xBA, 0x5D, 0x07),
        Color32::from_rgb(0x4A, 0x4A, 0xC4),
        Color32::from_rgb(0xAF, 0x28, 0x96),
        Color32::from_rgb(0x14, 0x8A, 0x08),
    ];
    palette[(h % palette.len() as u32) as usize]
}

/// A cover picture, or a coloured placeholder with the first letter while it loads.
pub fn paint_cover(ui: &Ui, images: &mut Images, url: Option<&str>, rect: Rect, rounding: f32, label: &str) {
    if let Some(u) = url {
        if let Some(img) = images.get(u) {
            egui::Image::new((img.tex.id(), rect.size())).rounding(rounding).paint_at(ui, rect);
            return;
        }
    }
    let c = hash_color(label);
    let top = c;
    let bottom = Color32::from_rgb(c.r() / 3, c.g() / 3, c.b() / 3);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    if rounding > 0.0 {
        ui.painter().rect_filled(rect, rounding, bottom);
        ui.painter().rect_filled(rect.shrink(rounding * 0.3), rounding, top.gamma_multiply(0.85));
    } else {
        ui.painter().add(egui::Shape::mesh(mesh));
    }
    let letter: String = label.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "♪".into());
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, letter, bold(rect.height() * 0.42), Color32::from_white_alpha(200));
}

/// The cover to show for a track (local files use the picture inside the file).
/// The cover of a playlist the person made: the picture they chose, else the first song's cover.
pub fn playlist_cover_key(p: &crate::store::UserPlaylist) -> Option<String> {
    match &p.cover {
        Some(c) => Some(format!("img:{c}")),
        None => p.tracks.first().and_then(track_cover_key),
    }
}

/// The add-to-Liked-Songs mark: a circled "+" , or a green check once the song is liked.
pub fn paint_like(p: &egui::Painter, c: egui::Pos2, liked: bool, hot: bool) {
    paint_like_sized(p, c, liked, hot, 9.5);
}

/// Same, with a chosen circle radius (the marks inside scale with it).
pub fn paint_like_sized(p: &egui::Painter, c: egui::Pos2, liked: bool, hot: bool, r: f32) {
    let k = r / 9.5;
    if liked {
        p.circle_filled(c, r, ACCENT);
        p.add(egui::Shape::line(vec![c + vec2(-4.0, 0.0) * k, c + vec2(-1.2, 3.0) * k, c + vec2(4.2, -3.0) * k], Stroke::new(1.9 * k.max(0.8), Color32::BLACK)));
    } else {
        let col = if hot { TEXT } else { SUB };
        let w = 1.5 * k.max(0.8);
        p.circle_stroke(c, r, Stroke::new(w, col));
        p.line_segment([c + vec2(-4.5, 0.0) * k, c + vec2(4.5, 0.0) * k], Stroke::new(w, col));
        p.line_segment([c + vec2(0.0, -4.5) * k, c + vec2(0.0, 4.5) * k], Stroke::new(w, col));
    }
}

pub fn track_cover_key(t: &Track) -> Option<String> {
    match (&t.thumb, &t.source) {
        (Some(u), _) => Some(u.clone()),
        (None, Source::Local(p)) => Some(format!("file:{p}")),
        _ => None,
    }
}

pub fn liked_cover(ui: &Ui, rect: Rect, rounding: f32) {
    let mut mesh = egui::Mesh::default();
    let a = Color32::from_rgb(0x45, 0x0A, 0xF5);
    let b = Color32::from_rgb(0xC4, 0xEF, 0xD9);
    mesh.colored_vertex(rect.left_top(), a);
    mesh.colored_vertex(rect.right_top(), Color32::from_rgb(0x8E, 0x8E, 0xE5));
    mesh.colored_vertex(rect.left_bottom(), Color32::from_rgb(0x8E, 0x8E, 0xE5));
    mesh.colored_vertex(rect.right_bottom(), b);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    ui.painter().add(egui::Shape::mesh(mesh));
    let _ = rounding;
    heart(ui.painter(), rect.center(), rect.height() * 0.32, true, Color32::WHITE);
}

pub fn simple_cover(ui: &Ui, rect: Rect, top: Color32, symbol: &str) {
    ui.painter().rect_filled(rect, 4.0, top);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, symbol, reg(rect.height() * 0.42), Color32::WHITE);
}

// ---------------------------------------------------------------- icons

pub fn heart(p: &egui::Painter, c: egui::Pos2, s: f32, filled: bool, color: Color32) {
    let mut pts = vec![];
    for i in 0..40 {
        let t = i as f32 / 40.0 * std::f32::consts::TAU;
        let x = 16.0 * t.sin().powi(3);
        let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
        pts.push(pos2(c.x + x * s / 17.0, c.y - y * s / 17.0 - s * 0.05));
    }
    if filled {
        p.add(egui::Shape::convex_polygon(pts.clone(), color, Stroke::NONE));
        // The heart isn't convex at the top notch: draw two circles to fill the lobes.
        p.circle_filled(pos2(c.x - s * 0.45, c.y - s * 0.32), s * 0.47, color);
        p.circle_filled(pos2(c.x + s * 0.45, c.y - s * 0.32), s * 0.47, color);
    } else {
        p.add(egui::Shape::closed_line(pts, Stroke::new(1.6_f32, color)));
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Play,
    Pause,
    Next,
    Prev,
    Shuffle,
    Repeat,
    Heart,
    HeartFilled,
    More,
    Queue,
    Lyrics,
    Volume,
    Download,
    Downloaded,
    Back,
    Forward,
    Close,
    Expand,
    Home,
    Search,
    Library,
    Plus,
    Timer,
    Radio,
    Settings,
}

pub fn paint_icon(p: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height()) * 0.5;
    let st = Stroke::new(1.8_f32, color);
    match icon {
        Icon::Play => {
            let pts = vec![pos2(c.x - s * 0.45, c.y - s * 0.6), pos2(c.x + s * 0.6, c.y), pos2(c.x - s * 0.45, c.y + s * 0.6)];
            p.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        Icon::Pause => {
            p.rect_filled(Rect::from_center_size(pos2(c.x - s * 0.3, c.y), vec2(s * 0.3, s * 1.2)), 1.0, color);
            p.rect_filled(Rect::from_center_size(pos2(c.x + s * 0.3, c.y), vec2(s * 0.3, s * 1.2)), 1.0, color);
        }
        Icon::Next | Icon::Prev => {
            let d = if icon == Icon::Next { 1.0 } else { -1.0 };
            let pts = vec![pos2(c.x - d * s * 0.5, c.y - s * 0.55), pos2(c.x + d * s * 0.35, c.y), pos2(c.x - d * s * 0.5, c.y + s * 0.55)];
            p.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
            p.rect_filled(Rect::from_center_size(pos2(c.x + d * s * 0.5, c.y), vec2(s * 0.2, s * 1.1)), 0.5, color);
        }
        Icon::Shuffle => {
            let l = c.x - s * 0.7;
            let r = c.x + s * 0.7;
            let t = c.y - s * 0.45;
            let b = c.y + s * 0.45;
            p.line_segment([pos2(l, b), pos2(r - s * 0.15, t)], st);
            p.line_segment([pos2(l, t), pos2(c.x - s * 0.1, c.y - s * 0.05)], st);
            p.line_segment([pos2(c.x + s * 0.1, c.y + s * 0.1), pos2(r - s * 0.15, b)], st);
            for (y, dy) in [(t, 1.0), (b, -1.0)] {
                p.line_segment([pos2(r, y), pos2(r - s * 0.35, y)], st);
                p.line_segment([pos2(r, y), pos2(r - s * 0.12, y + dy * s * 0.3)], st);
            }
        }
        Icon::Repeat => {
            let r = Rect::from_center_size(c, vec2(s * 1.5, s * 0.95));
            p.rect_stroke(r, s * 0.25, st);
            p.line_segment([pos2(r.right() - s * 0.35, r.top() - s * 0.22), pos2(r.right() - s * 0.1, r.top())], st);
            p.line_segment([pos2(r.right() - s * 0.35, r.top() + s * 0.22), pos2(r.right() - s * 0.1, r.top())], st);
        }
        Icon::Heart => heart(p, c, s * 0.9, false, color),
        Icon::HeartFilled => heart(p, c, s * 0.9, true, color),
        Icon::More => {
            for dx in [-1.0, 0.0, 1.0] {
                p.circle_filled(pos2(c.x + dx * s * 0.5, c.y), s * 0.13, color);
            }
        }
        Icon::Queue => {
            for (i, w) in [(0.0, 1.3), (1.0, 1.3), (2.0, 0.8)] {
                let y = c.y - s * 0.5 + i * s * 0.5;
                p.line_segment([pos2(c.x - s * 0.65, y), pos2(c.x - s * 0.65 + s * w, y)], st);
            }
            let pts = vec![pos2(c.x + s * 0.3, c.y + s * 0.25), pos2(c.x + s * 0.75, c.y + s * 0.5), pos2(c.x + s * 0.3, c.y + s * 0.75)];
            p.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        Icon::Lyrics => {
            p.rect_stroke(Rect::from_center_size(c, vec2(s * 1.4, s * 1.1)), 3.0, st);
            p.line_segment([pos2(c.x - s * 0.4, c.y - s * 0.15), pos2(c.x + s * 0.4, c.y - s * 0.15)], st);
            p.line_segment([pos2(c.x - s * 0.4, c.y + s * 0.2), pos2(c.x + s * 0.15, c.y + s * 0.2)], st);
        }
        Icon::Volume => {
            let pts = vec![pos2(c.x - s * 0.7, c.y - s * 0.25), pos2(c.x - s * 0.35, c.y - s * 0.25), pos2(c.x, c.y - s * 0.6), pos2(c.x, c.y + s * 0.6), pos2(c.x - s * 0.35, c.y + s * 0.25), pos2(c.x - s * 0.7, c.y + s * 0.25)];
            p.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
            p.add(egui::Shape::line(
                (0..=8).map(|i| {
                    let a = -0.8 + i as f32 * 0.2;
                    pos2(c.x + s * 0.15 + s * 0.4 * a.cos(), c.y + s * 0.4 * a.sin())
                }).collect(),
                st,
            ));
        }
        Icon::Download | Icon::Downloaded => {
            let filled = icon == Icon::Downloaded;
            if filled {
                p.circle_filled(c, s * 0.8, ACCENT);
            } else {
                p.circle_stroke(c, s * 0.8, st);
            }
            let col = if filled { Color32::BLACK } else { color };
            let st2 = Stroke::new(1.8_f32, col);
            p.line_segment([pos2(c.x, c.y - s * 0.4), pos2(c.x, c.y + s * 0.35)], st2);
            p.line_segment([pos2(c.x - s * 0.3, c.y + s * 0.05), pos2(c.x, c.y + s * 0.38)], st2);
            p.line_segment([pos2(c.x + s * 0.3, c.y + s * 0.05), pos2(c.x, c.y + s * 0.38)], st2);
        }
        Icon::Back | Icon::Forward => {
            let d = if icon == Icon::Back { -1.0 } else { 1.0 };
            p.line_segment([pos2(c.x - d * s * 0.2, c.y - s * 0.45), pos2(c.x + d * s * 0.25, c.y)], Stroke::new(2.2_f32, color));
            p.line_segment([pos2(c.x - d * s * 0.2, c.y + s * 0.45), pos2(c.x + d * s * 0.25, c.y)], Stroke::new(2.2_f32, color));
        }
        Icon::Close => {
            p.line_segment([pos2(c.x - s * 0.45, c.y - s * 0.45), pos2(c.x + s * 0.45, c.y + s * 0.45)], st);
            p.line_segment([pos2(c.x + s * 0.45, c.y - s * 0.45), pos2(c.x - s * 0.45, c.y + s * 0.45)], st);
        }
        Icon::Expand => {
            p.line_segment([pos2(c.x - s * 0.5, c.y + s * 0.15), pos2(c.x, c.y - s * 0.3)], st);
            p.line_segment([pos2(c.x + s * 0.5, c.y + s * 0.15), pos2(c.x, c.y - s * 0.3)], st);
        }
        Icon::Home => {
            let pts = vec![pos2(c.x, c.y - s * 0.75), pos2(c.x + s * 0.75, c.y - s * 0.05), pos2(c.x + s * 0.55, c.y - s * 0.05), pos2(c.x + s * 0.55, c.y + s * 0.7), pos2(c.x - s * 0.55, c.y + s * 0.7), pos2(c.x - s * 0.55, c.y - s * 0.05), pos2(c.x - s * 0.75, c.y - s * 0.05)];
            p.add(egui::Shape::closed_line(pts, Stroke::new(2.0_f32, color)));
        }
        Icon::Search => {
            p.circle_stroke(pos2(c.x - s * 0.15, c.y - s * 0.15), s * 0.5, Stroke::new(2.0_f32, color));
            p.line_segment([pos2(c.x + s * 0.22, c.y + s * 0.22), pos2(c.x + s * 0.7, c.y + s * 0.7)], Stroke::new(2.4_f32, color));
        }
        Icon::Library => {
            p.line_segment([pos2(c.x - s * 0.6, c.y - s * 0.7), pos2(c.x - s * 0.6, c.y + s * 0.7)], Stroke::new(2.0_f32, color));
            p.line_segment([pos2(c.x - s * 0.15, c.y - s * 0.7), pos2(c.x - s * 0.15, c.y + s * 0.7)], Stroke::new(2.0_f32, color));
            p.line_segment([pos2(c.x + s * 0.2, c.y - s * 0.6), pos2(c.x + s * 0.7, c.y + s * 0.7)], Stroke::new(2.0_f32, color));
        }
        Icon::Plus => {
            p.line_segment([pos2(c.x - s * 0.55, c.y), pos2(c.x + s * 0.55, c.y)], Stroke::new(2.0_f32, color));
            p.line_segment([pos2(c.x, c.y - s * 0.55), pos2(c.x, c.y + s * 0.55)], Stroke::new(2.0_f32, color));
        }
        Icon::Timer => {
            p.circle_stroke(pos2(c.x, c.y + s * 0.08), s * 0.62, st);
            p.line_segment([pos2(c.x, c.y + s * 0.08), pos2(c.x, c.y - s * 0.3)], st);
            p.line_segment([pos2(c.x, c.y + s * 0.08), pos2(c.x + s * 0.28, c.y + s * 0.2)], st);
            p.line_segment([pos2(c.x - s * 0.2, c.y - s * 0.75), pos2(c.x + s * 0.2, c.y - s * 0.75)], st);
        }
        Icon::Radio => {
            p.circle_filled(c, s * 0.18, color);
            for r in [0.45, 0.75] {
                for (a0, a1) in [(-0.9f32, 0.9f32), (std::f32::consts::PI - 0.9, std::f32::consts::PI + 0.9)] {
                    let pts: Vec<_> = (0..=8).map(|i| {
                        let a = a0 + (a1 - a0) * i as f32 / 8.0;
                        pos2(c.x + s * r * a.cos(), c.y + s * r * a.sin())
                    }).collect();
                    p.add(egui::Shape::line(pts, st));
                }
            }
        }
        Icon::Settings => {
            p.circle_stroke(c, s * 0.3, st);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                p.line_segment([pos2(c.x + s * 0.45 * a.cos(), c.y + s * 0.45 * a.sin()), pos2(c.x + s * 0.72 * a.cos(), c.y + s * 0.72 * a.sin())], Stroke::new(2.4_f32, color));
            }
        }
    }
}

/// True when the pointer is over the rect. The bottom and right edges don't count, so when two rows
/// touch and the pointer sits exactly on their shared edge, only the lower row is hovered (never both).
pub fn row_hovered(ui: &Ui, rect: Rect) -> bool {
    if !ui.rect_contains_pointer(rect) {
        return false;
    }
    match ui.input(|i| i.pointer.hover_pos()) {
        Some(p) => p.x >= rect.left() && p.x < rect.right() && p.y >= rect.top() && p.y < rect.bottom(),
        None => false,
    }
}

/// A round-hover icon button.
pub fn icon_button(ui: &mut Ui, icon: Icon, size: f32, color: Color32, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let col = if resp.hovered() { TEXT } else { color };
    if resp.hovered() && size >= 28.0 {
        ui.painter().circle_filled(rect.center(), size * 0.5, Color32::from_white_alpha(14));
    }
    paint_icon(ui.painter(), rect.shrink(size * 0.18), icon, col);
    if !tip.is_empty() {
        resp.on_hover_text(tip)
    } else {
        resp
    }
}

/// The big green round play button.
pub fn play_button(ui: &mut Ui, size: f32, playing: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let r = if resp.hovered() { size * 0.52 } else { size * 0.5 };
    ui.painter().circle_filled(rect.center(), r, ACCENT);
    paint_icon(ui.painter(), rect.shrink(size * 0.27), if playing { Icon::Pause } else { Icon::Play }, Color32::BLACK);
    resp.on_hover_text(if playing { "Pause" } else { "Play" })
}

pub fn pill(ui: &mut Ui, text: &str, selected: bool) -> Response {
    let font = reg(13.0);
    let galley = ui.fonts(|f| f.layout_no_wrap(text.to_string(), font, if selected { Color32::BLACK } else { TEXT }));
    let size = vec2(galley.size().x + 26.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let fill = if selected {
        TEXT
    } else if resp.hovered() {
        Color32::from_rgb(0x3A, 0x3A, 0x3A)
    } else {
        Color32::from_rgb(0x2A, 0x2A, 0x2A)
    };
    ui.painter().rect_filled(rect, 16.0, fill);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, TEXT);
    resp
}

pub fn outline_button(ui: &mut Ui, text: &str) -> Response {
    let font = bold(13.0);
    let galley = ui.fonts(|f| f.layout_no_wrap(text.to_string(), font, TEXT));
    let size = vec2(galley.size().x + 30.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let col = if resp.hovered() { TEXT } else { DIM };
    ui.painter().rect_stroke(rect, 16.0, Stroke::new(1.0_f32, col));
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, TEXT);
    resp
}

pub fn section_title(ui: &mut Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(egui::RichText::new(title).font(bold(22.0)).color(TEXT));
    ui.add_space(4.0);
}

// ---------------------------------------------------------------- song menu

pub enum RowContext<'a> {
    Plain,
    UserPlaylist(&'a str, usize),
    Queue(usize),
    Local,
}

/// The song options sheet (SongOptionsSheet in the original), as a right-click menu.
pub fn song_menu(ui: &mut Ui, app: &mut App, t: &Track, ctx: &RowContext) {
    let liked = app.store.is_liked(t);
    if ui.button(if liked { "Remove from Liked Songs" } else { "Add to Liked Songs" }).clicked() {
        app.actions.push(Action::ToggleLike(t.clone()));
        ui.close_menu();
    }
    ui.menu_button("Add to playlist", |ui| {
        if ui.button("+  New playlist").clicked() {
            app.ui_new_playlist = Some((String::new(), Some(t.clone())));
            ui.close_menu();
        }
        ui.separator();
        let lists: Vec<(String, String)> = app.store.playlists.iter().map(|p| (p.id.clone(), p.name.clone())).collect();
        if lists.is_empty() {
            ui.label(egui::RichText::new("No playlists yet").color(SUB));
        }
        for (id, name) in lists {
            if ui.button(name).clicked() {
                app.actions.push(Action::AddToPlaylist(id, t.clone()));
                ui.close_menu();
            }
        }
    });
    if ui.button("Add to queue").clicked() {
        app.actions.push(Action::AddQueue(t.clone()));
        ui.close_menu();
    }
    // Inside a playlist: keep the song in the list but have playback skip it.
    if let Some((pid, pname)) = app.hide_scope.clone() {
        let hidden = app.store.is_hidden(&pid, &t.key());
        if ui.button(if hidden { "Show in this playlist" } else { "Hide in this playlist" }).clicked() {
            app.actions.push(Action::HideInPlaylist(pid, pname, t.clone(), !hidden));
            ui.close_menu();
        }
    }
    if ui.button("Play next").clicked() {
        app.actions.push(Action::PlayNext(t.clone()));
        ui.close_menu();
    }
    if t.video_id().is_some() && ui.button("Start radio").clicked() {
        app.actions.push(Action::StartRadio(t.clone()));
        ui.close_menu();
    }
    ui.separator();
    if t.video_id().is_some() {
        let downloaded = app.store.download_for(t).is_some();
        let busy = app.downloading.iter().any(|x| x.key() == t.key());
        if busy {
            ui.label(egui::RichText::new("Downloading…").color(SUB));
        } else if downloaded {
            if ui.button("Downloaded — remove").clicked() {
                app.actions.push(Action::RemoveDownload(t.clone()));
                ui.close_menu();
            }
        } else if ui.button("Download").clicked() {
            app.actions.push(Action::Download(t.clone()));
            ui.close_menu();
        }
    }
    if t.album_id.is_some() && ui.button("Go to album").clicked() {
        app.actions.push(Action::GoAlbum(t.clone()));
        ui.close_menu();
    }
    if !t.artist.is_empty() && ui.button("Go to artist").clicked() {
        app.actions.push(Action::GoArtist(t.clone()));
        ui.close_menu();
    }
    if ui.button(if t.video_id().is_some() { "Share (copy link)" } else { "Copy file location" }).clicked() {
        app.actions.push(Action::Share(t.share_link()));
        ui.close_menu();
    }
    ui.menu_button("Alternative stream", |ui| alt_stream_menu(ui, app, t));
    match ctx {
        RowContext::UserPlaylist(pid, i) => {
            ui.separator();
            if ui.button("Remove from this playlist").clicked() {
                app.actions.push(Action::RemoveFromPlaylist(pid.to_string(), *i));
                ui.close_menu();
            }
        }
        RowContext::Queue(i) => {
            ui.separator();
            if ui.button("Remove from queue").clicked() {
                app.actions.push(Action::RemoveFromQueue(*i));
                ui.close_menu();
            }
        }
        RowContext::Local => {
            ui.separator();
            if ui.button("Remove from Local files").clicked() {
                app.actions.push(Action::RemoveLocal(t.clone()));
                ui.close_menu();
            }
        }
        RowContext::Plain => {}
    }
}

pub fn alt_stream_menu(ui: &mut Ui, app: &mut App, t: &Track) {
    match app.store.alt_streams.get(&t.key()) {
        Some(AltStream::YouTube(id)) => {
            ui.label(egui::RichText::new(format!("Current: YouTube video {id}")).color(SUB));
        }
        Some(AltStream::Local(p)) => {
            let name = std::path::Path::new(p).file_name().and_then(|n| n.to_str()).unwrap_or(p);
            ui.label(egui::RichText::new(format!("Current: local file {name}")).color(SUB));
        }
        None => {
            ui.label(egui::RichText::new("No alternative stream set").color(SUB));
        }
    }
    if ui.button("Use YouTube…").clicked() {
        app.ui_alt_link = Some((t.clone(), String::new()));
        ui.close_menu();
    }
    if ui.button("Use local audio file…").clicked() {
        let track = t.clone();
        ui.close_menu();
        if let Some(p) = rfd::FileDialog::new()
            .set_title("Choose an audio file")
            .add_filter("Audio", &crate::local::EXTENSIONS)
            .pick_file()
        {
            app.actions.push(Action::SetAlt(track, AltStream::Local(p.to_string_lossy().into_owned())));
        }
    }
    if app.store.alt_streams.contains_key(&t.key()) && ui.button("Clear alternative stream").clicked() {
        app.actions.push(Action::ClearAlt(t.clone()));
        ui.close_menu();
    }
}

// ---------------------------------------------------------------- song row

pub struct RowOpts<'a> {
    pub number: Option<usize>,
    pub show_cover: bool,
    pub show_album: bool,
    pub ctx: RowContext<'a>,
}

/// One song in a list. Returns true when it was clicked to play.
pub fn track_row(ui: &mut Ui, app: &mut App, t: &Track, opts: RowOpts) -> bool {
    let w = ui.available_width();
    let h = 56.0;
    // No gap between rows, so the hover highlight stays continuous when the pointer moves down.
    let gap = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 0.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    ui.spacing_mut().item_spacing.y = gap;
    let is_current = app.current.as_ref().map(|c| c.key() == t.key()).unwrap_or(false);
    // Hover comes from the pointer position, not from which widget is on top: the heart
    // and menu buttons sit inside the row, and asking the row itself made it flicker.
    let hovered = row_hovered(ui, rect) || resp.context_menu_opened();
    // Hidden in the playlist being shown: stays in the list, greyed out, and playback skips it.
    let hide_pid = app.hide_scope.clone();
    let hidden = hide_pid.as_ref().map(|(pid, _)| app.store.is_hidden(pid, &t.key())).unwrap_or(false);
    let fade = |c: Color32| if hidden { c.gamma_multiply(0.38) } else { c };
    // The grey highlight is only for the row under the pointer; the song playing is marked by its green title.
    if hovered {
        ui.painter().rect_filled(rect, 4.0, Color32::from_white_alpha(16));
    }
    // Songs on screen are lined up to be fetched (the first ones first), so most clicks play at once.
    if ui.is_rect_visible(rect) {
        app.actions.push(Action::Prefetch(t.clone()));
    }
    // Resting the pointer on a song for a moment starts fetching it, so clicking it plays at once.
    {
        let hid = ui.id().with(("hover_since", t.key(), rect.top() as i32));
        if hovered {
            let now = ui.input(|i| i.time);
            let since: f64 = ui.data_mut(|d| *d.get_temp_mut_or_insert_with(hid, || now));
            if now - since > 0.1 {
                app.actions.push(Action::PrefetchHot(t.clone()));
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(220));
            }
        } else {
            ui.data_mut(|d| d.remove::<f64>(hid));
        }
    }
    let mut x = rect.left() + 12.0;
    if let Some(n) = opts.number {
        let col = if is_current { ACCENT } else { SUB };
        if hovered {
            paint_icon(ui.painter(), Rect::from_center_size(pos2(x + 10.0, rect.center().y), vec2(16.0, 16.0)), if is_current && app.status.playing { Icon::Pause } else { Icon::Play }, TEXT);
        } else if is_current && app.status.playing {
            // Animated equalizer bars while it plays; when paused the number stays, in green.
            let t = ui.input(|i| i.time) as f32;
            let playing = app.status.playing;
            for k in 0..3 {
                let hgt = if playing { 4.0 + 9.0 * (0.5 + 0.5 * (t * 6.0 + k as f32 * 2.1).sin()) } else { 7.0 };
                let bx = x + 4.0 + k as f32 * 5.0;
                ui.painter().rect_filled(Rect::from_min_max(pos2(bx, rect.center().y + 7.0 - hgt), pos2(bx + 3.0, rect.center().y + 7.0)), 1.0, ACCENT);
            }
            if playing {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
            }
        } else {
            ui.painter().text(pos2(x + 10.0, rect.center().y), Align2::CENTER_CENTER, n.to_string(), reg(15.0), col);
        }
        x += 32.0;
    }
    if opts.show_cover {
        let cr = Rect::from_min_size(pos2(x, rect.top() + 8.0), vec2(40.0, 40.0));
        let key = track_cover_key(t);
        paint_cover(ui, &mut app.images, key.as_deref(), cr, 3.0, &t.title);
        if hidden {
            ui.painter().rect_filled(cr, 3.0, Color32::from_black_alpha(150));
        }
        x += 52.0;
    }
    let right_reserved = if t.duration_ms > 0 { 150.0 } else { 80.0 };
    let album_w = if opts.show_album && w > 760.0 { (w * 0.28).min(320.0) } else { 0.0 };
    let title_w = (rect.right() - right_reserved - album_w - x - 12.0).max(60.0);
    let title_col = fade(if is_current { ACCENT } else { TEXT });
    text_trunc(ui, pos2(x, rect.center().y - 1.0), &t.title, reg(15.0), title_col, title_w, Align2::LEFT_BOTTOM);
    let sub = if app.store.download_for(t).is_some() { format!("Downloaded · {}", t.artist) } else { t.artist.clone() };
    text_trunc(ui, pos2(x, rect.center().y + 2.0), if sub.is_empty() { "Unknown artist" } else { &sub }, reg(13.0), fade(SUB), title_w, Align2::LEFT_TOP);
    if album_w > 0.0 {
        let ax = rect.right() - right_reserved - album_w;
        text_trunc(ui, pos2(ax, rect.center().y), &t.album, reg(13.0), fade(SUB), album_w - 16.0, Align2::LEFT_CENTER);
    }
    // Right side: heart, duration, more.
    let liked = app.store.is_liked(t);
    let heart_rect = Rect::from_center_size(pos2(rect.right() - 128.0, rect.center().y), vec2(26.0, 26.0));
    let mut clicked_other = false;
    let hr = ui.interact(heart_rect, ui.id().with(("heart", t.key(), rect.top() as i32)), Sense::click());
    if liked || hovered {
        paint_like(ui.painter(), heart_rect.center(), liked, hr.hovered());
    }
    if hr.clicked() {
        app.actions.push(Action::ToggleLike(t.clone()));
        clicked_other = true;
    }
    if t.duration_ms > 0 {
        ui.painter().text(pos2(rect.right() - 62.0, rect.center().y), Align2::RIGHT_CENTER, format_ms(t.duration_ms), reg(13.0), fade(SUB));
    }
    // A hidden song shows a red "−" on hover: click it to show the song in the playlist again.
    if hidden && hovered {
        if let Some((pid, pname)) = hide_pid.clone() {
            let mr = Rect::from_center_size(pos2(heart_rect.center().x - 30.0, rect.center().y), vec2(26.0, 26.0));
            let mresp = ui.interact(mr, ui.id().with(("unhide", t.key(), rect.top() as i32)), Sense::click());
            let red = Color32::from_rgb(0xE9, 0x5D, 0x5D);
            let c = mr.center();
            ui.painter().circle_stroke(c, 7.5, Stroke::new(1.5_f32, red));
            ui.painter().line_segment([c + vec2(-3.5, 0.0), c + vec2(3.5, 0.0)], Stroke::new(1.5_f32, red));
            if mresp.on_hover_text("Show in this playlist").clicked() {
                app.actions.push(Action::HideInPlaylist(pid, pname, t.clone(), false));
                clicked_other = true;
            }
        }
    }
    let more_rect = Rect::from_center_size(pos2(rect.right() - 30.0, rect.center().y), vec2(30.0, 28.0));
    let ctx = opts.ctx;
    // The "⋯" button lives in a child area that doesn't move the list's cursor. (Placing it with
    // `allocate_ui_at_rect` pulled the cursor back up to the button's bottom edge, so the next row
    // started 5 px inside this one: both rows counted as hovered and the overlap showed as a bright line.)
    let mut more_ui = ui.child_ui(more_rect, egui::Layout::left_to_right(egui::Align::Center), None);
    more_ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    more_ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::NONE;
    more_ui.visuals_mut().widgets.inactive.fg_stroke = Stroke::new(1.0_f32, if hovered { TEXT } else { DIM });
    let more = more_ui
        .menu_button(egui::RichText::new("   ").font(reg(13.0)), |ui| {
            ui.set_min_width(210.0);
            song_menu(ui, app, t, &ctx);
        })
        .response;
    // Three dots drawn as shapes: the "⋯" character is missing from the Windows and macOS system fonts
    // and showed as an empty square there.
    paint_icon(ui.painter(), Rect::from_center_size(more_rect.center(), vec2(16.0, 16.0)), Icon::More, if hovered || more.hovered() { TEXT } else { DIM });
    if more.clicked() || more.hovered() {
        clicked_other = clicked_other || more.clicked();
    }
    resp.context_menu(|ui| song_menu(ui, app, t, &ctx));
    (resp.clicked() || resp.double_clicked()) && !clicked_other && !hidden
}

/// The page a card opens, when it is one we can load ahead of the click.
pub fn item_route(it: &Item) -> Option<crate::app::Route> {
    use crate::app::Route;
    if it.id == "liked" || it.id == "local" || it.id.starts_with("sp:") || it.id.starts_with("dz:") || it.track.is_some() {
        return None;
    }
    match it.kind {
        ItemKind::Playlist => Some(Route::Playlist(it.id.clone())),
        ItemKind::Album => Some(Route::Album(it.id.clone())),
        ItemKind::Artist => Some(Route::Artist(it.id.clone())),
        _ => None,
    }
}

/// Resting the pointer on a card or row for a moment starts loading the page it opens.
pub fn hover_prefetch(ui: &mut Ui, app: &mut App, id_salt: &str, hovered: bool, route: Option<crate::app::Route>) {
    let Some(route) = route else { return };
    let hid = ui.id().with(("page_hover", id_salt));
    if hovered {
        let now = ui.input(|i| i.time);
        let since: f64 = ui.data_mut(|d| *d.get_temp_mut_or_insert_with(hid, || now));
        if now - since > 0.12 {
            app.actions.push(Action::PrefetchRoute(route));
        } else {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(130));
        }
    } else {
        ui.data_mut(|d| d.remove::<f64>(hid));
    }
}

/// A square card (album, playlist, artist, song). Returns (open clicked, play clicked).
pub fn card(ui: &mut Ui, app: &mut App, it: &Item, width: f32) -> (bool, bool) {
    let h = width + 64.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, h), Sense::click());
    let hovered = ui.rect_contains_pointer(rect);
    hover_prefetch(ui, app, &format!("card_{}_{}", it.id, rect.left() as i32), hovered, item_route(it));
    // A song card: start fetching the song while the pointer rests on it.
    if hovered {
        if let Some(t) = &it.track {
            app.actions.push(Action::Prefetch(t.clone()));
        }
    }
    if hovered {
        ui.painter().rect_filled(rect, 8.0, CARD_HOVER);
    }
    let pad = 10.0;
    let img = Rect::from_min_size(rect.min + vec2(pad, pad), vec2(width - pad * 2.0, width - pad * 2.0));
    let round = if it.kind == ItemKind::Artist { img.width() / 2.0 } else { 6.0 };
    if it.id == "liked" {
        liked_cover(ui, img, round);
    } else {
        let key = it.thumb.clone().or_else(|| it.track.as_ref().and_then(track_cover_key));
        paint_cover(ui, &mut app.images, key.as_deref(), img, round, &it.title);
    }
    let mut play = false;
    let pr = Rect::from_center_size(img.right_bottom() - vec2(26.0, 26.0), vec2(44.0, 44.0));
    let p = ui.interact(pr, ui.id().with(("cardplay", &it.id, rect.left() as i32)), Sense::click());
    if hovered {
        ui.painter().circle_filled(pr.center(), if p.hovered() { 23.5 } else { 22.0 }, ACCENT);
        paint_icon(ui.painter(), pr.shrink(13.0), Icon::Play, Color32::BLACK);
        if p.clicked() {
            play = true;
        }
    }
    text_trunc(ui, pos2(rect.left() + pad, img.bottom() + 10.0), &it.title, bold(14.0), TEXT, width - pad * 2.0, Align2::LEFT_TOP);
    let sub = if it.subtitle.is_empty() { it.kind.label().to_string() } else { it.subtitle.clone() };
    let mut job = egui::text::LayoutJob::simple(sub, reg(12.5), SUB, width - pad * 2.0);
    job.wrap.max_rows = 2;
    job.wrap.overflow_character = Some('…');
    let galley = ui.fonts(|f| f.layout_job(job));
    ui.painter().galley(pos2(rect.left() + pad, img.bottom() + 30.0), galley, SUB);
    let tr = it.track.clone();
    resp.context_menu(|ui| {
        if let Some(t) = tr {
            song_menu(ui, app, &t, &RowContext::Plain);
        } else {
            if ui.button("Play").clicked() {
                app.actions.push(Action::PlayItem(it.clone(), false));
                ui.close_menu();
            }
            if ui.button("Shuffle play").clicked() {
                app.actions.push(Action::PlayItem(it.clone(), true));
                ui.close_menu();
            }
            if matches!(it.kind, ItemKind::Album | ItemKind::Playlist | ItemKind::Artist) && it.id != "liked" {
                let saved = app.store.is_saved(&it.kind, &it.id);
                let label = match (&it.kind, saved) {
                    (ItemKind::Artist, true) => "Unfollow",
                    (ItemKind::Artist, false) => "Follow",
                    (_, true) => "Remove from Library",
                    (_, false) => "Add to Library",
                };
                if ui.button(label).clicked() {
                    app.actions.push(Action::ToggleSaved(it.clone()));
                    ui.close_menu();
                }
            }
        }
    });
    (resp.clicked() && !play, play)
}

/// A horizontal row of cards with a title and a "Show all" link that opens the whole shelf as a grid.
pub fn shelf(ui: &mut Ui, app: &mut App, title: &str, items: &[Item], id: &str) {
    shelf_labeled(ui, app, None, title, items, id);
}

pub fn shelf_labeled(ui: &mut Ui, app: &mut App, label: Option<&str>, title: &str, items: &[Item], id: &str) {
    shelf_with(ui, app, label, title, items.len(), id, |ui, app, i, w| {
        let it = &items[i];
        let (open_it, play) = card(ui, app, it, w);
        if open_it {
            app.actions.push(Action::OpenItem(it.clone()));
        }
        if play {
            app.actions.push(Action::PlayItem(it.clone(), false));
        }
    });
}

/// Shelf frame (small grey label, bold title, "Show all") around any card drawing closure.
pub fn shelf_with(ui: &mut Ui, app: &mut App, label: Option<&str>, title: &str, n: usize, id: &str, mut draw: impl FnMut(&mut Ui, &mut App, usize, f32)) {
    if n == 0 {
        return;
    }
    let key = egui::Id::new(("shelf_open", id));
    let mut open: bool = ui.data(|d| d.get_temp(key)).unwrap_or(false);
    ui.add_space(20.0);
    if let Some(l) = label {
        ui.label(egui::RichText::new(l).size(11.0).color(SUB));
    }
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).font(bold(22.0)).color(TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if n > 5 {
                let t = if open { "Show less" } else { "Show all" };
                let r = ui.add(egui::Label::new(egui::RichText::new(t).font(bold(13.0)).color(SUB)).sense(Sense::click()));
                if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    open = !open;
                }
            }
        });
    });
    ui.add_space(4.0);
    if open {
        let w = ui.available_width();
        let per_row = ((w / 176.0).floor() as usize).max(2);
        let cw = w / per_row as f32 - 4.0;
        let mut i = 0;
        while i < n {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for _ in 0..per_row {
                    if i < n {
                        draw(ui, app, i, cw);
                        i += 1;
                    }
                }
            });
        }
    } else {
        egui::ScrollArea::horizontal().id_source(id).auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for i in 0..n {
                    draw(ui, app, i, 172.0);
                }
            });
        });
    }
    ui.data_mut(|d| d.insert_temp(key, open));
}

/// A "RADIO" station card: pastel square, round cover, click starts a radio from the song.
pub fn radio_card(ui: &mut Ui, app: &mut App, t: &Track, width: f32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, width + 56.0), Sense::click());
    let hovered = ui.rect_contains_pointer(rect);
    if hovered {
        ui.painter().rect_filled(rect, 8.0, CARD_HOVER);
    }
    let pad = 10.0;
    let img = Rect::from_min_size(rect.min + vec2(pad, pad), vec2(width - pad * 2.0, width - pad * 2.0));
    let pastel = [Color32::from_rgb(0xF5, 0x9B, 0xC0), Color32::from_rgb(0xFF, 0xA0, 0x7A), Color32::from_rgb(0x8F, 0xE3, 0xA4), Color32::from_rgb(0xFF, 0xD9, 0x4D)];
    let h: usize = t.artist.bytes().map(|b| b as usize).sum();
    ui.painter().rect_filled(img, 6.0, pastel[h % pastel.len()]);
    ui.painter().text(img.right_top() + vec2(-10.0, 10.0), Align2::RIGHT_TOP, "RADIO", bold(11.0), Color32::BLACK);
    let d = img.width() * 0.62;
    let cover = Rect::from_center_size(img.center() + vec2(0.0, 8.0), vec2(d, d));
    let key = track_cover_key(t);
    paint_cover(ui, &mut app.images, key.as_deref(), cover, d / 2.0, &t.artist);
    if hovered {
        let pr = Rect::from_center_size(img.right_bottom() - vec2(26.0, 26.0), vec2(44.0, 44.0));
        ui.painter().circle_filled(pr.center(), 22.0, ACCENT);
        paint_icon(ui.painter(), pr.shrink(13.0), Icon::Play, Color32::BLACK);
    }
    text_trunc(ui, pos2(rect.left() + pad, img.bottom() + 10.0), &format!("{} Radio", t.artist), bold(14.0), TEXT, width - pad * 2.0, Align2::LEFT_TOP);
    text_trunc(ui, pos2(rect.left() + pad, img.bottom() + 30.0), &format!("With {} and more", t.title), reg(12.5), SUB, width - pad * 2.0, Align2::LEFT_TOP);
    resp.clicked()
}

/// A grid of cards that wraps to the available width.
pub fn card_grid(ui: &mut Ui, app: &mut App, items: &[Item]) {
    let w = ui.available_width();
    let per_row = ((w / 190.0).floor() as usize).max(2);
    let cw = w / per_row as f32 - 4.0;
    for chunk in items.chunks(per_row) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for it in chunk {
                let (open, play) = card(ui, app, it, cw);
                if open {
                    app.actions.push(Action::OpenItem(it.clone()));
                }
                if play {
                    app.actions.push(Action::PlayItem(it.clone(), false));
                }
            }
        });
    }
}

/// A turning arc drawn around an exact centre (egui's spinner sits off-centre in small spots).
pub fn spinner_arc(ui: &Ui, c: egui::Pos2, r: f32, color: Color32) {
    let t = ui.input(|i| i.time) as f32;
    let start = t * 6.0;
    let pts: Vec<egui::Pos2> = (0..=24).map(|i| {
        let a = start + i as f32 / 24.0 * std::f32::consts::PI * 1.5;
        pos2(c.x + r * a.cos(), c.y + r * a.sin())
    }).collect();
    ui.painter().add(egui::Shape::line(pts, Stroke::new(2.5_f32, color)));
    ui.ctx().request_repaint();
}

pub fn spinner_text(ui: &mut Ui, text: &str) {
    ui.add_space(30.0);
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new().size(18.0).color(ACCENT));
        ui.label(egui::RichText::new(text).color(SUB));
    });
}

pub fn error_box(ui: &mut Ui, text: &str) -> bool {
    ui.add_space(30.0);
    ui.label(egui::RichText::new(text).color(SUB));
    ui.add_space(6.0);
    outline_button(ui, "Try again").clicked()
}

/// Seek/volume bar. Returns the new fraction while dragged or clicked.
pub fn bar(ui: &mut Ui, width: f32, frac: f32, id: &str) -> Option<f32> {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 16.0), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), vec2(width, 4.0));
    let active = resp.hovered() || resp.dragged();
    ui.painter().rect_filled(track, 2.0, Color32::from_rgb(0x4D, 0x4D, 0x4D));
    let mut f = frac.clamp(0.0, 1.0);
    let mut out = None;
    if let Some(p) = resp.interact_pointer_pos() {
        if resp.dragged() || resp.clicked() || resp.drag_stopped() {
            f = ((p.x - track.left()) / track.width()).clamp(0.0, 1.0);
            out = Some(f);
        }
    }
    let fill = Rect::from_min_max(track.min, pos2(track.left() + track.width() * f, track.max.y));
    ui.painter().rect_filled(fill, 2.0, if active { ACCENT } else { TEXT });
    if active {
        ui.painter().circle_filled(pos2(fill.right(), track.center().y), 6.0, TEXT);
    }
    let _ = id;
    out
}
