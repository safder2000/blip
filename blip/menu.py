"""The right-click menu: a pixel-styled pop-up with the pet in the header, one-tap care
buttons, sections, toggles and fly-out submenus."""
import time
import tkinter as tk

from . import kit
from .sprites import BIG, ICON, PALETTES, build_egg, build_pet, draw_bitmap, draw_text, text_width
from .ui import KEY, make_overlay, pixel_box, place



CALL_GREEN, CALL_HOVER = "#2fb865", "#28a359"
MSG_BLUE, MSG_HOVER = "#3d8bfd", "#2f78e6"
NET_BLUE = "#4ba3ff"


_measure = {}


def _mix(a, b, t):
    """Blend colour a into b by t (0..1)."""
    ca = [int(a[i:i + 2], 16) for i in (1, 3, 5)]
    cb = [int(b[i:i + 2], 16) for i in (1, 3, 5)]
    return "#" + "".join(f"{round(x * t + y * (1 - t)):02x}" for x, y in zip(ca, cb))


def tkfont_measure(text, f=None):
    import tkinter.font as tkfont
    f = f or kit.F["small"]
    if f not in _measure:
        _measure[f] = tkfont.Font(font=f)
    return _measure[f].measure(text)


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
            elif it.get("gap"):
                self.rows.append((yy, yy + round(4 * S), it))
                yy += round(4 * S)
            elif it.get("section"):
                h = round(30 * S) if it.get("badge") else round(24 * S)
                self.rows.append((yy, yy + h, it))
                yy += h
            elif it.get("person"):
                self.rows.append((yy, yy + round(34 * S), it))
                yy += round(34 * S)
            elif it.get("tools"):
                self.rows.append((yy, yy + round(40 * S), it))
                yy += round(40 * S)
            else:
                self.rows.append((yy, yy + rh, it))
                yy += rh
        H = self.H = yy + round(6 * S) + p

        # ---- frame
        pixel_box(cv, 2 * p, 2 * p, W + 2 * p, H + 2 * p, p, kit.SHADOW, "bg")
        pixel_box(cv, 0, 0, W, H, p, kit.FRAME, "bg")
        pixel_box(cv, p, p, W - p, H - p, p, kit.SURFACE, "bg")
        cv.config(height=H + 2 * p)
        self.draw_content()

        # ---- place
        l, t, r, b = app.work_at(x if anchor_right is None else anchor_right[1], y)
        if anchor_right is not None:     # submenu: to the right of the parent, else to its left
            px0, px1 = anchor_right
            x = px1 - p if px1 + W < r else px0 - W - p
        x = min(max(x, l + 4), r - W - 2 * p - 4)
        y = min(max(y, t + 4), b - H - 2 * p - 4)
        place(self.win, f"{W + 2 * p}x{H + 2 * p}+{int(x)}+{int(y)}")
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
            cv.create_text(tx, y0 + round(14 * S), text=h["name"], font=kit.F["bold"], fill=kit.INK, anchor="w",
                           tags=("fg", "head"))
            cv.create_text(tx, y0 + round(31 * S), text=h["status"], font=kit.F["tiny"], fill=kit.MUTED, anchor="w",
                           tags=("fg", "head"))
            bx = tx
            gap = round(5 * S)
            bw = round((x1 - tx - round(10 * S) - 2 * gap) / 3)
            by = y0 + round(44 * S)
            for icon, val, col in h["bars"]:
                cv.create_rectangle(bx, by, bx + bw, by + round(5 * S), fill=kit.TRACK, width=0, tags="fg")
                cv.create_rectangle(bx, by, bx + bw * max(0, min(100, val)) / 100, by + round(5 * S),
                                    fill=col, width=0, tags="fg")
                bx += bw + gap
        for ax0, ay0, ax1, ay1, (icon, lab, cmd, en, short) in self.act_rects:
            hov = self.hl == ("act", lab) and en
            fill = kit.ACCENT_SOFT if hov else kit.TILE
            pixel_box(cv, ax0, ay0, ax1, ay1, p, kit.ACCENT if hov else kit.LINE, "fg")
            pixel_box(cv, ax0 + 1, ay0 + 1, ax1 - 1, ay1 - 1, p, fill, "fg")
            ip = self.ip
            gw = text_width(ICON[icon], ip)
            col = (kit.ACCENT if hov else kit.INK) if en else kit.OFF
            draw_text(cv, (ax0 + ax1 - gw) / 2, ay0 + round(7 * S), ICON[icon], ip, col, "fg")
            cv.create_text((ax0 + ax1) / 2, ay1 - round(10 * S), text=short, font=kit.F["tiny"],
                           fill=(kit.INK if en else kit.OFF), tags="fg")
        if self.actions:
            cv.create_rectangle(p + pad, self.sep_after_actions, W - p - pad, self.sep_after_actions + 1,
                                fill=kit.LINE, width=0, tags="fg")
        # people (and their extra rows) sit together on a soft panel so the office stands out
        run = []
        for y0, y1, it in self.rows + [(0, 0, {})]:
            if it.get("person") or it.get("panel"):
                run.append((y0, y1))
            elif run:
                pixel_box(cv, p + round(6 * S), run[0][0], W - p - round(6 * S), run[-1][1], p, kit.TILE, "fg")
                run = []
        for y0, y1, it in self.rows:
            if it.get("gap"):
                continue
            if it.get("sep"):
                ym = (y0 + y1) // 2
                cv.create_rectangle(p + pad, ym, W - p - pad, ym + 1, fill=kit.LINE, width=0, tags="fg")
                continue
            if it.get("section") and it.get("badge"):
                self.draw_section(y0, y1, it)
                continue
            if it.get("section"):
                draw_text(cv, p + pad + round(2 * S), y1 - round(9 * S) - 5 * self.ip // 2 - 2,
                          it["section"].upper(), max(1, round(1 * S)), kit.MUTED, "fg")
                continue
            if it.get("person"):
                self.draw_person(y0, y1, it)
                continue
            if it.get("tools"):
                self.draw_tools(y0, y1, it)
                continue
            en = it.get("enabled", True)
            hov = en and self.hl == ("row", id(it))
            if hov or (self.child and self.child.owner is it):
                inset = round(8 * S) if it.get("panel") else round(4 * S)
                pixel_box(cv, p + inset, y0 + 1, W - p - inset, y1 - 1, p,
                          kit.ACCENT_SOFT if it.get("panel") else kit.HOVER, "fg")
            fg = kit.INK if en else kit.OFF
            ix = p + pad + round(2 * S)
            if it.get("icon"):
                draw_text(cv, ix, (y0 + y1 - 7 * self.ip) / 2, ICON[it["icon"]], self.ip,
                          (kit.ACCENT if hov else kit.MUTED) if en else kit.OFF, "fg")
            cv.create_text(ix + round(24 * S), (y0 + y1) / 2, text=it["label"], font=kit.F["body"], fill=fg,
                           anchor="w", tags="fg")
            rx = W - p - pad - round(2 * S)
            if it.get("submenu"):
                gw = text_width(ICON["chevron"], self.ip)
                draw_text(cv, rx - gw, (y0 + y1 - 7 * self.ip) / 2, ICON["chevron"], self.ip, kit.MUTED, "fg")
            elif it.get("check") is not None:
                tw, th = round(30 * S), round(16 * S)
                on = it["check"]
                tx0, ty0 = rx - tw, (y0 + y1 - th) / 2
                pixel_box(cv, tx0, ty0, rx, ty0 + th, max(1, p // 2 + 1), kit.ACCENT if on else kit.TRACK, "fg")
                k = th - round(6 * S)
                kx = rx - k - round(3 * S) if on else tx0 + round(3 * S)
                pixel_box(cv, kx, ty0 + round(3 * S), kx + k, ty0 + round(3 * S) + k, 1, kit.KNOB, "fg")
            elif it.get("hint"):
                cv.create_text(rx, (y0 + y1) / 2, text=it["hint"], font=kit.F["small"], fill=kit.MUTED, anchor="e",
                               tags="fg")

    def person_buttons(self, y0, y1, it):
        """Rects of the round icon buttons at the right of a person row."""
        S, p = self.S, self.p
        bw = round(28 * S)
        gap = round(4 * S)
        rx = self.W - p - round(10 * S)
        out = []
        for i, b in enumerate(reversed(it["buttons"])):
            x1 = rx - i * (bw + gap)
            out.append((x1 - bw, y0 + round(4 * S), x1, y1 - round(4 * S), len(it["buttons"]) - 1 - i))
        return out

    def link_rect(self, y0, y1, it):
        S, p = self.S, self.p
        x1 = self.W - p - round(12 * S)
        return x1 - round(44 * S), y0 + round(4 * S), x1, y1 - round(2 * S)

    def draw_section(self, y0, y1, it):
        """Section title with a live badge ('● 3 nearby') and a link on the right."""
        cv, S, p = self.cv, self.S, self.p
        pad = round(10 * S)
        ip = max(1, round(1 * S))
        ty = (y0 + y1) / 2 - 3.5 * ip + round(2 * S)
        tw = draw_text(cv, p + pad + round(2 * S), ty, it["section"].upper(), ip, kit.MUTED, "fg")
        live = it.get("live")
        col = kit.OK if live else kit.MUTED
        bx = p + pad + round(2 * S) + tw + round(10 * S)
        cy = (y0 + y1) / 2 + round(2 * S)
        d = round(7 * S)
        if live:                                       # soft halo around a green dot
            cv.create_oval(bx - round(2 * S), cy - d / 2 - round(2 * S), bx + d + round(2 * S),
                           cy + d / 2 + round(2 * S), fill=_mix(col, kit.SURFACE, 0.3), width=0, tags="fg")
        cv.create_oval(bx, cy - d / 2, bx + d, cy + d / 2, fill=col, width=0, tags="fg")
        cv.create_text(bx + d + round(6 * S), cy, text=it["badge"], font=kit.F["bold"] if live else kit.F["small"],
                       fill=col if live else kit.MUTED, anchor="w", tags="fg")
        if it.get("link"):
            x0, ly0, x1, ly1 = self.link_rect(y0, y1, it)
            hov = self.hl == ("link", id(it))
            if hov:
                pixel_box(cv, x0, ly0, x1, ly1, p, kit.HOVER, "fg")
            lab = it["link"][0]
            gw = text_width(ICON["chevron"], ip)
            lw = tkfont_measure(lab)
            sx = (x0 + x1 - lw - gw - round(4 * S)) / 2
            cv.create_text(sx, (ly0 + ly1) / 2, text=lab, font=kit.F["small"], fill=kit.ACCENT, anchor="w",
                           tags="fg")
            draw_text(cv, sx + lw + round(4 * S), (ly0 + ly1) / 2 - 3.5 * ip, ICON["chevron"], ip, kit.ACCENT, "fg")

    def tool_rects(self, y0, y1, it):
        S, p = self.S, self.p
        pad = round(10 * S)
        gap = round(4 * S)
        n = len(it["tools"])
        x0 = p + pad
        w = (self.W - 2 * p - 2 * pad - (n - 1) * gap) / n
        return [(x0 + i * (w + gap), y0 + round(3 * S), x0 + i * (w + gap) + w, y1 - round(3 * S), i)
                for i in range(n)]

    def draw_tools(self, y0, y1, it):
        cv, S, p = self.cv, self.S, self.p
        for x0, ty0, x1, ty1, i in self.tool_rects(y0, y1, it):
            tool = it["tools"][i]
            hov = self.hl == ("tool", id(it), i)
            open_ = self.child is not None and self.child.owner is tool
            pixel_box(cv, x0, ty0, x1, ty1, p, kit.ACCENT_SOFT if (hov or open_) else kit.TILE, "fg")
            col = kit.ACCENT if (hov or open_) else kit.INK
            gw = text_width(ICON[tool["icon"]], self.ip)
            lab = tool["label"]
            font = kit.F["small"]
            fw = tkfont_measure(lab, font)
            if gw + round(5 * S) + fw > (x1 - x0) - round(8 * S):       # long label: smaller type
                font = kit.F["tiny"]
                fw = tkfont_measure(lab, font)
            total = gw + round(4 * S) + fw
            sx = (x0 + x1 - total) / 2
            draw_text(cv, sx, (ty0 + ty1 - 7 * self.ip) / 2, ICON[tool["icon"]], self.ip, col, "fg")
            cv.create_text(sx + gw + round(4 * S), (ty0 + ty1) / 2, text=lab, font=font, fill=col,
                           anchor="w", tags="fg")
            if tool.get("badge"):
                cv.create_text(x1 - round(4 * S), ty0 + round(7 * S), text=tool["badge"], font=kit.F["tiny"],
                               fill=kit.ACCENT, anchor="e", tags="fg")

    def draw_person(self, y0, y1, it):
        cv, S, p = self.cv, self.S, self.p
        W = self.W
        pad = round(10 * S)
        row_hov = self.hl == ("row", id(it))
        if row_hov or (self.hl and self.hl[:2] == ("btn", id(it))):
            pixel_box(cv, p + round(8 * S), y0 + 1, W - p - round(8 * S), y1 - 1, p, kit.ACCENT_SOFT, "fg")
        pr = it["person"]
        g = build_pet(pr.get("stage") or "adult", pr.get("palette") if pr.get("palette") in PALETTES else "gold",
                      "happy" if row_hov else "normal", 1, species=pr.get("species") or "blip")
        xs = [x for x, _ in g]
        ys = [y for _, y in g]
        sp = max(1, round(0.9 * S))
        top = min(ys)
        ax = p + pad
        h = (y1 - y0) - round(6 * S)
        rows_fit = max(1, h // sp)
        for (x, y), c in g.items():
            if y - top < rows_fit:
                X, Y = ax + (x - min(xs)) * sp, y0 + round(3 * S) + (y - top) * sp
                cv.create_rectangle(X, Y, X + sp, Y + sp, fill=c, width=0, tags="fg")
        tx = ax + round(34 * S)
        busy = pr.get("status") == "busy"
        ty = (y0 + y1) / 2 - (round(6 * S) if busy else 0)
        t = cv.create_text(tx, ty, text=it["label"], font=kit.F["body"], fill=kit.INK, anchor="w", tags="fg")
        if pr.get("via") == "server":       # reached over the internet, not the office network
            bp = max(2, round(1.6 * S))
            gx = cv.bbox(t)[2] + round(7 * S)
            draw_bitmap(cv, gx, ty - 3.5 * bp, BIG["globe"], bp, NET_BLUE, "fg")
        if busy:
            cv.create_text(tx, (y0 + y1) / 2 + round(8 * S), text="busy", font=kit.F["tiny"], fill=kit.WARN,
                           anchor="w", tags="fg")
        for bx0, by0, bx1, by1, i in self.person_buttons(y0, y1, it):
            icon, cmd, en, _tip = it["buttons"][i]
            hov = self.hl == ("btn", id(it), i) and en
            if icon in ("phone", "chat"):   # phone-app style: green call, blue message, white glyph
                base, over = (CALL_GREEN, CALL_HOVER) if icon == "phone" else (MSG_BLUE, MSG_HOVER)
                fill = (over if hov else base) if en else kit.TILE
                col = "#ffffff" if en else kit.OFF
            else:
                fill = kit.ACCENT if hov else kit.TILE
                col = kit.ON_ACCENT if hov else (kit.ACCENT if en else kit.OFF)
            pixel_box(cv, bx0, by0, bx1, by1, p, fill, "fg")
            if icon in BIG:
                rows = BIG[icon]
                bp = max(1, round(1.5 * S))
                bw, bh = len(rows[0]) * bp, len(rows) * bp
                draw_bitmap(cv, (bx0 + bx1 - bw) / 2, (by0 + by1 - bh) / 2, rows, bp, col, "fg")
            else:
                gw = text_width(ICON[icon], self.ip)
                draw_text(cv, (bx0 + bx1 - gw) / 2, (by0 + by1 - 7 * self.ip) / 2, ICON[icon], self.ip, col,
                          "fg")

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
            if y0 <= y < y1 and it.get("section") and it.get("link"):
                x0, ly0, x1, ly1 = self.link_rect(y0, y1, it)
                if x0 <= x <= x1:
                    return ("link", id(it)), it
                return None, None
            if y0 <= y < y1 and not it.get("sep") and not it.get("section") and not it.get("gap"):
                if it.get("tools"):
                    for tx0, ty0, tx1, ty1, i in self.tool_rects(y0, y1, it):
                        if tx0 <= x <= tx1:
                            return ("tool", id(it), i), it
                    return None, None
                if it.get("person"):
                    for bx0, by0, bx1, by1, i in self.person_buttons(y0, y1, it):
                        if bx0 <= x <= bx1:
                            return ("btn", id(it), i), it
                return ("row", id(it)), it
        return None, None

    def on_motion(self, e):
        key, obj = self.hit(e.x, e.y)
        if key != self.hl:
            self.hl = key
            self.hover_since = time.monotonic()
            self.cv.config(cursor="hand2" if key and key[0] in ("act", "head", "btn", "tool", "link") else "")
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

    def open_sub(self, it, y0=None):
        if self.child:
            self.child.close_self()
        if y0 is None:
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
        elif key[0] == "btn":
            icon, cmd, en, _tip = obj["buttons"][key[2]]
            if not en:
                return
        elif key[0] == "link":
            cmd = obj["link"][1]
        elif key[0] == "tool":
            tool = obj["tools"][key[2]]
            if tool.get("submenu"):
                y0 = next(r[0] for r in self.rows if r[2] is obj)
                if self.child and self.child.owner is tool:
                    self.child.close_self()
                    self.child = None
                    self.draw_content()
                else:
                    self.open_sub(tool, y0)
                return
            cmd = tool.get("command")
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
