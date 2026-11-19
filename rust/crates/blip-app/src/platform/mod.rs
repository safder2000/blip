//! The few things that differ per system: showing a transparent window, finding the usable
//! screen area, and reading the mouse.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(not(any(windows, target_os = "macos")))]
compile_error!("Blip runs on Windows and macOS");
