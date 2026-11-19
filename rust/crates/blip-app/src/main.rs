//! Blip, the Rust version. Step 2 of the port: the pet on the desktop. It sits on the taskbar,
//! walks, can be dragged, thrown (it curls into a ball and ricochets) and dropped, and clicks
//! outside its pixels go to whatever is behind it. It looks like your pet from the Python Blip
//! (read-only: it never writes the Python app's files).
//!
//! Right-click / press and hold (the menu) arrives in step 3.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod platform;

use std::sync::Arc;
use std::time::{Duration, Instant};

use blip_pet::body::{Action, Body, Geo, Gesture, Look, Rect, ANIM, STEP};
use blip_pet::render;
use blip_pet::sprites::Stage;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId, WindowLevel};

struct App {
    window: Option<Arc<Window>>,
    surface: Option<platform::Surface>,
    body: Body,
    look: Look,
    geo: Geo,
    work: Rect,
    buf: Vec<u32>,
    t0: Instant,
    next_step: f64,
    next_anim: f64,
    next_work: f64,
    pressed: bool,
    shown: bool,
}

impl App {
    fn now(&self) -> f64 {
        self.t0.elapsed().as_secs_f64()
    }

    fn new() -> App {
        let (look, saved_x) = load_look();
        let (work, scale) = platform::work_area_at(saved_x.unwrap_or(f64::MAX), 500.0);
        let geo = Geo::new(scale);
        let x = saved_x.unwrap_or(work.right - geo.w as f64 - 140.0 * scale);
        let seed = Instant::now().elapsed().as_nanos() as u64 ^ (x as u64).wrapping_mul(0x9E3779B97F4A7C15);
        let body = Body::new(x, Body::ground(&geo, &work), seed);
        App {
            window: None,
            surface: None,
            body,
            look,
            geo,
            work,
            buf: Vec::new(),
            t0: Instant::now(),
            next_step: 0.0,
            next_anim: 0.0,
            next_work: 0.0,
            pressed: false,
            shown: false,
        }
    }

    fn tick(&mut self) {
        let now = self.now();
        if now >= self.next_work && self.body.action != Action::Held {
            // which screen are we on, and how big should we be there
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
                    if let Some(g) = self.body.release(&self.geo, self.next_step) {
                        if let Gesture::Thrown { .. } = g {
                            let (work, _) = platform::work_area_at(mx, my);
                            self.work = work;
                        }
                    }
                }
            }
            self.body.step(&self.geo, &self.work, self.next_step);
            if self.body.take_menu() {
                self.open_menu();
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
        self.draw(now);
    }

    fn draw(&mut self, now: f64) {
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

    fn open_menu(&mut self) {
        // step 3: the pixel menu. For now a happy hop so the gesture visibly works.
        self.body.happy_until = self.now() + 1.5;
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Blip")
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(false)
            .with_transparent(cfg!(target_os = "macos"))
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_inner_size(winit::dpi::PhysicalSize::new(self.geo.w as u32, self.geo.h as u32))
            .with_position(winit::dpi::PhysicalPosition::new(self.body.x as i32, self.body.y as i32));
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
        let window = Arc::new(el.create_window(attrs).expect("create the pet window"));
        self.surface = Some(platform::Surface::new(&window));
        self.window = Some(window);
        self.next_step = self.now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                let (mx, my) = platform::cursor();
                let now = self.now();
                self.body.press(mx, my, now);
                self.pressed = true;
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } => {
                self.open_menu();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if self.window.is_none() {
            return;
        }
        self.tick();
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(15)));
    }
}

/// The pet's look and position from the Python Blip's state.json (read only).
fn load_look() -> (Look, Option<f64>) {
    let path = platform::data_dir().join("state.json");
    let st: serde_json::Value =
        std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let s = |k: &str, d: &str| st.get(k).and_then(|v| v.as_str()).unwrap_or(d).to_string();
    let stage_name = s("stage", "adult");
    let color = s("color", "auto");
    let palette = if st.get("gone").and_then(|v| v.as_bool()) == Some(true) {
        "ghost".to_string()
    } else if st.get("sick").and_then(|v| v.as_bool()) == Some(true) {
        "sick".to_string()
    } else if color != "auto" && blip_pet::sprites::PALETTES.contains(&color.as_str()) {
        color
    } else {
        match stage_name.as_str() {
            "baby" | "egg" => "baby".to_string(),
            "child" => "child".to_string(),
            _ => s("palette", "gold"),
        }
    };
    let look = Look { species: s("species", "blip"), palette, stage: Stage::from_name(&stage_name) };
    (look, st.get("x").and_then(|v| v.as_f64()))
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
