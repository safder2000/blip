//! Blip, the Rust version: the office desktop pet.
//!
//! Testing without a mouse: BLIP_DEMO=menu | timer-menu | card | care:meal | ... runs that after 2 s.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod basecamp;
mod cards;
mod gui;
mod http;
mod net;
mod office;
mod opus;
mod overlay;
mod platform;
mod shot;
mod soft;
mod sound;
mod updater;
mod voice;
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
    wins: windows::Windows,
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
        let Some(a) = self.app.as_mut() else { return };
        if std::env::var_os("BLIP_LOG_EVENTS").is_some() && !matches!(event, WindowEvent::RedrawRequested) {
            log(&format!("event {id:?} {event:?}"));
        }
        if self.wins.owns(id) {
            self.wins.event(a, el, id, event);
        } else {
            a.window_event(el, id, event);
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let Some(a) = self.app.as_mut() else { return };
        if a.quit {
            return;
        }
        a.tick(el);
        self.wins.tick(a, el);
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(15)));
    }

    fn exiting(&mut self, _el: &ActiveEventLoop) {
        if let Some(a) = self.app.as_mut() {
            a.shutdown();
        }
    }
}

/// Test log: with BLIP_LOG=<file>, append a line (used to check runs on the Mac over ssh).
pub fn log(msg: &str) {
    if let Some(p) = std::env::var_os("BLIP_LOG") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
            let _ = writeln!(f, "{:.1} {msg}", blip_core::now());
        }
    }
}

/// The tray / menu-bar icon: the pet, drawn small.
pub fn tray_image(look: &Look) -> tray_icon::Icon {
    let (rgba, size) = pet_rgba(look, 32);
    tray_icon::Icon::from_rgba(rgba, size, size).expect("tray icon")
}

/// The pet as a window icon (RGBA, size).
pub fn window_icon(a: &app::App) -> (Vec<u8>, u32) {
    pet_rgba(&a.look(), 64)
}

fn pet_rgba(look: &Look, size: u32) -> (Vec<u8>, u32) {
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
    let size = size as i32;
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
    (rgba, size as u32)
}

/// Release checks: `--selftest-opus` (bundled libopus encodes and decodes) and `--selftest-https`
/// (TLS works from the installed app). Exit code 0 = fine.
fn selftest(arg: &str) -> Option<i32> {
    match arg {
        "--selftest-opus" => {
            let ok = (|| {
                let mut e = opus::Encoder::new()?;
                let mut d = opus::Decoder::new()?;
                let frame: Vec<i16> = (0..opus::FRAME).map(|i| ((i as f64 * 0.2).sin() * 8000.0) as i16).collect();
                let pkt = e.encode(&frame)?;
                let out = d.decode(Some(&pkt))?;
                Some(out.len() == opus::FRAME)
            })()
            .unwrap_or(false);
            println!("opus {}", if ok { "ok" } else { "MISSING" });
            Some(if ok { 0 } else { 1 })
        }
        "--selftest-https" => {
            // any HTTP answer proves TLS works (GitHub may rate-limit CI machines with 403)
            let ok = !matches!(http::get_json("https://github.com/safder2000/blip", None, "text/html"), Err(http::Fail::Transport(_)));
            println!("https {}", if ok { "ok" } else { "FAILED" });
            Some(if ok { 0 } else { 1 })
        }
        "--version" => {
            println!("Blip {}", windows::VERSION);
            Some(0)
        }
        _ => None,
    }
}

fn main() {
    if let Some(code) = std::env::args().nth(1).and_then(|a| selftest(&a)) {
        std::process::exit(code);
    }
    // one Blip at a time (same lock as the Python version, so they never run side by side);
    // a test run with its own data folder gets its own lock
    let name = if std::env::var_os("BLIP_DATA").is_some() { "BlipDesktopPetTest" } else { "BlipDesktopPet" };
    if !platform::single_instance(name) {
        return;
    }
    #[allow(unused_mut)]
    let mut builder = EventLoop::builder();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder.with_activation_policy(ActivationPolicy::Accessory); // no Dock icon
    }
    let event_loop = builder.build().expect("event loop");
    let mut runner = Runner { app: None, wins: windows::Windows::default(), started: false };
    event_loop.run_app(&mut runner).expect("run");
}
