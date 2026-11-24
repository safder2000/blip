//! Add-ons that aren't part of Blip itself, so Blip stays light for everyone: a game is downloaded
//! only when someone gets it in the shop, from the GitHub release that matches this Blip, and
//! checked against its SHA-256 before it's kept. Nothing runs until it's played.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};

use crate::http;
use crate::windows::VERSION;

const REPO: &str = "safder2000/blip";

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Missing,
    Downloading,
    Ready,
    Failed(String),
}

pub struct Addons {
    dir: PathBuf,
    hoops: Arc<Mutex<State>>,
}

fn asset() -> &'static str {
    if cfg!(windows) {
        "blip-hoops-windows-x64.exe"
    } else {
        "blip-hoops-mac-arm64"
    }
}

fn file_name() -> &'static str {
    if cfg!(windows) {
        "blip-hoops.exe"
    } else {
        "blip-hoops"
    }
}

impl Addons {
    pub fn new(data_dir: &std::path::Path) -> Addons {
        let dir = data_dir.join("addons");
        let ready = dir.join(file_name()).exists() && std::fs::read_to_string(dir.join("blip-hoops.version")).map(|v| v.trim() == VERSION).unwrap_or(false);
        Addons { dir, hoops: Arc::new(Mutex::new(if ready { State::Ready } else { State::Missing })) }
    }

    /// Where the game is (BLIP_HOOPS points at a local build when testing).
    pub fn hoops_path(&self) -> PathBuf {
        std::env::var_os("BLIP_HOOPS").map(PathBuf::from).unwrap_or_else(|| self.dir.join(file_name()))
    }

    pub fn hoops(&self) -> State {
        if std::env::var_os("BLIP_HOOPS").is_some() {
            return State::Ready;
        }
        self.hoops.lock().map(|s| s.clone()).unwrap_or(State::Missing)
    }

    /// Download the game for this version of Blip, in the background.
    pub fn fetch_hoops(&self) {
        {
            let mut s = self.hoops.lock().unwrap();
            if *s == State::Downloading {
                return;
            }
            *s = State::Downloading;
        }
        let (dir, state) = (self.dir.clone(), self.hoops.clone());
        std::thread::spawn(move || {
            let r = download(&dir);
            crate::log(&format!("hoops add-on download: {r:?}"));
            *state.lock().unwrap() = match r {
                Ok(()) => State::Ready,
                Err(e) => State::Failed(e),
            };
        });
    }

    /// Remove the downloaded game (when it's not wanted any more).
    pub fn remove_hoops(&self) {
        let _ = std::fs::remove_file(self.dir.join(file_name()));
        let _ = std::fs::remove_file(self.dir.join("blip-hoops.version"));
        *self.hoops.lock().unwrap() = State::Missing;
    }
}

fn download(dir: &std::path::Path) -> Result<(), String> {
    let base = format!("https://github.com/{REPO}/releases/download/v{VERSION}/{}", asset());
    let sum = http::get_bytes(&format!("{base}.sha256"), 4096).map_err(|e| format!("checksum: {e}"))?;
    let want = String::from_utf8_lossy(&sum).split_whitespace().next().unwrap_or("").to_ascii_lowercase();
    if want.len() != 64 {
        return Err("bad checksum file".into());
    }
    let bin = http::get_bytes(&base, 64 << 20).map_err(|e| format!("download: {e}"))?;
    let got: String = Sha256::digest(&bin).iter().map(|b| format!("{b:02x}")).collect();
    if got != want {
        return Err("checksum didn't match".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{}.part", file_name()));
    std::fs::write(&tmp, &bin).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
    }
    let dest = dir.join(file_name());
    let _ = std::fs::remove_file(&dest);
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("blip-hoops.version"), VERSION).map_err(|e| e.to_string())?;
    Ok(())
}
