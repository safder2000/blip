//! macOS: a transparent, borderless window whose content view shows the pet as a CGImage on its
//! layer. Clicks on see-through pixels are let through by toggling "ignores mouse events" depending
//! on what is under the cursor. Coordinates are physical pixels like winit's (points x backing scale).
//!
//! Not run on a real Mac yet: built and smoke-tested by CI only.

use blip_pet::body::Rect;
use core_graphics::base::{kCGBitmapByteOrder32Little, kCGImageAlphaPremultipliedFirst, kCGRenderingIntentDefault};
use core_graphics::color_space::CGColorSpace;
use core_graphics::data_provider::CGDataProvider;
use core_graphics::image::CGImage;
use foreign_types::ForeignType;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::msg_send;
use objc2_app_kit::{NSEvent, NSScreen, NSView};
use objc2_foundation::MainThreadMarker;
use std::sync::Arc;

pub struct Surface {
    view: Retained<NSView>,
    window: Arc<winit::window::Window>,
    scale: f64,
    passthrough: bool,
}

impl Surface {
    pub fn new(window: &Arc<winit::window::Window>) -> Surface {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let view = match window.window_handle().expect("window handle").as_raw() {
            RawWindowHandle::AppKit(h) => unsafe { Retained::retain(h.ns_view.as_ptr() as *mut NSView).expect("view") },
            _ => unreachable!("not an AppKit window"),
        };
        unsafe {
            view.setWantsLayer(true);
            let ns_window: *mut AnyObject = msg_send![&*view, window];
            if !ns_window.is_null() {
                let _: () = msg_send![ns_window, setHasShadow: false];
            }
        }
        Surface { view, window: window.clone(), scale: window.scale_factor(), passthrough: false }
    }

    /// Show `pixels` (premultiplied 0xAARRGGBB, w x h physical) with the top-left at (x, y) physical.
    pub fn present(&mut self, pixels: &[u32], w: i32, h: i32, x: i32, y: i32) {
        self.scale = self.window.scale_factor();
        let _ = self.window.request_inner_size(winit::dpi::PhysicalSize::new(w as u32, h as u32));
        self.window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
        let bytes: Vec<u8> = pixels.iter().flat_map(|p| p.to_le_bytes()).collect();
        let provider = CGDataProvider::from_buffer(Arc::new(bytes));
        let image = CGImage::new(
            w as usize,
            h as usize,
            8,
            32,
            w as usize * 4,
            &CGColorSpace::create_device_rgb(),
            kCGBitmapByteOrder32Little | kCGImageAlphaPremultipliedFirst,
            &provider,
            false,
            kCGRenderingIntentDefault,
        );
        unsafe {
            let layer: *mut AnyObject = msg_send![&*self.view, layer];
            if !layer.is_null() {
                let contents = image.as_ptr() as *mut AnyObject;
                let _: () = msg_send![layer, setContents: contents];
                let _: () = msg_send![layer, setContentsScale: self.scale];
            }
        }
    }

    /// Let clicks through when the cursor is over a see-through part of the window.
    pub fn set_click_through_at_cursor(&mut self, over_pet: bool) {
        let want = !over_pet;
        if want != self.passthrough {
            self.passthrough = want;
            let _ = self.window.set_cursor_hittest(over_pet);
        }
    }
}

fn screens() -> Vec<(f64, Rect, f64)> {
    // (backing scale, visible frame in top-left physical coords, main screen height)
    let Some(mtm) = MainThreadMarker::new() else { return vec![] };
    let list = NSScreen::screens(mtm);
    let main_h = unsafe { list.firstObject() }.map(|s| s.frame().size.height).unwrap_or(900.0);
    list.iter()
        .map(|s| {
            let scale = s.backingScaleFactor();
            let v = s.visibleFrame();
            let top = main_h - (v.origin.y + v.size.height);
            (
                scale,
                Rect {
                    left: v.origin.x * scale,
                    top: top * scale,
                    right: (v.origin.x + v.size.width) * scale,
                    bottom: (top + v.size.height) * scale,
                },
                main_h,
            )
        })
        .collect()
}

pub fn work_area_at(x: f64, y: f64) -> (Rect, f64) {
    let mut best: Option<(f64, Rect, f64)> = None;
    for (scale, r, _) in screens() {
        let d = (if x < r.left { r.left - x } else if x > r.right { x - r.right } else { 0.0 })
            + (if y < r.top { r.top - y } else if y > r.bottom { y - r.bottom } else { 0.0 });
        if best.map_or(true, |b| d < b.0) {
            best = Some((d, r, scale));
        }
    }
    best.map(|(_, r, s)| (r, s)).unwrap_or((Rect { left: 0.0, top: 25.0, right: 1440.0, bottom: 900.0 }, 1.0))
}

pub fn cursor() -> (f64, f64) {
    let p = unsafe { NSEvent::mouseLocation() };
    let list = screens();
    let (scale, main_h) = list.first().map(|(s, _, h)| (*s, *h)).unwrap_or((1.0, 900.0));
    (p.x * scale, (main_h - p.y) * scale)
}

pub fn left_button_down() -> bool {
    unsafe { NSEvent::pressedMouseButtons() & 1 != 0 }
}

pub fn data_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()))
        .join("Library/Application Support/Blip")
}

/// macOS set to dark mode.
pub fn system_dark() -> bool {
    std::process::Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("dark"))
        .unwrap_or(true)
}

/// The login name, as a default display name.
pub fn user_name() -> String {
    std::env::var("USER").unwrap_or_default()
}

fn agent_path() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()))
        .join("Library/LaunchAgents/com.safder2000.blip.plist")
}

/// The Blip.app this binary runs from, if any.
pub fn app_bundle() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors().find(|p| p.extension().map_or(false, |e| e == "app")).map(|p| p.to_path_buf())
}

/// "Open at login" is on.
pub fn startup_enabled() -> bool {
    agent_path().exists()
}

pub fn set_startup(on: bool) {
    let path = agent_path();
    if !on {
        let _ = std::fs::remove_file(path);
        return;
    }
    let args: Vec<String> = match app_bundle() {
        Some(b) => vec!["/usr/bin/open".into(), b.display().to_string()],
        None => vec![std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default()],
    };
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let items: String = args.iter().map(|a| format!("<string>{}</string>", esc(a))).collect();
    let plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\
         <key>Label</key><string>com.safder2000.blip</string><key>ProgramArguments</key><array>{items}</array>\
         <key>RunAtLoad</key><true/></dict></plist>\n"
    );
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, plist);
}

/// The signed-in person's display name, or "".
pub fn full_name() -> String {
    std::process::Command::new("id")
        .arg("-F")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

pub fn open_path(path: &std::path::Path) {
    let _ = std::process::Command::new("open").arg(path).spawn();
}

/// Show or hide Blip's Dock icon (and app menu) while running.
pub fn show_in_dock(show: bool) {
    unsafe {
        let app: *mut AnyObject = msg_send![objc2::class!(NSApplication), sharedApplication];
        if !app.is_null() {
            let policy: isize = if show { 0 } else { 1 }; // Regular / Accessory
            let _: bool = msg_send![app, setActivationPolicy: policy];
            if show {
                let _: () = msg_send![app, activateIgnoringOtherApps: true];
            }
        }
    }
}
