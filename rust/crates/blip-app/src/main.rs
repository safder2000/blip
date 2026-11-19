//! Blip, the Rust version: the office desktop pet.
//!
//! Testing without a mouse: BLIP_DEMO=menu | timer-menu | card | care:meal | ... runs that after 2 s.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod cards;
mod office;
mod overlay;
mod platform;
mod sound;
mod windows;

use std::time::{Duration, Instant};

use blip_pet::body::Look;
use blip_pet::sprites;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

struct Runner {
    app: Option<app::App>,
    started: bool,
}

impl ApplicationHandler for Runner {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.started {
            return;
        }
        self.started = true;
        let mut a = app::App::new();
        a.start(el);
        self.app = Some(a);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let Some(a) = self.app.as_mut() {
            a.window_event(el, id, event);
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let Some(a) = self.app.as_mut() else { return };
        if a.quit {
            return;
        }
        a.tick(el);
        windows::open_requested(a, el);
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(15)));
    }

    fn exiting(&mut self, _el: &ActiveEventLoop) {
        if let Some(a) = self.app.as_mut() {
            a.shutdown();
        }
    }
}

/// The tray / menu-bar icon: the pet, drawn small.
pub fn tray_image(look: &Look) -> tray_icon::Icon {
    let g = sprites::build_pet(&sprites::PetLook {
        stage: sprites::Stage::Adult,
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

fn main() {
    #[allow(unused_mut)]
    let mut builder = EventLoop::builder();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder.with_activation_policy(ActivationPolicy::Accessory); // no Dock icon
    }
    let event_loop = builder.build().expect("event loop");
    let mut runner = Runner { app: None, started: false };
    event_loop.run_app(&mut runner).expect("run");
}
