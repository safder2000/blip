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
| `blip-app` | The desktop pet | next |

## Plan

1. **Server**: done. Same port, messages, status page and `/health`, plus the plain-line protocol of pets
   older than v0.7. The Python `tests/test_net.py` and `tests/test_voice.py` pass against it:
   `BLIP_SERVER_BIN=rust/target/release/blip-server python -m unittest tests.test_net tests.test_voice`.
   Then it replaces the Python server on the NAS (app `blip`).
2. **Pet window**: a transparent, borderless, always-on-top window (`winit` + `softbuffer`), with pixel
   sprites ported from `blip/sprites.py`, click-through outside the pet, physics, drag and throw.
3. **Tray / menu bar** (`tray-icon`) and the pixel right-click menu and cards in their own overlay windows.
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
