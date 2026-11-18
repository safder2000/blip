# Blip

A little pixel pet that lives on your Windows taskbar and helps around the office:
calls and messages between colleagues' pets, timers, stopwatches, alarms, and Basecamp reminders.

![Blip](assets/blip.png)

## Install

### Windows

Download **`BlipSetup-x.y.z.exe`** from the [latest release](https://github.com/safder2000/blip/releases/latest) and run it.
No admin rights needed. It installs to `%LOCALAPPDATA%\Programs\Blip` and starts Blip.

- The setup isn't code-signed, so Windows may say *"Windows protected your PC"* the first time:
  click **More info → Run anyway**.
- The first time Blip uses the network, Windows may ask to allow it. Click **Allow** (private networks).
- Blip updates itself: it checks GitHub every few hours, downloads the new setup, verifies its SHA-256,
  installs silently and restarts. Settings → General → Updates shows whether you are up to date; *Check now* checks on the spot.

### Mac (Apple Silicon)

Open Terminal and paste:

```
curl -fsSL https://raw.githubusercontent.com/safder2000/blip/main/install-mac.sh | bash
```

That downloads the latest `Blip-mac-arm64.zip`, checks its SHA-256, installs Blip.app into Applications and starts it.
No Apple developer account or licence is needed. Installing this way avoids the "unidentified developer" warning
you'd get from a browser download. macOS asks once for the local network (finding colleagues) and the
microphone (voice calls). Blip.app updates itself like the Windows version. Click Blip's Dock icon to bring the pet
and its menu back.

## Using it

- **Left-click** pat · **drag** pick up · **right-click** for the menu (care buttons, timers, colleagues, settings).
- **Throw it:** fling the pet and it curls into a ball and rockets off with a little flame trail, ricochets off
  the screen edges, the top and the taskbar, rolls to a stop and unfolds (dizzy after a wild ride).
  Just let go and it drops and bounces; once in a while it surprises you with a parachute.
- Click a poop to clean it. Click the timer sign to open *Timers & alarms*.
- Pets don't talk like people: they chirp in their own language ("mew?", "arf!", "beep~") with a little
  icon for what they want. Real information ("Sent!", "DND on", "Timer 25:00") shows on a signboard,
  and calls, messages, alarms and Basecamp reminders pop up as cards.
- Sounds are off by default. Settings → General → Sounds has a volume slider that plays a sample as you set it.
  Every sound is synthesised by the code itself, so no audio files ship with Blip.
- Windows and the menu follow your system's light or dark mode, switching live when it changes. Settings → General → Appearance picks Match system, Light or Dark.
- **Settings → Pet** to pick a look (Blip, Kitty, Bunny, Bear, Pup, Chick, Bot) and a colour.
  The first time Blip starts it asks your name and lets you pick your pet; colleagues see both.
- Blip also lives in the **system tray** (the ^ next to the clock): click it for the menu,
  right-click for shortcuts, or untick *Show pet on desktop* to keep just the tray icon.
  A hidden pet still pops out when someone calls or an alarm rings.

### Calls and messages (office network)

Pets on the same network find each other automatically, with nothing to set up
(UDP broadcast on port 47610, messages on TCP 47611). Pets also share the addresses of pets they know,
so if one PC can reach another floor, everyone gets introduced.

- The right-click menu lists everyone nearby with their pet's face and a **call** and **message** button.
  **Call**: their pet runs to the middle of their screen and rings.
  They answer with *Coming*, *2 min*, *Busy* or a custom reply, and your pet shows it.
- **Message** sends a short note (clicking the name does the same). History is in **Buddies & inbox**.
- **Voice calls:** when both PCs have a microphone, the ringing card has a **Talk** button. Audio goes straight
  between the two computers over the office network, encrypted with the group key. Use a headset to avoid echo.
- **Do not disturb** silences calls (callers get an automatic "busy").
- By default everyone in the office who has Blip can see each other. Turn on **Settings → Office →
  Private group** and share a group password to limit it to your team. Every packet is signed with it.
- If colleagues on another floor or isolated Wi-Fi still don't appear, add one of their PCs' IPs under
  *Other floors*, or run a relay on any always-on machine and put `host:47620` in *Relay*:

  ```
  python -m blip.relay
  ```

### Timers, stopwatch, alarms

Right-click → **Timer ▸** 1–60 min or *Other…* (`25`, `1:30`, `90s`, `2h 15m`), **Stopwatch**, **Alarm…**.
**Timer ▸ All timers & alarms** (or clicking the sign above the pet) adds labels and repeating alarms (weekdays, every day, weekends), and pauses, resets or deletes them.
The countdown shows on a pixel sign above the pet. When it rings, choose *Stop*, *+5 min* or *Snooze*.

### Basecamp

Settings → **Basecamp**, then either:

- paste a Basecamp **personal access token** and click *Connect*, or
- register an integration at <https://launchpad.37signals.com/integrations> with redirect URI
  `http://localhost:47615/callback`, enter its client ID and secret, and click *Sign in with browser*.
  To build the ID and secret into releases, add the repository secrets `BASECAMP_CLIENT_ID` and `BASECAMP_CLIENT_SECRET`.
  Then everyone can just click *Sign in with browser*.

Blip checks your overdue and due-today assignments every 15 minutes. It pops up a card for new ones,
and a daily summary at the time you choose (default 09:30). Tick to-dos off from **Basecamp tasks…**.

## Data

Everything lives in `%APPDATA%\Blip`: `state.json` (the pet), `config.json`, `timers.json`, `inbox.json`, `error.log`.

## Development

```
python blip_main.pyw              # run from source (Python 3.10+, no packages needed)
python blip_main.pyw --fast       # pet time 60x faster, separate save
python -m unittest discover -s tests -t .
python packaging/make_icon.py     # regenerate assets/blip.ico (needs Pillow)
```

### Releasing

```
git tag v0.2.0
git push origin v0.2.0
```

The `release` workflow runs the tests, builds `Blip.exe` with PyInstaller and `BlipSetup-0.2.0.exe` with Inno Setup,
then publishes both the setup and its `.sha256` as a GitHub release. Installed pets update themselves from there.
