# Blip

A little pixel pet that lives on your Windows taskbar and helps around the office:
calls and messages between colleagues' pets, timers, stopwatches, alarms, and Basecamp reminders.

![Blip](assets/blip.png)

## Install

Download **`BlipSetup-x.y.z.exe`** from the [latest release](https://github.com/safder2000/blip/releases/latest) and run it.
No admin rights needed. It installs to `%LOCALAPPDATA%\Programs\Blip` and starts Blip.

- The setup isn't code-signed, so Windows may say *"Windows protected your PC"* the first time:
  click **More info → Run anyway**.
- The first time Blip uses the network, Windows may ask to allow it. Click **Allow** (private networks).
- Blip updates itself: it checks GitHub every few hours, downloads the new setup, verifies its SHA-256,
  installs silently and restarts. (Settings → *Install updates automatically*.)

## Using it

- **Left-click** pat · **drag** pick up · **right-click** everything else.
- Click a poop to clean it. Click the timer sign to open *Timers & alarms*.

### Calls and messages (office network)

Pets on the same network find each other automatically (UDP broadcast on port 47610, messages on TCP 47611).

- Right-click → **Call ▸ name**: their pet runs to the middle of their screen and rings.
  They answer with *Coming*, *2 min*, *Busy* or a custom reply, and your pet shows it.
- Right-click → **Message ▸ name** for a short note. History is in **Buddies & inbox**.
- **Do not disturb** silences calls (callers get an automatic "busy").
- Only pets with the same **office passphrase** (Settings → Office network) can see each other.
  Every packet is signed with it.
- Pets on other floors/VLANs or isolated Wi-Fi can't hear broadcasts. Either list their IPs in
  *Extra PCs*, or run a relay on any always-on machine and put `host:47620` in *Relay*:

  ```
  python -m blip.relay
  ```

### Timers, stopwatch, alarms

Right-click → **Timer ▸** 1–60 min or *Other…* (`25`, `1:30`, `90s`, `2h 15m`), **Stopwatch**, **Alarm…**.
**Timers & alarms…** adds labels and repeating alarms (weekdays, every day, weekends), and pauses, resets or deletes them.
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
