"""Pixel-styled pop-up cards (calls, messages, alarms, tasks) that stack above the pet."""
import tkinter as tk

from . import kit
from .sprites import COL, ICON, draw_text, text_width

from .plat import KEY, make_transparent
INK = COL["out"]
PAPER = "#fffdf7"
ACCENT = {
    "call": "#3fb67a", "msg": "#6b7fd7", "alarm": "#ef5b6b", "timer": "#f0a020",
    "task": "#2f8fd8", "info": "#9a7cc8", "update": "#2fb5a8",
}


def pixel_box(cv, x0, y0, x1, y1, p, fill, tags=()):
    """A rectangle with one-pixel stepped corners."""
    cv.create_rectangle(x0 + p, y0, x1 - p, y1, fill=fill, width=0, tags=tags)
    cv.create_rectangle(x0, y0 + p, x1, y1 - p, fill=fill, width=0, tags=tags)


def make_overlay(root):
    t = tk.Toplevel(root)
    t.overrideredirect(True)
    t.attributes("-topmost", True)
    make_transparent(t)
    return t


class PixelCard:
    def __init__(self, stack, title, body, buttons=(), kind="info", icon=None, key=None,
                 timeout=None, on_close=None):
        """buttons: [(label, callback)] - the first is the highlighted one.
        A callback may return True to keep the card open."""
        self.stack, self.key, self.on_close = stack, key, on_close
        self.buttons = list(buttons)
        self.body_id = None
        app = stack.app
        S = app.S
        p = self.p = max(2, round(2 * S))
        accent = ACCENT.get(kind, kind)
        W = round(300 * S)
        self.win = make_overlay(app.root)
        cv = self.cv = tk.Canvas(self.win, width=W, height=10, bg=KEY, highlightthickness=0, bd=0)
        cv.pack()
        pad = 6 * p
        hh = 7 * p + 8 * p
        # header text
        x = 2 * p + pad
        ty = p + (hh - 7 * p) // 2
        if icon:
            x += draw_text(cv, x, ty, ICON[icon], p, "#ffffff", "fg") + 3 * p
        maxchars = max(4, (W - x - 14 * p - pad) // (6 * p))
        draw_text(cv, x, ty, title.upper()[:maxchars], p, "#ffffff", "fg")
        # close x
        cx0 = W - 2 * p - pad - 5 * p
        draw_text(cv, cx0, ty, "X", p, "#ffffff", ("fg", "close"))
        cv.create_rectangle(cx0 - 2 * p, p, cx0 + 7 * p, p + hh, fill="", outline="", tags="close")
        cv.tag_bind("close", "<Button-1>", lambda e: self.close("x"))
        cv.tag_bind("close", "<Enter>", lambda e: cv.config(cursor="hand2"))
        cv.tag_bind("close", "<Leave>", lambda e: cv.config(cursor=""))
        # body
        y = p + hh + pad
        if body:
            t = self.body_id = cv.create_text(2 * p + pad, y, text=body, anchor="nw", width=W - 4 * p - 2 * pad,
                                              font=kit.F["body"], fill=INK, tags="fg")
            y = cv.bbox(t)[3] + pad
        # buttons, right-aligned, wrapping
        bh = 7 * p + 8 * p
        rows, row, roww = [], [], 0
        avail = W - 4 * p - 2 * pad
        for label, cb in buttons:
            bw = text_width(label.upper(), p) + 12 * p
            if row and roww + bw + 3 * p > avail:
                rows.append(row)
                row, roww = [], 0
            row.append((label, cb, bw))
            roww += bw + 3 * p
        if row:
            rows.append(row)
        first = True
        for row in rows:
            total = sum(b[2] for b in row) + 3 * p * (len(row) - 1)
            bx = W - 2 * p - pad - total
            for label, cb, bw in row:
                tag = f"b{id(cb)}_{bx}"
                fill = accent if first else "#ffffff"
                fg = "#ffffff" if first else INK
                pixel_box(cv, bx, y, bx + bw, y + bh, p, INK, (tag, "fg"))
                pixel_box(cv, bx + p, y + p, bx + bw - p, y + bh - p, p, fill, (tag, "fg", tag + "f"))
                draw_text(cv, bx + 6 * p, y + 4 * p, label.upper(), p, fg, (tag, "fg"))
                cv.tag_bind(tag, "<Button-1>", lambda e, cb=cb: self._press(cb))
                cv.tag_bind(tag, "<Enter>", lambda e, t=tag, f=fill: (cv.itemconfig(t + "f", fill=_shade(f)),
                                                                       cv.config(cursor="hand2")))
                cv.tag_bind(tag, "<Leave>", lambda e, t=tag, f=fill: (cv.itemconfig(t + "f", fill=f),
                                                                       cv.config(cursor="")))
                bx += bw + 3 * p
                first = False
            y += bh + 3 * p
        H = y - 3 * p + pad if rows else y
        H += 2 * p
        # frame: shadow, ink border, paper, accent header
        pixel_box(cv, 2 * p, 2 * p, W, H + 2 * p, p, INK, "bg")
        pixel_box(cv, 0, 0, W - 2 * p, H, p, INK, "bg")
        pixel_box(cv, p, p, W - 3 * p, H - p, p, PAPER, "bg")
        cv.create_rectangle(2 * p, p, W - 4 * p, p + hh, fill=accent, width=0, tags=("bg", "head"))
        cv.create_rectangle(p, 2 * p, W - 3 * p, p + hh, fill=accent, width=0, tags=("bg", "head"))
        cv.create_rectangle(p, p + hh, W - 3 * p, p + hh + p, fill=INK, width=0, tags="bg")
        cv.tag_lower("bg")
        cv.config(height=H + 2 * p)
        self.W, self.H = W, H + 2 * p
        # drag by header
        cv.tag_bind("head", "<ButtonPress-1>", self._grab)
        cv.tag_bind("head", "<B1-Motion>", self._move)
        self.dragged = False
        if timeout:
            self.win.after(int(timeout * 1000), lambda: self.close("timeout"))

    def set_body(self, text):
        if self.body_id:
            try:
                self.cv.itemconfig(self.body_id, text=text)
            except tk.TclError:
                pass

    def _press(self, cb):
        keep = False
        try:
            keep = cb() is True
        finally:
            if not keep:
                self.close("button")

    def _grab(self, e):
        self._off = (e.x_root - self.win.winfo_x(), e.y_root - self.win.winfo_y())

    def _move(self, e):
        self.dragged = True
        self.win.geometry(f"+{e.x_root - self._off[0]}+{e.y_root - self._off[1]}")

    def place(self, x, y):
        self.win.geometry(f"{self.W}x{self.H}+{int(x)}+{int(y)}")

    def close(self, why="code"):
        if self in self.stack.cards:
            self.stack.cards.remove(self)
            try:
                self.win.destroy()
            except tk.TclError:
                pass
            if self.on_close:
                self.on_close(why)
            self.stack.layout()


def _shade(hexcol, f=0.88):
    h = hexcol.lstrip("#")
    r, g, b = (int(h[i:i + 2], 16) for i in (0, 2, 4))
    return "#%02x%02x%02x" % (int(r * f), int(g * f), int(b * f))


class CardStack:
    def __init__(self, app):
        self.app = app
        self.cards = []

    def show(self, *a, **kw):
        key = kw.get("key")
        if key:
            self.close(key)
        c = PixelCard(self, *a, **kw)
        self.cards.append(c)
        self.layout()
        return c

    def get(self, key):
        return next((c for c in self.cards if c.key == key), None)

    def close(self, key, why="code"):
        for c in [c for c in self.cards if c.key == key]:
            c.close(why)

    def layout(self):
        app = self.app
        l, t, r, b = app.work
        cx = (app.ring_target if app.ring_target is not None else app.x) + app.W / 2
        y = app.y + app.bub_h * 0.3
        for c in reversed(self.cards):   # newest nearest the pet
            if c.dragged:
                continue
            y -= c.H + 4
            x = min(max(cx - c.W / 2, l + 8), r - c.W - 8)
            c.place(x, max(t + 8, y))
        for c in self.cards:
            try:
                c.win.lift()
            except tk.TclError:
                pass
