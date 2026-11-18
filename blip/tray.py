"""Blip's icon in the Windows notification area (system tray).

Left-click opens Blip's own menu next to the tray; right-click shows a short list of
shortcuts. Runs on pystray's own thread; every action is handed to the UI thread.
If pystray or Pillow isn't installed (running from source), Blip simply has no tray icon.
"""
import sys

try:
    import pystray
    from PIL import Image, ImageDraw
except ImportError:      # optional when running from source
    pystray = None
if sys.platform == "darwin":   # pystray needs the main thread on macOS, which Tk owns; the Dock icon is used instead
    pystray = None

from .sprites import PALETTES, build_pet


def render(species, palette, size=64):
    g = build_pet("adult", palette if palette in PALETTES else "gold", "happy", 1, species=species)
    xs = [x for x, _ in g]
    ys = [y for _, y in g]
    x0, y0 = min(xs), min(ys)
    w, h = max(xs) - x0 + 1, max(ys) - y0 + 1
    cell = max(1, size // max(w, h))
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    ox, oy = (size - w * cell) // 2, (size - h * cell) // 2
    for (x, y), c in g.items():
        X, Y = ox + (x - x0) * cell, oy + (y - y0) * cell
        d.rectangle([X, Y, X + cell - 1, Y + cell - 1], fill=c)
    return img


class Tray:
    def __init__(self, app):
        self.app = app
        self.icon = None
        if pystray is None:
            return
        post = app.post
        Item, Menu = pystray.MenuItem, pystray.Menu

        def do(fn):
            return lambda icon, item: post(fn)

        menu = Menu(
            Item("Open Blip", do(app.menu_at_cursor), default=True),
            Item("Show pet on desktop", do(app.toggle_visible), checked=lambda i: app.st.get("visible", True)),
            Item("Do not disturb", do(lambda: app.set_dnd(not app.cfg["dnd"])), checked=lambda i: bool(app.cfg["dnd"])),
            Menu.SEPARATOR,
            Item("Timers & alarms", do(lambda: app.open_window("timers"))),
            Item("Buddies & inbox", do(lambda: app.open_window("buddies"))),
            Item("Basecamp tasks", do(lambda: app.open_window("tasks"))),
            Item("Settings", do(lambda: app.open_window("settings"))),
            Menu.SEPARATOR,
            Item("Check for updates", do(lambda: app.check_updates(manual=True))),
            Item("Quit Blip", do(app.quit)),
        )
        try:
            self.icon = pystray.Icon("Blip", render(app.species(), app.palette()), "Blip", menu)
            self.icon.run_detached()
        except Exception:
            self.icon = None

    def set_look(self, species, palette):
        if self.icon:
            try:
                self.icon.icon = render(species, palette)
            except Exception:
                pass

    def set_title(self, text):
        if self.icon:
            try:
                self.icon.title = text[:120]
            except Exception:
                pass

    def refresh_menu(self):
        if self.icon:
            try:
                self.icon.update_menu()
            except Exception:
                pass

    def stop(self):
        if self.icon:
            try:
                self.icon.stop()
            except Exception:
                pass
            self.icon = None
