//! Blip's regular windows, ported from `blip/windows.py`: Settings (pet, general, menu, office,
//! Basecamp), Notes, Timers & alarms, Buddies, Basecamp tasks, the first-run welcome and the small
//! "ask" dialogs. Each is a `GuiWin`; asking for one that is already open brings it forward.

use std::path::PathBuf;
use std::time::Instant;

use blip_core::clock::{self, describe_days, fmt_clock, parse_clock, parse_duration, parse_when};
use blip_core::notes;
use blip_core::pet::COLORS;
use blip_core::timers::{self, Timers};
use blip_pet::sprites::{self, Color};
use blip_ui::theme::Theme;
use egui::{Align, CornerRadius, Layout, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{json, Value};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::app::App;
use crate::gui::{self, c, GuiWin, Kind};
use crate::office;
use crate::platform;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const SPECIES_NAMES: [(&str, &str); 7] =
    [("blip", "Blip"), ("kitty", "Kitty"), ("bunny", "Bunny"), ("bear", "Bear"), ("pup", "Pup"), ("chick", "Chick"), ("bot", "Bot")];
const REPEAT: [(&str, &str); 4] = [("once", "Once"), ("weekdays", "Weekdays"), ("everyday", "Every day"), ("weekends", "Weekends")];
const WHEN: [(&str, &str); 5] =
    [("none", "No reminder"), ("30m", "In 30 min"), ("1h", "In 1 hour"), ("tomorrow", "Tomorrow 9:00"), ("pick", "Pick a time")];
const MENU_ITEMS: [(&str, &str, &str); 7] = [
    ("care", "Care buttons", "Feed, snack, play, clean, medicine and sleep."),
    ("notes", "Notes", "Your notes and reminders."),
    ("focus", "Timer, alarm & stopwatch", "One row of small buttons."),
    ("office", "People nearby", "Call and message colleagues' pets."),
    ("basecamp", "Basecamp tasks", "Only shows while Basecamp is connected."),
    ("dnd", "Do not disturb", "Quick switch for calls."),
    ("wander", "Wander around", "Quick switch for walking around."),
];
const PAGES: [(&str, &str, &str); 5] =
    [("pet", "paw", "Pet"), ("general", "gear", "General"), ("menu", "list", "Menu"), ("office", "people", "Office"), ("basecamp", "check", "Basecamp")];

fn repeat_days(k: &str) -> Vec<u32> {
    match k {
        "weekdays" => clock::WEEKDAYS.to_vec(),
        "everyday" => clock::EVERYDAY.to_vec(),
        "weekends" => vec![5, 6],
        _ => vec![],
    }
}

pub fn fmt_age(sec: f64) -> String {
    let s = sec.max(0.0) as i64;
    let (d, h, m) = (s / 86400, s % 86400 / 3600, s % 3600 / 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

pub fn ago(ts: f64) -> String {
    let d = (blip_core::now() - ts).max(0.0);
    if d < 60.0 {
        "just now".into()
    } else if d < 3600.0 {
        format!("{} min ago", (d / 60.0) as i64)
    } else if d < 86400.0 {
        format!("{} h ago", (d / 3600.0) as i64)
    } else {
        format!("{} d ago", (d / 86400.0) as i64)
    }
}

pub fn hhmm(ts: f64) -> String {
    use chrono::{Local, TimeZone};
    Local.timestamp_opt(ts as i64, 0).single().map(|t| t.format("%H:%M").to_string()).unwrap_or_default()
}

// ================================================================ pages

struct SettingsPage {
    page: String,
    pet_name: String,
    my_name: String,
    server: String,
    server_note: Option<(String, Color, f64)>,
    private: bool,
    group: String,
    show_group: bool,
    peers: String,
    bc_token: String,
    bc_id: String,
    bc_secret: String,
    bc_morning: String,
}

struct NotesPage {
    text: String,
    when: String,
    picked: String,
    err: String,
    focus: bool,
}

struct TimersPage {
    dur: String,
    label: String,
    at: String,
    alabel: String,
    repeat: String,
    err: String,
}

struct BuddiesPage {
    my_name: String,
}

struct WelcomePage {
    name: String,
    pet: String,
    focus: bool,
}

struct AskPage {
    kind: String,
    prompt: String,
    text: String,
    hint: String,
    ok: String,
    err: String,
    focus: bool,
}

enum Page {
    Settings(Box<SettingsPage>),
    Notes(NotesPage),
    Timers(TimersPage),
    Buddies(BuddiesPage),
    Tasks,
    Welcome(WelcomePage),
    Ask(AskPage),
}

struct Open {
    name: String,
    win: GuiWin,
    page: Page,
}

#[derive(Default)]
pub struct Windows {
    open: Vec<Open>,
    shots: Option<PathBuf>,
    started: bool,
    script_at: Option<Instant>,
}

impl Windows {
    pub fn owns(&self, id: WindowId) -> bool {
        self.open.iter().any(|o| o.win.id() == id)
    }

    pub fn event(&mut self, app: &mut App, el: &ActiveEventLoop, id: WindowId, ev: WindowEvent) {
        let Some(i) = self.open.iter().position(|o| o.win.id() == id) else { return };
        let o = &mut self.open[i];
        o.win.on_event(&ev);
        if o.win.closed {
            let o = self.open.remove(i);
            if let Page::Welcome(w) = &o.page {
                finish_welcome(app, el, w);
            }
        }
    }

    pub fn repaint_all(&mut self) {
        for o in self.open.iter_mut() {
            o.win.repaint();
        }
    }

    /// Open the windows asked for and draw the ones that need it.
    pub fn tick(&mut self, app: &mut App, el: &ActiveEventLoop) {
        if !self.started {
            self.started = true;
            self.shots = std::env::var("BLIP_SHOT").ok().map(PathBuf::from);
        }
        let want: Vec<String> = app.want_window.drain(..).collect();
        for w in want {
            self.open_window(app, el, &w);
        }
        if app.windows_dirty {
            app.windows_dirty = false;
            self.repaint_all();
        }
        // test script (BLIP_DEMO "ui:..." steps): one step every 0.3 s into the newest window
        let now = Instant::now();
        if !app.ui_script.is_empty() && self.script_at.map_or(true, |t| t <= now) {
            if let Some(o) = self.open.last_mut() {
                let step = app.ui_script.remove(0);
                if let Some(xy) = step.strip_prefix("click:") {
                    o.win.inject(&format!("move:{xy}"));
                    app.ui_script.insert(0, "down".into());
                    app.ui_script.insert(1, "up".into());
                } else if let Some(w) = step.strip_prefix("wait:") {
                    let secs: f64 = w.parse().unwrap_or(1.0);
                    self.script_at = Some(now + std::time::Duration::from_secs_f64(secs));
                } else {
                    o.win.inject(&step);
                }
                if !step.starts_with("wait:") {
                    self.script_at = Some(now + std::time::Duration::from_millis(250));
                }
            }
        }
        let t = app.theme;
        for o in self.open.iter_mut() {
            if !o.win.due(now) {
                continue;
            }
            let Open { win, page, name } = o;
            let mut close = false;
            win.frame(&t, |ui| close = page_ui(ui, &t, app, el, page));
            if let Some(dir) = &self.shots {
                let (w, h, rgba) = win.rgba();
                let file = match page {
                    Page::Settings(s) => format!("settings-{}", s.page),
                    _ => name.replace(':', "-"),
                };
                let _ = crate::shot::save_png(&dir.join(format!("{file}.png")), w, h, &rgba);
            }
            if close {
                win.closed = true;
            }
        }
        let mut i = 0;
        while i < self.open.len() {
            if self.open[i].win.closed {
                let o = self.open.remove(i);
                if let Page::Welcome(w) = &o.page {
                    finish_welcome(app, el, w);
                }
            } else {
                i += 1;
            }
        }
    }

    fn open_window(&mut self, app: &mut App, el: &ActiveEventLoop, what: &str) {
        let (name, arg) = match what.split_once(':') {
            Some((n, a)) => (n.to_string(), a.to_string()),
            None => (what.to_string(), String::new()),
        };
        let name = if name == "status" { "settings".to_string() } else { name };
        let key = if name == "ask" { format!("ask:{arg}") } else { name.clone() };
        if let Some(o) = self.open.iter_mut().find(|o| o.name == key) {
            if let (Page::Settings(s), false) = (&mut o.page, arg.is_empty()) {
                s.page = arg;
            }
            o.win.focus();
            o.win.repaint();
            return;
        }
        let (title, size, page) = match name.as_str() {
            "settings" => {
                let p = Box::new(SettingsPage {
                    page: if PAGES.iter().any(|p| p.0 == arg) { arg.clone() } else { "pet".into() },
                    pet_name: app.pet.s("name").to_string(),
                    my_name: app.cfg.str("display_name").to_string(),
                    server: app.cfg.str("server").to_string(),
                    server_note: None,
                    private: app.cfg.str("passphrase") != office::DEFAULT_GROUP,
                    group: if app.cfg.str("passphrase") != office::DEFAULT_GROUP { app.cfg.str("passphrase").to_string() } else { String::new() },
                    show_group: false,
                    peers: app.cfg.str("manual_peers").to_string(),
                    bc_token: String::new(),
                    bc_id: app.cfg.str("bc_client_id").to_string(),
                    bc_secret: app.cfg.str("bc_client_secret").to_string(),
                    bc_morning: app.cfg.str("bc_morning").to_string(),
                });
                ("Settings", (720.0, 640.0), Page::Settings(p))
            }
            "notes" => ("Notes", (600.0, 620.0), Page::Notes(NotesPage { text: String::new(), when: "none".into(), picked: String::new(), err: String::new(), focus: true })),
            "timers" => (
                "Timers & alarms",
                (580.0, 660.0),
                Page::Timers(TimersPage { dur: String::new(), label: String::new(), at: hhmm(blip_core::now() + 3600.0), alabel: String::new(), repeat: "once".into(), err: String::new() }),
            ),
            "alarm" => {
                return self.open_window(app, el, "ask:alarm");
            }
            "buddies" => ("Buddies", (580.0, 640.0), Page::Buddies(BuddiesPage { my_name: app.cfg.str("display_name").to_string() })),
            "tasks" => ("Basecamp", (600.0, 560.0), Page::Tasks),
            "welcome" => {
                let mut guess = app.cfg.str("display_name").to_string();
                if !app.cfg.bool("named") {
                    let full = platform::full_name();
                    guess = if full.is_empty() { title_case(&guess.replace(['.', '_'], " ")) } else { full };
                }
                ("Welcome to Blip", (600.0, 640.0), Page::Welcome(WelcomePage { name: guess, pet: app.pet.s("name").to_string(), focus: true }))
            }
            "ask" => {
                let (title, prompt, text, hint, ok) = ask_texts(app, &arg);
                (title, (420.0, 165.0), Page::Ask(AskPage { kind: arg.clone(), prompt, text, hint, ok, err: String::new(), focus: true }))
            }
            _ => return,
        };
        let (ax, ay, work) = app.window_anchor();
        let s = work_scale(app);
        let (w, h) = (size.0 * s, size.1 * s + 32.0 * s);
        let x = (ax - w / 2.0).clamp(work.left + 10.0, (work.right - w - 10.0).max(work.left + 10.0));
        let y = (ay - h - 10.0).min(work.bottom - h - 10.0).max(work.top + 10.0);
        let icon = Some(crate::window_icon(app));
        if let Some(win) = GuiWin::new(el, title, size, (x, y), &app.theme, icon) {
            self.open.push(Open { name: key, win, page });
        }
    }
}

fn work_scale(app: &App) -> f64 {
    app.geo.s
}

fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut ch = w.chars();
            match ch.next() {
                Some(f) => f.to_uppercase().collect::<String>() + &ch.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn ask_texts(app: &App, kind: &str) -> (&'static str, String, String, String, String) {
    let (k, rest) = kind.split_once(':').unwrap_or((kind, ""));
    match k {
        "alarm" => ("New alarm", "Ring at what time today? (e.g. 13:00 or 5pm)".into(), hhmm(blip_core::now() + 3600.0), "13:00".into(), "Add alarm".into()),
        "timer" => ("New timer", "How long? (25, 1:30, 90s, 2h 15m)".into(), "25".into(), "25".into(), "Start".into()),
        "note-time" => (
            "Reminder",
            "When? (in 20m, 5pm, 17:30, tomorrow 10 - empty = no reminder)".into(),
            String::new(),
            "in 30m".into(),
            "Set".into(),
        ),
        "message" => {
            let name = office::peer_name(app, rest).unwrap_or_else(|| "your colleague".into());
            ("Message", format!("Message to {name}"), String::new(), "Type a message\u{2026}".into(), "Send".into())
        }
        "reply" => ("Reply", "Your reply:".into(), String::new(), "On my way!".into(), "Send".into()),
        _ => ("Blip", String::new(), String::new(), String::new(), "OK".into()),
    }
}

/// Draw one window's contents; true = close the window.
fn page_ui(ui: &mut Ui, t: &Theme, app: &mut App, el: &ActiveEventLoop, page: &mut Page) -> bool {
    match page {
        Page::Settings(p) => {
            settings_ui(ui, t, app, el, p);
            false
        }
        Page::Notes(p) => {
            notes_ui(ui, t, app, el, p);
            false
        }
        Page::Timers(p) => {
            timers_ui(ui, t, app, el, p);
            false
        }
        Page::Buddies(p) => {
            buddies_ui(ui, t, app, el, p);
            false
        }
        Page::Tasks => {
            crate::office::tasks_ui(ui, t, app, el);
            false
        }
        Page::Welcome(p) => welcome_ui(ui, t, app, p),
        Page::Ask(p) => ask_ui(ui, t, app, el, p),
    }
}

/// Window title with the pixel icon, like kit.Window's header.
fn header(ui: &mut Ui, t: &Theme, icon: &str, title: &str, subtitle: &str) {
    ui.horizontal(|ui| {
        let p = gui::px(ui, 2.0);
        gui::icon(ui, icon, p, c(t.accent));
        ui.add_space(4.0);
        gui::h2(ui, title);
    });
    if !subtitle.is_empty() {
        gui::muted(ui, t, subtitle);
    }
    ui.add_space(10.0);
}

// ---------------------------------------------------------------- settings
fn settings_ui(ui: &mut Ui, t: &Theme, app: &mut App, el: &ActiveEventLoop, s: &mut SettingsPage) {
    let h = ui.available_height();
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(Vec2::new(150.0, h), Layout::top_down(Align::Min), |ui| {
            ui.set_min_height(h);
            for (key, icon, text) in PAGES {
                let sel = s.page == key;
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(146.0, 36.0), Sense::click());
                let fill = if sel { Some(t.accent_soft) } else if resp.hovered() { Some(t.hover) } else { None };
                if let Some(f) = fill {
                    ui.painter().rect_filled(rect, CornerRadius::same(8), c(f));
                }
                let col = if sel { c(t.accent) } else { c(t.muted) };
                let p = gui::px(ui, 2.0);
                gui::paint_glyph(ui, rect.min.x + 12.0, rect.center().y - 3.5 * p, blip_pet::pixfont::icon(icon), p, col);
                let font = egui::FontId::new(14.0, egui::FontFamily::Name("bold".into()));
                ui.painter().text(Pos2::new(rect.min.x + 12.0 + 5.0 * p + 10.0, rect.center().y), egui::Align2::LEFT_CENTER, text, font, if sel { c(t.ink) } else { c(t.muted) });
                if resp.clicked() {
                    s.page = key.to_string();
                }
                ui.add_space(2.0);
            }
            ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    gui::muted(ui, t, &format!("Blip v{VERSION}"));
                });
            });
        });
        ui.add_space(12.0);
        ui.vertical(|ui| {
            egui::ScrollArea::vertical().id_salt(&s.page).auto_shrink([false, false]).show(ui, |ui| {
                ui.set_max_width(ui.available_width() - 8.0);
                match s.page.as_str() {
                    "general" => page_general(ui, t, app, el, s),
                    "menu" => page_menu(ui, t, app),
                    "office" => page_office(ui, t, app, el, s),
                    "basecamp" => crate::office::basecamp_page(ui, t, app, el, &mut s.bc_token, &mut s.bc_id, &mut s.bc_secret, &mut s.bc_morning),
                    _ => page_pet(ui, t, app, el, s),
                }
                ui.add_space(12.0);
            });
        });
    });
}

fn species_tiles(ui: &mut Ui, t: &Theme, app: &mut App) {
    let pal = app.pet.palette();
    let pal = if sprites::PALETTES.contains(&pal.as_str()) { pal } else { "gold".into() };
    let cur = app.pet.species().to_string();
    let mut pick = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
        for (key, name) in SPECIES_NAMES {
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(100.0, 96.0), Sense::click());
            let sel = key == cur;
            ui.painter().rect_filled(rect, CornerRadius::same(10), c(if resp.hovered() { t.hover } else { t.surface }));
            ui.painter().rect_stroke(rect, CornerRadius::same(10), Stroke::new(if sel { 2.0 } else { 1.0 }, c(if sel { t.accent } else { t.line })), StrokeKind::Inside);
            let mood = if sel || resp.hovered() { "happy" } else { "normal" };
            let g = gui::pet_sprite(key, &pal, "adult", mood, 1);
            let p = gui::px(ui, 2.0);
            let (_, _, bw, bh) = blip_ui::sprite_bounds(&g);
            gui::paint_sprite(ui, &g, rect.center().x - bw as f32 * p / 2.0, rect.min.y + 62.0 - bh as f32 * p, p);
            let font = egui::FontId::proportional(12.5);
            ui.painter().text(Pos2::new(rect.center().x, rect.max.y - 16.0), egui::Align2::CENTER_CENTER, name, font, if sel { c(t.ink) } else { c(t.muted) });
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                pick = Some(key);
            }
        }
    });
    if let Some(k) = pick {
        app.set_look(Some(k), None);
    }
}

fn swatches(ui: &mut Ui, t: &Theme, app: &mut App) {
    let cur = app.pet.s("color").to_string();
    let mut pick = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
        for col in COLORS {
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
            let sel = cur == col || (cur.is_empty() && col == "auto");
            let inner = rect.shrink(4.0);
            if col == "auto" {
                let parts = ["mint", "sky", "gold"];
                for (i, pn) in parts.iter().enumerate() {
                    let w = inner.width() / 3.0;
                    let r = Rect::from_min_size(Pos2::new(inner.min.x + i as f32 * w, inner.min.y), Vec2::new(w, inner.height()));
                    ui.painter().rect_filled(r, 0.0, c(sprites::palette(pn).0));
                }
            } else {
                let (a, b, _) = sprites::palette(col);
                ui.painter().rect_filled(inner, CornerRadius::same(4), c(a));
                let k = inner.height() / 3.0;
                let r = Rect::from_min_max(Pos2::new(inner.min.x, inner.max.y - k), inner.max);
                ui.painter().rect_filled(r, CornerRadius::same(2), c(b));
            }
            let ring = if sel { Some((2.0, t.accent)) } else if resp.hovered() { Some((1.0, t.muted)) } else { Some((1.0, t.line)) };
            if let Some((w, rc)) = ring {
                ui.painter().rect_stroke(rect.shrink(if sel { 0.0 } else { 3.0 }), CornerRadius::same(6), Stroke::new(w, c(rc)), StrokeKind::Inside);
            }
            if resp.clicked() {
                pick = Some(col);
            }
        }
    });
    if let Some(col) = pick {
        app.set_look(None, Some(col));
    }
}

fn page_pet(ui: &mut Ui, t: &Theme, app: &mut App, el: &ActiveEventLoop, s: &mut SettingsPage) {
    gui::card(ui, t, 14, |ui| {
        ui.horizontal_top(|ui| {
            let stage = if app.pet.stage() == "egg" && !app.pet.gone() { "egg" } else if app.pet.stage() == "egg" { "baby" } else { app.pet.stage() }.to_string();
            let frame = ((blip_core::now() * 2.0) as u32) % 2;
            let g = gui::pet_sprite(app.pet.species(), &app.pet.palette(), &stage, "happy", frame);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
            let p = gui::px(ui, 4.0);
            gui::avatar(ui, &g, p, 30.0);
            ui.add_space(10.0);
            ui.vertical(|ui| {
                let r = gui::field(ui, &mut s.pet_name, 220.0, "Name", false);
                if gui::committed(ui, &r) {
                    let v: String = s.pet_name.trim().chars().take(16).collect();
                    if !v.is_empty() && v != app.pet.s("name") {
                        app.pet.set("name", json!(v));
                        app.chirp("happy", Some("heart"), 3.0);
                        app.save();
                    }
                }
                let pet = &app.pet;
                gui::muted(
                    ui,
                    t,
                    &format!("{}  \u{b7}  age {}  \u{b7}  {}  \u{b7}  gen {}", pet.stage_name(), fmt_age(pet.f("age")), pet.mood_word(), pet.f("gen") as i64),
                );
                ui.add_space(4.0);
                for (icon, name, key) in [("meal", "Food", "full"), ("play", "Fun", "happy"), ("moon", "Energy", "energy"), ("clean", "Clean", "hygiene"), ("heart", "Health", "health")] {
                    let v = app.pet.stat(key);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        gui::icon(ui, icon, gui::px(ui, 1.6), c(t.muted));
                        let (r, _) = ui.allocate_exact_size(Vec2::new(48.0, 14.0), Sense::hover());
                        ui.painter().text(r.left_center(), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(11.5), c(t.muted));
                        let col = if v > 60.0 { t.ok } else if v > 30.0 { t.warn } else { t.danger };
                        gui::bar(ui, t, 180.0, (v / 100.0) as f32, col);
                    });
                }
                ui.add_space(6.0);
                let acts = [("Feed", "care:meal"), ("Snack", "care:snack"), ("Play", "care:play"), ("Clean", "care:clean"), ("Medicine", "care:meds"), ("Sleep / wake", "care:sleep")];
                for row in acts.chunks(3) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for (label, cmd) in row {
                            if gui::button(ui, t, label, None, Kind::Soft, true, true).clicked() {
                                app.run(el, cmd);
                            }
                        }
                    });
                }
            });
        });
    });
    gui::section(ui, t, "Look");
    species_tiles(ui, t, app);
    gui::section(ui, t, "Colour");
    swatches(ui, t, app);
    ui.add_space(4.0);
    gui::muted(ui, t, "Auto: changes as your pet grows, and its grown-up colour shows how well it was looked after.");
    if app.pet.gone() {
        ui.add_space(10.0);
        if gui::button(ui, t, "Hatch a new egg", Some("heart"), Kind::Primary, false, true).clicked() {
            app.run(el, "new-egg");
            s.pet_name = app.pet.s("name").to_string();
        }
    }
}

fn theme_tile(ui: &mut Ui, t: &Theme, key: &str, name: &str, sel: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(112.0, 96.0), Sense::click());
    let prev = Rect::from_min_size(rect.min, Vec2::new(112.0, 70.0));
    let draw = |ui: &Ui, area: Rect, th: &Theme| {
        let p = ui.painter().with_clip_rect(area);
        p.rect_filled(prev, CornerRadius::same(8), c(th.bg));
        let card = Rect::from_min_size(prev.min + Vec2::new(12.0, 12.0), Vec2::new(88.0, 46.0));
        p.rect_filled(card, CornerRadius::same(5), c(th.surface));
        p.rect_stroke(card, CornerRadius::same(5), Stroke::new(1.0, c(th.line)), StrokeKind::Inside);
        p.rect_filled(Rect::from_min_size(card.min + Vec2::new(8.0, 9.0), Vec2::new(40.0, 5.0)), CornerRadius::same(2), c(th.ink));
        p.rect_filled(Rect::from_min_size(card.min + Vec2::new(8.0, 19.0), Vec2::new(60.0, 4.0)), CornerRadius::same(2), c(th.muted));
        p.rect_filled(Rect::from_min_size(card.min + Vec2::new(8.0, 30.0), Vec2::new(28.0, 9.0)), CornerRadius::same(3), c(th.accent));
    };
    match key {
        "light" => draw(ui, prev, &blip_ui::theme::LIGHT),
        "dark" => draw(ui, prev, &blip_ui::theme::DARK),
        _ => {
            let half = prev.center().x;
            draw(ui, Rect::from_min_max(prev.min, Pos2::new(half, prev.max.y)), &blip_ui::theme::LIGHT);
            draw(ui, Rect::from_min_max(Pos2::new(half, prev.min.y), prev.max), &blip_ui::theme::DARK);
        }
    }
    ui.painter().rect_stroke(prev, CornerRadius::same(8), Stroke::new(if sel { 2.0 } else { 1.0 }, c(if sel { t.accent } else { t.line })), StrokeKind::Inside);
    ui.painter().text(Pos2::new(rect.center().x, rect.max.y - 10.0), egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(12.5), if sel { c(t.ink) } else { c(t.muted) });
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

fn page_general(ui: &mut Ui, t: &Theme, app: &mut App, el: &ActiveEventLoop, s: &mut SettingsPage) {
    gui::card(ui, t, 14, |ui| {
        gui::setting_row(ui, t, "Your name", "How you appear to colleagues' pets.", |ui| {
            let r = gui::field(ui, &mut s.my_name, 170.0, "", false);
            if gui::committed(ui, &r) {
                set_my_name(app, el, &s.my_name);
            }
        });
        gui::hline(ui, t);
        gui::setting_row(ui, t, "Appearance", "Colours for these windows and the menu.", |_| {});
        ui.add_space(6.0);
        let cur = if app.cfg.str("theme").is_empty() { "system".to_string() } else { app.cfg.str("theme").to_string() };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            for (key, name) in [("system", "Match system"), ("light", "Light"), ("dark", "Dark")] {
                if theme_tile(ui, t, key, name, cur == key) {
                    app.apply_theme(key);
                }
            }
        });
        gui::hline(ui, t);
        let mut on = app.cfg.bool("sound");
        if gui::toggle_row(ui, t, "Sounds", "Chirps for calls, messages, timers and alarms.", &mut on) {
            app.cfg.set("sound", json!(on));
            if on {
                let v = app.volume();
                app.sound.preview(v);
            }
        }
        ui.add_space(6.0);
        ui.add_enabled_ui(on, |ui| {
            ui.horizontal(|ui| {
                gui::icon(ui, "speaker", gui::px(ui, 1.6), c(t.muted));
                let mut v = app.volume() as f32;
                ui.spacing_mut().slider_width = 260.0;
                let r = ui.add(egui::Slider::new(&mut v, 0.0..=100.0).show_value(false));
                if r.changed() {
                    app.cfg.set("volume", json!(v.round() as i64));
                }
                if r.drag_stopped() || (r.clicked() && !r.dragged()) {
                    app.sound.preview(v as f64);
                }
                gui::muted(ui, t, &format!("{}%", v.round()));
            });
        });
        gui::hline(ui, t);
        let mut start = app.startup_on;
        let title = if cfg!(target_os = "macos") { "Open at login" } else { "Start with Windows" };
        if gui::toggle_row(ui, t, title, "Blip appears when you sign in.", &mut start) {
            // a test run (BLIP_DATA) must not touch the real login item
            if std::env::var_os("BLIP_DATA").is_none() {
                platform::set_startup(start);
            }
            app.startup_on = platform::startup_enabled();
        }
        if cfg!(target_os = "macos") {
            gui::hline(ui, t);
            let mut dock = app.cfg.bool("mac_dock");
            if gui::toggle_row(ui, t, "Show in the Dock", "Off: Blip lives only on your desktop. Right-click the pet, or press and hold it, for the menu.", &mut dock) {
                app.cfg.set("mac_dock", json!(dock));
                platform::show_in_dock(dock);
            }
        }
        gui::hline(ui, t);
        let mut wander = app.body.wander;
        if gui::toggle_row(ui, t, "Wander around", "Let your pet walk along the bottom of the screen.", &mut wander) {
            app.run(el, "wander");
        }
    });
    gui::section(ui, t, "Updates");
    gui::card(ui, t, 14, |ui| {
        let mut auto = app.cfg.bool("auto_update");
        if gui::toggle_row(ui, t, "Update automatically", "New versions install themselves quietly in the background.", &mut auto) {
            app.cfg.set("auto_update", json!(auto));
        }
        gui::hline(ui, t);
        let (kind, text) = app.update_line();
        let col = match kind {
            "ok" => t.ok,
            "new" | "installing" => t.accent,
            "error" => t.danger,
            "checking" => t.warn,
            _ => t.muted,
        };
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(format!("Blip {VERSION}"));
                ui.horizontal(|ui| {
                    gui::dot(ui, col, 8.0);
                    gui::colored(ui, &text, if matches!(kind, "idle" | "checking") { t.muted } else { col }, 12.5);
                });
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let busy = matches!(kind, "checking" | "installing");
                if gui::button(ui, t, "Check now", Some("update"), Kind::Soft, true, !busy).clicked() {
                    app.run(el, "update:check");
                }
            });
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if gui::button(ui, t, "Release notes", Some("open"), Kind::Ghost, true, true).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab("https://github.com/safder2000/blip/releases"));
            }
            if gui::button(ui, t, "Open data folder", Some("open"), Kind::Ghost, true, true).clicked() {
                platform::open_path(&blip_core::store::data_dir());
            }
        });
    });
}

fn set_my_name(app: &mut App, el: &ActiveEventLoop, v: &str) {
    let v: String = v.trim().chars().take(24).collect();
    if !v.is_empty() && v != app.cfg.str("display_name") {
        app.cfg.set("display_name", json!(v));
        app.run(el, "net:rescan");
    }
}

fn page_menu(ui: &mut Ui, t: &Theme, app: &mut App) {
    gui::muted(ui, t, "Choose what shows when you right-click your pet. Settings is always there; everything you hide stays available here in Settings.");
    ui.add_space(10.0);
    gui::card(ui, t, 14, |ui| {
        for (i, (key, title, desc)) in MENU_ITEMS.iter().enumerate() {
            if i > 0 {
                gui::hline(ui, t);
            }
            let mut on = app.cfg.menu_on(key);
            if gui::toggle_row(ui, t, title, desc, &mut on) {
                app.cfg.set_menu(key, on);
            }
        }
    });
}

fn set_server(app: &mut App, el: &ActiveEventLoop, s: &mut SettingsPage) {
    use blip_proto::link;
    let text = s.server.trim().to_string();
    if text.is_empty() {
        if !app.cfg.str("server").is_empty() {
            app.cfg.set("server", json!(""));
            app.run(el, "net:restart");
        }
        s.server.clear();
        s.server_note = None;
        return;
    }
    let Some(info) = link::parse(&text) else {
        s.server_note = Some(("That doesn't look like a link or an address.".into(), t_danger(app), f64::MAX));
        return;
    };
    let same_server = link::parse(app.cfg.str("server")).map(|l| l.web) == Some(info.web.clone());
    let team = if !info.team.is_empty() { info.team.clone() } else if same_server { app.cfg.str("team").to_string() } else { String::new() };
    let base = if text.starts_with("ws://") || text.starts_with("wss://") { text.split('#').next().unwrap_or("").to_string() } else { info.web.clone() };
    s.server = base.clone();
    s.server_note = None;
    if base == app.cfg.str("server") && team == app.cfg.str("team") {
        return;
    }
    app.cfg.set("server", json!(base));
    app.cfg.set("team", json!(team));
    app.run(el, "net:restart");
}

fn t_danger(app: &App) -> Color {
    app.theme.danger
}

fn page_office(ui: &mut Ui, t: &Theme, app: &mut App, el: &ActiveEventLoop, s: &mut SettingsPage) {
    gui::card(ui, t, 14, |ui| {
        let (text, col) = office::status(app);
        ui.horizontal(|ui| {
            gui::dot(ui, col, 12.0);
            gui::bold(ui, &text);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if gui::button(ui, t, "Look again", Some("reset"), Kind::Soft, true, true).clicked() {
                    app.run(el, "net:rescan");
                }
            });
        });
        ui.add_space(4.0);
        gui::muted(ui, t, &office::this_pc());
        gui::muted(ui, t, "Pets on the same office network find each other on their own - nothing to set up.");
    });
    gui::section(ui, t, "Who can see you");
    gui::card(ui, t, 14, |ui| {
        let mut net = app.cfg.bool("network");
        if gui::toggle_row(ui, t, "Office network", "Find pets nearby and take calls and messages.", &mut net) {
            app.cfg.set("network", json!(net));
            app.run(el, "net:restart");
        }
        gui::hline(ui, t);
        if gui::toggle_row(ui, t, "Private group", "Only pets with the same group password can see each other. Off: everyone in the office who has Blip.", &mut s.private) && !s.private {
            if app.cfg.str("passphrase") != office::DEFAULT_GROUP {
                app.cfg.set("passphrase", json!(office::DEFAULT_GROUP));
                app.run(el, "net:restart");
            }
            s.group.clear();
        }
        if s.private {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let r = gui::field(ui, &mut s.group, 240.0, "Group password", !s.show_group);
                if gui::committed(ui, &r) {
                    let v = s.group.trim().to_string();
                    if !v.is_empty() && v != app.cfg.str("passphrase") {
                        app.cfg.set("passphrase", json!(v));
                        app.run(el, "net:restart");
                    }
                }
                if gui::icon_button(ui, t, if s.show_group { "x" } else { "info" }).clicked() {
                    s.show_group = !s.show_group;
                }
            });
        }
    });
    gui::section(ui, t, "Online server (optional)");
    gui::card(ui, t, 14, |ui| {
        gui::muted(ui, t, "Talk to teammates anywhere: paste the invite link someone shared, or type a server's IP and port. Messages and calls are encrypted end to end; the server can't read them.");
        ui.add_space(8.0);
        let w = ui.available_width();
        let r = gui::field(ui, &mut s.server, w, "https://blip.example.com/#team=...  or  192.168.1.200:30620", false);
        if gui::committed(ui, &r) {
            set_server(app, el, s);
        }
        ui.add_space(6.0);
        let now = blip_core::now();
        let on = !app.cfg.str("server").is_empty();
        let (text, col) = match &s.server_note {
            Some((m, c, until)) if *until > now => (m.clone(), *c),
            _ => office::server_status(app),
        };
        ui.horizontal(|ui| {
            gui::dot(ui, col, 8.0);
            gui::colored(ui, &text, col, 12.5);
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if gui::button(ui, t, "Copy link", Some("open"), Kind::Soft, true, on).clicked() {
                if let Some(info) = blip_proto::link::parse(app.cfg.str("server")) {
                    let inv = blip_proto::link::invite_link(&info, app.cfg.str("team"));
                    ui.ctx().copy_text(inv);
                    s.server_note = Some(("Link copied. Anyone who pastes it joins your team.".into(), t.ok, now + 4.0));
                }
            }
            if gui::button(ui, t, "Disconnect", Some("x"), Kind::Ghost, true, on).clicked() {
                s.server.clear();
                set_server(app, el, s);
            }
        });
        if on {
            let private = !app.cfg.str("team").is_empty();
            ui.add_space(6.0);
            gui::muted(
                ui,
                t,
                if private {
                    "Team: private (from an invite link). Only people with that link see you."
                } else {
                    "Team: everyone using this server. Colleagues just enter the same address."
                },
            );
            if private && gui::button(ui, t, "Join the shared team instead", None, Kind::Ghost, true, true).clicked() {
                app.cfg.set("team", json!(""));
                app.run(el, "net:restart");
            }
        }
    });
    gui::section(ui, t, "Other floors (optional)");
    gui::card(ui, t, 14, |ui| {
        gui::muted(ui, t, "Only if colleagues on another floor or Wi-Fi don't show up. Adding one of their PCs is enough - pets introduce each other.");
        ui.add_space(8.0);
        gui::setting_row(ui, t, "Their PC address", "One or more IPs, comma separated.", |ui| {
            let r = gui::field(ui, &mut s.peers, 170.0, "192.168.1.20", false);
            if gui::committed(ui, &r) && s.peers.trim() != app.cfg.str("manual_peers") {
                app.cfg.set("manual_peers", json!(s.peers.trim()));
                app.run(el, "net:restart");
            }
        });
    });
}

// ---------------------------------------------------------------- notes
fn check_box(ui: &mut Ui, t: &Theme, on: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
    let r = rect.shrink(1.0);
    if on {
        ui.painter().rect_filled(r, CornerRadius::same(5), c(t.accent));
        let p = gui::px(ui, 2.0);
        gui::paint_glyph(ui, r.center().x - 2.5 * p, r.center().y - 3.5 * p, blip_pet::pixfont::icon("check"), p, c(t.on_accent));
    } else {
        ui.painter().rect_filled(r, CornerRadius::same(5), c(if resp.hovered() { t.hover } else { t.tile }));
        ui.painter().rect_stroke(r, CornerRadius::same(5), Stroke::new(1.5, c(if resp.hovered() { t.accent } else { t.off })), StrokeKind::Inside);
    }
    resp.clicked()
}

fn when_ts(choice: &str, typed: &str) -> Result<Option<f64>, String> {
    let now = blip_core::now();
    match choice {
        "none" => Ok(None),
        "pick" => parse_when(typed, now).map(Some).ok_or_else(|| "Try 'in 20m', '5pm', '17:30' or 'tomorrow 10'.".to_string()),
        "30m" => Ok(parse_when("in 30m", now)),
        "1h" => Ok(parse_when("in 1h", now)),
        _ => Ok(parse_when("tomorrow 9:00", now)),
    }
}

fn notes_ui(ui: &mut Ui, t: &Theme, app: &mut App, el: &ActiveEventLoop, s: &mut NotesPage) {
    header(ui, t, "note", "Notes", "Jot things down. Add a time and your pet reminds you.");
    let mut add = false;
    gui::card(ui, t, 14, |ui| {
        let w = ui.available_width();
        let r = gui::field(ui, &mut s.text, w, "Write a note\u{2026}", false);
        if s.focus {
            r.request_focus();
            s.focus = false;
        }
        if gui::enter_pressed(ui, &r) {
            add = true;
        }
        ui.add_space(8.0);
        if gui::chips(ui, t, &WHEN, &mut s.when) {
            s.err.clear();
        }
        if s.when == "pick" {
            ui.add_space(8.0);
            let r = gui::field(ui, &mut s.picked, 260.0, "in 20m \u{b7} 5pm \u{b7} 17:30 \u{b7} tomorrow 10", false);
            if gui::enter_pressed(ui, &r) {
                add = true;
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            gui::colored(ui, &s.err, t.danger, 12.5);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = if s.when == "none" { "Add note" } else { "Add reminder" };
                if gui::button(ui, t, label, Some("plus"), Kind::Primary, true, true).clicked() {
                    add = true;
                }
            });
        });
    });
    if add {
        let text = s.text.trim().to_string();
        if text.is_empty() {
            s.focus = true;
        } else {
            match when_ts(&s.when, &s.picked) {
                Err(e) => s.err = e,
                Ok(ts) => {
                    app.notes.add(&text, ts, blip_core::now());
                    s.text.clear();
                    s.picked.clear();
                    s.when = "none".into();
                    s.err.clear();
                    s.focus = true;
                    if let Some(ts) = ts {
                        let d = clock::describe_when(ts, blip_core::now());
                        app.board(&format!("Reminder {d}"), 4.0);
                    }
                }
            }
        }
    }
    let items = app.notes.ordered();
    let open_n = items.iter().filter(|n| !notes::done(n)).count();
    let pend = app.notes.pending();
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let mut txt = if items.is_empty() { String::new() } else { format!("{open_n} note{}", if open_n == 1 { "" } else { "s" }) };
        if pend > 0 {
            txt += &format!("  \u{b7}  {pend} reminder{} set", if pend == 1 { "" } else { "s" });
        }
        gui::muted(ui, t, &txt);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let any_done = items.iter().any(notes::done);
            if gui::button(ui, t, "Clear done", Some("trash"), Kind::Ghost, true, any_done).clicked() {
                app.notes.clear_done();
            }
        });
    });
    ui.add_space(4.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if items.is_empty() {
            gui::empty_state(ui, t, "No notes yet", "Write something above. Pick a time and your pet pops it up when it's due, even after a restart.");
            return;
        }
        let now = blip_core::now();
        for n in &items {
            let nid = notes::id(n).to_string();
            let done = notes::done(n);
            gui::card(ui, t, 10, |ui| {
                ui.horizontal_top(|ui| {
                    if check_box(ui, t, done) {
                        app.notes.complete(&nid, !done);
                    }
                    let btn_w = 70.0;
                    let w = (ui.available_width() - btn_w).max(100.0);
                    ui.allocate_ui_with_layout(Vec2::new(w, 0.0), Layout::top_down(Align::Min), |ui| {
                        ui.set_width(w);
                        let mut rt = RichText::new(notes::text(n)).size(14.0).color(c(if done { t.muted } else { t.ink }));
                        if done {
                            rt = rt.strikethrough();
                        }
                        ui.add(egui::Label::new(rt).wrap());
                        if let Some(ts) = notes::remind(n) {
                            let when = clock::describe_when(ts, now);
                            if done || notes::fired(n) {
                                let info = if done { format!("reminder {when}") } else { format!("reminded {when}") };
                                gui::colored(ui, &info, t.muted, 12.0);
                            } else {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 5.0;
                                    gui::icon(ui, "bell", gui::px(ui, 1.6), c(t.accent));
                                    gui::colored(ui, &when, t.accent, 12.0);
                                });
                            }
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if gui::icon_button(ui, t, "trash").clicked() {
                            app.notes.delete(&nid);
                        }
                        if gui::icon_button(ui, t, "bell").clicked() {
                            app.want_window.push(format!("ask:note-time:{nid}"));
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }
    });
    let _ = el;
}

// ---------------------------------------------------------------- timers
fn timers_ui(ui: &mut Ui, t: &Theme, app: &mut App, el: &ActiveEventLoop, s: &mut TimersPage) {
    header(ui, t, "hourglass", "Timers & alarms", "Countdowns show on a little sign above your pet.");
    gui::card(ui, t, 14, |ui| {
        gui::bold(ui, "Timer");
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for m in [1, 5, 10, 15, 25, 30, 45, 60] {
                if gui::button(ui, t, &format!("{m}m"), None, Kind::Soft, true, true).clicked() {
                    app.start_timer(m as f64 * 60.0, s.label.trim());
                }
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let r = gui::field(ui, &mut s.dur, 90.0, "25 / 1:30", false);
            let go_enter = gui::enter_pressed(ui, &r);
            gui::field(ui, &mut s.label, 150.0, "Label (optional)", false);
            let go = gui::button(ui, t, "Start", Some("playtri"), Kind::Primary, true, true).clicked() || go_enter;
            if go {
                match parse_duration(&s.dur) {
                    Some(secs) if secs > 0.0 => {
                        s.err.clear();
                        app.start_timer(secs, s.label.trim());
                    }
                    _ => s.err = "Try 25, 1:30, 90s or 2h 15m.".into(),
                }
            }
            if gui::button(ui, t, "Stopwatch", Some("stopwatch"), Kind::Soft, true, true).clicked() {
                app.start_stopwatch(s.label.trim());
            }
        });
    });
    ui.add_space(10.0);
    gui::card(ui, t, 14, |ui| {
        gui::bold(ui, "Alarm");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            gui::field(ui, &mut s.at, 70.0, "9:30", false);
            gui::field(ui, &mut s.alabel, 150.0, "Label (optional)", false);
            if gui::button(ui, t, "Add alarm", Some("bell"), Kind::Primary, true, true).clicked() {
                match parse_clock(&s.at) {
                    Some((hh, mm)) => {
                        s.err.clear();
                        app.add_alarm(hh, mm, &repeat_days(&s.repeat), s.alabel.trim());
                    }
                    None => s.err = "Alarm time like 9:30, 17:45 or 5pm.".into(),
                }
            }
        });
        ui.add_space(8.0);
        gui::chips(ui, t, &REPEAT, &mut s.repeat);
    });
    if !s.err.is_empty() {
        gui::colored(ui, &s.err, t.danger, 12.5);
    }
    gui::section(ui, t, "Running");
    ui.add_space(4.0);
    let now = blip_core::now();
    let order = |k: &str| match k {
        "timer" => 0,
        "stopwatch" => 1,
        _ => 2,
    };
    let mut items = app.timers.items.clone();
    let key = |i: &timers::Item| -> f64 {
        i.get("end").and_then(Value::as_f64).unwrap_or_else(|| {
            i.get("hh").and_then(Value::as_f64).unwrap_or(0.0) * 60.0 + i.get("mm").and_then(Value::as_f64).unwrap_or(0.0)
        })
    };
    items.sort_by(|a, b| order(timers::kind(a)).cmp(&order(timers::kind(b))).then(key(a).total_cmp(&key(b))));
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if items.is_empty() {
            gui::empty_state(ui, t, "Nothing running", "Start a timer or add an alarm above.");
            return;
        }
        for it in &items {
            let kind = timers::kind(it).to_string();
            let iid = timers::id(it).to_string();
            let left = it.get("left").map_or(false, |v| !v.is_null());
            let running = it.get("running").and_then(Value::as_bool).unwrap_or(false);
            let enabled = it.get("enabled").and_then(Value::as_bool).unwrap_or(true);
            let (big, icon, sub, frac) = match kind.as_str() {
                "timer" => {
                    let rem = Timers::remaining(it, now);
                    let dur = it.get("dur").and_then(Value::as_f64).unwrap_or(1.0).max(1.0);
                    (fmt_clock(rem, true), "hourglass", format!("of {}{}", fmt_clock(dur, true), if left { "  \u{b7}  paused" } else { "" }), Some((rem / dur) as f32))
                }
                "stopwatch" => (fmt_clock(Timers::elapsed(it, now), false), "stopwatch", if running { "running".into() } else { "paused".into() }, None),
                _ => {
                    let hh = it.get("hh").and_then(Value::as_i64).unwrap_or(0);
                    let mm = it.get("mm").and_then(Value::as_i64).unwrap_or(0);
                    let mut sub = if enabled { describe_days(&timers::days(it)) } else { "off".into() };
                    if let Some(sn) = it.get("snooze").and_then(Value::as_f64) {
                        sub += &format!("  \u{b7}  snoozed {}", fmt_clock(sn - now, true));
                    }
                    (format!("{hh:02}:{mm:02}"), "bell", sub, None)
                }
            };
            let paused = (kind == "timer" && left) || (kind == "stopwatch" && !running) || (kind == "alarm" && !enabled);
            gui::card(ui, t, 10, |ui| {
                ui.horizontal(|ui| {
                    let p = gui::px(ui, 2.0);
                    ui.allocate_ui_with_layout(Vec2::new(150.0, 24.0), Layout::left_to_right(Align::Center), |ui| {
                        ui.set_width(150.0);
                        gui::icon(ui, icon, p, c(t.accent));
                        ui.add_space(4.0);
                        gui::pixel_text(ui, &big, p, c(t.ink));
                    });
                    ui.vertical(|ui| {
                        let name = if timers::label(it).is_empty() {
                            match kind.as_str() {
                                "timer" => "Timer",
                                "stopwatch" => "Stopwatch",
                                _ => "Alarm",
                            }
                            .to_string()
                        } else {
                            timers::label(it).to_string()
                        };
                        gui::bold(ui, &name);
                        gui::muted(ui, t, &sub);
                        if let Some(f) = frac {
                            gui::bar(ui, t, 120.0, f, t.accent);
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if gui::icon_button(ui, t, "trash").clicked() {
                            app.timers.remove(&iid);
                        }
                        if kind != "alarm" && gui::icon_button(ui, t, "reset").clicked() {
                            app.timers.reset(&iid, blip_core::now());
                        }
                        if gui::icon_button(ui, t, if paused { "playtri" } else { "pause" }).clicked() {
                            app.timers.toggle_pause(&iid, blip_core::now());
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }
    });
    let _ = el;
}

// ---------------------------------------------------------------- buddies
fn buddies_ui(ui: &mut Ui, t: &Theme, app: &mut App, el: &ActiveEventLoop, s: &mut BuddiesPage) {
    header(ui, t, "people", "Buddies", "Pets on your office network show up here by themselves.");
    gui::card(ui, t, 14, |ui| {
        gui::setting_row(ui, t, "You appear as", "", |ui| {
            let r = gui::field(ui, &mut s.my_name, 170.0, "", false);
            if gui::committed(ui, &r) {
                set_my_name(app, el, &s.my_name);
            }
        });
        gui::hline(ui, t);
        let mut dnd = app.cfg.bool("dnd");
        if gui::toggle_row(ui, t, "Do not disturb", "Calls are declined with \u{201c}busy\u{201d} and nothing rings.", &mut dnd) {
            app.run(el, "dnd");
        }
    });
    gui::section(ui, t, "Nearby");
    gui::muted(ui, t, &office::status(app).0);
    ui.add_space(4.0);
    let peers = office::peers(app);
    let inbox = office::inbox(app);
    let h = ui.available_height();
    egui::ScrollArea::vertical().id_salt("people").max_height((h * 0.45).max(120.0)).auto_shrink([false, true]).show(ui, |ui| {
        if peers.is_empty() {
            gui::empty_state(ui, t, "No one nearby yet", "Colleagues appear here automatically once they install Blip on the same network.");
            ui.vertical_centered(|ui| {
                if gui::button(ui, t, "Look again", Some("reset"), Kind::Soft, true, true).clicked() {
                    app.run(el, "net:rescan");
                }
            });
        }
        for p in &peers {
            gui::card(ui, t, 8, |ui| {
                ui.horizontal(|ui| {
                    let g = gui::pet_sprite(&p.species, &p.palette, &p.stage, "normal", 1);
                    let px = gui::px(ui, 1.5);
                    gui::avatar(ui, &g, px, 30.0);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            gui::bold(ui, &p.name);
                            if p.via == "server" {
                                gui::icon(ui, "globe", gui::px(ui, 1.6), c(blip_ui::menu::NET_BLUE));
                            }
                        });
                        let busy = p.status == "busy";
                        ui.horizontal(|ui| {
                            gui::dot(ui, if busy { t.warn } else { t.ok }, 8.0);
                            let mut txt = if busy { "Do not disturb".to_string() } else { "Available".to_string() };
                            if !p.ip.is_empty() {
                                txt += &format!("  \u{b7}  {}", p.ip);
                            }
                            gui::muted(ui, t, &txt);
                        });
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if gui::button(ui, t, "Message", Some("mail"), Kind::Soft, true, true).clicked() {
                            app.want_window.push(format!("ask:message:{}", p.id));
                        }
                        if gui::button(ui, t, "Call", Some("phone"), Kind::Primary, true, true).clicked() {
                            app.run(el, &format!("call:{}", p.id));
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }
    });
    gui::section(ui, t, "Inbox");
    ui.add_space(4.0);
    egui::ScrollArea::vertical().id_salt("inbox").max_height((ui.available_height() - 40.0).max(80.0)).auto_shrink([false, false]).show(ui, |ui| {
        if inbox.is_empty() {
            gui::muted(ui, t, "No messages yet.");
        }
        for e in inbox.iter().rev().take(50) {
            let mine = e.out;
            let layout = if mine { Layout::right_to_left(Align::Min) } else { Layout::left_to_right(Align::Min) };
            ui.with_layout(layout, |ui| {
                egui::Frame::new()
                    .fill(c(if mine { t.accent_soft } else { t.surface }))
                    .stroke(Stroke::new(1.0, c(if mine { t.accent_soft } else { t.line })))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.set_max_width(340.0);
                        let who = if mine { format!("You \u{2192} {}", e.name) } else { e.name.clone() };
                        gui::colored(ui, &format!("{who}  {}", hhmm(e.ts)), t.muted, 11.5);
                        // call notes start with an emoji the UI font lacks: show a pixel icon
                        let (icon, text) = match e.text.chars().next() {
                            Some('\u{1F4DE}') => (Some("phone"), e.text.trim_start_matches('\u{1F4DE}').trim_start()),
                            Some('\u{1F3A7}') => (Some("speaker"), e.text.trim_start_matches('\u{1F3A7}').trim_start()),
                            _ => (None, e.text.as_str()),
                        };
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            if let Some(i) = icon {
                                gui::icon(ui, i, gui::px(ui, 1.6), c(t.accent));
                            }
                            ui.add(egui::Label::new(RichText::new(text).size(14.0)).wrap());
                        });
                    });
                if !mine && !e.peer.is_empty() && gui::button(ui, t, "Reply", None, Kind::Ghost, true, true).clicked() {
                    app.want_window.push(format!("ask:message:{}", e.peer));
                }
            });
            ui.add_space(4.0);
        }
    });
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if gui::button(ui, t, "Clear inbox", Some("trash"), Kind::Ghost, true, !inbox.is_empty()).clicked() {
            app.run(el, "inbox:clear");
        }
    });
}

// ---------------------------------------------------------------- welcome
fn welcome_ui(ui: &mut Ui, t: &Theme, app: &mut App, s: &mut WelcomePage) -> bool {
    header(ui, t, "heart", "Welcome to Blip", "Two quick things and you're set.");
    let mut done = false;
    gui::card(ui, t, 14, |ui| {
        gui::h2(ui, "What's your name?");
        gui::muted(ui, t, "Colleagues see this name with your pet when you call or message them.");
        ui.add_space(6.0);
        let w = ui.available_width();
        let r = gui::field(ui, &mut s.name, w, "", false);
        if s.focus {
            r.request_focus();
            s.focus = false;
        }
        if gui::enter_pressed(ui, &r) {
            done = true;
        }
    });
    gui::section(ui, t, "Pick your pet");
    species_tiles(ui, t, app);
    ui.add_space(4.0);
    swatches(ui, t, app);
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        ui.label("Pet's name");
        gui::field(ui, &mut s.pet, 160.0, "", false);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if gui::button(ui, t, "Let's go!", Some("heart"), Kind::Primary, false, true).clicked() {
                done = true;
            }
        });
    });
    done
}

fn finish_welcome(app: &mut App, el: &ActiveEventLoop, w: &WelcomePage) {
    let name: String = w.name.trim().chars().take(24).collect();
    let name = if name.is_empty() { app.cfg.str("display_name").to_string() } else { name };
    let pet: String = w.pet.trim().chars().take(16).collect();
    app.cfg.set("display_name", json!(name));
    app.cfg.set("named", json!(true));
    if !pet.is_empty() {
        app.pet.set("name", json!(pet));
    }
    app.save();
    app.run(el, "net:rescan");
    app.chirp("love", Some("heart"), 5.0);
    let first = name.split_whitespace().next().unwrap_or("there").to_string();
    app.board(&format!("Hi {first}!"), 5.0);
}

// ---------------------------------------------------------------- ask dialogs
fn ask_ui(ui: &mut Ui, t: &Theme, app: &mut App, el: &ActiveEventLoop, s: &mut AskPage) -> bool {
    let mut ok = false;
    let mut cancel = false;
    gui::bold(ui, &s.prompt);
    ui.add_space(8.0);
    let w = ui.available_width();
    let r = gui::field(ui, &mut s.text, w, &s.hint, false);
    if s.focus {
        r.request_focus();
        s.focus = false;
    }
    if gui::enter_pressed(ui, &r) {
        ok = true;
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        cancel = true;
    }
    ui.add_space(4.0);
    gui::colored(ui, &s.err, t.danger, 12.5);
    ui.add_space(4.0);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if gui::button(ui, t, &s.ok, None, Kind::Primary, true, true).clicked() {
            ok = true;
        }
        if gui::button(ui, t, "Cancel", None, Kind::Soft, true, true).clicked() {
            cancel = true;
        }
    });
    if cancel {
        return true;
    }
    if !ok {
        return false;
    }
    let text = s.text.trim().to_string();
    let (k, rest) = s.kind.split_once(':').unwrap_or((s.kind.as_str(), ""));
    let now = blip_core::now();
    match k {
        "alarm" => match parse_clock(&text) {
            Some((hh, mm)) => app.add_alarm(hh, mm, &[], ""),
            None => {
                s.err = "Alarm time like 9:30, 17:45 or 5pm.".into();
                return false;
            }
        },
        "timer" => match parse_duration(&text) {
            Some(secs) if secs > 0.0 => app.start_timer(secs, ""),
            _ => {
                s.err = "Try 25, 1:30, 90s or 2h 15m.".into();
                return false;
            }
        },
        "note-time" => {
            if text.is_empty() {
                app.notes.set_reminder(rest, None);
            } else {
                match parse_when(&text, now) {
                    Some(ts) => app.notes.set_reminder(rest, Some(ts)),
                    None => {
                        s.err = "Try 5pm, 17:30 or in 20m.".into();
                        return false;
                    }
                }
            }
            app.windows_dirty = true;
        }
        "message" => {
            if text.is_empty() {
                return false;
            }
            app.run(el, &format!("msg:{rest}:{text}"));
        }
        "reply" => {
            if text.is_empty() {
                return false;
            }
            app.run(el, &format!("reply:{rest}:{text}"));
        }
        _ => {}
    }
    true
}
