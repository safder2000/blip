# Blip

A pixel desktop pet for the office: calls and messages between colleagues' pets, timers, alarms
and Basecamp reminders. Windows and Mac.

![Blip](assets/blip.png)

## Install

**Windows:** download `BlipSetup-x.y.z.exe` from the [latest release](https://github.com/safder2000/blip/releases/latest)
and run it. If Windows says "protected your PC", click More info → Run anyway.

**Mac (Apple Silicon):** paste this in Terminal:

```
curl -fsSL https://raw.githubusercontent.com/safder2000/blip/main/install-mac.sh | bash
```

Nothing else is needed: Python is bundled inside the app. Blip updates itself.

## Use

Right-click the pet for everything: care, timers, colleagues, settings.
Drag to move it, fling it to throw it.

Pets on the same network find each other automatically. To reach teammates elsewhere, paste a
server's invite link in Settings → Office → Online server. To host your own server, see [SERVER.md](SERVER.md).

## Data

`%APPDATA%\Blip` on Windows, `~/Library/Application Support/Blip` on Mac.

## Development

```
python blip_main.pyw                          # run from source (Python 3.10+)
python -m unittest discover -s tests -t .     # tests
```

Push a tag like `v1.2.3` to build and publish a release for Windows and Mac.
