//! Where Blip keeps its files, and reading/writing JSON safely (write to a temp file, then rename,
//! so a crash never leaves half a file). Same folder as the Python Blip.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// `%APPDATA%\Blip` on Windows, `~/Library/Application Support/Blip` on macOS; `BLIP_DATA` overrides.
pub fn data_dir() -> PathBuf {
    let d = if let Ok(over) = std::env::var("BLIP_DATA") {
        PathBuf::from(over)
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join("Library/Application Support/Blip")
    } else {
        PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| ".".into())).join("Blip")
    };
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

pub fn write_json(path: &Path, v: &Value) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", path.extension().and_then(|e| e.to_str()).unwrap_or("json")));
    std::fs::write(&tmp, serde_json::to_string_pretty(v).unwrap_or_default())?;
    std::fs::rename(&tmp, path)
}
