# Blip in Rust

The Rust port of Blip. The Python Blip in the repository root keeps working, and gets the updates,
until the Rust app replaces it in one switch-over release.

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
| `blip-app` | The desktop pet: transparent window, walk, drag, throw, click-through, tray / menu-bar icon, right-click menu | steps 2-3 done on Windows; Mac built and launched by CI, not yet seen on a real Mac |

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
4. **Windows**: Settings, Notes, Timers, Buddies (`egui`, charcoal theme, same layout as today).
5. **Office network**: UDP discovery, TCP messages and the server link (`tokio`), using `blip-proto`.
6. **Voice** (`cpal` + Opus), **Basecamp**, **updater**.
7. **Switch-over release**: same installer name, AppId and settings files (`%APPDATA%\Blip`,
   `~/Library/Application Support/Blip`), so installed Python Blips update themselves to the Rust one
   and keep their pet, notes, timers and team.

## Build

```
cd rust
cargo test --workspace
cargo build --release -p blip-server     # target/release/blip-server
```
