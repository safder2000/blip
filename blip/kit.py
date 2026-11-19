"""A small flat widget kit for Blip's windows: buttons, toggles, inputs, chips, cards, avatars.

Everything is drawn with tk primitives so it looks the same on every PC; corners are
pixel-stepped to match the pet's pixel-art style.
"""
import ctypes
import sys
import tkinter as tk
import tkinter.font as tkfont

from .sprites import COL, GH, GW, ICON, PALETTES, build_egg, build_pet, draw_text, text_width

THEMES = {
    "light": dict(
        BG="#f5f3fa", SURFACE="#ffffff", LINE="#e6e0f0", INK=COL["out"], MUTED="#7b7393",
        ACCENT="#6b7fd7", ACCENT_DK="#5869c4", ACCENT_SOFT="#eef0fc", ACCENT_SOFT2="#e2e6fa", ON_ACCENT="#ffffff",
        HOVER="#f2eff9", OK="#34b37a", WARN="#eea320", WARN_INK="#b7791f", WARN_SOFT="#fdf3df",
        DANGER="#e5484d", DANGER_SOFT="#fdeeee", OFF="#b9b3c8", OFF_FILL="#c9cff0", TRACK="#d8d2e6",
        TRACK_SOFT="#ece8f4", TRACK_LIT="#d9d3e6", BORDER_HOV="#cfc6e2", PLACEHOLDER="#a9a2bd", TILE="#f7f5fb",
        KNOB="#ffffff", FRAME=COL["out"], SHADOW=COL["out"], PAPER="#fffdf7", PAPER_INK=COL["out"],
        PAPER_BTN="#ffffff"),
    "dark": dict(           # neutral charcoal, no blue tint
        BG="#191919", SURFACE="#1f1f1f", LINE="#303030", INK="#efefef", MUTED="#9c9c9c",
        ACCENT="#8a9bef", ACCENT_DK="#7385e0", ACCENT_SOFT="#2e2e2e", ACCENT_SOFT2="#383838", ON_ACCENT="#ffffff",
        HOVER="#272727", OK="#3ec58a", WARN="#e8ad48", WARN_INK="#e8bf72", WARN_SOFT="#2f2a20",
        DANGER="#ee6b6b", DANGER_SOFT="#352222", OFF="#5e5e5e", OFF_FILL="#3a3a3a", TRACK="#3a3a3a",
        TRACK_SOFT="#282828", TRACK_LIT="#4a4a4a", BORDER_HOV="#474747", PLACEHOLDER="#6b6b6b", TILE="#262626",
        KNOB="#ffffff", FRAME="#404040", SHADOW="#0a0a0a", PAPER="#1f1f1f", PAPER_INK="#efefef",
        PAPER_BTN="#2a2a2a"),
}
THEME = "dark"
globals().update(THEMES[THEME])


def set_theme(name):
    """'dark', 'light' or 'system'. Windows opened afterwards use the new colours."""
    global THEME
    if name == "system":
        from .plat import system_dark
        name = "dark" if system_dark() else "light"
    THEME = name if name in THEMES else "dark"
    globals().update(THEMES[THEME])
    return THEME

F = {}


def init(root):
    """Pick fonts once per Tk root."""
    fams = set(tkfont.families(root))
    if sys.platform == "darwin":        # macOS sizes are in points at 72 dpi: a little bigger
        body = semi = "Helvetica Neue"
        F.update(body=(body, 13), small=(body, 12), tiny=(body, 11), bold=(semi, 13, "bold"),
                 h2=(semi, 15, "bold"), h1=(semi, 18, "bold"), mono=("Menlo", 12))
        return
    body = "Segoe UI Variable Text" if "Segoe UI Variable Text" in fams else "Segoe UI"
    semi = "Segoe UI Variable Text Semibold" if "Segoe UI Variable Text Semibold" in fams else (
        "Segoe UI Semibold" if "Segoe UI Semibold" in fams else body)
    F.update(body=(body, 10), small=(body, 9), tiny=(body, 8), bold=(semi, 10), h2=(semi, 12),
             h1=(semi, 15), mono=("Consolas", 10))


def scale(w):
    from .plat import ui_scale
    return ui_scale(w)


def px_of(w, base=2):
    return max(1, round(base * scale(w)))


def pixel_box(cv, x0, y0, x1, y1, p, fill, tags=()):
    """A rectangle with one-step pixel corners."""
    cv.create_rectangle(x0 + p, y0, x1 - p, y1, fill=fill, width=0, tags=tags)
    cv.create_rectangle(x0, y0 + p, x1, y1 - p, fill=fill, width=0, tags=tags)


def framed_box(cv, x0, y0, x1, y1, p, border, fill, tags=()):
    pixel_box(cv, x0, y0, x1, y1, p, border, tags)
    pixel_box(cv, x0 + 1, y0 + 1, x1 - 1, y1 - 1, p, fill, tags)


def bg_of(w):
    try:
        return w.cget("bg")
    except tk.TclError:
        return BG


def tint_titlebar(top, caption=None, text=None, border=None):
    """Windows 11: colour the native title bar to match the window."""
    if sys.platform != "win32":
        return
    caption, text, border = caption or BG, text or INK, border or LINE
    try:
        top.update_idletasks()
        hwnd = ctypes.windll.user32.GetParent(top.winfo_id())

        def cref(h):
            return ctypes.c_int(int(h[1:3], 16) | int(h[3:5], 16) << 8 | int(h[5:7], 16) << 16)
        dwm = ctypes.windll.dwmapi
        for attr, col in ((35, caption), (36, text), (34, border)):
            dwm.DwmSetWindowAttribute(hwnd, attr, ctypes.byref(cref(col)), 4)
    except Exception:
        pass


def pet_photo(root, species="blip", palette="gold", size=32):
    """A PhotoImage of the pet, for window icons."""
    g = build_pet("adult", palette, "happy", 1, species=species)
    xs = [x for x, _ in g]
    ys = [y for _, y in g]
    x0, y0 = min(xs), min(ys)
    w, h = max(xs) - x0 + 1, max(ys) - y0 + 1
    cell = max(1, size // max(w, h))
    img = tk.PhotoImage(master=root, width=size, height=size)
    ox, oy = (size - w * cell) // 2, (size - h * cell) // 2
    for (x, y), c in g.items():
        X, Y = ox + (x - x0) * cell, oy + (y - y0) * cell
        img.put(c, to=(X, Y, X + cell, Y + cell))
    return img


# ------------------------------------------------------------------ buttons
def btn_styles():
    return {
        "primary": (ACCENT, ACCENT_DK, ON_ACCENT, ACCENT),
        "secondary": (SURFACE, HOVER, INK, LINE),
        "ghost": (None, HOVER, INK, None),
        "danger": (SURFACE, DANGER_SOFT, DANGER, LINE),
        "soft": (ACCENT_SOFT, ACCENT_SOFT2, ACCENT if THEME == "dark" else ACCENT_DK, ACCENT_SOFT),
    }


class Btn(tk.Canvas):

    def __init__(self, parent, text="", command=None, kind="secondary", icon=None, small=False,
                 width=None, tip=None):
        S = scale(parent)
        self.font = F["small"] if small else F["body"]
        self.text, self.icon, self.kind, self.command = text, icon, kind, command
        self.p = px_of(parent, 2)
        self.ip = max(1, round((1.5 if small else 2) * S))     # icon pixel size
        fw = tkfont.Font(font=self.font).measure(text) if text else 0
        iw = text_width(ICON[icon], self.ip) if icon else 0
        gap = round(6 * S) if (icon and text) else 0
        padx = round((8 if small else 12) * S) if text else round((6 if small else 8) * S)
        h = round((26 if small else 32) * S)
        w = width or (fw + iw + gap + 2 * padx)
        super().__init__(parent, width=w, height=h, bg=bg_of(parent), highlightthickness=0, bd=0,
                         cursor="hand2")
        self.w, self.h, self.fw, self.iw, self.gap = w, h, fw, iw, gap
        self.enabled = True
        self.hover = False
        self.bind("<Enter>", lambda e: self._hover(True))
        self.bind("<Leave>", lambda e: self._hover(False))
        self.bind("<ButtonRelease-1>", self._click)
        self.draw()

    def _hover(self, on):
        self.hover = on and self.enabled
        self.draw()

    def _click(self, e):
        if self.enabled and self.command and 0 <= e.x <= self.w and 0 <= e.y <= self.h:
            self.command()

    def set_enabled(self, on):
        self.enabled = on
        self.config(cursor="hand2" if on else "")
        self.draw()

    def set_text(self, text):
        self.text = text
        self.draw()

    def draw(self):
        self.delete("all")
        fill, hov, fg, border = btn_styles()[self.kind]
        bg = bg_of(self)
        fill = fill or bg
        if self.hover:
            fill = hov
        if not self.enabled:
            fg = OFF
            if self.kind == "primary":
                fill, border = OFF_FILL, OFF_FILL
        p = self.p
        if border:
            framed_box(self, 0, 0, self.w, self.h, p, border, fill)
        else:
            pixel_box(self, 0, 0, self.w, self.h, p, fill)
        cw = self.iw + self.gap + self.fw
        x = (self.w - cw) / 2
        if self.icon:
            draw_text(self, round(x), round((self.h - 7 * self.ip) / 2), ICON[self.icon], self.ip, fg)
            x += self.iw + self.gap
        if self.text:
            self.create_text(x, self.h / 2, text=self.text, font=self.font, fill=fg, anchor="w")


class Toggle(tk.Canvas):
    def __init__(self, parent, value=False, command=None):
        S = scale(parent)
        self.w, self.h = round(40 * S), round(22 * S)
        super().__init__(parent, width=self.w, height=self.h, bg=bg_of(parent), highlightthickness=0, bd=0,
                         cursor="hand2")
        self.p = px_of(parent, 2)
        self.value, self.command = bool(value), command
        self.bind("<ButtonRelease-1>", lambda e: self.set(not self.value, notify=True))
        self.draw()

    def set(self, value, notify=False):
        self.value = bool(value)
        self.draw()
        if notify and self.command:
            self.command(self.value)

    def get(self):
        return self.value

    def draw(self):
        self.delete("all")
        p, w, h = self.p, self.w, self.h
        pixel_box(self, 0, 0, w, h, p, ACCENT if self.value else TRACK)
        m = round(3 * p / 2) + 1
        k = h - 2 * m
        x0 = w - m - k if self.value else m
        pixel_box(self, x0, m, x0 + k, m + k, p, KNOB)


class Check(tk.Canvas):
    """A pixel checkbox."""

    def __init__(self, parent, value=False, command=None):
        S = scale(parent)
        self.s = round(20 * S)
        super().__init__(parent, width=self.s, height=self.s, bg=bg_of(parent), highlightthickness=0, bd=0,
                         cursor="hand2")
        self.value, self.command, self.hov = bool(value), command, False
        self.bind("<ButtonRelease-1>", lambda e=None: self.set(not self.value, notify=True))
        self.bind("<Enter>", lambda e=None: self._hover(True))
        self.bind("<Leave>", lambda e=None: self._hover(False))
        self.draw()

    def _hover(self, on):
        self.hov = on
        self.draw()

    def set(self, value, notify=False):
        self.value = bool(value)
        self.draw()
        if notify and self.command:
            self.command(self.value)

    def get(self):
        return self.value

    def draw(self):
        self.delete("all")
        p, s = px_of(self, 2), self.s
        if self.value:
            pixel_box(self, 0, 0, s, s, p, ACCENT)
            ip = max(1, round(2 * scale(self)))
            draw_text(self, (s - text_width(ICON["check"], ip)) / 2, (s - 7 * ip) / 2, ICON["check"], ip, ON_ACCENT)
        else:
            framed_box(self, 0, 0, s, s, p, ACCENT if self.hov else TRACK, SURFACE)


class VolumeSlider(tk.Canvas):
    """Pixel 'signal bars' that light up from mint to pink. Click or drag to set the volume;
    a sample chirp plays at that level when you let go. Click the speaker to mute."""
    BAR_COLORS = ["#7ad9b0", "#8cd5a0", "#a6d08c", "#c2ca78", "#dcc36a", "#f0b964", "#f6a26c", "#f68a7e",
                  "#f07592", "#e868a6"]

    def __init__(self, parent, value=60, command=None, preview=None, enabled=True):
        S = scale(parent)
        self.n = 10
        self.bw, self.gap = round(12 * S), round(5 * S)
        self.x0 = round(34 * S)
        self.h = round(36 * S)
        self.w = self.x0 + self.n * (self.bw + self.gap) + round(48 * S)
        super().__init__(parent, width=self.w, height=self.h, bg=bg_of(parent), highlightthickness=0, bd=0,
                         cursor="hand2")
        self.value = int(value)
        self.last_on = self.value or 60
        self.command, self.preview, self.enabled = command, preview, enabled
        self.bind("<Button-1>", self._press)
        self.bind("<B1-Motion>", self._drag)
        self.bind("<ButtonRelease-1>", self._release)
        self.draw()

    def set_enabled(self, on):
        self.enabled = on
        self.draw()

    def _level_at(self, x):
        if x < self.x0 - round(4 * scale(self)):
            return None   # the speaker
        return max(0, min(self.n, int((x - self.x0) // (self.bw + self.gap)) + 1))

    def _set(self, value):
        if value != self.value:
            self.value = value
            if value:
                self.last_on = value
            self.draw()
            if self.command:
                self.command(value)

    def _press(self, e):
        lvl = self._level_at(e.x)
        if lvl is None:
            self._set(0 if self.value else self.last_on)
        else:
            self._set(lvl * 10)

    def _drag(self, e):
        lvl = self._level_at(e.x)
        if lvl is not None:
            self._set(lvl * 10)

    def _release(self, e):
        if self.preview and self.value and self.enabled:
            self.preview(self.value)

    def draw(self):
        self.delete("all")
        S = scale(self)
        p = px_of(self, 2)
        lvl = self.value // 10
        on = self.enabled
        ink = INK if on else OFF
        sy = (self.h - 7 * p) / 2
        draw_text(self, 0, sy, ICON["speaker"], p, ink)
        if lvl == 0:
            draw_text(self, 6 * p, sy + p, "x", max(1, p // 2 + 1), DANGER if on else ink)
        else:
            for i in range(min(3, 1 + lvl // 4)):
                x = 6 * p + i * 2 * p
                hh = (2 + 2 * i) * p
                self.create_rectangle(x, self.h / 2 - hh / 2, x + p, self.h / 2 + hh / 2, fill=ink, width=0)
        for i in range(self.n):
            x = self.x0 + i * (self.bw + self.gap)
            bh = round(8 * S + (self.h - 10 * S) * (i + 1) / self.n)
            lit = i < lvl
            col = self.BAR_COLORS[i] if (lit and on) else (TRACK_LIT if lit else TRACK_SOFT)
            pixel_box(self, x, self.h - bh, x + self.bw, self.h, p, col)
        pct = "Mute" if lvl == 0 else f"{self.value}%"
        self.create_text(self.w - round(4 * S), self.h / 2, text=pct, anchor="e", font=F["small"],
                         fill=MUTED if on else OFF)


class Field(tk.Frame):
    """Flat text input with a 1px border that turns accent on focus."""

    def __init__(self, parent, value="", width=24, show=None, placeholder="", on_commit=None, mono=False):
        super().__init__(parent, bg=LINE, padx=1, pady=1)
        S = scale(parent)
        inner = tk.Frame(self, bg=SURFACE)
        inner.pack(fill="both", expand=True)
        self.entry = tk.Entry(inner, width=width, relief="flat", bd=0, bg=SURFACE, fg=INK, insertbackground=INK,
                              font=F["mono"] if mono else F["body"], show=show or "", highlightthickness=0,
                              disabledbackground=SURFACE)
        self.entry.pack(fill="x", padx=round(8 * S), pady=round(6 * S))
        self.placeholder = placeholder
        self.on_commit = on_commit
        self._ph = False
        if value:
            self.entry.insert(0, value)
        self._show = show or ""
        self.entry.bind("<FocusIn>", self._in)
        self.entry.bind("<FocusOut>", self._out)
        self.entry.bind("<Return>", lambda e: self._commit())
        self._put_placeholder()

    def _put_placeholder(self):
        if self.placeholder and not self.entry.get():
            self._ph = True
            self.entry.config(fg=PLACEHOLDER, show="")
            self.entry.insert(0, self.placeholder)

    def _in(self, e):
        self.config(bg=ACCENT)
        if self._ph:
            self._ph = False
            self.entry.delete(0, "end")
            self.entry.config(fg=INK, show=self._show)

    def _out(self, e):
        self.config(bg=LINE)
        self._commit()
        self._put_placeholder()

    def _commit(self):
        if self.on_commit:
            self.on_commit(self.get())

    def get(self):
        return "" if self._ph else self.entry.get()

    def set(self, value):
        self._ph = False
        self.entry.config(fg=INK, show=self._show)
        self.entry.delete(0, "end")
        self.entry.insert(0, value or "")
        if self.focus_get() is not self.entry:
            self._put_placeholder()

    def set_show(self, show):
        self._show = show
        if not self._ph:
            self.entry.config(show=show)


class Chips(tk.Frame):
    """A row of choices; one is selected."""

    def __init__(self, parent, options, value=None, command=None, small=True):
        super().__init__(parent, bg=bg_of(parent))
        self.command = command
        self.value = value
        self.btns = {}
        for i, (val, label) in enumerate(options):
            b = Btn(self, label, lambda v=val: self.set(v, True), kind="secondary", small=small)
            b.pack(side="left", padx=(0, round(4 * scale(parent))))
            self.btns[val] = b
        self.set(value)

    def set(self, value, notify=False):
        self.value = value
        for v, b in self.btns.items():
            b.kind = "soft" if v == value else "secondary"
            b.draw()
        if notify and self.command:
            self.command(value)

    def get(self):
        return self.value


class Card(tk.Frame):
    """White panel with a hairline border. Put children in .body."""

    def __init__(self, parent, pad=14):
        super().__init__(parent, bg=LINE, padx=1, pady=1)
        S = scale(parent)
        self.body = tk.Frame(self, bg=SURFACE, padx=round(pad * S), pady=round(pad * S))
        self.body.pack(fill="both", expand=True)


def label(parent, text, kind="body", fg=None, **kw):
    opts = {"font": F.get(kind, F["body"]), "fg": fg or INK, "bg": bg_of(parent), "anchor": "w", "justify": "left"}
    opts.update(kw)
    return tk.Label(parent, text=text, **opts)


def muted(parent, text, **kw):
    return label(parent, text, "small", MUTED, **kw)


def hline(parent, pady=8):
    f = tk.Frame(parent, bg=LINE, height=1)
    f.pack(fill="x", pady=round(pady * scale(parent)))
    return f


class SettingRow(tk.Frame):
    """Title + description on the left, a control on the right."""

    def __init__(self, parent, title, desc=None):
        super().__init__(parent, bg=bg_of(parent))
        S = scale(parent)
        left = tk.Frame(self, bg=bg_of(parent))
        left.pack(side="left", fill="x", expand=True)
        label(left, title, "body").pack(anchor="w")
        if desc:
            muted(left, desc, wraplength=round(320 * S)).pack(anchor="w")
        self.right = tk.Frame(self, bg=bg_of(parent))
        self.right.pack(side="right", padx=(round(12 * S), 0))


class Scroller(tk.Frame):
    """Vertically scrolling container; put children in .inner. Mouse wheel scrolls."""

    def __init__(self, parent, height=240, bg=None):
        bg = bg or bg_of(parent)
        super().__init__(parent, bg=bg)
        self.cv = tk.Canvas(self, bg=bg, highlightthickness=0, bd=0, height=height)
        self.cv.pack(fill="both", expand=True)
        self.inner = tk.Frame(self.cv, bg=bg)
        self.win = self.cv.create_window(0, 0, window=self.inner, anchor="nw")
        self.inner.bind("<Configure>", lambda e: self.cv.configure(scrollregion=self.cv.bbox("all")))
        self.cv.bind("<Configure>", lambda e: self.cv.itemconfig(self.win, width=e.width))
        self.bind_all_wheel()

    def bind_all_wheel(self):
        def wheel(e):
            if self.inner.winfo_height() > self.cv.winfo_height():
                self.cv.yview_scroll(int(-e.delta / 120), "units")
        self.cv.bind("<Enter>", lambda e: self.cv.bind_all("<MouseWheel>", wheel))
        self.cv.bind("<Leave>", lambda e: self.cv.unbind_all("<MouseWheel>"))

    def clear(self):
        for c in self.inner.winfo_children():
            c.destroy()


class Avatar(tk.Canvas):
    """A pet drawn at a fixed size; animate=True makes it breathe and blink."""

    def __init__(self, parent, species="blip", palette="gold", stage="adult", mood="normal", px=3,
                 animate=False, bg=None, crop=True):
        self.px = px
        self.crop = crop
        w, h = (24 * px, 26 * px) if crop else (GW * px, GH * px)
        super().__init__(parent, width=w, height=h, bg=bg or bg_of(parent), highlightthickness=0, bd=0)
        self.w, self.h = w, h
        self.species, self.palette, self.stage, self.mood = species, palette, stage, mood
        self.frame = 0
        self.animate = animate
        self.draw()
        if animate:
            self.after(300, self._tick)

    def set(self, **kw):
        for k, v in kw.items():
            setattr(self, k, v)
        self.draw()

    def _tick(self):
        if not self.winfo_exists():
            return
        self.frame += 1
        self.draw()
        self.after(320, self._tick)

    def draw(self):
        self.delete("all")
        f = self.frame
        if self.stage == "egg":
            g = build_egg(f, 0.5)
        else:
            blink = self.animate and f % 11 == 0
            g = build_pet(self.stage, self.palette if self.palette in PALETTES else "gold", self.mood, f,
                          squash=self.animate and f % 6 == 0, blink=blink, species=self.species)
        px = self.px
        ox = (self.w - GW * px) / 2
        oy = self.h - GH * px
        for (x, y), c in g.items():
            X, Y = ox + x * px, oy + y * px
            if Y >= 0:
                self.create_rectangle(X, Y, X + px, Y + px, fill=c, width=0)


# ------------------------------------------------------------------ window base
class Window:
    """A tool window: light background, pixel title, tinted title bar. Single instance per name."""
    title = "Blip"
    subtitle = ""
    icon = "heart"
    size = None

    def __init__(self, app):
        self.app = app
        S = self.S = app.S
        t = self.top = tk.Toplevel(app.root, bg=BG)
        t.withdraw()        # place it before it appears: popping up on the main screen first and then
        #                     jumping to a screen with another scale makes Windows put it back
        t.title(f"{self.title} - Blip")
        t.protocol("WM_DELETE_WINDOW", self.close)
        t.resizable(False, False)
        head = tk.Frame(t, bg=BG)
        head.pack(fill="x", padx=round(20 * S), pady=(round(16 * S), round(4 * S)))
        p = px_of(t, 2)
        cv = tk.Canvas(head, bg=BG, highlightthickness=0, height=7 * p + 2)
        cv.pack(anchor="w", fill="x")
        x = draw_text(cv, 0, 1, ICON[self.icon], p, ACCENT) + 4 * p
        draw_text(cv, x, 1, self.title.upper(), p, INK)
        if self.subtitle:
            muted(head, self.subtitle).pack(anchor="w", pady=(round(4 * S), 0))
        self.body = tk.Frame(t, bg=BG)
        self.body.pack(fill="both", expand=True, padx=round(20 * S), pady=(round(8 * S), round(20 * S)))
        self.build(self.body)
        t.update_idletasks()
        l, tp, r, b = app.work
        w, h = t.winfo_reqwidth(), t.winfo_reqheight()
        x = int(min(max(app.x + app.W / 2 - w / 2, l + 10), r - w - 10))
        y = int(max(tp + 10, min(app.y - h - 10, b - h - 10)))
        t.geometry(f"+{x}+{y}")
        t.deiconify()
        tint_titlebar(t)
        t.attributes("-topmost", True)
        t.after(400, lambda: t.winfo_exists() and t.attributes("-topmost", False))
        t.focus_force()
        self._tick()

    def alive(self):
        try:
            return bool(self.top.winfo_exists())
        except tk.TclError:
            return False

    def _tick(self):
        if not self.alive():
            return
        try:
            self.refresh()
        except tk.TclError:
            return
        self.top.after(1000, self._tick)

    def build(self, f):
        pass

    def refresh(self):
        pass

    def close(self):
        self.top.destroy()
