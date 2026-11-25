//! Blip's regular windows: a normal system window (title bar, close button, moves and resizes
//! like any other) whose inside is laid out with egui and drawn on the CPU by `soft.rs`.
//! Also the small kit of Blip-styled widgets the pages are built from (cards, toggles, chips,
//! buttons, pixel icons, pet avatars), matching `blip/kit.py`.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use blip_pet::pixfont;
use blip_pet::sprites::{self, build_egg, build_pet, Color, PetLook, Sprite, Stage};
use blip_ui::theme::Theme;
use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Pos2, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Icon, Window, WindowId};

use crate::soft::Painter;

pub struct GuiWin {
    pub window: Arc<Window>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    _sb: softbuffer::Context<Arc<Window>>,
    pub ctx: egui::Context,
    state: egui_winit::State,
    painter: Painter,
    pub next_paint: Option<Instant>,
    pub closed: bool,
    dark: Option<bool>,
    shown: bool,
    raise_until: Option<Instant>,
    buf: Vec<u32>,
    last_pos: Pos2,
}

/// The system UI font files (regular, bold) with their collection index.
fn font_files() -> (Vec<(String, u32)>, Vec<(String, u32)>) {
    if cfg!(windows) {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        let win = |f: &str| (format!("{windir}\\Fonts\\{f}"), 0);
        (vec![win("segoeui.ttf"), win("arial.ttf")], vec![win("seguisb.ttf"), win("segoeuib.ttf"), win("arialbd.ttf")])
    } else {
        (
            vec![("/System/Library/Fonts/SFNS.ttf".into(), 0), ("/System/Library/Fonts/Helvetica.ttc".into(), 0)],
            vec![("/System/Library/Fonts/Helvetica.ttc".into(), 1), ("/System/Library/Fonts/Supplemental/Arial Bold.ttf".into(), 0)],
        )
    }
}

fn first_font(list: &[(String, u32)]) -> Option<egui::FontData> {
    for (p, i) in list {
        if let Ok(bytes) = std::fs::read(p) {
            let mut fd = egui::FontData::from_owned(bytes);
            fd.index = *i;
            return Some(fd);
        }
    }
    None
}

fn setup_fonts(ctx: &egui::Context) {
    let mut defs = egui::FontDefinitions::default();
    let (reg, bold) = font_files();
    let fallback: Vec<String> = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    if let Some(fd) = first_font(&reg) {
        defs.font_data.insert("system".into(), Arc::new(fd));
        defs.families.entry(FontFamily::Proportional).or_default().insert(0, "system".into());
    }
    let mut bold_list = vec![];
    if let Some(fd) = first_font(&bold) {
        defs.font_data.insert("system-bold".into(), Arc::new(fd));
        bold_list.push("system-bold".to_string());
    } else if defs.font_data.contains_key("system") {
        bold_list.push("system".to_string());
    }
    bold_list.extend(fallback);
    defs.families.insert(FontFamily::Name("bold".into()), bold_list);
    ctx.set_fonts(defs);
}

pub fn c(col: Color) -> Color32 {
    Color32::from_rgb((col >> 16) as u8, (col >> 8) as u8, col as u8)
}

fn visuals(t: &Theme) -> egui::Visuals {
    let mut v = if t.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = c(t.bg);
    v.window_fill = c(t.surface);
    v.extreme_bg_color = c(t.tile);
    v.text_edit_bg_color = Some(c(t.tile));
    v.faint_bg_color = c(t.hover);
    v.override_text_color = Some(c(t.ink));
    v.weak_text_color = Some(c(t.muted));
    v.hyperlink_color = c(t.accent);
    v.selection.bg_fill = c(t.accent).gamma_multiply(0.4);
    v.selection.stroke = Stroke::new(1.0, c(t.accent));
    let r = CornerRadius::same(8);
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, c(t.line));
    w.noninteractive.fg_stroke = Stroke::new(1.0, c(t.ink));
    w.noninteractive.corner_radius = r;
    for (wv, bg, line) in [(&mut w.inactive, t.tile, t.line), (&mut w.hovered, t.hover, t.accent), (&mut w.active, t.hover, t.accent), (&mut w.open, t.hover, t.line)] {
        wv.bg_fill = c(bg);
        wv.weak_bg_fill = c(bg);
        wv.bg_stroke = Stroke::new(1.0, c(line));
        wv.fg_stroke = Stroke::new(1.0, c(t.ink));
        wv.corner_radius = r;
        wv.expansion = 0.0;
    }
    v.window_corner_radius = CornerRadius::same(12);
    v
}

fn style(ctx: &egui::Context) {
    ctx.all_styles_mut(|s| {
        use egui::TextStyle::*;
        s.text_styles.insert(Body, FontId::proportional(14.0));
        s.text_styles.insert(Button, FontId::proportional(14.0));
        s.text_styles.insert(Small, FontId::proportional(12.0));
        s.text_styles.insert(Heading, FontId::new(18.0, FontFamily::Name("bold".into())));
        s.text_styles.insert(Monospace, FontId::monospace(13.0));
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(12.0, 6.0);
        s.spacing.interact_size.y = 30.0;
        s.spacing.slider_rail_height = 6.0;
        s.spacing.scroll = egui::style::ScrollStyle::thin();
        s.interaction.selectable_labels = false;
    });
}

impl GuiWin {
    pub fn new(el: &ActiveEventLoop, title: &str, size: (f64, f64), at: (f64, f64), t: &Theme, icon: Option<(Vec<u8>, u32)>) -> Option<GuiWin> {
        let mut attrs = Window::default_attributes()
            .with_title(title)
            .with_inner_size(LogicalSize::new(size.0, size.1))
            .with_min_inner_size(LogicalSize::new(size.0.min(360.0), 240.0))
            .with_visible(false)
            .with_theme(Some(if t.dark { winit::window::Theme::Dark } else { winit::window::Theme::Light }));
        if let Some((rgba, n)) = icon {
            attrs = attrs.with_window_icon(Icon::from_rgba(rgba, n, n).ok());
        }
        let window = Arc::new(el.create_window(attrs).ok()?);
        // `at` is the top-left corner (physical pixels), already kept on screen by the caller
        let s = window.scale_factor();
        window.set_outer_position(PhysicalPosition::new(at.0, at.1));
        let sb = softbuffer::Context::new(window.clone()).ok()?;
        let surface = softbuffer::Surface::new(&sb, window.clone()).ok()?;
        let ctx = egui::Context::default();
        setup_fonts(&ctx);
        style(&ctx);
        let state = egui_winit::State::new(ctx.clone(), egui::ViewportId::ROOT, el, Some(s as f32), None, Some(4096));
        Some(GuiWin {
            window,
            surface,
            _sb: sb,
            ctx,
            state,
            painter: Painter::default(),
            next_paint: Some(Instant::now()),
            closed: false,
            dark: None,
            shown: false,
            raise_until: None,
            buf: vec![],
            last_pos: Pos2::ZERO,
        })
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    /// Bring it in front of other apps. Asked again for a moment afterwards: when the request comes
    /// from Blip's menu, the menu closing can hand the foreground back to the app below.
    pub fn focus(&mut self) {
        self.window.set_minimized(false);
        crate::platform::bring_to_front(&self.window);
        self.raise_until = Some(Instant::now() + Duration::from_millis(500));
    }

    /// Called every tick: keeps bringing it forward until the raise window passes.
    pub fn keep_raised(&mut self) {
        if let Some(t) = self.raise_until {
            if Instant::now() > t {
                self.raise_until = None;
            } else if !self.window.has_focus() {
                crate::platform::bring_to_front(&self.window);
            }
        }
    }

    pub fn on_event(&mut self, ev: &WindowEvent) {
        match ev {
            WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                self.closed = true;
                return;
            }
            WindowEvent::RedrawRequested => {
                self.next_paint = Some(Instant::now());
                return;
            }
            _ => {}
        }
        let r = self.state.on_window_event(&self.window, ev);
        if r.repaint {
            self.next_paint = Some(Instant::now());
        }
    }

    /// Test hook: feed a step straight into the UI ("move:x,y", "down", "up", "type:text",
    /// "key:enter|esc|tab"). Coordinates are window pixels, as in the BLIP_SHOT pictures.
    pub fn inject(&mut self, step: &str) {
        let ppp = self.window.scale_factor() as f32;
        let input = self.state.egui_input_mut();
        let (k, a) = step.split_once(':').unwrap_or((step, ""));
        let m = egui::Modifiers::default();
        match k {
            "move" => {
                let mut it = a.split(',').filter_map(|v| v.trim().parse::<f32>().ok());
                if let (Some(x), Some(y)) = (it.next(), it.next()) {
                    let pos = Pos2::new(x / ppp, y / ppp);
                    self.last_pos = pos;
                    input.events.push(egui::Event::PointerMoved(pos));
                }
            }
            "down" | "up" => input.events.push(egui::Event::PointerButton { pos: self.last_pos, button: egui::PointerButton::Primary, pressed: k == "down", modifiers: m }),
            "type" => input.events.push(egui::Event::Text(a.to_string())),
            "scroll" => {
                let dy: f32 = a.parse().unwrap_or(-300.0);
                input.events.push(egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: Vec2::new(0.0, dy), phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Unknown, modifiers: m });
            }
            "key" => {
                let key = match a {
                    "enter" => egui::Key::Enter,
                    "esc" => egui::Key::Escape,
                    _ => egui::Key::Tab,
                };
                for pressed in [true, false] {
                    input.events.push(egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: m });
                }
            }
            _ => {}
        }
        self.next_paint = Some(Instant::now());
    }

    pub fn repaint(&mut self) {
        self.next_paint = Some(Instant::now());
    }

    pub fn due(&self, now: Instant) -> bool {
        self.next_paint.map_or(false, |t| t <= now)
    }

    /// Lay out and draw one frame. `ui` builds the window's contents.
    pub fn frame(&mut self, t: &Theme, mut build: impl FnMut(&mut Ui)) {
        if self.dark != Some(t.dark) {
            self.dark = Some(t.dark);
            self.ctx.set_visuals(visuals(t));
            self.window.set_theme(Some(if t.dark { winit::window::Theme::Dark } else { winit::window::Theme::Light }));
        }
        let input = self.state.take_egui_input(&self.window);
        let bg = c(t.bg);
        let out = self.ctx.run_ui(input, |root| {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(bg).inner_margin(Margin::same(18)))
                .show(root, |ui| build(ui));
        });
        self.state.handle_platform_output(&self.window, out.platform_output);
        let delay = out.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.repaint_delay).unwrap_or(Duration::MAX);
        // egui asks again when something animates; otherwise refresh once a second for live values
        let delay = delay.min(Duration::from_secs(1));
        self.next_paint = Some(Instant::now() + delay);
        let prims = self.ctx.tessellate(out.shapes, out.pixels_per_point);
        // handled here, then cleared (egui checks in debug builds that no delta is dropped unused)
        let mut td = out.textures_delta;
        self.painter.update_textures(&td);
        let size = self.window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            self.painter.free_textures(&td);
            td.clear();
            return;
        };
        let (wu, hu) = (w.get() as usize, h.get() as usize);
        self.buf.clear();
        self.buf.resize(wu * hu, t.bg);
        self.painter.paint(&prims, out.pixels_per_point, &mut self.buf, wu, hu);
        self.painter.free_textures(&td);
        td.clear();
        if self.surface.resize(w, h).is_ok() {
            if let Ok(mut b) = self.surface.buffer_mut() {
                if b.len() == self.buf.len() {
                    b.copy_from_slice(&self.buf);
                }
                let _ = b.present();
            }
        }
        if !self.shown {
            self.shown = true;
            self.window.set_visible(true);
            self.focus();
            self.next_paint = Some(Instant::now());
        }
    }

    /// The last drawn frame as RGBA (for tests: BLIP_SHOT saves it as a picture).
    pub fn rgba(&self) -> (u32, u32, Vec<u8>) {
        let s = self.window.inner_size();
        let mut v = Vec::with_capacity(self.buf.len() * 4);
        for p in &self.buf {
            v.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8, 255]);
        }
        (s.width, s.height, v)
    }
}

// ================================================================ the kit

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Primary,
    Soft,
    Ghost,
    Danger,
}

fn snap(ui: &Ui, v: f32) -> f32 {
    let ppp = ui.ctx().pixels_per_point();
    (v * ppp).round() / ppp
}

/// Cell size for pixel art: `want` points, rounded to whole screen pixels.
pub fn px(ui: &Ui, want: f32) -> f32 {
    let ppp = ui.ctx().pixels_per_point();
    (want * ppp).round().max(1.0) / ppp
}

/// A 5x7 pixel icon at (x, y), `p` points per cell.
pub fn paint_glyph(ui: &Ui, x: f32, y: f32, rows: [u8; 7], p: f32, col: Color32) {
    let (x, y) = (snap(ui, x), snap(ui, y));
    let painter = ui.painter();
    for (r, bits) in rows.iter().enumerate() {
        let mut col_start: Option<usize> = None;
        for cx in 0..=5usize {
            let on = cx < 5 && bits & (0x10 >> cx) != 0;
            match (on, col_start) {
                (true, None) => col_start = Some(cx),
                (false, Some(s)) => {
                    let r0 = Rect::from_min_max(Pos2::new(x + s as f32 * p, y + r as f32 * p), Pos2::new(x + cx as f32 * p, y + (r + 1) as f32 * p));
                    painter.rect_filled(r0, 0.0, col);
                    col_start = None;
                }
                _ => {}
            }
        }
    }
}

pub fn icon(ui: &mut Ui, name: &str, p: f32, col: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(5.0 * p, 7.0 * p), Sense::hover());
    paint_glyph(ui, rect.min.x, rect.min.y, pixfont::icon(name), p, col);
    resp
}

/// Pixel-font capitals, like the section titles in the Python app.
pub fn pixel_text(ui: &mut Ui, text: &str, p: f32, col: Color32) -> Response {
    let w = pixfont::text_width(text, 1) as f32 * p;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w.max(1.0), 7.0 * p), Sense::hover());
    for (i, ch) in text.chars().enumerate() {
        paint_glyph(ui, rect.min.x + i as f32 * 6.0 * p, rect.min.y, pixfont::glyph(ch), p, col);
    }
    resp
}

/// Paint a sprite with its top-left cell at (x, y).
pub fn paint_sprite(ui: &Ui, g: &Sprite, x: f32, y: f32, p: f32) {
    let (bx, by, _, _) = blip_ui::sprite_bounds(g);
    let (x, y) = (snap(ui, x), snap(ui, y));
    let mut cells: Vec<(i32, i32, Color)> = g.iter().map(|(&(cx, cy), &col)| (cy, cx, col)).collect();
    cells.sort();
    let painter = ui.painter();
    let mut i = 0;
    while i < cells.len() {
        let (cy, cx, col) = cells[i];
        let mut j = i + 1;
        while j < cells.len() && cells[j].0 == cy && cells[j].1 == cells[j - 1].1 + 1 && cells[j].2 == col {
            j += 1;
        }
        let run = (j - i) as f32;
        let r = Rect::from_min_size(Pos2::new(x + (cx - bx) as f32 * p, y + (cy - by) as f32 * p), Vec2::new(run * p, p));
        painter.rect_filled(r, 0.0, c(col));
        i = j;
    }
}

pub fn pet_sprite(species: &str, palette: &str, stage: &str, mood: &str, frame: u32) -> Sprite {
    if stage == "egg" {
        return build_egg(frame, 0.5);
    }
    let palette = if sprites::PALETTES.contains(&palette) { palette } else { "gold" };
    build_pet(&PetLook { stage: Stage::from_name(stage), palette, mood, frame, facing: 0, squash: false, blink: false, species })
}

/// Like `pet_sprite`, wearing things (item ids).
pub fn pet_sprite_wear(species: &str, palette: &str, stage: &str, mood: &str, frame: u32, wear: &[String]) -> Sprite {
    let mut g = pet_sprite(species, palette, stage, mood, frame);
    if stage != "egg" {
        let palette = if sprites::PALETTES.contains(&palette) { palette } else { "gold" };
        let look = PetLook { stage: Stage::from_name(stage), palette, mood, frame, facing: 0, squash: false, blink: false, species };
        sprites::add_wear(&mut g, &look, wear);
    }
    g
}

/// A pet drawn in a box of `cells` x `cells` sprite cells, centred.
pub fn avatar(ui: &mut Ui, g: &Sprite, p: f32, cells: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(cells * p), Sense::click());
    let (_, _, bw, bh) = blip_ui::sprite_bounds(g);
    let x = rect.center().x - bw as f32 * p / 2.0;
    let y = rect.max.y - bh as f32 * p;
    paint_sprite(ui, g, x, y, p);
    resp
}

pub fn card<R>(ui: &mut Ui, t: &Theme, pad: i8, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(c(t.surface))
        .stroke(Stroke::new(1.0, c(t.line)))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(pad))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn section(ui: &mut Ui, t: &Theme, text: &str) {
    ui.add_space(14.0);
    ui.label(RichText::new(text.to_uppercase()).family(FontFamily::Name("bold".into())).size(11.5).color(c(t.muted)).extra_letter_spacing(0.8));
    ui.add_space(2.0);
}

pub fn hline(ui: &mut Ui, t: &Theme) {
    ui.add_space(6.0);
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, c(t.line));
    ui.add_space(6.0);
}

pub fn muted(ui: &mut Ui, t: &Theme, text: &str) -> Response {
    ui.add(egui::Label::new(RichText::new(text).color(c(t.muted)).size(12.5)).wrap())
}

pub fn bold(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).family(FontFamily::Name("bold".into())).size(14.0))
}

pub fn h2(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).family(FontFamily::Name("bold".into())).size(17.0))
}

pub fn colored(ui: &mut Ui, text: &str, col: Color, size: f32) -> Response {
    ui.add(egui::Label::new(RichText::new(text).color(c(col)).size(size)).wrap())
}

/// A small square status light.
pub fn dot(ui: &mut Ui, col: Color, size: f32) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(2), c(col));
}

/// Blip's button: filled (primary / danger), soft, or ghost; optional pixel icon.
pub fn button(ui: &mut Ui, t: &Theme, label: &str, icon_name: Option<&str>, kind: Kind, small: bool, enabled: bool) -> Response {
    let font = FontId::new(if small { 13.0 } else { 14.0 }, FontFamily::Name("bold".into()));
    let (fg, bg, hover_bg) = match kind {
        Kind::Primary => (t.on_accent, Some(t.accent), Some(blend(t.accent, 0x000000, 0.12))),
        Kind::Danger => (0xffffff, Some(t.danger), Some(blend(t.danger, 0x000000, 0.12))),
        Kind::Soft => (t.ink, Some(t.accent_soft), Some(t.hover)),
        Kind::Ghost => (t.muted, None, Some(t.hover)),
    };
    let galley = ui.painter().layout_no_wrap(label.to_string(), font.clone(), c(fg));
    let p = px(ui, if small && !label.is_empty() { 1.6 } else { 2.0 });
    let icon_w = if icon_name.is_some() { 5.0 * p + if label.is_empty() { 0.0 } else { 7.0 } } else { 0.0 };
    let pad = if small { 10.0 } else { 14.0 };
    let h = if small { 28.0 } else { 34.0 };
    let w = (galley.size().x + icon_w + pad * 2.0).max(if label.is_empty() { h } else { 0.0 });
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), sense);
    let hov = enabled && resp.hovered();
    let fill = if hov { hover_bg.or(bg) } else { bg };
    let alpha = if enabled { 1.0 } else { 0.45 };
    if let Some(f) = fill {
        ui.painter().rect_filled(rect, CornerRadius::same(8), c(f).gamma_multiply(alpha));
    }
    let fgc = if kind == Kind::Ghost && hov { c(t.ink) } else { c(fg) }.gamma_multiply(alpha);
    let total = icon_w + galley.size().x;
    let mut x = rect.center().x - total / 2.0;
    if let Some(n) = icon_name {
        paint_glyph(ui, x, rect.center().y - 3.5 * p, pixfont::icon(n), p, fgc);
        x += icon_w;
    }
    if !label.is_empty() {
        ui.painter().galley_with_override_text_color(Pos2::new(x, rect.center().y - galley.size().y / 2.0), galley, fgc);
    }
    if enabled && hov {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub fn icon_button(ui: &mut Ui, t: &Theme, icon_name: &str) -> Response {
    button(ui, t, "", Some(icon_name), Kind::Ghost, true, true)
}

pub fn blend(a: Color, b: Color, f: f64) -> Color {
    let ch = |s: u32| -> u32 {
        let (x, y) = (((a >> s) & 255) as f64, ((b >> s) & 255) as f64);
        ((x + (y - x) * f).round() as u32) << s
    };
    ch(16) | ch(8) | ch(0)
}

/// An on/off switch; returns true when flipped.
pub fn toggle(ui: &mut Ui, t: &Theme, on: &mut bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(40.0, 22.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    let k = ui.ctx().animate_bool(resp.id, *on);
    let track = if *on { t.accent } else { t.track };
    ui.painter().rect_filled(rect, CornerRadius::same(11), c(track));
    let r = 8.0;
    let cx = rect.left() + 11.0 + k * (rect.width() - 22.0);
    ui.painter().circle_filled(Pos2::new(cx, rect.center().y), r, c(t.knob));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// A row with a title and description on the left and a control on the right.
pub fn setting_row<R>(ui: &mut Ui, t: &Theme, title: &str, desc: &str, right: impl FnOnce(&mut Ui) -> R) -> R {
    setting_row_w(ui, t, title, desc, 190.0, right)
}

pub fn setting_row_w<R>(ui: &mut Ui, t: &Theme, title: &str, desc: &str, right_w: f32, right: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        let right_w = right_w.min(ui.available_width() * 0.45);
        let left_w = (ui.available_width() - right_w - 8.0).max(80.0);
        ui.allocate_ui_with_layout(Vec2::new(left_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(left_w);
            bold(ui, title);
            if !desc.is_empty() {
                muted(ui, t, desc);
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right).inner
    })
    .inner
}

pub fn toggle_row(ui: &mut Ui, t: &Theme, title: &str, desc: &str, on: &mut bool) -> bool {
    setting_row_w(ui, t, title, desc, 52.0, |ui| toggle(ui, t, on))
}

/// Segmented choice; returns true when the choice changed.
pub fn chips(ui: &mut Ui, t: &Theme, opts: &[(&str, &str)], sel: &mut String) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for (key, label) in opts {
            let on = sel == key;
            let font = FontId::proportional(13.0);
            let galley = ui.painter().layout_no_wrap(label.to_string(), font, c(if on { t.on_accent } else { t.ink }));
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(galley.size().x + 22.0, 28.0), Sense::click());
            let fill = if on { t.accent } else if resp.hovered() { t.hover } else { t.tile };
            ui.painter().rect_filled(rect, CornerRadius::same(14), c(fill));
            if !on {
                ui.painter().rect_stroke(rect, CornerRadius::same(14), Stroke::new(1.0, c(t.line)), StrokeKind::Inside);
            }
            let pos = rect.center() - galley.size() / 2.0;
            ui.painter().galley(pos, galley, c(t.ink));
            if resp.clicked() && !on {
                *sel = key.to_string();
                changed = true;
            }
        }
    });
    changed
}

/// A one-line text field; returns its response (use `lost_focus()` to commit).
pub fn field(ui: &mut Ui, text: &mut String, width: f32, hint: &str, password: bool) -> Response {
    ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(hint)
            .password(password)
            .desired_width(width)
            .margin(Margin::symmetric(8, 6)),
    )
}

/// True when the field was committed (Enter pressed or focus left).
pub fn committed(ui: &Ui, r: &Response) -> bool {
    r.lost_focus() || (r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
}

pub fn enter_pressed(ui: &Ui, r: &Response) -> bool {
    r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
}

/// A progress bar for the pet's needs etc.
pub fn bar(ui: &mut Ui, t: &Theme, width: f32, frac: f32, col: Color) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(width, 6.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(3), c(t.track));
    let mut f = r;
    f.set_width(r.width() * frac.clamp(0.0, 1.0));
    if f.width() > 0.5 {
        ui.painter().rect_filled(f, CornerRadius::same(3), c(col));
    }
}

/// A slider that snaps to `steps`, with the value as a percentage. Returns the new value.
pub fn step_slider(ui: &mut Ui, t: &Theme, value: f64, steps: &[f64], width: f32) -> Option<f64> {
    let label_w = 44.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width + label_w, 24.0), Sense::click_and_drag());
    let track = Rect::from_min_max(Pos2::new(rect.min.x + 8.0, rect.center().y - 2.0), Pos2::new(rect.min.x + width - 8.0, rect.center().y + 2.0));
    let n = steps.len().max(2) - 1;
    let pos = |i: usize| track.min.x + track.width() * i as f32 / n as f32;
    let cur = steps.iter().enumerate().min_by(|a, b| (a.1 - value).abs().total_cmp(&(b.1 - value).abs())).map(|(i, _)| i).unwrap_or(0);
    ui.painter().rect_filled(track, CornerRadius::same(2), c(t.track));
    let filled = Rect::from_min_max(track.min, Pos2::new(pos(cur), track.max.y));
    ui.painter().rect_filled(filled, CornerRadius::same(2), c(t.accent));
    for i in 0..=n {
        ui.painter().circle_filled(Pos2::new(pos(i), track.center().y), 2.0, c(if i <= cur { t.accent } else { t.line }));
    }
    let k = Pos2::new(pos(cur), track.center().y);
    ui.painter().circle_filled(k, 8.0, c(t.knob));
    ui.painter().circle_stroke(k, 8.0, Stroke::new(1.5, c(t.accent)));
    ui.painter().text(Pos2::new(rect.max.x, rect.center().y), egui::Align2::RIGHT_CENTER, format!("{:.0}%", steps[cur] * 100.0), FontId::proportional(12.5), c(t.muted));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let p = resp.interact_pointer_pos()?;
    if !(resp.clicked() || resp.dragged() || resp.drag_started()) {
        return None;
    }
    let f = ((p.x - track.min.x) / track.width()).clamp(0.0, 1.0);
    let i = (f * n as f32).round() as usize;
    (i != cur).then(|| steps[i])
}

/// The centred "nothing here" message.
pub fn empty_state(ui: &mut Ui, t: &Theme, title: &str, text: &str) {
    ui.add_space(14.0);
    ui.vertical_centered(|ui| {
        bold(ui, title);
        ui.label(RichText::new(text).color(c(t.muted)).size(12.5));
    });
    ui.add_space(8.0);
}

const BAR_COLORS: [Color; 10] = [0x7ad9b0, 0x8cd5a0, 0xa6d08c, 0xc2ca78, 0xdcc36a, 0xf0b964, 0xf6a26c, 0xf68a7e, 0xf07592, 0xe868a6];

fn pixel_rect(ui: &Ui, r: Rect, p: f32, col: Color32) {
    // a box with notched pixel corners, like blip-ui's pixel_box
    ui.painter().rect_filled(Rect::from_min_max(Pos2::new(r.min.x + p, r.min.y), Pos2::new(r.max.x - p, r.max.y)), 0.0, col);
    ui.painter().rect_filled(Rect::from_min_max(Pos2::new(r.min.x, r.min.y + p), Pos2::new(r.max.x, r.max.y - p)), 0.0, col);
}

/// The pixel "signal bars" volume control from the Python app: 10 bars lighting up mint to pink.
/// Click or drag to set; the speaker mutes / unmutes. Returns the new level (0-100) when it changed.
pub fn volume_bars(ui: &mut Ui, t: &Theme, value: f64, last_on: f64, dim: bool) -> Option<f64> {
    let p = px(ui, 2.0);
    let (bw, gap, x0, h) = (12.0, 5.0, 34.0, 36.0);
    let n = 10;
    let w = x0 + n as f32 * (bw + gap) + 48.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click_and_drag());
    let lvl = (value / 10.0).round() as usize;
    let ink = if dim { c(t.off) } else { c(t.ink) };
    let sy = rect.center().y - 3.5 * p;
    paint_glyph(ui, rect.min.x, sy, pixfont::icon("speaker"), p, ink);
    if lvl == 0 {
        ui.painter().text(Pos2::new(rect.min.x + 6.0 * p + 4.0, rect.center().y), egui::Align2::LEFT_CENTER, "x", FontId::proportional(13.0), c(t.danger));
    } else {
        for i in 0..(1 + lvl / 4).min(3) {
            let x = rect.min.x + 6.0 * p + i as f32 * 2.0 * p;
            let hh = (2.0 + 2.0 * i as f32) * p;
            ui.painter().rect_filled(Rect::from_min_max(Pos2::new(x, rect.center().y - hh / 2.0), Pos2::new(x + p, rect.center().y + hh / 2.0)), 0.0, ink);
        }
    }
    for i in 0..n {
        let x = rect.min.x + x0 + i as f32 * (bw + gap);
        let bh = 8.0 + (h - 10.0) * (i + 1) as f32 / n as f32;
        let lit = i < lvl;
        let col = if lit && !dim { c(BAR_COLORS[i]) } else if lit { c(t.track).gamma_multiply(1.6) } else { c(t.track) };
        pixel_rect(ui, Rect::from_min_max(Pos2::new(x, rect.max.y - bh), Pos2::new(x + bw, rect.max.y)), p, col);
    }
    let pct = if lvl == 0 { "Mute".to_string() } else { format!("{}%", lvl * 10) };
    ui.painter().text(Pos2::new(rect.max.x - 4.0, rect.center().y), egui::Align2::RIGHT_CENTER, pct, FontId::proportional(12.5), if dim { c(t.off) } else { c(t.muted) });
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let pos = resp.interact_pointer_pos()?;
    if !(resp.clicked() || resp.dragged() || resp.drag_started()) {
        return None;
    }
    let rx = pos.x - rect.min.x;
    let new = if rx < x0 - 4.0 {
        if !resp.clicked() {
            return None;
        }
        if lvl == 0 { last_on.max(10.0) } else { 0.0 }
    } else {
        (((rx - x0) / (bw + gap)).floor() as i64 + 1).clamp(0, n as i64) as f64 * 10.0
    };
    (new != (lvl * 10) as f64).then_some(new)
}
