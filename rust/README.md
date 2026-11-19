# Blip in Rust

Blip, written in Rust. From v0.8.0 the released app is this one; installed Python Blips update
themselves to it and keep their pet, notes, timers, team and settings.

## Why

The Python app draws its windows with Tk, which works on Windows but is weak on macOS. It has no
menu-bar icon, focus on borderless windows is unreliable, mouse buttons and scrolling differ between
platforms, and screens with different scales cause trouble. The Rust app uses each system's own window
and tray handling, and builds natively for Apple Silicon and Intel Macs.

## Compatibility rule

Rust and Python pets must work together while people update. `blip-proto` is byte-for-byte
compatible with the Python wire format: signed envelopes, team encryption, room ids, voice packets.
`crates/blip-proto/tests/vectors.json` holds reference values made by the Python code
(`python rust/tools/make_vectors.py`), and the Rust tests must reproduce them exactly.

## Crates

| Crate | What | Status |
|---|---|---|
| `blip-proto` | Wire format, team crypto, link parsing, voice packets | done, matches Python |
| `blip-server` | Online server, drop-in for `blip/server.py` | done, passes the Python server tests |
| `blip-pet` | Sprites (all 8,064 poses match Python pixel for pixel), movement, physics, rendering | done |
| `blip-ui` | Software drawing: pixel boxes, pixel font and icons, system-font text, themes, the right-click menu | done |
| `blip-app` | The desktop pet and everything around it: windows, office network, calls, voice, Basecamp, updater | done; tested on Windows and on a Mac mini |

## Plan

1. **Server**: done. Same port, messages, status page and `/health`, plus the plain-line protocol of pets
   older than v0.7. The Python `tests/test_net.py` and `tests/test_voice.py` pass against it:
   `BLIP_SERVER_BIN=rust/target/release/blip-server python -m unittest tests.test_net tests.test_voice`.
   Then it replaces the Python server on the NAS (app `blip`).
2. **Pet window**: done. Windows: a layered window (real per-pixel transparency; Windows sends clicks on
   see-through pixels to whatever is behind). Mac: transparent window, image on its layer, clicks let
   through where there's no pet, no Dock icon. Sprites ported from `blip/sprites.py`, physics from
   `blip/app.py`. Run it: `cargo run --release -p blip-app` (it reads your pet's look from the Python
   Blip's state.json, read-only).
3. **Tray / menu bar and menu**: done. `tray-icon` gives a real tray icon on Windows and a menu-bar icon on
   Mac (Open menu, Show pet, Quit). The pixel menu is drawn by `blip-ui` (same layout as `blip/menu.py`,
   preview: `cargo run -p blip-ui --example menu_preview -- <dir> 1.25`) and opens on right-click,
   Control-click on a Mac, or press and hold. For now it only lists what works in Rust; the rest
   joins as later steps land. Testing without a mouse: `BLIP_DEMO_MENU=1` opens it by itself.
4. **Windows**: done. Settings, Notes, Timers & alarms, Buddies, Basecamp tasks, Welcome and small
   dialogs, laid out with `egui` and drawn by a small CPU renderer (`blip-app/src/soft.rs`) into
   normal system windows, so no GPU or OpenGL is involved on any computer.
5. **Office network**: done. UDP discovery, TCP messages and the encrypted server link, checked
   against the Python pets (LAN and server, messages, calls, replies).
6. **Voice, Basecamp, updater**: done. Voice uses `cpal` and the shipped libopus (LAN UDP or Opus
   through the server); calls work both ways with Python pets. Basecamp: token or OAuth, reminder
   cards, tick off to-dos. Updates from GitHub releases with SHA-256 checks.
7. **Switch-over release**: same installer name, AppId and settings files (`%APPDATA%\Blip`,
   `~/Library/Application Support/Blip`), so installed Python Blips update themselves to the Rust one.

## Testing hooks

Run with `BLIP_DATA=<folder>` to use a separate copy of the data (and a separate single-instance
lock). `BLIP_DEMO="window:settings;at:5:menu;ui:click:300,140"` runs commands, timed commands and
UI steps; `BLIP_SHOT=<folder>` saves every window, card, menu and the pet as PNGs (handy over ssh on
a Mac, where screen capture isn't allowed); `BLIP_LOG=<file>` writes a small event log;
`BLIP_NO_AUDIO=1` / `BLIP_FAKE_MIC=1` test calls without sound devices; `BLIP_UDP_PORT` /
`BLIP_TCP_PORT` move the office network ports.

## Build

```
cd rust
cargo test --workspace
cargo build --release -p blip-server     # target/release/blip-server
cargo build --release -p blip-app        # target/release/blip(.exe), the pet
```
