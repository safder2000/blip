//! Self-update from GitHub Releases (port of `blip/updater.py`). A release carries
//! BlipSetup-<ver>.exe (+ .sha256) for Windows and Blip-mac-arm64.zip (+ .sha256) for macOS.
//! When a newer version is out, Blip downloads it, checks the SHA-256, starts the installer (or
//! the Mac swap script) and quits; the new Blip starts by itself.

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use winit::event_loop::ActiveEventLoop;

use crate::app::App;
use crate::cards::ShowCard;
use crate::http;

const API: &str = "https://api.github.com/repos/safder2000/blip/releases/latest";
const MAC_ASSET: &str = "Blip-mac-arm64.zip";

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub name: String,
    pub sha_url: String,
}

enum Event {
    Checked(Result<Option<Release>, String>, bool, bool),
    Downloaded(Result<PathBuf, String>),
}

pub struct Updater {
    busy: bool,
    pending: Option<Release>,
    next_check: f64,
    retry_install: f64,
    tx: Sender<Event>,
    rx: Receiver<Event>,
}

impl Default for Updater {
    fn default() -> Self {
        let (tx, rx) = channel();
        let r = blip_core::Rng::new().f();
        Updater { busy: false, pending: None, next_check: blip_core::now() + 60.0 + 60.0 * r, retry_install: 0.0, tx, rx }
    }
}

pub fn parse_version(s: &str) -> (u64, u64, u64) {
    let mut nums = s.split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).map(|x| x.parse::<u64>().unwrap_or(0));
    (nums.next().unwrap_or(0), nums.next().unwrap_or(0), nums.next().unwrap_or(0))
}

pub fn is_newer(remote: &str, local: &str) -> bool {
    parse_version(remote) > parse_version(local)
}

/// Installed from a real installer / Blip.app (not a development build).
pub fn installed() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    if cfg!(target_os = "macos") {
        return exe.ancestors().any(|p| p.extension().map_or(false, |e| e == "app"));
    }
    exe.parent().map_or(false, |d| d.join("unins000.exe").exists())
}

fn pick(rel: &Value) -> Option<Release> {
    let mac = cfg!(target_os = "macos");
    let mut item = None;
    let mut sha = String::new();
    for a in rel.get("assets")?.as_array()? {
        let n = a.get("name").and_then(Value::as_str).unwrap_or("").to_lowercase();
        let url = a.get("browser_download_url").and_then(Value::as_str).unwrap_or("").to_string();
        let real = a.get("name").and_then(Value::as_str).unwrap_or("").to_string();
        if mac {
            if n == MAC_ASSET.to_lowercase() {
                item = Some((url, real));
            } else if n == format!("{}.sha256", MAC_ASSET.to_lowercase()) {
                sha = url;
            }
        } else if n.starts_with("blipsetup") && n.ends_with(".exe") {
            item = Some((url, real));
        } else if n.ends_with(".exe.sha256") {
            sha = url;
        }
    }
    let (url, name) = item?;
    Some(Release { version: rel.get("tag_name").and_then(Value::as_str).unwrap_or("").trim_start_matches('v').to_string(), url, name, sha_url: sha })
}

pub fn check(app: &mut App, manual: bool, quiet: bool) {
    if app.updater.busy {
        return;
    }
    app.updater.busy = true;
    app.update_state = Some(("checking", "Checking for updates\u{2026}".into()));
    if manual && !quiet {
        app.board("Checking...", 3.0);
    }
    let tx = app.updater.tx.clone();
    std::thread::spawn(move || {
        let r = match http::get_json(API, None, "application/vnd.github+json") {
            Ok(rel) => Ok(pick(&rel).filter(|r| is_newer(&r.version, crate::windows::VERSION))),
            Err(e) => Err(e.to_string()),
        };
        let _ = tx.send(Event::Checked(r, manual, quiet));
    });
}

fn download(rel: Release) -> Result<PathBuf, String> {
    if rel.sha_url.is_empty() {
        return Err("release has no checksum file".into());
    }
    let data = http::get_bytes(&rel.url, 400 << 20).map_err(|e| e.to_string())?;
    let want = http::get_bytes(&rel.sha_url, 4096).map_err(|e| e.to_string())?;
    let want = String::from_utf8_lossy(&want).split_whitespace().next().unwrap_or("").to_lowercase();
    let got: String = Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect();
    if got != want {
        return Err("download checksum mismatch".into());
    }
    let dir = std::env::temp_dir().join("blip-update");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(&rel.name);
    std::fs::write(&path, data).map_err(|e| e.to_string())?;
    Ok(path)
}

const MAC_SWAP: &str = r#"#!/bin/bash
# Blip updater: wait for the old app to quit, swap in the new one, start it.
OLD_PID="$1"; NEW_ZIP="$2"; APP="$3"
for i in $(seq 1 60); do kill -0 "$OLD_PID" 2>/dev/null || break; sleep 0.5; done
WORK="$(mktemp -d)"
/usr/bin/ditto -x -k "$NEW_ZIP" "$WORK" || exit 1
[ -d "$WORK/Blip.app" ] || exit 1
rm -rf "$APP.old"
mv "$APP" "$APP.old" && mv "$WORK/Blip.app" "$APP" && rm -rf "$APP.old"
/usr/bin/xattr -dr com.apple.quarantine "$APP" 2>/dev/null
/usr/bin/open "$APP"
rm -rf "$WORK"
"#;

fn launch_installer(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let bundle = crate::platform::app_bundle().ok_or("not running as Blip.app")?;
        let script = path.with_file_name("blip-swap.sh");
        std::fs::write(&script, MAC_SWAP).map_err(|e| e.to_string())?;
        std::process::Command::new("/bin/bash")
            .arg(&script)
            .arg(std::process::id().to_string())
            .arg(path)
            .arg(bundle)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = MAC_SWAP;
        std::process::Command::new(path)
            .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS"])
            .current_dir(path.parent().unwrap_or(std::path::Path::new(".")))
            .creation_flags(0x0000_0008 | 0x0000_0200) // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn install(app: &mut App, rel: Release) {
    if app.busy_with_people() {
        app.update_state = Some(("new", format!("Blip {} installs after your call", rel.version)));
        app.updater.pending = Some(rel);
        app.updater.retry_install = blip_core::now() + 600.0;
        return;
    }
    app.update_state = Some(("installing", format!("Installing Blip {}\u{2026} Blip restarts by itself", rel.version)));
    app.board(&format!("Updating v{}", rel.version), 30.0);
    app.updater.busy = true;
    let tx = app.updater.tx.clone();
    std::thread::spawn(move || {
        let _ = tx.send(Event::Downloaded(download(rel)));
    });
}

fn card(app: &mut App, el: &ActiveEventLoop, title: &str, body: &str, buttons: Vec<(&str, &str)>, timeout: Option<f64>, key: &str) {
    let buttons = buttons.into_iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    app.show_card(el, ShowCard { key, title, body, kind: "update", icon: Some("task"), buttons, timeout, on_close: "" });
}

pub fn run(app: &mut App, el: &ActiveEventLoop, cmd: &str) -> bool {
    match cmd {
        "update:check" => check(app, true, true),
        "update:check-loud" => check(app, true, false),
        "update:install" => {
            if let Some(r) = app.updater.pending.take() {
                install(app, r);
            }
        }
        _ => return false,
    }
    let _ = el;
    true
}

pub fn tick(app: &mut App, el: &ActiveEventLoop) {
    let now = blip_core::now();
    if now >= app.updater.next_check {
        app.updater.next_check = now + 6.0 * 3600.0 + 1800.0 * blip_core::Rng::new().f();
        check(app, false, false);
    }
    if app.updater.retry_install > 0.0 && now >= app.updater.retry_install {
        app.updater.retry_install = 0.0;
        if let Some(r) = app.updater.pending.take() {
            install(app, r);
        }
    }
    let evs: Vec<Event> = app.updater.rx.try_iter().collect();
    for e in evs {
        app.updater.busy = false;
        app.windows_dirty = true;
        match e {
            Event::Checked(Err(err), manual, quiet) => {
                app.update_state = Some(("error", "Couldn't reach GitHub. Check your internet and try again.".into()));
                if manual && !quiet {
                    card(app, el, "Update check", &format!("Couldn't check for updates.\n{err}"), vec![("OK", "")], Some(30.0), "");
                }
            }
            Event::Checked(Ok(None), manual, quiet) => {
                app.cfg.set("last_update_check", json!(now));
                app.update_state = Some(("ok", format!("You're up to date  \u{b7}  checked {}", crate::windows::ago(now))));
                if manual && !quiet {
                    card(app, el, "Up to date", &format!("You have the latest Blip (v{}).", crate::windows::VERSION), vec![("OK", "")], Some(20.0), "");
                }
            }
            Event::Checked(Ok(Some(rel)), manual, quiet) => {
                app.cfg.set("last_update_check", json!(now));
                app.update_state = Some(("new", format!("Blip {} is available", rel.version)));
                if installed() && (app.cfg.bool("auto_update") || manual) {
                    install(app, rel);
                } else if installed() {
                    let body = format!("Blip v{} is out.", rel.version);
                    app.updater.pending = Some(rel);
                    card(app, el, "Update ready", &body, vec![("Update now", "update:install"), ("Later", "")], None, "update");
                } else {
                    app.update_state = Some(("new", format!("Blip {} is out (this is a development build)", rel.version)));
                    if !quiet {
                        card(app, el, "Update", &format!("Blip v{} is out (this is a development build).", rel.version), vec![("OK", "")], Some(60.0), "update");
                    }
                }
            }
            Event::Downloaded(Err(err)) => {
                app.update_state = Some(("error", format!("Update failed: {err}")));
                card(app, el, "Update failed", &format!("Couldn't download the update.\n{err}"), vec![("OK", "")], Some(60.0), "");
            }
            Event::Downloaded(Ok(path)) => match launch_installer(&path) {
                Ok(()) => {
                    app.run(el, "quit");
                    return;
                }
                Err(err) => {
                    app.update_state = Some(("error", format!("Update failed: {err}")));
                    card(app, el, "Update failed", &format!("Couldn't start the update.\n{err}"), vec![("OK", "")], Some(60.0), "");
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("0.8.1", "0.8.0"));
        assert!(is_newer("v1.0", "0.9.9"));
        assert!(!is_newer("0.8.0", "0.8.0"));
        assert!(!is_newer("0.7.7", "0.8.0"));
        assert_eq!(parse_version("v0.10.2-beta"), (0, 10, 2));
    }

    #[test]
    fn picks_platform_asset() {
        let rel = serde_json::json!({"tag_name": "v0.9.0", "assets": [
            {"name": "BlipSetup-0.9.0.exe", "browser_download_url": "https://x/setup.exe"},
            {"name": "BlipSetup-0.9.0.exe.sha256", "browser_download_url": "https://x/setup.sha"},
            {"name": "Blip-mac-arm64.zip", "browser_download_url": "https://x/mac.zip"},
            {"name": "Blip-mac-arm64.zip.sha256", "browser_download_url": "https://x/mac.sha"}]});
        let r = pick(&rel).unwrap();
        assert_eq!(r.version, "0.9.0");
        if cfg!(target_os = "macos") {
            assert_eq!(r.url, "https://x/mac.zip");
            assert_eq!(r.sha_url, "https://x/mac.sha");
        } else {
            assert_eq!(r.url, "https://x/setup.exe");
            assert_eq!(r.sha_url, "https://x/setup.sha");
        }
    }
}
