//! Blip, the Rust version (steps 2-3 of the port): the pet on the desktop, its right-click menu and
//! its tray / menu-bar icon. It sits on the taskbar, walks, can be dragged and thrown, and clicks
//! outside its pixels go to whatever is behind it. It looks like your pet from the Python Blip
//! (read-only: it never writes the Python app's files).
//!
//! Menu: right-click, Control-click (Mac), or press and hold the pet. Or the tray / menu-bar icon.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod platform;

use std::sync::Arc;
use std::time::{Duration, Instant};

use blip_pet::body::{Action, Body, Geo, Gesture, Look, Rect, ANIM, STEP};
use blip_pet::render;
use blip_pet::sprites::{self, Stage};
use blip_ui::menu::{Item, Menu, PetCard};
use blip_ui::text::Fonts;
use blip_ui::theme::{self, Theme};
use blip_ui::Canvas;
use serde_json::Value;
use tray_icon::menu::{CheckMenuItem, Menu as TrayMenu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton as TrayButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId, WindowLevel};

/// The open right-click menu: its own borderless window.
struct MenuWin {
    window: Arc<Window>,
    surface: platform::Surface,
    menu: Menu,
    canvas: Canvas,
    x: i32,
    y: i32,
    had_focus: bool,
    opened: f64,
    away_since: Option<f64>,
    dirty: bool,
    /// testing only: stays open whatever the focus does
    sticky: bool,
}

struct Tray {
    _icon: TrayIcon,
    open: MenuItem,
    show: CheckMenuItem,
    quit: MenuItem,
}

struct App {
    window: Option<Arc<Window>>,
    surface: Option<platform::Surface>,
    body: Body,
    look: Look,
    state: Value,
    geo: Geo,
    work: Rect,
    buf: Vec<u32>,
    t0: Instant,
    next_step: f64,
    next_anim: f64,
    next_work: f64,
    pressed: bool,
    shown: bool,
    visible: bool,
    menu: Option<MenuWin>,
    fonts: Option<Fonts>,
    theme: Theme,
    tray: Option<Tray>,
    modifiers: ModifiersState,
    quit: bool,
    /// testing: BLIP_DEMO_MENU=1 opens the menu by itself after 2 s (no mouse needed)
    demo_menu: bool,
}

fn base_attrs(w: i32, h: i32, x: i32, y: i32) -> winit::window::WindowAttributes {
    let attrs = Window::default_attributes()
        .with_title("Blip")
        .with_decorations(false)
        .with_resizable(false)
        .with_visible(false)
        .with_transparent(cfg!(target_os = "macos"))
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_inner_size(winit::dpi::PhysicalSize::new(w as u32, h as u32))
        .with_position(winit::dpi::PhysicalPosition::new(x, y));
    #[cfg(windows)]
    let attrs = {
        use winit::platform::windows::WindowAttributesExtWindows;
        attrs.with_skip_taskbar(true)
    };
    #[cfg(target_os = "macos")]
    let attrs = {
        use winit::platform::macos::WindowAttributesExtMacOS;
        attrs.with_has_shadow(false)
    };
    attrs
}

impl App {
    fn now(&self) -> f64 {
        self.t0.elapsed().as_secs_f64()
    }

    fn new() -> App {
        let state = read_json("state.json");
        let config = read_json("config.json");
        let (look, saved_x) = look_from_state(&state);
        let (work, scale) = platform::work_area_at(saved_x.unwrap_or(f64::MAX), 500.0);
        let geo = Geo::new(scale);
        let x = saved_x.unwrap_or(work.right - geo.w as f64 - 140.0 * scale);
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7);
        let mut body = Body::new(x, Body::ground(&geo, &work), seed);
        body.wander = state.get("wander").and_then(Value::as_bool).unwrap_or(true);
        let theme = theme::pick(config.get("theme").and_then(Value::as_str).unwrap_or("system"), platform::system_dark());
        App {
            window: None,
            surface: None,
            body,
            look,
            state,
            geo,
            work,
            buf: Vec::new(),
            t0: Instant::now(),
            next_step: 0.0,
            next_anim: 0.0,
            next_work: 0.0,
            pressed: false,
            shown: false,
            visible: true,
            menu: None,
            fonts: Fonts::system(),
            theme,
            tray: None,
            modifiers: ModifiersState::empty(),
            quit: false,
            demo_menu: std::env::var("BLIP_DEMO_MENU").is_ok(),
        }
    }

    // ------------------------------------------------ the pet
    fn tick(&mut self, el: &ActiveEventLoop) {
        let now = self.now();
        if now >= self.next_work && self.body.action != Action::Held {
            let (cx, cy) = (self.body.x + self.geo.w as f64 / 2.0, self.body.y + self.geo.h as f64 - 2.0);
            let (work, scale) = platform::work_area_at(cx, cy);
            self.work = work;
            if (scale.max(1.0) - self.geo.s).abs() > 0.01 {
                self.geo = Geo::new(scale);
            }
            self.next_work = now + 0.5;
        }
        let mut steps = 0;
        while self.next_step <= now && steps < 10 {
            if self.pressed {
                let (mx, my) = platform::cursor();
                if platform::left_button_down() {
                    self.body.drag(mx, my, self.next_step);
                } else {
                    self.pressed = false;
                    if let Some(Gesture::Thrown { .. }) = self.body.release(&self.geo, self.next_step) {
                        self.work = platform::work_area_at(mx, my).0;
                    }
                }
            }
            self.body.step(&self.geo, &self.work, self.next_step);
            if self.body.take_menu() {
                let (mx, my) = platform::cursor();
                self.open_menu(el, mx, my);
            }
            self.next_step += STEP;
            steps += 1;
        }
        if self.next_step < now {
            self.next_step = now + STEP; // fell behind (sleep, debugger): don't fast-forward
        }
        if now >= self.next_anim {
            self.body.anim(now);
            self.next_anim = now + ANIM;
        }
        if self.demo_menu && now > 2.0 {
            self.demo_menu = false;
            let (x, y) = (self.body.x + self.geo.w as f64 / 2.0, self.body.y + self.geo.h as f64 * 0.6);
            self.open_menu(el, x, y);
            if let Some(m) = self.menu.as_mut() {
                m.sticky = true;
                m.menu.hover = m.menu.hit(m.menu.size().0 / 2, m.menu.size().1 - 40);
            }
        }
        if self.visible {
            self.draw_pet(now);
        }
        self.menu_tick(now);
    }

    fn draw_pet(&mut self, now: f64) {
        let g = self.body.sprite(&self.look, &self.geo, now);
        render::render(&g, &self.geo, &mut self.buf);
        let (x, y) = (self.body.x.round() as i32, self.body.y.round() as i32);
        if !self.shown {
            if let Some(w) = &self.window {
                w.set_visible(true);
                self.shown = true;
            }
        }
        if let Some(s) = self.surface.as_mut() {
            s.present(&self.buf, self.geo.w, self.geo.h, x, y);
            let (mx, my) = platform::cursor();
            s.set_click_through_at_cursor(render::opaque(&self.buf, &self.geo, mx as i32 - x, my as i32 - y));
        }
    }

    fn set_visible(&mut self, on: bool) {
        self.visible = on;
        if let Some(w) = &self.window {
            w.set_visible(on);
        }
        if let Some(t) = &self.tray {
            t.show.set_checked(on);
        }
    }

    // ------------------------------------------------ the menu
    fn build_menu(&self) -> Menu {
        let st = &self.state;
        let stats = st.get("stats").cloned().unwrap_or_default();
        let stat = |k: &str| stats.get(k).and_then(Value::as_f64).unwrap_or(60.0);
        let stage = st.get("stage").and_then(Value::as_str).unwrap_or("adult").to_string();
        let stage_name = match stage.as_str() {
            "egg" => "Egg",
            "baby" => "Baby",
            "child" => "Kid",
            _ => "Grown-up",
        };
        let low = stat("full").min(stat("happy")).min(stat("hygiene")) < 20.0;
        let card = PetCard {
            name: st.get("name").and_then(Value::as_str).unwrap_or("Blip").to_string(),
            status: format!("{stage_name}  \u{b7}  {}", if low { "needs you" } else { "okay" }),
            species: self.look.species.clone(),
            palette: self.look.palette.clone(),
            stage,
            mood: self.body.mood(self.now()).to_string(),
            bars: vec![("meal", stat("full"), 0xf0a020), ("play", stat("happy"), 0xef5b8b), ("moon", stat("energy"), 0x6b7fd7)],
        };
        // Only what already works in the Rust version shows up; care, notes, timers and
        // colleagues join as their steps land.
        let items = vec![
            Item::toggle("walk", "Wander around", self.body.wander, "wander"),
            Item::row("x", "Hide pet", "hide"),
            Item::Sep,
            Item::row("power", "Quit Blip", "quit"),
        ];
        let mut m = Menu::new(Some(card), vec![], items, self.geo.s);
        m.card_cmd = String::new();
        m
    }

    fn open_menu(&mut self, el: &ActiveEventLoop, at_x: f64, at_y: f64) {
        self.close_menu();
        let menu = self.build_menu();
        let (w, h) = menu.size();
        let (work, _) = platform::work_area_at(at_x, at_y);
        let x = ((at_x + 4.0).min(work.right - w as f64 - 4.0)).max(work.left + 4.0) as i32;
        let y = ((at_y - 20.0).min(work.bottom - h as f64 - 4.0)).max(work.top + 4.0) as i32;
        let Ok(window) = el.create_window(base_attrs(w, h, x, y)) else { return };
        let window = Arc::new(window);
        let surface = platform::Surface::new(&window);
        window.set_visible(true);
        window.focus_window();
        self.menu = Some(MenuWin {
            window,
            surface,
            menu,
            canvas: Canvas::new(w, h),
            x,
            y,
            had_focus: false,
            opened: self.now(),
            away_since: None,
            dirty: true,
            sticky: false,
        });
    }

    fn close_menu(&mut self) {
        self.menu = None; // dropping the window closes it
    }

    fn menu_tick(&mut self, now: f64) {
        let Some(m) = self.menu.as_mut() else { return };
        // Borderless windows often don't get focus on macOS, so focus can't always tell us the
        // user moved on: close once the pointer has stayed away from the menu for a while.
        if m.sticky {
            if now - m.opened > 15.0 {
                self.menu = None;
                return;
            }
        } else if !m.had_focus {
            let (mx, my) = platform::cursor();
            let (w, h) = m.menu.size();
            let inside = mx >= (m.x - 12) as f64 && mx <= (m.x + w + 12) as f64 && my >= (m.y - 12) as f64 && my <= (m.y + h + 12) as f64;
            if inside {
                m.away_since = None;
            } else if m.away_since.is_none() {
                m.away_since = Some(now);
            } else if now - m.away_since.unwrap() > 3.0 && now - m.opened > 1.0 {
                self.menu = None;
                return;
            }
        }
        if m.dirty {
            if let Some(f) = self.fonts.as_mut() {
                m.menu.draw(&mut m.canvas, &self.theme, f);
            }
            let (w, h) = m.menu.size();
            m.surface.present(&m.canvas.px, w, h, m.x, m.y);
            m.surface.set_click_through_at_cursor(true);
            m.dirty = false;
        }
    }

    fn run(&mut self, el: &ActiveEventLoop, cmd: &str) {
        match cmd {
            "wander" => {
                self.body.wander = !self.body.wander;
                if !self.body.wander && self.body.action == Action::Walk {
                    self.body.action = Action::Idle;
                }
            }
            "hide" => self.set_visible(false),
            "show" => self.set_visible(true),
            "quit" => {
                self.quit = true;
                el.exit();
            }
            _ => {}
        }
    }

    // ------------------------------------------------ tray / menu bar
    fn make_tray(&mut self) {
        let open = MenuItem::new("Open Blip menu", true, None);
        let show = CheckMenuItem::new("Show pet on desktop", true, true, None);
        let quit = MenuItem::new("Quit Blip", true, None);
        let menu = TrayMenu::new();
        let _ = menu.append_items(&[&open, &show, &PredefinedMenuItem::separator(), &quit]);
        let icon = tray_image(&self.look);
        let built = TrayIconBuilder::new()
            .with_tooltip("Blip")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build();
        if let Ok(icon) = built {
            self.tray = Some(Tray { _icon: icon, open, show, quit });
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
            } else if ev.id == *t.quit.id() {
                self.run(el, "quit");
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = base_attrs(self.geo.w, self.geo.h, self.body.x as i32, self.body.y as i32);
        let window = Arc::new(el.create_window(attrs).expect("create the pet window"));
        self.surface = Some(platform::Surface::new(&window));
        self.window = Some(window);
        self.next_step = self.now();
        self.make_tray(); // after the event loop is running (required on macOS)
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.menu.as_ref().map(|m| m.window.id()) == Some(id) {
            self.menu_event(el, event);
            return;
        }
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                let (mx, my) = platform::cursor();
                if self.modifiers.control_key() && cfg!(target_os = "macos") {
                    self.open_menu(el, mx, my); // Control-click = right-click on a Mac
                    return;
                }
                self.close_menu();
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

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if self.window.is_none() || self.quit {
            return;
        }
        self.tray_events(el);
        self.tick(el);
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(15)));
    }
}

impl App {
    fn menu_event(&mut self, el: &ActiveEventLoop, event: WindowEvent) {
        let Some(m) = self.menu.as_mut() else { return };
        match event {
            WindowEvent::Focused(true) => m.had_focus = true,
            WindowEvent::Focused(false) => {
                if m.had_focus && !m.sticky {
                    self.close_menu(); // clicked somewhere else
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
                let cmd = m.menu.hover.and_then(|h| m.menu.command(h));
                if let Some(cmd) = cmd {
                    self.close_menu();
                    self.run(el, &cmd);
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.logical_key == Key::Named(NamedKey::Escape) => {
                self.close_menu();
            }
            _ => {}
        }
    }
}

/// The tray / menu-bar icon: the pet, drawn small.
fn tray_image(look: &Look) -> tray_icon::Icon {
    let g = sprites::build_pet(&sprites::PetLook {
        stage: Stage::Adult,
        palette: &look.palette,
        mood: "happy",
        frame: 1,
        facing: 0,
        squash: false,
        blink: false,
        species: &look.species,
    });
    let size = 32i32;
    let (bx, by, bw, bh) = blip_ui::sprite_bounds(&g);
    let cell = (size / bw.max(bh).max(1)).max(1);
    let (ox, oy) = ((size - bw * cell) / 2, (size - bh * cell) / 2);
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for (&(x, y), &c) in &g {
        for dy in 0..cell {
            for dx in 0..cell {
                let (px, py) = (ox + (x - bx) * cell + dx, oy + (y - by) * cell + dy);
                if (0..size).contains(&px) && (0..size).contains(&py) {
                    let i = ((py * size + px) * 4) as usize;
                    rgba[i..i + 4].copy_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8, 255]);
                }
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, size as u32, size as u32).expect("tray icon")
}

fn read_json(name: &str) -> Value {
    std::fs::read_to_string(platform::data_dir().join(name))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// The pet's look and position from the Python Blip's state.json (read only).
fn look_from_state(st: &Value) -> (Look, Option<f64>) {
    let s = |k: &str, d: &str| st.get(k).and_then(Value::as_str).unwrap_or(d).to_string();
    let stage_name = s("stage", "adult");
    let color = s("color", "auto");
    let palette = if st.get("gone").and_then(Value::as_bool) == Some(true) {
        "ghost".to_string()
    } else if st.get("sick").and_then(Value::as_bool) == Some(true) {
        "sick".to_string()
    } else if color != "auto" && sprites::PALETTES.contains(&color.as_str()) {
        color
    } else {
        match stage_name.as_str() {
            "baby" | "egg" => "baby".to_string(),
            "child" => "child".to_string(),
            _ => s("palette", "gold"),
        }
    };
    let look = Look { species: s("species", "blip"), palette, stage: Stage::from_name(&stage_name) };
    (look, st.get("x").and_then(Value::as_f64))
}

fn main() {
    #[allow(unused_mut)]
    let mut builder = EventLoop::builder();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder.with_activation_policy(ActivationPolicy::Accessory); // no Dock icon
    }
    let event_loop = builder.build().expect("event loop");
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("run");
}
