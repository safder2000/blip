//! Windows: the pet is a layered window. UpdateLayeredWindow gives it real per-pixel transparency,
//! and Windows itself sends clicks on see-through pixels to whatever is behind the pet.
//! Coordinates are physical pixels (winit makes the process per-monitor DPI aware).

use std::ptr::null_mut;

use blip_pet::body::Rect;
use windows_sys::Win32::Foundation::{HWND, POINT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, MonitorFromPoint, ReleaseDC,
    SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
    GetMonitorInfoW, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, ShowWindow, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, UpdateLayeredWindow, GWL_EXSTYLE, SW_SHOWNOACTIVATE,
    ULW_ALPHA, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

pub struct Surface {
    hwnd: HWND,
    pub failures: u64,
    /// never take focus (the pet, poops, cards): clicking them leaves your app active
    pub no_activate: bool,
    /// what was last handed to Windows: (x, y, w, h, hash of the pixels)
    last: Option<(i32, i32, i32, i32, u64)>,
    /// where the window is (moved with SetWindowPos, not UpdateLayeredWindow)
    at: Option<(i32, i32)>,
}

impl Surface {
    pub fn new(window: &std::sync::Arc<winit::window::Window>) -> Surface {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let hwnd = match window.window_handle().expect("window handle").as_raw() {
            RawWindowHandle::Win32(h) => h.hwnd.get() as HWND,
            _ => unreachable!("not a Win32 window"),
        };
        let s = Surface { hwnd, failures: 0, no_activate: false, last: None, at: None };
        s.ensure_layered();
        s
    }

    /// Layered: per-pixel alpha; tool window: no taskbar button, no Alt-Tab entry.
    /// winit rewrites the window style when it changes visibility and such, dropping these,
    /// so this is checked before every frame (it's one cheap call).
    fn ensure_layered(&self) {
        unsafe {
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            let mut want = (WS_EX_LAYERED | WS_EX_TOOLWINDOW) as isize;
            if self.no_activate {
                want |= WS_EX_NOACTIVATE as isize;
            }
            if ex & want != want {
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex | want);
            }
        }
    }

    /// Show `pixels` (premultiplied 0xAARRGGBB, w x h) with the window's top-left at (x, y).
    pub fn present(&mut self, pixels: &[u32], w: i32, h: i32, x: i32, y: i32) {
        self.ensure_layered();
        // Only touch the window when the picture or its place changed. Re-sending the same frame
        // dozens of times a second makes Windows drop real mouse clicks on the window now and then.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for &p in pixels {
            hash = (hash ^ p as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
        if self.last == Some((x, y, w, h, hash)) {
            return;
        }
        let same_picture = self.last.map_or(false, |l| (l.2, l.3, l.4) == (w, h, hash));
        self.last = Some((x, y, w, h, hash));
        // Move the window the ordinary way. Moving it through UpdateLayeredWindow (as the pet walks)
        // leaves Windows routing real mouse clicks past it until something else takes the
        // foreground - the "pet can't be clicked until I click the taskbar" bug.
        if self.at != Some((x, y)) {
            unsafe {
                SetWindowPos(self.hwnd, null_mut(), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE);
            }
            self.at = Some((x, y));
        }
        if same_picture {
            return;
        }
        unsafe {
            let screen = GetDC(null_mut());
            let dc = CreateCompatibleDC(screen);
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            };
            let mut bits: *mut core::ffi::c_void = null_mut();
            let bmp = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if !bmp.is_null() && !bits.is_null() {
                // u32 0xAARRGGBB is B,G,R,A in memory: exactly what a 32-bit DIB wants
                std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u32, (w * h) as usize);
                let old = SelectObject(dc, bmp);
                let blend = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };
                let _ = (x, y);
                let size = SIZE { cx: w, cy: h };
                let src = POINT { x: 0, y: 0 };
                if UpdateLayeredWindow(self.hwnd, screen, std::ptr::null(), &size, dc, &src, 0, &blend, ULW_ALPHA) == 0 {
                    self.failures += 1;
                }
                SelectObject(dc, old);
                DeleteObject(bmp);
            }
            DeleteDC(dc);
            ReleaseDC(null_mut(), screen);
        }
    }

    /// Make it visible. Windows that never take focus are shown without activating them,
    /// so a card or the pet appearing never steals the keyboard from what you're typing in.
    pub fn show(&mut self, window: &winit::window::Window) -> bool {
        self.ensure_layered();
        if self.no_activate {
            unsafe { ShowWindow(self.hwnd, SW_SHOWNOACTIVATE) };
            return true;
        }
        window.set_visible(true);
        true
    }

    /// Windows already lets clicks through see-through pixels of a layered window.
    pub fn set_click_through_at_cursor(&mut self, _over_pet: bool) {}
}

/// Usable desktop (without the taskbar) of the screen nearest (x, y), and its scale (1.0 = 100%).
pub fn work_area_at(x: f64, y: f64) -> (Rect, f64) {
    unsafe {
        let mon = MonitorFromPoint(POINT { x: x as i32, y: y as i32 }, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let mut rect = Rect { left: 0.0, top: 0.0, right: 1920.0, bottom: 1040.0 };
        if GetMonitorInfoW(mon, &mut mi) != 0 {
            let r = mi.rcWork;
            rect = Rect { left: r.left as f64, top: r.top as f64, right: r.right as f64, bottom: r.bottom as f64 };
        }
        let (mut dx, mut dy) = (96u32, 96u32);
        let scale = if GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) == 0 { dx as f64 / 96.0 } else { 1.0 };
        (rect, scale)
    }
}

/// Mouse position on screen, physical pixels.
pub fn cursor() -> (f64, f64) {
    let mut p = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut p) };
    (p.x as f64, p.y as f64)
}

pub fn left_button_down() -> bool {
    unsafe { (GetAsyncKeyState(VK_LBUTTON as i32) as u16 & 0x8000) != 0 }
}

pub fn data_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| ".".into())).join("Blip")
}

/// Windows set to dark mode for apps.
pub fn system_dark() -> bool {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0".encode_utf16().collect();
    let name: Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();
    let mut v: u32 = 1;
    let mut size = 4u32;
    let ok = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_DWORD, null_mut(),
                     &mut v as *mut u32 as *mut _, &mut size)
    } == 0;
    ok && v == 0
}

/// The login name, as a default display name.
pub fn user_name() -> String {
    std::env::var("USERNAME").unwrap_or_default()
}

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_NAME: &str = "BlipPet"; // same entry the Python version used

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// "Start with Windows" is on.
pub fn startup_enabled() -> bool {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let mut size = 0u32;
    unsafe {
        RegGetValueW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), wide(RUN_NAME).as_ptr(), RRF_RT_REG_SZ, null_mut(), null_mut(), &mut size) == 0
    }
}

pub fn set_startup(on: bool) {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ,
    };
    let mut key: HKEY = null_mut();
    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), 0, KEY_SET_VALUE, &mut key) != 0 {
            return;
        }
        if on {
            let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
            let val = wide(&format!("\"{exe}\""));
            RegSetValueExW(key, wide(RUN_NAME).as_ptr(), 0, REG_SZ, val.as_ptr() as *const u8, (val.len() * 2) as u32);
        } else {
            RegDeleteValueW(key, wide(RUN_NAME).as_ptr());
        }
        RegCloseKey(key);
    }
}

/// The signed-in person's display name ("Safder Ali"), or "".
pub fn full_name() -> String {
    use windows_sys::Win32::Security::Authentication::Identity::{GetUserNameExW, NameDisplay};
    let mut size = 0u32;
    unsafe {
        GetUserNameExW(NameDisplay, null_mut(), &mut size);
        if size == 0 {
            return String::new();
        }
        let mut buf = vec![0u16; size as usize + 1];
        if GetUserNameExW(NameDisplay, buf.as_mut_ptr(), &mut size) == 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..size as usize]).trim().to_string()
    }
}

pub fn open_path(path: &std::path::Path) {
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}

/// macOS only (the Dock icon); nothing to do on Windows.
pub fn show_in_dock(_show: bool) {}


/// False if another Blip already runs (a named mutex, like the Python version).
pub fn single_instance(name: &str) -> bool {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    let n = wide(&format!("Local\\{name}"));
    unsafe {
        let h = CreateMutexW(std::ptr::null(), 0, n.as_ptr());
        let err = GetLastError();
        // the handle is kept open for the life of the process
        h.is_null() || err != ERROR_ALREADY_EXISTS
    }
}
