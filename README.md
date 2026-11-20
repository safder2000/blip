# Blip

A pixel desktop pet for the office: calls and messages between colleagues' pets, timers, alarms
and Basecamp reminders. Windows and Mac.

![Blip](assets/blip.png)

## Install

**Windows:** paste this in PowerShell:

```
irm https://raw.githubusercontent.com/safder2000/blip/main/install-win.ps1 | iex
```

Or download `BlipSetup-x.y.z.exe` from the [latest release](https://github.com/safder2000/blip/releases/latest) and run it
(if Windows says "protected your PC", click More info → Run anyway).

**Mac (Apple Silicon):** paste this in Terminal:

```
curl -fsSL https://raw.githubusercontent.com/safder2000/blip/main/install-mac.sh | bash
```

Nothing else is needed: Blip is a single small app with nothing to install alongside it. It updates itself.

## Use

Right-click the pet for everything: care, timers, colleagues, settings.
Drag to move it, fling it to throw it; patting and throwing it is how you play with it.

A looked-after pet slowly finds and digs up coins while you're at your desk. Spend them in the
Shop (hats, glasses, pickaxes, pills) and swap wearables with buddies. It never asks for
attention and can be switched off in Settings; see [docs/coins.md](docs/coins.md).

Pets on the same network find each other automatically. To reach teammates elsewhere, paste a
server's invite link in Settings → Office → Online server. To host your own server, see [SERVER.md](SERVER.md).

## Data

`%APPDATA%\Blip` on Windows, `~/Library/Application Support/Blip` on Mac.

## Development

The app is written in Rust (`rust/`); see [rust/README.md](rust/README.md).

```
cd rust
cargo run --release -p blip-app               # run the pet
cargo test --workspace                        # tests
```

The older Python app (`blip/`) is kept for reference and for its tests of the online server.
Push a tag like `v1.2.3` to build and publish a release for Windows and Mac (a tag with a dash,
like `v1.2.3-rc.1`, makes a pre-release that installed Blips ignore).
