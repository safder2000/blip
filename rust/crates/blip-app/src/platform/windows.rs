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
    GetCursorPos, GetWindowLongPtrW, SetWindowLongPtrW, UpdateLayeredWindow, GWL_EXSTYLE, ULW_ALPHA,
    WS_EX_LAYERED, WS_EX_TOOLWINDOW,
};

pub struct Surface {
    hwnd: HWND,
    pub failures: u64,
}

impl Surface {
    pub fn new(window: &std::sync::Arc<winit::window::Window>) -> Surface {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let hwnd = match window.window_handle().expect("window handle").as_raw() {
            RawWindowHandle::Win32(h) => h.hwnd.get() as HWND,
            _ => unreachable!("not a Win32 window"),
        };
        let s = Surface { hwnd, failures: 0 };
        s.ensure_layered();
        s
    }

    /// Layered: per-pixel alpha; tool window: no taskbar button, no Alt-Tab entry.
    /// winit rewrites the window style when it changes visibility and such, dropping these,
    /// so this is checked before every frame (it's one cheap call).
    fn ensure_layered(&self) {
        unsafe {
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            let want = (WS_EX_LAYERED | WS_EX_TOOLWINDOW) as isize;
            if ex & want != want {
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex | want);
            }
        }
    }

    /// Show `pixels` (premultiplied 0xAARRGGBB, w x h) with the window's top-left at (x, y).
    pub fn present(&mut self, pixels: &[u32], w: i32, h: i32, x: i32, y: i32) {
        self.ensure_layered();
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
                let dst = POINT { x, y };
                let size = SIZE { cx: w, cy: h };
                let src = POINT { x: 0, y: 0 };
                if UpdateLayeredWindow(self.hwnd, screen, &dst, &size, dc, &src, 0, &blend, ULW_ALPHA) == 0 {
                    self.failures += 1;
                }
                SelectObject(dc, old);
                DeleteObject(bmp);
            }
            DeleteDC(dc);
            ReleaseDC(null_mut(), screen);
        }
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
