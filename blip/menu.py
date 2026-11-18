"""The right-click menu: a pixel-styled pop-up with the pet in the header, one-tap care
buttons, sections, toggles and fly-out submenus."""
import time
import tkinter as tk

from . import kit
from .sprites import ICON, PALETTES, build_egg, build_pet, draw_text, text_width
from .ui import KEY, make_overlay, pixel_box

SURFACE, INK, MUTED, ACCENT, HOVER, LINE = kit.SURFACE, kit.INK, kit.MUTED, kit.ACCENT, kit.HOVER, kit.LINE
OFF = "#b9b3c8"


class PopMenu:
    def __init__(self, app, items, x, y, parent=None, header=None, actions=None, anchor_right=None):
        """items: dicts with label/icon/command/enabled/hint/submenu/check, or {"sep": True},
        or {"section": "TITLE"}. header: dict for the pet card. actions: [(icon, label, cmd, enabled)]."""
        self.app, self.parent, self.child = app, parent, None
        S = self.S = app.S
        p = self.p = max(2, round(2 * S))
        self.ip = max(1, round(2 * S))
        W = self.W = round(252 * S)
        self.items = items
        self.header, self.actions = header, actions or []
        self.rows, self.act_rects = [], []
        self.hl = None
        self.hover_since = 0.0
        self.closed = False
        self.had_focus = False
        self.t0 = time.monotonic()
        self.win = make_overlay(app.root)
        cv = self.cv = tk.Canvas(self.win, width=W + 2 * p, height=10, bg=KEY, highlightthickness=0, bd=0)
        cv.pack()

        # ---- layout
        pad = round(10 * S)
        y0 = p + round(6 * S)
        yy = y0
        if header:
            self.head_rect = (p + pad, yy, W - p - pad, yy + round(58 * S))
            yy += round(62 * S)
        if self.actions:
            self.act_y = yy
            n = len(self.actions)
            gap = round(4 * S)
            bw = (W - 2 * p - 2 * pad - (n - 1) * gap) / n
            bh = round(44 * S)
            for i, a in enumerate(self.actions):
                ax = p + pad + i * (bw + gap)
                self.act_rects.append((ax, yy, ax + bw, yy + bh, a))
            yy += bh + round(8 * S)
            self.sep_after_actions = yy
            yy += round(5 * S)
        rh = round(30 * S)
        for it in items:
            if it.get("sep"):
                self.rows.append((yy, yy + round(9 * S), it))
                yy += round(9 * S)
            elif it.get("section"):
                self.rows.append((yy, yy + round(24 * S), it))
                yy += round(24 * S)
            else:
                self.rows.append((yy, yy + rh, it))
                yy += rh
        H = self.H = yy + round(6 * S) + p

        # ---- frame
        pixel_box(cv, 2 * p, 2 * p, W + 2 * p, H + 2 * p, p, INK, "bg")
        pixel_box(cv, 0, 0, W, H, p, INK, "bg")
        pixel_box(cv, p, p, W - p, H - p, p, SURFACE, "bg")
        cv.config(height=H + 2 * p)
        self.draw_content()

        # ---- place
        l, t, r, b = app.work
        if anchor_right is not None:     # submenu: to the right of the parent, else to its left
            px0, px1 = anchor_right
            x = px1 - p if px1 + W < r else px0 - W - p
        x = min(max(x, l + 4), r - W - 2 * p - 4)
        y = min(max(y, t + 4), b - H - 2 * p - 4)
        self.win.geometry(f"{W + 2 * p}x{H + 2 * p}+{int(x)}+{int(y)}")
        self.x, self.y = x, y

        cv.bind("<Motion>", self.on_motion)
        cv.bind("<Leave>", self.on_leave)
        cv.bind("<ButtonRelease-1>", self.on_click)
        self.win.bind("<Escape>", lambda e: self.root().close())
        if parent is None:
            self.win.focus_force()
            self.win.after(150, self._watch)
        self.win.after(250, self._hover_tick)

    # ------------------------------------------------ drawing
    def root(self):
        m = self
        while m.parent:
            m = m.parent
        return m

    def draw_content(self):
        cv, S, p = self.cv, self.S, self.p
        cv.delete("fg")
        W = self.W
        pad = round(10 * S)
        if self.header:
            h = self.header
            x0, y0, x1, y1 = self.head_rect
            pixel_box(cv, x0, y0, x1, y1, p, kit.ACCENT_SOFT, ("fg", "head"))
            g = build_egg(1, 0.5) if h["stage"] == "egg" else build_pet(
                h["stage"], h["palette"] if h["palette"] in PALETTES else "gold", h["mood"], 1, species=h["species"])
            xs = [x for x, _ in g]
            ys = [y for _, y in g]
            sp = max(1, round(1.6 * S))
            sw, sh = (max(xs) - min(xs) + 1) * sp, (max(ys) - min(ys) + 1) * sp
            ax = x0 + round(8 * S) + (round(44 * S) - sw) / 2
            ay = y0 + (y1 - y0 - sh) / 2
            for (x, y), c in g.items():
                X, Y = ax + (x - min(xs)) * sp, ay + (y - min(ys)) * sp
                cv.create_rectangle(X, Y, X + sp, Y + sp, fill=c, width=0, tags=("fg", "head"))
            tx = x0 + round(60 * S)
            cv.create_text(tx, y0 + round(14 * S), text=h["name"], font=kit.F["bold"], fill=INK, anchor="w",
                           tags=("fg", "head"))
            cv.create_text(tx, y0 + round(31 * S), text=h["status"], font=kit.F["tiny"], fill=MUTED, anchor="w",
                           tags=("fg", "head"))
            bx = tx
            gap = round(5 * S)
            bw = round((x1 - tx - round(10 * S) - 2 * gap) / 3)
            by = y0 + round(44 * S)
            for icon, val, col in h["bars"]:
                cv.create_rectangle(bx, by, bx + bw, by + round(5 * S), fill="#dcd6ea", width=0, tags="fg")
                cv.create_rectangle(bx, by, bx + bw * max(0, min(100, val)) / 100, by + round(5 * S),
                                    fill=col, width=0, tags="fg")
                bx += bw + gap
        for ax0, ay0, ax1, ay1, (icon, lab, cmd, en, short) in self.act_rects:
            hov = self.hl == ("act", lab) and en
            fill = kit.ACCENT_SOFT if hov else "#f7f5fb"
            pixel_box(cv, ax0, ay0, ax1, ay1, p, kit.ACCENT if hov else LINE, "fg")
            pixel_box(cv, ax0 + 1, ay0 + 1, ax1 - 1, ay1 - 1, p, fill, "fg")
            ip = self.ip
            gw = text_width(ICON[icon], ip)
            col = (ACCENT if hov else INK) if en else OFF
            draw_text(cv, (ax0 + ax1 - gw) / 2, ay0 + round(7 * S), ICON[icon], ip, col, "fg")
            cv.create_text((ax0 + ax1) / 2, ay1 - round(10 * S), text=short, font=kit.F["tiny"],
                           fill=(INK if en else OFF), tags="fg")
        if self.actions:
            cv.create_rectangle(p + pad, self.sep_after_actions, W - p - pad, self.sep_after_actions + 1,
                                fill=LINE, width=0, tags="fg")
        for y0, y1, it in self.rows:
            if it.get("sep"):
                ym = (y0 + y1) // 2
                cv.create_rectangle(p + pad, ym, W - p - pad, ym + 1, fill=LINE, width=0, tags="fg")
                continue
            if it.get("section"):
                draw_text(cv, p + pad + round(2 * S), y1 - round(9 * S) - 5 * self.ip // 2 - 2,
                          it["section"].upper(), max(1, round(1 * S)), MUTED, "fg")
                continue
            en = it.get("enabled", True)
            hov = en and self.hl == ("row", id(it))
            if hov or (self.child and self.child.owner is it):
                pixel_box(cv, p + round(4 * S), y0 + 1, W - p - round(4 * S), y1 - 1, p, HOVER, "fg")
            fg = INK if en else OFF
            ix = p + pad + round(2 * S)
            if it.get("icon"):
                draw_text(cv, ix, (y0 + y1 - 7 * self.ip) / 2, ICON[it["icon"]], self.ip,
                          (ACCENT if hov else MUTED) if en else OFF, "fg")
            cv.create_text(ix + round(24 * S), (y0 + y1) / 2, text=it["label"], font=kit.F["body"], fill=fg,
                           anchor="w", tags="fg")
            rx = W - p - pad - round(2 * S)
            if it.get("submenu"):
                gw = text_width(ICON["chevron"], self.ip)
                draw_text(cv, rx - gw, (y0 + y1 - 7 * self.ip) / 2, ICON["chevron"], self.ip, MUTED, "fg")
            elif it.get("check") is not None:
                tw, th = round(30 * S), round(16 * S)
                on = it["check"]
                tx0, ty0 = rx - tw, (y0 + y1 - th) / 2
                pixel_box(cv, tx0, ty0, rx, ty0 + th, max(1, p // 2 + 1), ACCENT if on else "#d8d2e6", "fg")
                k = th - round(6 * S)
                kx = rx - k - round(3 * S) if on else tx0 + round(3 * S)
                pixel_box(cv, kx, ty0 + round(3 * S), kx + k, ty0 + round(3 * S) + k, 1, "#ffffff", "fg")
            elif it.get("hint"):
                cv.create_text(rx, (y0 + y1) / 2, text=it["hint"], font=kit.F["small"], fill=MUTED, anchor="e",
                               tags="fg")

    # ------------------------------------------------ interaction
    def hit(self, x, y):
        for ax0, ay0, ax1, ay1, a in self.act_rects:
            if ax0 <= x <= ax1 and ay0 <= y <= ay1:
                return ("act", a[1]), a
        if self.header:
            x0, y0, x1, y1 = self.head_rect
            if x0 <= x <= x1 and y0 <= y <= y1:
                return ("head", 0), None
        for y0, y1, it in self.rows:
            if y0 <= y < y1 and not it.get("sep") and not it.get("section"):
                return ("row", id(it)), it
        return None, None

    def on_motion(self, e):
        key, obj = self.hit(e.x, e.y)
        if key != self.hl:
            self.hl = key
            self.hover_since = time.monotonic()
            self.cv.config(cursor="hand2" if key and key[0] in ("act", "head") else "")
            self.draw_content()

    def on_leave(self, e):
        if self.hl and not (self.child and self.hl == ("row", id(self.child.owner))):
            self.hl = None
            self.draw_content()

    def _hover_tick(self):
        if self.closed:
            return
        if self.hl and self.hl[0] == "row" and time.monotonic() - self.hover_since > 0.22:
            it = next((r[2] for r in self.rows if id(r[2]) == self.hl[1]), None)
            if it and it.get("submenu") and it.get("enabled", True):
                if not (self.child and self.child.owner is it):
                    self.open_sub(it)
            elif it and self.child:
                self.child.close_self()
                self.child = None
                self.draw_content()
        self.win.after(120, self._hover_tick)

    def open_sub(self, it):
        if self.child:
            self.child.close_self()
        y0 = next(r[0] for r in self.rows if r[2] is it)
        sub = it["submenu"]()
        if not sub:
            sub = [{"label": "(nothing here yet)", "enabled": False}]
        self.child = PopMenu(self.app, sub, 0, self.y + y0 - round(8 * self.S), parent=self,
                             anchor_right=(self.x, self.x + self.W))
        self.child.owner = it
        self.draw_content()

    def on_click(self, e):
        key, obj = self.hit(e.x, e.y)
        if not key:
            return
        if key[0] == "head":
            cmd = self.header.get("command")
        elif key[0] == "act":
            icon, lab, cmd, en, _short = obj
            if not en:
                return
        else:
            it = obj
            if not it.get("enabled", True):
                return
            if it.get("submenu"):
                self.open_sub(it)
                return
            cmd = it.get("command")
        self.root().close()
        if cmd:
            self.app.root.after(10, cmd)

    def _watch(self):
        """Close when the user clicks somewhere else (the app loses focus)."""
        if self.closed:
            return
        try:
            f = self.app.root.focus_get()
        except (KeyError, tk.TclError):
            f = None
        if f is not None:
            self.had_focus = True
        elif self.had_focus or time.monotonic() - self.t0 > 1.5:
            self.close()
            return
        else:
            try:
                self.win.focus_force()
            except tk.TclError:
                pass
        self.win.after(150, self._watch)

    def close_self(self):
        if self.closed:
            return
        self.closed = True
        if self.child:
            self.child.close_self()
        try:
            self.win.destroy()
        except tk.TclError:
            pass

    def close(self):
        r = self.root()
        r.close_self()
        if getattr(r.app, "menu", None) is r:
            r.app.menu = None
