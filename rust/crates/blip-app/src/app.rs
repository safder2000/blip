//! The running app: the pet window, its life and needs, the menu, cards, poops, sound, timers and
//! reminders, and the tray / menu-bar icon. Windows (Settings, Notes...) live in `windows.rs`.

use std::sync::Arc;
use std::time::Instant;

use blip_core::config::Config;
use blip_core::cry::cry;
use blip_core::notes::{self, Notes};
use blip_core::pet::{BodyCmd, Ev, Pet, EGG_TIME};
use blip_core::timers::{self, Timers};
use blip_core::{clock, store};
use blip_pet::body::{Action, Body, Geo, Gesture, Landing, Look, Rect, ANIM, STEP};
use blip_pet::sprites::Stage;
use blip_ui::menu::{CareButton, Item, Menu, PetCard, Tool};
use blip_ui::scene::{self, Frame, Particle};
use blip_ui::text::Fonts;
use blip_ui::theme::{self, Theme};
use blip_ui::Canvas;
use serde_json::{json, Value};
use tray_icon::menu::{CheckMenuItem, Menu as TrayMenu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton as TrayButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::WindowId;

use crate::cards::{CardOut, CardStack, ShowCard};
use crate::overlay::Overlay;
use crate::platform;
use crate::sound::Sound;

/// The open right-click menu.
struct MenuWin {
    win: Overlay,
    menu: Menu,
    canvas: Canvas,
    had_focus: bool,
    opened: f64,
    away_since: Option<f64>,
    dirty: bool,
    sticky: bool,
    /// a mouse button was down at the last tick (so a new click outside closes the menu)
    btn_was: bool,
}

struct PoopWin {
    win: Overlay,
    x: f64,
    /// picked up at (app time): dissolves with a sparkle, then goes
    gone_at: Option<f64>,
}

struct Tray {
    _icon: TrayIcon,
    open: MenuItem,
    show: CheckMenuItem,
    settings: MenuItem,
    quit: MenuItem,
}

pub struct App {
    pub office: crate::office::Office,
    pub bc: crate::basecamp::Basecamp,
    pub updater: crate::updater::Updater,
    pub voice: crate::voice::Voice,
    next_theme: f64,
    pub cfg: Config,
    pub pet: Pet,
    pub body: Body,
    pub timers: Timers,
    pub notes: Notes,
    pub geo: Geo,
    pub work: Rect,
    pet_win: Option<Overlay>,
    canvas: Canvas,
    menu: Option<MenuWin>,
    pub cards: CardStack,
    poops: Vec<PoopWin>,
    parts: Vec<Particle>,
    bubble: Option<(String, Option<&'static str>, f64)>,
    board: Option<(String, f64)>,
    food: Option<(&'static str, f64)>,
    board_rect: Option<(i32, i32, i32, i32)>,
    sign_rect: Option<(i32, i32, i32, i32)>,
    top_row: i32,
    alerts: Vec<(String, &'static str)>,
    temp_shown: bool,
    pub sound: Sound,
    pub fonts: Fonts,
    pub theme: Theme,
    tray: Option<Tray>,
    t0: Instant,
    next_step: f64,
    next_anim: f64,
    next_sim: f64,
    next_work: f64,
    last_save: f64,
    last_nag: f64,
    pressed: bool,
    modifiers: ModifiersState,
    pub quit: bool,
    demo: Option<String>,
    /// windows the user asked for (Settings, Notes...), opened by the windows module
    pub want_window: Vec<String>,
    /// test steps for the newest window (BLIP_DEMO "ui:click:x,y" ...)
    pub ui_script: Vec<String>,
    later: Vec<String>,
    timed: Vec<(f64, String)>,
    /// something changed that the open windows show: redraw them now
    pub windows_dirty: bool,
    pub startup_on: bool,
    /// (kind, text) for Settings -> Updates: idle | checking | ok | new | installing | error
    pub update_state: Option<(&'static str, String)>,
}

fn look_of(pet: &Pet) -> Look {
    let stage = match pet.stage() {
        "child" => Stage::Child,
        "adult" => Stage::Adult,
        _ => Stage::Baby,
    };
    Look { species: pet.species().to_string(), palette: pet.palette(), stage }
}

impl App {
    pub fn new() -> App {
        let dir = store::data_dir();
        let wall = blip_core::now();
        let cfg = Config::load(dir.join("config.json"), &platform::user_name());
        let mut pet = Pet::load(dir.join("state.json"), wall);
        if pet.stage() == "egg" {
            pet.surprise_look();
        }
        let saved_x = pet.st.get("x").and_then(Value::as_f64);
        let (work, scale) = platform::work_area_at(saved_x.unwrap_or(f64::MAX), 500.0);
        let geo = Geo::new(scale);
        let x = saved_x.unwrap_or(work.right - geo.w as f64 - 140.0 * scale);
        let mut body = Body::new(x, Body::ground(&geo, &work), blip_core::Rng::new().f().to_bits());
        body.wander = pet.st.get("wander").and_then(Value::as_bool).unwrap_or(true);
        body.scale_hint = geo.s;
        let fonts = Fonts::system().expect("a system font (Segoe UI / SF)");
        let theme = theme::pick(cfg.str("theme"), platform::system_dark());
        let mut app = App {
            office: crate::office::Office::load(&dir),
            bc: Default::default(),
            updater: Default::default(),
            voice: Default::default(),
            next_theme: 0.0,
            cfg,
            timers: Timers::load(dir.join("timers.json")),
            notes: Notes::load(dir.join("notes.json")),
            pet,
            body,
            geo,
            work,
            pet_win: None,
            canvas: Canvas::new(1, 1),
            menu: None,
            cards: CardStack::default(),
            poops: vec![],
            parts: vec![],
            bubble: None,
            board: None,
            food: None,
            board_rect: None,
            sign_rect: None,
            top_row: 14,
            alerts: vec![],
            temp_shown: false,
            sound: Sound::new(),
            fonts,
            theme,
            tray: None,
            t0: Instant::now(),
            next_step: 0.0,
            next_anim: 0.0,
            next_sim: 1.0,
            next_work: 0.0,
            last_save: 0.0,
            last_nag: 0.0,
            pressed: false,
            modifiers: ModifiersState::empty(),
            quit: false,
            demo: std::env::var("BLIP_DEMO").ok().or_else(|| std::env::var("BLIP_DEMO_MENU").ok().map(|_| "menu".into())),
            want_window: vec![],
            ui_script: vec![],
            later: vec![],
            timed: vec![],
            windows_dirty: false,
            startup_on: platform::startup_enabled(),
            update_state: None,
        };
        if app.pet.gone() {
            app.body.set(Action::Gone, 1e18, 0.0);
        } else if app.pet.asleep() {
            app.body.set(Action::Sleep, 1e18, 0.0);
        }
        app
    }

    pub fn now(&self) -> f64 {
        self.t0.elapsed().as_secs_f64()
    }

    fn visible(&self) -> bool {
        self.pet.st.get("visible").and_then(Value::as_bool).unwrap_or(true) || self.temp_shown
    }

    pub fn start(&mut self, el: &ActiveEventLoop) {
        let (x, y) = (self.body.x as i32, self.body.y as i32);
        self.pet_win = Overlay::new(el, self.geo.w, self.geo.h, x, y, true);
        crate::log(&format!("start: pet window at {x},{y} size {}x{} scale {} work {:?}", self.geo.w, self.geo.h, self.geo.s, self.work));
        let poops = self.pet.poops();
        for px in poops {
            self.poop_window(el, px);
        }
        self.make_tray();
        self.next_step = self.now();
        if self.pet.stage() == "egg" {
            self.board("A new egg!", 5.0);
        }
        if !self.cfg.bool("named") {
            self.want_window.push("welcome".into());
        }
        crate::office::start(self);
        if self.cfg.bool("mac_dock") {
            platform::show_in_dock(true);
        }
    }

    // ------------------------------------------------ used by the windows
    /// Where windows open: (pet centre x, pet top y, that screen's work area), physical pixels.
    pub fn window_anchor(&self) -> (f64, f64, Rect) {
        let (cx, top) = (self.body.x + self.geo.w as f64 / 2.0, self.body.y + self.geo.h as f64 * 0.3);
        let (work, _) = platform::work_area_at(cx, top);
        (cx, top, work)
    }

    pub fn look(&self) -> Look {
        look_of(&self.pet)
    }

    pub fn volume(&self) -> f64 {
        if self.cfg.v.contains_key("volume") {
            self.cfg.f64("volume")
        } else {
            60.0
        }
    }

    pub fn set_look(&mut self, species: Option<&str>, color: Option<&str>) {
        self.pet.set("look_chosen", json!(true));
        if let Some(s) = species {
            self.pet.set("species", json!(s));
        }
        if let Some(c) = color {
            self.pet.set("color", json!(c));
        }
        if let Some(t) = &self.tray {
            let _ = t._icon.set_icon(Some(crate::tray_image(&look_of(&self.pet))));
        }
        self.save();
        self.run_later("net:rescan");
    }

    /// A command to run on the next tick (when there is no event loop handle at hand).
    pub fn run_later(&mut self, cmd: &str) {
        self.later.push(cmd.to_string());
    }

    pub fn apply_theme(&mut self, name: &str) {
        self.cfg.set("theme", json!(name));
        self.cfg.set("theme_chosen", json!(true));
        self.theme = theme::pick(name, platform::system_dark());
        self.menu = None;
        self.windows_dirty = true;
    }

    pub fn update_line(&self) -> (&'static str, String) {
        if let Some((k, t)) = &self.update_state {
            return (k, t.clone());
        }
        let last = self.cfg.f64("last_update_check");
        if last > 0.0 && blip_core::now() - last < 2.0 * 86400.0 {
            ("ok", format!("You're up to date  \u{b7}  checked {}", crate::windows::ago(last)))
        } else {
            ("idle", "Checks for updates every few hours".into())
        }
    }

    pub fn start_timer(&mut self, secs: f64, label: &str) {
        self.timers.add_timer(secs, label, blip_core::now());
        self.board(&format!("Timer {}", clock::fmt_clock(secs, true)), 3.0);
        self.play_sound("pop", false);
    }

    pub fn start_stopwatch(&mut self, label: &str) {
        self.timers.add_stopwatch(label, blip_core::now());
        self.board("Stopwatch on", 3.0);
        self.play_sound("pop", false);
    }

    pub fn add_alarm(&mut self, hh: u32, mm: u32, days: &[u32], label: &str) {
        self.timers.add_alarm(hh, mm, days, label, blip_core::now());
        self.board(&format!("Alarm {hh:02}:{mm:02} {}", clock::describe_days(days)), 4.0);
        self.play_sound("pop", false);
    }

    pub fn save(&mut self) {
        self.pet.set("x", json!(self.body.x.round()));
        self.pet.set("wall", json!(blip_core::now()));
        self.pet.set("wander", json!(self.body.wander));
        self.pet.save();
        self.last_save = self.now();
    }

    // ------------------------------------------------ speech
    pub fn chirp(&mut self, mood: &'static str, icon: Option<&'static str>, secs: f64) {
        if self.pet.gone() {
            self.bubble = Some(("...".into(), None, self.now() + secs));
        } else if self.pet.stage() != "egg" {
            let species = self.pet.species().to_string();
            let text = cry(&species, mood, &mut self.pet.rng);
            self.bubble = Some((text, icon, self.now() + secs));
        }
    }

    pub fn board(&mut self, text: &str, secs: f64) {
        self.board = Some((text.to_string(), self.now() + secs));
    }

    fn spawn(&mut self, kind: &'static str, n: u32) {
        let rng = &mut self.pet.rng;
        scene::spawn(&mut self.parts, kind, n, self.top_row, &mut || rng.f());
    }

    pub fn play_sound(&mut self, name: &str, looped: bool) {
        let (on, vol) = (self.cfg.bool("sound"), self.cfg.f64("volume"));
        self.sound.play(name, looped, on, if self.cfg.v.contains_key("volume") { vol } else { 60.0 });
    }

    // ------------------------------------------------ the loop
    pub fn tick(&mut self, el: &ActiveEventLoop) {
        let now = self.now();
        if let Some(d) = self.demo.clone() {
            if now > 2.0 {
                self.demo = None;
                self.run_demo(el, &d);
            }
        }
        if now >= self.next_work && self.body.action != Action::Held {
            let (cx, cy) = (self.body.x + self.geo.w as f64 / 2.0, self.body.y + self.geo.h as f64 - 2.0);
            let (work, scale) = platform::work_area_at(cx, cy);
            self.work = work;
            if (scale.max(1.0) - self.geo.s).abs() > 0.01 {
                self.geo = Geo::new(scale);
                self.body.scale_hint = self.geo.s;
            }
            self.next_work = now + 0.5;
        }
        let mut steps = 0;
        while self.next_step <= now && steps < 10 {
            self.physics_step(el, self.next_step);
            self.next_step += STEP;
            steps += 1;
        }
        if self.next_step < now {
            self.next_step = now + STEP;
        }
        if now >= self.next_anim {
            self.anim(now);
            self.flush_events(el); // e.g. finished eating: love bubble and hearts right away
            self.next_anim = now + ANIM;
        }
        if now >= self.next_sim {
            self.sim(el, now);
            self.next_sim = now + 1.0;
        }
        self.draw_pet(now);
        self.poop_tick(now);
        self.menu_tick(now);
        let anchor = (
            self.body.ring_target.unwrap_or(self.body.x) + self.geo.w as f64 / 2.0,
            self.body.y + self.geo.bub_h as f64 * 0.3,
        );
        let work = (self.work.left, self.work.top, self.work.right, self.work.bottom);
        let outs = self.cards.layout(&self.theme, &mut self.fonts, anchor.0, anchor.1, work, now);
        for CardOut::Run(cmd) in outs {
            self.run(el, &cmd);
        }
        self.tray_events(el);
        crate::office::tick(self, el);
        crate::voice::tick(self, el);
        crate::basecamp::tick(self, el);
        crate::updater::tick(self, el);
        if now >= self.next_theme {
            // "Match system": follow the OS when it switches between light and dark
            self.next_theme = now + 5.0;
            let setting = self.cfg.str("theme").to_string();
            if setting.is_empty() || setting == "system" {
                let t = theme::pick(&setting, platform::system_dark());
                if t.dark != self.theme.dark {
                    self.theme = t;
                    self.menu = None;
                    self.windows_dirty = true;
                }
            }
        }
        if !self.timed.is_empty() {
            let due: Vec<String> = self.timed.iter().filter(|(t, _)| *t <= now).map(|(_, c)| c.clone()).collect();
            self.timed.retain(|(t, _)| *t > now);
            for c in due {
                self.run_demo(el, &c);
            }
        }
        for cmd in std::mem::take(&mut self.later) {
            self.run(el, &cmd);
        }
    }

    fn physics_step(&mut self, el: &ActiveEventLoop, t: f64) {
        if self.pressed {
            let (mx, my) = platform::cursor();
            if platform::left_button_down() {
                let was_held = self.body.action == Action::Held;
                if self.body.drag(mx, my, t) && !was_held && self.body.action == Action::Held {
                    self.pet.woken_by_drag();
                    self.food = None;
                    self.flush_events(el);
                }
            } else {
                self.pressed = false;
                let g = self.body.release(&self.geo, t);
                crate::log(&format!("pet release -> {g:?}"));
                match g {
                    Some(Gesture::Pat) => {
                        if self.alerts.is_empty() {
                            self.pet.pat(blip_core::now());
                            self.flush_events(el);
                        }
                    }
                    Some(Gesture::Thrown { ball }) => {
                        self.pet.tossed(ball, blip_core::now());
                        self.flush_events(el);
                        self.work = platform::work_area_at(mx, my).0;
                        if !self.alerts.is_empty() {
                            self.body.ring_target = None;
                        }
                    }
                    Some(Gesture::Menu) | None => {}
                }
            }
        }
        self.body.slow = self.pet.b("sick");
        let before = self.body.action;
        self.body.step(&self.geo, &self.work, t);
        if before != Action::Unfold && self.body.action == Action::Unfold {
            crate::log(&format!("landing {:?} for {}", self.body.landing, self.pet.species()));
            // the proud one pops out of its ball its species' way; the others have their own bits
            let own: Option<&[(&'static str, u32)]> = match self.body.landing {
                Landing::Proud => None,
                Landing::Dizzy => Some(&[("star", 3)]),
                Landing::Shake | Landing::Flop => Some(&[("dust", 2)]),
                Landing::Wiggle => Some(&[("note", 1)]),
                _ => Some(&[]),
            };
            let burst: &[(&'static str, u32)] = if let Some(b) = own { b } else { match self.pet.species() {
                "kitty" => &[("heart", 2)],
                "bunny" => &[("spark", 4)],
                "bear" => &[("heart", 1), ("spark", 2)],
                "pup" => &[("note", 2)],
                "chick" => &[("note", 3)],
                "bot" => &[("spark", 6)],
                _ => &[("spark", 3)],
            } };
            for (k, n) in burst {
                self.spawn(k, *n);
            }
        }
        if self.body.take_menu() {
            let (mx, my) = platform::cursor();
            self.open_menu(el, mx, my);
        }
    }

    fn anim(&mut self, now: f64) {
        let was = self.body.action;
        self.body.anim(now);
        match was {
            Action::Eat if now >= self.body.until() => {
                let food = self.food.take().map(|f| f.0).unwrap_or("meal");
                self.body.set(Action::Idle, 2.0, now);
                self.pet.finished_eating(food);
            }
            Action::Unfold if self.body.action == Action::Idle && self.pet.hatched() => {
                // say something only some of the time, and fitting the reaction
                let r = self.pet.rng.f();
                let say: Option<&'static str> = match self.body.landing {
                    Landing::Dizzy => Some("dizzy"),
                    Landing::Proud if r < 0.6 => Some("wow"),
                    Landing::Hop if r < 0.4 => Some("happy"),
                    Landing::Wiggle if r < 0.5 => Some("music"),
                    Landing::Look if r < 0.5 => Some("ask"),
                    Landing::Shake | Landing::Flop if r < 0.35 => Some("oops"),
                    _ => None,
                };
                if let Some(m) = say {
                    self.chirp(m, None, 2.0);
                }
            }
            Action::Unfold if self.body.landing == Landing::Dizzy && self.body.frame % 2 == 0 => self.spawn("star", 1),
            _ => {}
        }
        if self.pet.asleep() && !self.pet.gone() && self.body.frame % 5 == 0 {
            self.spawn("z", 1);
        }
        if self.body.action == Action::Ring && self.body.frame % 2 == 0 {
            self.spawn("bang", 1);
        }
        scene::age(&mut self.parts, self.body.frame);
    }

    fn sim(&mut self, el: &ActiveEventLoop, now: f64) {
        let wall = blip_core::now();
        let gap = (wall - self.pet.f("wall")).max(0.0);
        self.pet.set("wall", json!(wall));
        let idle = matches!(self.body.action, Action::Idle | Action::Walk);
        let cmd = if gap > 120.0 { self.pet.catch_up(gap) } else { self.pet.step(gap, idle) };
        if let Some(c) = cmd {
            self.body_cmd(c, now);
        }
        for it in self.timers.tick(wall) {
            self.timer_fired(el, &it);
        }
        for n in self.notes.due(wall) {
            self.reminder_fired(el, &n);
        }
        self.flush_events(el);
        let quiet = self.bubble.as_ref().map_or(true, |b| now >= b.2) && self.alerts.is_empty();
        if quiet && now - self.last_nag > 30.0 && self.pet.nag() {
            self.last_nag = now;
            self.flush_events(el);
        }
        if now - self.last_save > 30.0 {
            self.save();
        }
    }

    pub fn body_cmd(&mut self, c: BodyCmd, now: f64) {
        match c {
            BodyCmd::Eat { food, secs } => {
                self.food = Some((food, now));
                self.body.set(Action::Eat, secs, now);
            }
            BodyCmd::Play { secs } => {
                self.body.set(Action::Play, secs, now);
                self.body.jump();
            }
            BodyCmd::Sleep => self.body.set(Action::Sleep, 1e18, now),
            BodyCmd::Wake => self.body.set(Action::Idle, 0.0, now),
            BodyCmd::Idle(s) => self.body.set(Action::Idle, s, now),
        }
    }

    /// Show what the pet's model said happened.
    pub fn flush_events(&mut self, el: &ActiveEventLoop) {
        let now = self.now();
        for ev in std::mem::take(&mut self.pet.events) {
            match ev {
                Ev::Chirp { mood, icon, secs } => self.chirp(mood, icon, secs),
                Ev::Board(t, secs) => self.board(&t, secs),
                Ev::Spawn(k, n) => self.spawn(k, n),
                Ev::Sound(name) => self.play_sound(name, false),
                Ev::Happy(secs) => self.body.happy_until = now + secs,
                Ev::Poop => {
                    let p = self.geo.p as f64;
                    let px = (self.body.x + self.geo.w as f64 / 2.0 - self.body.dir as f64 * 10.0 * p)
                        .clamp(self.work.left + 6.0 * p, self.work.right - 6.0 * p);
                    self.pet.place_poop(px);
                    self.poop_window(el, px);
                }
                Ev::PoopsGone => {
                    let now = self.now();
                    for p in self.poops.iter_mut() {
                        p.gone_at.get_or_insert(now);
                    }
                }
                Ev::Asleep(true) => self.body.set(Action::Sleep, 1e18, now),
                Ev::Asleep(false) => self.body.set(Action::Idle, 0.0, now),
                Ev::Left => {
                    self.body.set(Action::Gone, 1e18, now);
                    self.bubble = Some(("...".into(), None, now + 12.0));
                    let name = self.pet.s("name").to_string();
                    self.show_card(el, ShowCard {
                        key: "goodbye",
                        title: "Goodbye",
                        body: &format!("{name} floated off to the pet planet. Right-click and hatch a new egg any time."),
                        kind: "info",
                        icon: Some("heart"),
                        buttons: vec![("New egg".into(), "new-egg".into()), ("OK".into(), String::new())],
                        timeout: None,
                        on_close: "",
                        slider: None,
                    });
                }
                Ev::Hatched => self.body.set(Action::Idle, 2.0, now),
                Ev::Grew => {}
            }
        }
    }

    // ------------------------------------------------ drawing the pet
    fn draw_pet(&mut self, now: f64) {
        if !self.visible() {
            if let Some(w) = self.pet_win.as_mut() {
                w.hide();
            }
            return;
        }
        let egg = (self.pet.stage() == "egg" && !self.pet.gone()).then(|| (self.pet.f("age") / EGG_TIME).min(1.0));
        let mood = self.face(now);
        let look = look_of(&self.pet);
        let mut g = self.body.sprite_as(&look, &self.geo, mood, egg);
        self.top_row = g.keys().map(|c| c.1).min().unwrap_or(14);
        if let (Some((food, t0)), Action::Eat) = (self.food, self.body.action) {
            scene::add_food(&mut g, food, (now - t0) / 2.2, self.body.dir > 0);
        }
        scene::add_particles(&mut g, &self.parts);
        let geo = self.geo;
        if self.canvas.w != geo.w || self.canvas.h != geo.h {
            self.canvas = Canvas::new(geo.w, geo.h);
        } else {
            self.canvas.clear();
        }
        self.canvas.sprite(&g, geo.ox, geo.bub_h, geo.p);
        let f = Frame { w: geo.w, ox: geo.ox, bub_h: geo.bub_h, p: geo.p, s: geo.s, top_row: self.top_row };
        self.board_rect = None;
        self.sign_rect = None;
        if let Some((t, until)) = self.board.clone().filter(|b| now < b.1) {
            let _ = until;
            self.board_rect = Some(scene::board(&mut self.canvas, &f, &t));
        } else if let Some((t, icon, _)) = self.bubble.clone().filter(|b| now < b.2) {
            scene::bubble(&mut self.canvas, &mut self.fonts, &f, &t, icon);
        } else if let Some((icon, text, iid)) = self.timers.display(blip_core::now()) {
            let it = self.timers.get(&iid).cloned().unwrap_or_default();
            let paused = (timers::kind(&it) == "timer" && it.get("left").map_or(false, |v| !v.is_null()))
                || (timers::kind(&it) == "stopwatch" && !it.get("running").and_then(Value::as_bool).unwrap_or(true));
            let urgent = timers::kind(&it) == "timer" && !paused && Timers::remaining(&it, blip_core::now()) <= 10.0;
            let more = self.timers.active_count().saturating_sub(1);
            self.sign_rect = Some(scene::sign(&mut self.canvas, &f, icon, &text, more, if paused { 1 } else if urgent { 2 } else { 0 }, self.body.frame));
        }
        let (x, y) = ((self.body.x + self.body.jitter).round() as i32, self.body.y.round() as i32);
        if let Some(w) = self.pet_win.as_mut() {
            w.present(&self.canvas, x, y);
        }
        crate::shot::canvas_every("pet", &self.canvas, 2.0);
    }

    /// The face to draw (`mood()` in app.py).
    fn face(&self, now: f64) -> &'static str {
        if self.pet.gone() {
            return "normal";
        }
        match self.body.action {
            Action::Held => return "held",
            Action::Unfold => return self.body.mood(now),
            Action::Fall if self.body.airborne => return if self.body.chute > 0 { "happy" } else { "held" },
            Action::Ring => return "alert",
            _ => {}
        }
        if self.pet.asleep() {
            return "sleep";
        }
        if self.pet.b("sick") {
            return "sick";
        }
        if self.body.action == Action::Eat {
            return "eat";
        }
        if self.body.action == Action::Play || now < self.body.happy_until {
            return "happy";
        }
        self.pet.mood()
    }

    // ------------------------------------------------ poops
    fn poop_window(&mut self, el: &ActiveEventLoop, px: f64) {
        let cv = scene::poop(self.geo.p);
        let (work, _) = platform::work_area_at(px, self.work.bottom - 2.0);
        let (x, y) = ((px - cv.w as f64 / 2.0) as i32, (work.bottom as i32) - cv.h);
        if let Some(mut w) = Overlay::new(el, cv.w, cv.h, x, y, true) {
            if self.visible() {
                w.present(&cv, x, y);
            }
            self.poops.push(PoopWin { win: w, x: px, gone_at: None });
        }
    }

    /// Poops being picked up: rise, dissolve and sparkle for half a second, then go.
    fn poop_tick(&mut self, now: f64) {
        const DUR: f64 = 0.55;
        let p = self.geo.p;
        let mut i = 0;
        while i < self.poops.len() {
            let Some(t0) = self.poops[i].gone_at else {
                i += 1;
                continue;
            };
            let t = (now - t0) / DUR;
            if t >= 1.0 {
                self.poops.remove(i);
                continue;
            }
            let base = scene::poop(p);
            let mut cv = blip_ui::Canvas::new(base.w, base.h);
            let lift = (t * 2.0 * p as f64) as i32;
            for (y, row) in blip_pet::sprites::POOP.iter().enumerate() {
                for (x, ch) in row.chars().enumerate() {
                    let c = match ch {
                        'o' => blip_pet::sprites::col::OUT,
                        'b' => blip_pet::sprites::col::POOP,
                        _ => continue,
                    };
                    // each cell vanishes at its own moment
                    let h = ((x * 37 + y * 61 + 11) % 97) as f64 / 97.0;
                    if h < t * 1.3 {
                        continue;
                    }
                    let (cx, cy) = (x as i32 * p, y as i32 * p - lift);
                    cv.rect(cx, cy, cx + p, cy + p, c);
                }
            }
            // sparkles at the corners, growing then fading
            let k = if t < 0.5 { t * 2.0 } else { 2.0 - t * 2.0 };
            let arm = ((k * 1.5).round() as i32).max(0);
            for (sx, sy) in [(1, 1), (6, 0), (0, 4), (7, 3)] {
                let (cx, cy) = (sx * p, sy * p);
                cv.rect(cx, cy, cx + p, cy + p, 0xfff1a8);
                for d in 1..=arm {
                    for (dx, dy) in [(d, 0), (-d, 0), (0, d), (0, -d)] {
                        let (x, y) = (cx + dx * p / 2, cy + dy * p / 2);
                        cv.rect(x, y, x + (p / 2).max(1), y + (p / 2).max(1), 0xffffff);
                    }
                }
            }
            crate::shot::canvas_every("poop-vanish", &cv, 0.15);
            let pw = &mut self.poops[i];
            let (x, y) = (pw.win.x, pw.win.y);
            pw.win.present(&cv, x, y);
            i += 1;
        }
    }

    // ------------------------------------------------ ringing
    pub fn start_alert(&mut self, key: &str, sound: &'static str, dash: bool) {
        self.alerts.retain(|(k, _)| k != key);
        self.alerts.push((key.to_string(), sound));
        self.play_sound(sound, true);
        if !self.pet.st.get("visible").and_then(Value::as_bool).unwrap_or(true) {
            self.temp_shown = true; // hidden pets pop out while something rings
        }
        if self.pet.asleep() && !self.pet.gone() {
            self.pet.set("asleep", json!(false));
        }
        if dash {
            self.body.ring_target = Some((self.work.left + self.work.right) / 2.0 - self.geo.w as f64 / 2.0);
        }
        if self.pet.hatched() && self.body.action != Action::Held {
            self.food = None;
            let now = self.now();
            self.body.set(Action::Ring, 1e18, now);
            self.body.jump();
        }
    }

    pub fn stop_alert(&mut self, key: &str) {
        let before = self.alerts.len();
        self.alerts.retain(|(k, _)| k != key);
        if self.alerts.len() == before {
            return;
        }
        if let Some((_, s)) = self.alerts.last().cloned() {
            self.play_sound(s, true);
        } else {
            self.sound.stop();
            self.body.ring_target = None;
            if self.body.action == Action::Ring {
                let now = self.now();
                self.body.set(Action::Idle, 1.0, now);
            }
            self.temp_shown = false;
        }
    }

    pub fn show_card(&mut self, el: &ActiveEventLoop, c: ShowCard) {
        let now = self.now();
        self.cards.show(el, &mut self.fonts, self.geo.s, now, c);
    }

    fn timer_fired(&mut self, el: &ActiveEventLoop, it: &timers::Item) {
        let iid = timers::id(it).to_string();
        let key = format!("tm:{iid}");
        let label = timers::label(it).to_string();
        let (title, icon, body, buttons, kind) = if timers::kind(it) == "timer" {
            let dur = it.get("dur").and_then(Value::as_f64).unwrap_or(0.0);
            (
                "Time's up!",
                "hourglass",
                format!("{}Your {} timer is done.", if label.is_empty() { String::new() } else { format!("{label}\n") }, clock::fmt_clock(dur, true)),
                vec![("Stop".into(), String::new()), ("+5 min".into(), format!("timer:300:{label}"))],
                "timer",
            )
        } else {
            let (hh, mm) = (it.get("hh").and_then(Value::as_u64).unwrap_or(0), it.get("mm").and_then(Value::as_u64).unwrap_or(0));
            (
                "Alarm",
                "bell",
                format!("{}It's {hh:02}:{mm:02}.", if label.is_empty() { String::new() } else { format!("{label}\n") }),
                vec![("Stop".into(), String::new()), ("Snooze 5".into(), format!("snooze:{iid}:5")), ("Snooze 10".into(), format!("snooze:{iid}:10"))],
                "alarm",
            )
        };
        self.start_alert(&key, "alarm", false);
        let close = format!("stop-alert:{key}");
        self.show_card(el, ShowCard { key: &key, title, body: &body, kind, icon: Some(icon), buttons, timeout: Some(180.0), on_close: &close, slider: None });
    }

    fn reminder_fired(&mut self, el: &ActiveEventLoop, n: &notes::Note) {
        let nid = notes::id(n).to_string();
        let key = format!("note:{nid}");
        self.start_alert(&key, "alarm", false);
        self.board("Reminder!", 6.0);
        let close = format!("stop-alert:{key}");
        self.show_card(el, ShowCard {
            key: &key,
            title: "Reminder",
            body: notes::text(n),
            kind: "task",
            icon: Some("bell"),
            buttons: vec![
                ("Done".into(), format!("note-done:{nid}")),
                ("10 min".into(), format!("note-snooze:{nid}:10")),
                ("1 hour".into(), format!("note-snooze:{nid}:60")),
            ],
            timeout: Some(300.0),
            on_close: &close,
            slider: None,
        });
    }

    // ------------------------------------------------ the menu
    fn build_menu(&self, kind: &str) -> Menu {
        let now = self.now();
        let s = self.geo.s;
        if kind == "timer" {
            let n = self.timers.active_count();
            let mut items: Vec<Item> = [1, 5, 10, 15, 25, 30, 45, 60]
                .iter()
                .map(|m| Item::row("hourglass", &if *m == 1 { "1 minute".to_string() } else { format!("{m} minutes") }, &format!("timer:{}:", m * 60)))
                .collect();
            items.push(Item::row("plus", "Other\u{2026}", "window:timers"));
            items.push(Item::Sep);
            items.push(Item::Row {
                icon: "gear",
                label: "All timers & alarms".into(),
                hint: if n > 0 { n.to_string() } else { String::new() },
                check: None,
                cmd: "window:timers".into(),
                enabled: true,
            });
            return Menu::new(None, vec![], items, s);
        }
        let pet = &self.pet;
        let (alive, hatched) = (!pet.gone(), pet.hatched());
        let card = PetCard {
            name: pet.s("name").to_string(),
            status: format!("{}  \u{b7}  {}", pet.stage_name(), pet.mood_word()),
            species: pet.species().to_string(),
            palette: pet.palette(),
            stage: if pet.stage() == "egg" && !pet.gone() { "egg".into() } else if pet.stage() == "egg" { "baby".into() } else { pet.stage().to_string() },
            mood: match self.face(now) {
                m @ ("happy" | "sad" | "sleep" | "sick") => m.to_string(),
                _ => "normal".into(),
            },
            bars: vec![("meal", pet.stat("full"), 0xf0a020), ("play", pet.stat("happy"), 0xef5b8b), ("moon", pet.stat("energy"), 0x6b7fd7)],
        };
        let mut care = vec![];
        if self.cfg.menu_on("care") {
            let n = pet.poops().len();
            let b = |icon: &'static str, label: String, cmd: &str, enabled: bool| CareButton { icon, label, cmd: cmd.into(), enabled };
            care = vec![
                b("meal", "Feed".into(), "care:meal", hatched),
                b("snack", "Snack".into(), "care:snack", hatched),
                b("play", "Play".into(), "care:play", hatched),
                b("clean", if n > 0 { format!("Clean ({n})") } else { "Clean".into() }, "care:clean", alive),
                b("pill", "Meds".into(), "care:meds", hatched),
                b("moon", if pet.asleep() { "Wake".into() } else { "Sleep".into() }, "care:sleep", hatched),
            ];
        }
        let mut items = vec![];
        if self.cfg.menu_on("notes") {
            let pending = self.notes.pending();
            let open = self.notes.items.iter().filter(|n| !notes::done(n)).count();
            items.push(Item::Row {
                icon: "note",
                label: "Notes".into(),
                hint: if pending > 0 { format!("{pending} reminder{}", if pending == 1 { "" } else { "s" }) } else if open > 0 { open.to_string() } else { String::new() },
                check: None,
                cmd: "window:notes".into(),
                enabled: true,
            });
        }
        if self.cfg.menu_on("focus") {
            let n = self.timers.active_count();
            items.push(Item::Tools(vec![
                Tool { icon: "hourglass", label: "Timer".into(), cmd: "menu:timer".into(), badge: if n > 0 { n.to_string() } else { String::new() } },
                Tool { icon: "bell", label: "Alarm".into(), cmd: "window:alarm".into(), badge: String::new() },
                Tool { icon: "stopwatch", label: "Stopwatch".into(), cmd: "stopwatch".into(), badge: String::new() },
            ]));
        }
        let mut extra = self.office_items();
        if self.cfg.menu_on("basecamp") && crate::basecamp::connected(self) {
            let due = self.bc.items.len();
            extra.push(Item::Row {
                icon: "task",
                label: "Basecamp tasks".into(),
                hint: if due > 0 { due.to_string() } else { String::new() },
                check: None,
                cmd: "window:tasks".into(),
                enabled: true,
            });
        }
        if self.cfg.menu_on("dnd") {
            extra.push(Item::toggle("dnd", "Do not disturb", self.cfg.bool("dnd"), "dnd"));
        }
        if self.cfg.menu_on("wander") {
            extra.push(Item::toggle("walk", "Wander around", self.body.wander, "wander"));
        }
        if !extra.is_empty() {
            if !items.is_empty() {
                items.push(Item::Sep);
            }
            items.extend(extra);
        }
        if !items.is_empty() {
            items.push(Item::Sep);
        }
        items.push(Item::row("gear", "Settings", "window:settings"));
        if pet.gone() {
            items.push(Item::row("heart", "Hatch a new egg", "new-egg"));
        }
        items.push(Item::row("power", "Quit Blip", "quit"));
        let mut m = Menu::new(Some(card), care, items, s);
        m.card_cmd = "window:settings:pet".into();
        m
    }

    /// Colleagues on the office network / server (filled in by the network step).
    fn office_items(&self) -> Vec<Item> {
        crate::office::menu_items(self)
    }

    pub fn open_menu(&mut self, el: &ActiveEventLoop, at_x: f64, at_y: f64) {
        crate::log(&format!("menu opens at {at_x},{at_y}"));
        self.open_menu_kind(el, at_x, at_y, "main");
    }

    fn open_menu_kind(&mut self, el: &ActiveEventLoop, at_x: f64, at_y: f64, kind: &str) {
        self.menu = None;
        let menu = self.build_menu(kind);
        let (w, h) = menu.size();
        let (work, _) = platform::work_area_at(at_x, at_y);
        let x = ((at_x + 4.0).min(work.right - w as f64 - 4.0)).max(work.left + 4.0) as i32;
        let y = ((at_y - 20.0).min(work.bottom - h as f64 - 4.0)).max(work.top + 4.0) as i32;
        let Some(mut win) = Overlay::new(el, w, h, x, y, false) else { return };
        win.x = x;
        win.y = y;
        win.window.focus_window();
        self.menu = Some(MenuWin { win, menu, canvas: Canvas::new(w, h), had_focus: false, opened: self.now(), away_since: None, dirty: true, sticky: false, btn_was: platform::any_button_down() });
    }

    fn menu_tick(&mut self, now: f64) {
        let Some(m) = self.menu.as_mut() else { return };
        // A click anywhere outside the menu closes it. The menu often can't take focus (the pet
        // never does), so losing focus alone isn't enough: watch the mouse buttons directly.
        let down = platform::any_button_down();
        let fresh = down && !m.btn_was;
        m.btn_was = down;
        if fresh && now - m.opened > 0.15 {
            let (mx, my) = platform::cursor();
            if !m.win.contains(mx, my, 0) {
                crate::log("menu closed: click outside");
                self.menu = None;
                return;
            }
        }
        if m.sticky {
            if now - m.opened > 15.0 {
                self.menu = None;
            }
        } else if !m.had_focus {
            // Borderless windows often don't get focus on macOS: close once the pointer has
            // stayed away from the menu for a while instead.
            let (mx, my) = platform::cursor();
            if m.win.contains(mx, my, 12) {
                m.away_since = None;
            } else if m.away_since.is_none() {
                m.away_since = Some(now);
            } else if now - m.away_since.unwrap_or(now) > 3.0 && now - m.opened > 1.0 {
                self.menu = None;
                return;
            }
        }
        if let Some(m) = self.menu.as_mut() {
            if m.dirty {
                m.menu.draw(&mut m.canvas, &self.theme, &mut self.fonts);
                crate::shot::canvas("menu", &m.canvas);
                let (x, y) = (m.win.x, m.win.y);
                m.win.present(&m.canvas, x, y);
                m.dirty = false;
            }
        }
    }

    // ------------------------------------------------ events
    pub fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.menu.as_ref().map(|m| m.win.id()) == Some(id) {
            self.menu_event(el, event);
            return;
        }
        if self.cards.owns(id) {
            let outs = self.cards.event(id, &event);
            for CardOut::Run(cmd) in outs {
                self.run(el, &cmd);
            }
            return;
        }
        if let Some(i) = self.poops.iter().position(|p| p.win.id() == id) {
            if let WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } = event {
                if self.poops[i].gone_at.is_none() {
                    let now = self.now();
                    self.poops[i].gone_at = Some(now);
                    let x = self.poops[i].x;
                    self.pet.clean_one(x);
                    self.play_sound("pop", false);
                    self.flush_events(el);
                }
            }
            return;
        }
        if self.pet_win.as_ref().map(|w| w.id()) != Some(id) {
            return;
        }
        match event {
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                let (mx, my) = platform::cursor();
                crate::log(&format!("pet left press at {mx},{my} body {},{} action {:?}", self.body.x, self.body.y, self.body.action));
                if self.modifiers.control_key() && cfg!(target_os = "macos") {
                    self.open_menu(el, mx, my); // Control-click = right-click on a Mac
                    return;
                }
                self.menu = None;
                let (lx, ly) = (mx as i32 - self.body.x as i32, my as i32 - self.body.y as i32);
                let inside = |r: Option<(i32, i32, i32, i32)>| r.map_or(false, |r| r.0 <= lx && lx < r.2 && r.1 <= ly && ly < r.3);
                if inside(self.board_rect) {
                    self.board = None;
                    return;
                }
                if inside(self.sign_rect) {
                    self.want_window.push("timers".into());
                    return;
                }
                let now = self.now();
                self.body.press(mx, my, now);
                self.pressed = true;
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } => {
                let (mx, my) = platform::cursor();
                self.open_menu(el, mx, my);
            }
            _ => {}
        }
    }

    fn menu_event(&mut self, el: &ActiveEventLoop, event: WindowEvent) {
        let Some(m) = self.menu.as_mut() else { return };
        match event {
            WindowEvent::Focused(true) => m.had_focus = true,
            WindowEvent::Focused(false) => {
                if m.had_focus && !m.sticky {
                    self.menu = None;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let hit = m.menu.hit(position.x as i32, position.y as i32);
                if hit != m.menu.hover {
                    m.menu.hover = hit;
                    m.dirty = true;
                }
            }
            WindowEvent::CursorLeft { .. } => {
                if m.menu.hover.is_some() {
                    m.menu.hover = None;
                    m.dirty = true;
                }
            }
            WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => {
                if let Some(cmd) = m.menu.hover.and_then(|h| m.menu.command(h)) {
                    self.menu = None;
                    if !cmd.is_empty() {
                        self.run(el, &cmd);
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.logical_key == Key::Named(NamedKey::Escape) => self.menu = None,
            _ => {}
        }
    }

    // ------------------------------------------------ commands (menu, cards, tray)
    pub fn run(&mut self, el: &ActiveEventLoop, cmd: &str) {
        let now = self.now();
        let wall = blip_core::now();
        let busy = matches!(self.body.action, Action::Eat | Action::Held | Action::Ring);
        let parts: Vec<&str> = cmd.splitn(3, ':').collect();
        match parts.as_slice() {
            ["care", what] => {
                let c = match *what {
                    "meal" => self.pet.feed("meal", busy),
                    "snack" => self.pet.feed("snack", busy),
                    "play" => self.pet.play(busy),
                    "meds" => self.pet.medicine(busy),
                    "sleep" => self.pet.toggle_sleep(busy),
                    _ => {
                        self.pet.clean();
                        None
                    }
                };
                if let Some(c) = c {
                    self.body_cmd(c, now);
                }
                self.flush_events(el);
                self.save();
            }
            ["timer", secs, label] => {
                let secs: f64 = secs.parse().unwrap_or(300.0);
                self.start_timer(secs, label);
                self.chirp("happy", Some("hourglass"), 2.0);
            }
            ["stopwatch"] => self.start_stopwatch(""),
            ["snooze", iid, mins] => {
                let m: f64 = mins.parse().unwrap_or(5.0);
                self.timers.snooze(iid, m, wall);
            }
            ["note-done", nid] => self.notes.complete(nid, true),
            ["note-snooze", nid, mins] => {
                let m: f64 = mins.parse().unwrap_or(10.0);
                self.notes.snooze(nid, m, wall);
            }
            ["stop-alert", rest @ ..] => {
                let key = rest.join(":");
                self.stop_alert(&key);
            }
            ["menu", "timer"] => {
                let (mx, my) = platform::cursor();
                self.open_menu_kind(el, mx, my, "timer");
            }
            ["window", rest @ ..] => self.want_window.push(rest.join(":")),
            ["dnd"] => {
                let on = !self.cfg.bool("dnd");
                self.cfg.set("dnd", json!(on));
                self.board(if on { "Do not disturb" } else { "DND off" }, 3.0);
                self.windows_dirty = true;
            }
            ["wander"] => {
                self.body.wander = !self.body.wander;
                if !self.body.wander && self.body.action == Action::Walk {
                    self.body.set(Action::Idle, 3.0, now);
                }
                self.save();
            }
            ["new-egg"] => {
                self.cards.remove("goodbye");
                self.pet.new_egg(wall);
                self.body.set(Action::Idle, 2.0, now);
                self.flush_events(el);
            }
            ["hide"] => self.set_visible(false),
            ["show"] => self.set_visible(true),
            ["quit"] => {
                self.save();
                self.quit = true;
                el.exit();
            }
            _ => {
                if crate::basecamp::run(self, el, cmd) || crate::updater::run(self, el, cmd) {
                    return;
                }
                crate::office::run(self, el, cmd)
            }
        }
    }

    /// Run a command `secs` seconds from now.
    pub fn run_at(&mut self, secs: f64, cmd: &str) {
        let t = self.now() + secs;
        self.timed.push((t, cmd.to_string()));
    }

    /// Something got done (a Basecamp to-do): sparkles, a happy hop.
    pub fn cheer_done(&mut self) {
        self.spawn("spark", 6);
        let now = self.now();
        self.body.happy_until = now + 3.0;
        self.board("Done!", 3.0);
        self.chirp("wow", None, 3.0);
        let h = self.pet.stat("happy");
        self.pet.set_stat("happy", h + 5.0);
    }

    /// Ringing, calling or talking: not a good moment to restart for an update.
    pub fn busy_with_people(&self) -> bool {
        !self.alerts.is_empty() || crate::office::in_call(self) || crate::voice::active(self)
    }

    pub fn set_visible(&mut self, on: bool) {
        self.pet.set("visible", json!(on));
        self.temp_shown = false;
        for p in self.poops.iter_mut() {
            if on {
                // redrawn on the next frame
            } else {
                p.win.hide();
            }
        }
        if on {
            let p = self.geo.p;
            for pw in self.poops.iter_mut() {
                let cv = scene::poop(p);
                let (x, y) = (pw.win.x, pw.win.y);
                pw.win.present(&cv, x, y);
            }
            self.chirp("happy", None, 3.0);
        }
        if let Some(t) = &self.tray {
            t.show.set_checked(on);
        }
        self.save();
    }

    // ------------------------------------------------ tray / menu bar
    fn make_tray(&mut self) {
        let open = MenuItem::new("Open Blip menu", true, None);
        let show = CheckMenuItem::new("Show pet on desktop", true, self.visible(), None);
        let settings = MenuItem::new("Settings\u{2026}", true, None);
        let quit = MenuItem::new("Quit Blip", true, None);
        let menu = TrayMenu::new();
        let _ = menu.append_items(&[&open, &show, &settings, &PredefinedMenuItem::separator(), &quit]);
        let built = TrayIconBuilder::new()
            .with_tooltip("Blip")
            .with_icon(crate::tray_image(&look_of(&self.pet)))
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build();
        if let Ok(icon) = built {
            self.tray = Some(Tray { _icon: icon, open, show, settings, quit });
        }
    }

    fn tray_events(&mut self, el: &ActiveEventLoop) {
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click { button: TrayButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                let (mx, my) = platform::cursor();
                self.open_menu(el, mx, my);
            }
        }
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            let Some(t) = &self.tray else { continue };
            if ev.id == *t.open.id() {
                let (mx, my) = platform::cursor();
                self.open_menu(el, mx, my);
            } else if ev.id == *t.show.id() {
                let on = t.show.is_checked();
                self.set_visible(on);
            } else if ev.id == *t.settings.id() {
                self.want_window.push("settings".into());
            } else if ev.id == *t.quit.id() {
                self.run(el, "quit");
            }
        }
    }

    // ------------------------------------------------ testing without a mouse
    fn run_demo(&mut self, el: &ActiveEventLoop, what: &str) {
        if what.contains(';') {
            for w in what.split(';') {
                self.run_demo(el, w.trim());
            }
            return;
        }
        if let Some(rest) = what.strip_prefix("at:") {
            // "at:SECONDS:command" runs the command that many seconds after start
            if let Some((secs, cmd)) = rest.split_once(':') {
                let t: f64 = secs.parse().unwrap_or(0.0);
                self.timed.push((t, cmd.to_string()));
            }
            return;
        }
        if let Some(v) = what.strip_prefix("throw:") {
            // "throw:vx:vy" (px per step), as if flung with the mouse
            let mut it = v.split(':').filter_map(|x| x.parse::<f64>().ok());
            let (vx, vy) = (it.next().unwrap_or(20.0), it.next().unwrap_or(-10.0));
            let now = self.now();
            self.body.y -= 120.0 * self.geo.s;
            if let Gesture::Thrown { ball } = self.body.fling(vx * self.geo.s, vy * self.geo.s, &self.geo, now) {
                self.pet.tossed(ball, blip_core::now());
                crate::log(&format!("thrown ball={ball} fire={} fun={:.1}", self.body.fire, self.pet.stat("happy")));
                self.flush_events(el);
            }
            return;
        }
        if let Some(step) = what.strip_prefix("ui:") {
            self.ui_script.push(step.to_string());
            return;
        }
        let (x, y) = (self.body.x + self.geo.w as f64 / 2.0, self.body.y + self.geo.h as f64 * 0.6);
        match what {
            "menu" | "timer-menu" => {
                self.open_menu_kind(el, x, y, if what == "menu" { "main" } else { "timer" });
                if let Some(m) = self.menu.as_mut() {
                    m.sticky = true;
                }
            }
            "card" => {
                self.notes.add("Demo: send the invoice to Ravi", Some(blip_core::now() - 1.0), blip_core::now());
            }
            "poop" => {
                self.pet.events.push(Ev::Poop);
                self.flush_events(el);
            }
            other => self.run(el, other),
        }
    }

    pub fn shutdown(&mut self) {
        crate::office::shutdown(self);
        self.save();
    }

    pub fn arc_pet_window(&self) -> Option<Arc<winit::window::Window>> {
        self.pet_win.as_ref().map(|w| w.window.clone())
    }
}
