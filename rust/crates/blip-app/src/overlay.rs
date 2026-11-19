//! A borderless, always-on-top, see-through window that shows a [`Canvas`]: used for the pet, its
//! menu, cards and poops. Pixels that aren't drawn stay transparent (and let clicks through).

use std::sync::Arc;

use blip_ui::Canvas;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId, WindowLevel};

use crate::platform;

pub struct Overlay {
    pub window: Arc<Window>,
    surface: platform::Surface,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    shown: bool,
    /// let clicks through see-through pixels (pet, poop) or catch them all (menu, cards)
    pub click_through: bool,
}

pub fn attrs(w: i32, h: i32, x: i32, y: i32) -> winit::window::WindowAttributes {
    let a = Window::default_attributes()
        .with_title("Blip")
        .with_decorations(false)
        .with_resizable(false)
        .with_visible(false)
        .with_transparent(cfg!(target_os = "macos"))
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_inner_size(winit::dpi::PhysicalSize::new(w.max(1) as u32, h.max(1) as u32))
        .with_position(winit::dpi::PhysicalPosition::new(x, y));
    #[cfg(windows)]
    let a = {
        use winit::platform::windows::WindowAttributesExtWindows;
        a.with_skip_taskbar(true)
    };
    #[cfg(target_os = "macos")]
    let a = {
        use winit::platform::macos::WindowAttributesExtMacOS;
        a.with_has_shadow(false)
    };
    a
}

impl Overlay {
    pub fn new(el: &ActiveEventLoop, w: i32, h: i32, x: i32, y: i32, click_through: bool) -> Option<Overlay> {
        let window = Arc::new(el.create_window(attrs(w, h, x, y)).ok()?);
        let surface = platform::Surface::new(&window);
        Some(Overlay { window, surface, x, y, w, h, shown: false, click_through })
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    /// Show `cv` with the top-left at (x, y).
    pub fn present(&mut self, cv: &Canvas, x: i32, y: i32) {
        self.x = x;
        self.y = y;
        self.w = cv.w;
        self.h = cv.h;
        if !self.shown {
            self.window.set_visible(true);
            self.shown = true;
        }
        self.surface.present(&cv.px, cv.w, cv.h, x, y);
        let over = if self.click_through {
            let (mx, my) = platform::cursor();
            let (lx, ly) = (mx as i32 - x, my as i32 - y);
            lx >= 0 && ly >= 0 && lx < cv.w && ly < cv.h && cv.px[(ly * cv.w + lx) as usize] >> 24 != 0
        } else {
            true
        };
        self.surface.set_click_through_at_cursor(over);
    }

    pub fn hide(&mut self) {
        self.window.set_visible(false);
        self.shown = false;
    }

    /// Screen point inside this window?
    pub fn contains(&self, sx: f64, sy: f64, margin: i32) -> bool {
        sx >= (self.x - margin) as f64 && sx <= (self.x + self.w + margin) as f64 && sy >= (self.y - margin) as f64 && sy <= (self.y + self.h + margin) as f64
    }
}
