"""Blip's tool windows: Settings (pet, general, office, Basecamp), Timers & alarms,
Buddies & inbox, Basecamp tasks. Built on the flat kit in kit.py."""
import socket
import time
import tkinter as tk
import webbrowser
from tkinter import ttk

from . import __version__, kit
from .kit import (ACCENT, ACCENT_SOFT, BG, DANGER, INK, LINE, MUTED, OK, SURFACE, WARN, Avatar, Btn, Card, Chips,
                  Field, SettingRow, Scroller, Toggle, label, muted)
from .net import DEFAULT_GROUP, local_ips
from .sprites import COLORS, ICON, PALETTES, SPECIES, draw_text, text_width
from .timers import EVERYDAY, WEEKDAYS, describe_days, fmt_clock, parse_clock, parse_duration
from .ui import pixel_box

REPEAT = [("once", "Once"), ("weekdays", "Weekdays"), ("everyday", "Every day"), ("weekends", "Weekends")]
REPEAT_DAYS = {"once": [], "weekdays": WEEKDAYS, "everyday": EVERYDAY, "weekends": [5, 6]}


def style(root):
    """Only the few ttk widgets left (combobox) - keep them flat."""
    s = ttk.Style(root)
    try:
        s.theme_use("clam")
    except tk.TclError:
        pass
    s.configure("TCombobox", fieldbackground=SURFACE, background=SURFACE, bordercolor=LINE, arrowcolor=INK,
                lightcolor=SURFACE, darkcolor=SURFACE, padding=4)


def S_(w, v):
    return round(v * kit.scale(w))


# ------------------------------------------------------------------ dialogs
def ask_text(app, title, prompt, parent=None, initial="", placeholder="", ok="OK"):
    """A small modal prompt in Blip's style. Returns the text or None."""
    top = tk.Toplevel(parent or app.root, bg=BG)
    top.title(title)
    top.resizable(False, False)
    top.transient(parent or app.root)
    f = tk.Frame(top, bg=BG)
    f.pack(padx=S_(top, 22), pady=S_(top, 18))
    label(f, prompt, "bold", wraplength=S_(top, 320)).pack(anchor="w")
    fld = Field(f, initial, width=36, placeholder=placeholder)
    fld.pack(fill="x", pady=(S_(top, 10), S_(top, 14)))
    res = {"v": None}

    def done(v):
        res["v"] = v
        top.destroy()
    bar = tk.Frame(f, bg=BG)
    bar.pack(fill="x")
    Btn(bar, ok, lambda: done(fld.get()), kind="primary").pack(side="right")
    Btn(bar, "Cancel", lambda: done(None)).pack(side="right", padx=S_(top, 8))
    fld.entry.bind("<Return>", lambda e: done(fld.get()))
    top.bind("<Escape>", lambda e: done(None))
    kit.tint_titlebar(top)
    top.update_idletasks()
    l, t, r, b = app.work
    w, h = top.winfo_reqwidth(), top.winfo_reqheight()
    x = int(min(max(app.x + app.W / 2 - w / 2, l + 10), r - w - 10))
    y = int(max(t + 10, app.y - h - 10))
    top.geometry(f"+{x}+{y}")
    top.attributes("-topmost", True)
    fld.entry.focus_force()
    fld.entry.select_range(0, "end")
    top.grab_set()
    top.wait_window()
    return res["v"]


def section(parent, text, pady=(14, 6)):
    f = tk.Frame(parent, bg=kit.bg_of(parent))
    f.pack(fill="x", pady=(S_(parent, pady[0]), S_(parent, pady[1])))
    p = kit.px_of(parent, 1)
    cv = tk.Canvas(f, bg=kit.bg_of(parent), highlightthickness=0, height=7 * p + 1)
    cv.pack(fill="x")
    draw_text(cv, 0, 0, text.upper(), p, MUTED)
    return f


def empty_state(parent, title, text, btn=None):
    f = tk.Frame(parent, bg=kit.bg_of(parent))
    f.pack(fill="x", pady=S_(parent, 18))
    label(f, title, "bold").pack()
    muted(f, text, wraplength=S_(parent, 340), justify="center").pack(pady=(S_(parent, 4), 0))
    if btn:
        Btn(f, btn[0], btn[1], kind="soft", small=True).pack(pady=(S_(parent, 10), 0))
    return f


class IconBtn(Btn):
    def __init__(self, parent, icon, command, tip=None, kind="ghost"):
        super().__init__(parent, "", command, kind=kind, icon=icon, small=True)


# ------------------------------------------------------------------ settings
class NavItem(tk.Canvas):
    def __init__(self, parent, icon, text, command):
        S = kit.scale(parent)
        self.w, self.h = round(168 * S), round(36 * S)
        super().__init__(parent, width=self.w, height=self.h, bg=BG, highlightthickness=0, bd=0, cursor="hand2")
        self.icon, self.text, self.command = icon, text, command
        self.sel = self.hov = False
        self.bind("<Enter>", lambda e: self._set(hov=True))
        self.bind("<Leave>", lambda e: self._set(hov=False))
        self.bind("<ButtonRelease-1>", lambda e: command())
        self.draw()

    def _set(self, **kw):
        for k, v in kw.items():
            setattr(self, k, v)
        self.draw()

    def draw(self):
        self.delete("all")
        S = kit.scale(self)
        p = kit.px_of(self, 2)
        if self.sel or self.hov:
            pixel_box(self, 0, 0, self.w, self.h, p, ACCENT_SOFT if self.sel else "#ece9f4")
        if self.sel:
            self.create_rectangle(0, round(9 * S), round(3 * S), self.h - round(9 * S), fill=ACCENT, width=0)
        ip = kit.px_of(self, 2)
        draw_text(self, round(14 * S), (self.h - 7 * ip) / 2, ICON[self.icon], ip, ACCENT if self.sel else MUTED)
        self.create_text(round(42 * S), self.h / 2, text=self.text, anchor="w",
                         font=kit.F["bold"] if self.sel else kit.F["body"], fill=INK)


class SettingsWindow(kit.Window):
    title = "Settings"
    icon = "gear"
    PAGES = [("pet", "paw", "Pet"), ("general", "gear", "General"), ("office", "people", "Office"),
             ("basecamp", "check", "Basecamp")]

    def build(self, f):
        S = self.S
        side = tk.Frame(f, bg=BG)
        side.pack(side="left", fill="y", padx=(0, round(16 * S)))
        self.nav = {}
        for key, icon, text in self.PAGES:
            n = NavItem(side, icon, text, lambda k=key: self.show(k))
            n.pack(pady=(0, round(2 * S)))
            self.nav[key] = n
        tk.Frame(side, bg=BG).pack(fill="y", expand=True)
        muted(side, f"Blip v{__version__}").pack(anchor="w", padx=round(14 * S), pady=(round(20 * S), 0))
        self.content = tk.Frame(f, bg=BG, width=round(480 * S), height=round(560 * S))
        self.content.pack(side="left", fill="both", expand=True)
        self.content.pack_propagate(False)
        self.page = None
        self.accounts = []
        self.net_restart_job = None
        self.show("pet")

    def show(self, key):
        if key == self.page:
            return
        self.page = key
        for k, n in self.nav.items():
            n._set(sel=(k == key))
        for c in self.content.winfo_children():
            c.destroy()
        sc = Scroller(self.content, height=round(560 * self.S), bg=BG)
        sc.pack(fill="both", expand=True)
        getattr(self, "page_" + key)(sc.inner)

    def refresh(self):
        fn = getattr(self, "refresh_" + (self.page or ""), None)
        if fn:
            fn()

    # ---------------------------------------------------------------- pet page
    def page_pet(self, f):
        app, S = self.app, self.S
        st = app.st
        card = Card(f)
        card.pack(fill="x")
        b = card.body
        left = tk.Frame(b, bg=SURFACE)
        left.pack(side="left", padx=(0, round(14 * S)))
        pal = app.palette() if app.palette() in PALETTES else "gold"
        stage = st["stage"] if st["stage"] != "egg" or st["gone"] else "egg"
        if stage == "egg" and st["gone"]:
            stage = "baby"
        self.big = Avatar(left, app.species(), pal, stage, "happy", px=kit.px_of(f, 4), animate=True, bg=SURFACE)
        self.big.pack()
        right = tk.Frame(b, bg=SURFACE)
        right.pack(side="left", fill="both", expand=True)
        self.name_f = Field(right, st["name"], width=18, on_commit=self._rename)
        self.name_f.pack(anchor="w", fill="x")
        self.pet_status = muted(right, "")
        self.pet_status.pack(anchor="w", pady=(round(4 * S), round(6 * S)))
        self.bars = tk.Canvas(right, bg=SURFACE, highlightthickness=0, height=round(5 * 15 * S), width=round(250 * S))
        self.bars.pack(anchor="w")
        acts = tk.Frame(right, bg=SURFACE)
        acts.pack(anchor="w", pady=(round(8 * S), 0))
        for i, (tip, cmd) in enumerate((("Feed", lambda: app.feed("meal")), ("Snack", lambda: app.feed("snack")),
                                        ("Play", app.play), ("Clean", app.clean),
                                        ("Medicine", app.medicine), ("Sleep / wake", app.toggle_sleep))):
            Btn(acts, tip, cmd, kind="soft", small=True, width=round(80 * S)).grid(
                row=i // 3, column=i % 3, padx=(0, round(4 * S)), pady=(0, round(4 * S)))

        section(f, "Look")
        grid = tk.Frame(f, bg=BG)
        grid.pack(fill="x")
        self.tiles = {}
        for i, (key, name) in enumerate(SPECIES.items()):
            t = SpeciesTile(grid, key, name, pal, lambda k=key: self._pick_species(k))
            t.grid(row=i // 4, column=i % 4, padx=(0, round(8 * S)), pady=(0, round(8 * S)), sticky="w")
            self.tiles[key] = t
        section(f, "Colour")
        sw = tk.Frame(f, bg=BG)
        sw.pack(fill="x")
        self.swatches = {}
        for c in COLORS:
            s = Swatch(sw, c, lambda c=c: self._pick_color(c))
            s.pack(side="left", padx=(0, round(6 * S)))
            self.swatches[c] = s
        muted(f, "Auto: changes as your pet grows, and its grown-up colour shows how well it was looked after.",
              wraplength=round(440 * S)).pack(anchor="w", pady=(round(6 * S), 0))
        if st["gone"]:
            Btn(f, "Hatch a new egg", lambda: (app.new_egg(), self._reload()), kind="primary", icon="heart").pack(
                anchor="w", pady=(round(14 * S), 0))
        self._mark_look()
        self.refresh_pet()

    def _reload(self):
        p, self.page = self.page, None
        self.show(p)

    def _rename(self, v):
        v = v.strip()[:16]
        if v and v != self.app.st["name"]:
            self.app.st["name"] = v
            self.app.say(f"I'm {v}!")
            self.app.save()

    def _pick_species(self, k):
        self.app.set_look(species=k)
        self._mark_look()

    def _pick_color(self, c):
        self.app.set_look(color=c)
        self._mark_look()

    def _mark_look(self):
        app = self.app
        pal = app.palette() if app.palette() in PALETTES else "gold"
        for k, t in self.tiles.items():
            t.set(sel=(k == app.species()), palette=pal)
        for c, s in self.swatches.items():
            s.set(c == app.st.get("color", "auto"))
        self.big.set(species=app.species(), palette=pal)

    def refresh_pet(self):
        from .app import STAGE_NAMES, fmt_age
        app, S = self.app, self.S
        st, s = app.st, app.st["stats"]
        self.pet_status.config(text=f"{STAGE_NAMES.get(st['stage'], '')}  ·  age {fmt_age(st['age'])}  ·  "
                                    f"{app.mood_word()}  ·  gen {st['gen']}")
        cv = self.bars
        cv.delete("all")
        rows = (("meal", "Food", s["full"]), ("play", "Fun", s["happy"]), ("moon", "Energy", s["energy"]),
                ("clean", "Clean", s["hygiene"]), ("heart", "Health", s["health"]))
        ip = max(1, round(1 * S))
        for i, (icon, name, v) in enumerate(rows):
            y = i * round(15 * S)
            draw_text(cv, 0, y + round(3 * S), ICON[icon], ip, MUTED)
            cv.create_text(round(12 * S), y + round(7 * S), text=name, anchor="w", font=kit.F["tiny"], fill=MUTED)
            x0, x1 = round(60 * S), round(246 * S)
            col = OK if v > 60 else WARN if v > 30 else DANGER
            cv.create_rectangle(x0, y + round(4 * S), x1, y + round(10 * S), fill="#ece7f5", width=0)
            cv.create_rectangle(x0, y + round(4 * S), x0 + (x1 - x0) * v / 100, y + round(10 * S), fill=col, width=0)

    # ---------------------------------------------------------------- general page
    def page_general(self, f):
        app, cfg, S = self.app, self.app.cfg, self.S
        card = Card(f)
        card.pack(fill="x")
        b = card.body
        r = SettingRow(b, "Your name", "How you appear to colleagues' pets.")
        r.pack(fill="x")
        Field(r.right, cfg["display_name"], width=16, on_commit=self._set_name).pack()
        kit.hline(b)
        self._toggle_row(b, "Sounds", "Chirps for calls, messages, timers and alarms.", cfg["sound"],
                         lambda v: self._cfg(sound=v))
        kit.hline(b)
        self._toggle_row(b, "Start with Windows", "Blip appears when you sign in.", app.startup_enabled(),
                         app.set_startup)
        kit.hline(b)
        self._toggle_row(b, "Wander around", "Let your pet walk along the taskbar.", app.st["wander"], app.set_wander)

        section(f, "Updates")
        c2 = Card(f)
        c2.pack(fill="x")
        b2 = c2.body
        self._toggle_row(b2, "Update automatically", "New versions install themselves quietly in the background.",
                         cfg["auto_update"], lambda v: self._cfg(auto_update=v))
        kit.hline(b2)
        row = tk.Frame(b2, bg=SURFACE)
        row.pack(fill="x")
        label(row, f"You have Blip {__version__}").pack(side="left")
        Btn(row, "Check now", lambda: app.check_updates(manual=True), kind="soft", small=True, icon="update").pack(
            side="right")
        row2 = tk.Frame(b2, bg=SURFACE)
        row2.pack(fill="x", pady=(round(10 * S), 0))
        Btn(row2, "Release notes", lambda: webbrowser.open("https://github.com/safder2000/blip/releases"),
            kind="ghost", small=True, icon="open").pack(side="left")
        Btn(row2, "Open data folder", lambda: webbrowser.open(str(app.data)), kind="ghost", small=True,
            icon="open").pack(side="left", padx=(round(6 * S), 0))

    def _toggle_row(self, parent, title, desc, value, command):
        r = SettingRow(parent, title, desc)
        r.pack(fill="x")
        t = Toggle(r.right, value, command)
        t.pack()
        return t

    def _cfg(self, **kw):
        self.app.cfg.update(kw)
        self.app.cfg.save()

    def _set_name(self, v):
        v = v.strip()[:24]
        if v and v != self.app.cfg["display_name"]:
            self._cfg(display_name=v)
            self.app.net_rescan()

    # ---------------------------------------------------------------- office page
    def page_office(self, f):
        app, cfg, S = self.app, self.app.cfg, self.S
        card = Card(f)
        card.pack(fill="x")
        b = card.body
        top = tk.Frame(b, bg=SURFACE)
        top.pack(fill="x")
        self.net_dot = tk.Canvas(top, width=round(12 * S), height=round(12 * S), bg=SURFACE, highlightthickness=0)
        self.net_dot.pack(side="left", padx=(0, round(8 * S)))
        self.net_label = label(top, "", "bold")
        self.net_label.pack(side="left")
        Btn(top, "Look again", app.net_rescan, kind="soft", small=True, icon="reset").pack(side="right")
        ips = ", ".join(local_ips()) or "no network"
        muted(b, f"This PC: {socket.gethostname()}  ·  {ips}").pack(anchor="w", pady=(round(6 * S), 0))
        muted(b, "Pets on the same office network find each other on their own - nothing to set up.",
              wraplength=round(420 * S)).pack(anchor="w", pady=(round(2 * S), 0))

        section(f, "Who can see you")
        c2 = Card(f)
        c2.pack(fill="x")
        b2 = c2.body
        self._toggle_row(b2, "Office network", "Find pets nearby and take calls and messages.", cfg["network"],
                         lambda v: (self._cfg(network=v), self._restart_net()))
        kit.hline(b2)
        private = cfg["passphrase"] != DEFAULT_GROUP
        self.priv_toggle = self._toggle_row(
            b2, "Private group", "Only pets with the same group password can see each other. "
            "Off: everyone in the office who has Blip.", private, self._private)
        self.priv_box = tk.Frame(b2, bg=SURFACE)
        self.priv_field = Field(self.priv_box, cfg["passphrase"] if private else "", width=26, show="•",
                                placeholder="Group password", on_commit=self._set_group)
        self.priv_field.pack(side="left", pady=(round(10 * S), 0))
        self.priv_shown = False
        Btn(self.priv_box, "", self._toggle_show, kind="ghost", small=True, icon="info").pack(
            side="left", padx=(round(6 * S), 0), pady=(round(10 * S), 0))
        if private:
            self.priv_box.pack(fill="x")

        section(f, "Other floors (optional)")
        c3 = Card(f)
        c3.pack(fill="x")
        b3 = c3.body
        muted(b3, "Only if colleagues on another floor or Wi-Fi don't show up. Adding one of their PCs is enough - "
                  "pets introduce each other.", wraplength=round(420 * S)).pack(anchor="w")
        r = SettingRow(b3, "Their PC address", "One or more IPs, comma separated.")
        r.pack(fill="x", pady=(round(10 * S), 0))
        Field(r.right, cfg["manual_peers"], width=18, placeholder="192.168.1.20",
              on_commit=lambda v: (self._cfg(manual_peers=v.strip()), self._restart_net())).pack()
        kit.hline(b3)
        r = SettingRow(b3, "Relay", "host:port of a Blip relay, if you run one.")
        r.pack(fill="x")
        Field(r.right, cfg["relay"], width=18, placeholder="server:47620",
              on_commit=lambda v: (self._cfg(relay=v.strip()), self._restart_net())).pack()
        self.refresh_office()

    def _private(self, on):
        if on:
            self.priv_box.pack(fill="x")
            self.priv_field.entry.focus_set()
        else:
            self.priv_box.pack_forget()
            if self.app.cfg["passphrase"] != DEFAULT_GROUP:
                self._cfg(passphrase=DEFAULT_GROUP)
                self._restart_net()

    def _set_group(self, v):
        v = v.strip()
        if v and v != self.app.cfg["passphrase"]:
            self._cfg(passphrase=v)
            self._restart_net()

    def _toggle_show(self):
        self.priv_shown = not self.priv_shown
        self.priv_field.set_show("" if self.priv_shown else "•")

    def _restart_net(self):
        if self.net_restart_job:
            self.top.after_cancel(self.net_restart_job)
        self.net_restart_job = self.top.after(600, self.app.restart_net)

    def refresh_office(self):
        app = self.app
        txt = app.net_status()
        n = len(app.net.online()) if app.net else 0
        col = OK if (app.net and n) else WARN if app.net else DANGER
        self.net_label.config(text=txt)
        self.net_dot.delete("all")
        S = self.S
        pixel_box(self.net_dot, 0, 0, round(12 * S), round(12 * S), max(1, round(2 * S)), col)

    # ---------------------------------------------------------------- basecamp page
    def page_basecamp(self, f):
        cfg, S = self.app.cfg, self.S
        card = Card(f)
        card.pack(fill="x")
        b = card.body
        self.bc_title = label(b, "", "h2")
        self.bc_title.pack(anchor="w")
        self.bc_sub = muted(b, "", wraplength=round(420 * S))
        self.bc_sub.pack(anchor="w", pady=(round(4 * S), round(12 * S)))
        self.bc_btns = tk.Frame(b, bg=SURFACE)
        self.bc_btns.pack(anchor="w")
        self.bc_state = None

        section(f, "Reminders")
        c2 = Card(f)
        c2.pack(fill="x")
        b2 = c2.body
        self._toggle_row(b2, "Remind me", "Cards for to-dos that are overdue or due today, checked every 15 minutes.",
                         cfg["bc_enabled"], lambda v: self._cfg(bc_enabled=v))
        kit.hline(b2)
        r = SettingRow(b2, "Daily summary", "Your whole Basecamp day, once each morning.")
        r.pack(fill="x")
        Field(r.right, cfg["bc_morning"], width=8, placeholder="9:30", on_commit=self._set_morning).pack()

        section(f, "Advanced")
        c3 = Card(f)
        c3.pack(fill="x")
        b3 = c3.body
        muted(b3, "Use a personal access token instead of signing in, or your own Launchpad integration.",
              wraplength=round(420 * S)).pack(anchor="w")
        r = SettingRow(b3, "Access token")
        r.pack(fill="x", pady=(round(8 * S), 0))
        self.tok = Field(r.right, "", width=18, show="•", placeholder="Paste token")
        self.tok.pack(side="left")
        Btn(r.right, "Use", self._use_token, kind="soft", small=True).pack(side="left", padx=(round(6 * S), 0))
        kit.hline(b3)
        r = SettingRow(b3, "Client ID")
        r.pack(fill="x")
        Field(r.right, cfg["bc_client_id"], width=22, on_commit=lambda v: self._cfg(bc_client_id=v.strip())).pack()
        r = SettingRow(b3, "Client secret")
        r.pack(fill="x", pady=(round(6 * S), 0))
        Field(r.right, cfg["bc_client_secret"], width=22, show="•",
              on_commit=lambda v: self._cfg(bc_client_secret=v.strip())).pack()
        self.refresh_basecamp()

    def _set_morning(self, v):
        v = v.strip()
        if not v or parse_clock(v):
            self._cfg(bc_morning=v or "9:30")

    def _use_token(self):
        tok = self.tok.get().strip()
        if tok:
            self.app.bc.connect_token(tok, self._connected)

    def _connected(self, ok, msg, accts):
        if accts:
            self.accounts = accts
        self.bc_state = None
        if self.alive() and self.page == "basecamp":
            self.refresh_basecamp()
        self.app.cards.show("Basecamp", msg, [("OK", lambda: None)], kind="task", icon="task", timeout=20)

    def refresh_basecamp(self):
        app, S = self.app, self.S
        bc = app.bc
        over = sum(1 for i in bc.items if i["scope"] == "overdue")
        today = len(bc.items) - over
        state = (bc.connected, app.cfg.get("bc_account_name"), over, today, bc.last_error, len(self.accounts))
        if state == self.bc_state:
            return
        self.bc_state = state
        for c in self.bc_btns.winfo_children():
            c.destroy()
        if bc.connected:
            self.bc_title.config(text=f"Connected to {app.cfg.get('bc_account_name') or 'Basecamp'}")
            sub = f"{over} overdue  ·  {today} due today" if bc.last_fetch else "Fetching your to-dos…"
            if bc.last_error:
                sub = bc.last_error
            self.bc_sub.config(text=sub)
            Btn(self.bc_btns, "Open my tasks", lambda: app.open_window("tasks"), kind="primary", small=True,
                icon="task").pack(side="left")
            Btn(self.bc_btns, "Disconnect", self._disconnect, kind="danger", small=True).pack(
                side="left", padx=(round(6 * S), 0))
            if len(self.accounts) > 1:
                acc = Chips(self.bc_btns, [(a["href"], a["name"]) for a in self.accounts], app.cfg.get("bc_account"),
                            lambda href: app.bc.set_account(href, next(a["name"] for a in self.accounts
                                                                        if a["href"] == href)))
                acc.pack(side="left", padx=(round(10 * S), 0))
        else:
            self.bc_title.config(text="Connect Basecamp")
            have = all(bc.client_creds())
            self.bc_sub.config(text="Blip will remind you about to-dos that are due today or overdue, "
                                    "and you can tick them off from here." if have else
                                    "This build has no Basecamp integration. Use an access token below.")
            b = Btn(self.bc_btns, "Sign in with Basecamp", lambda: app.bc.connect_oauth(self._connected),
                    kind="primary", icon="open")
            b.set_enabled(have)
            b.pack(side="left")

    def _disconnect(self):
        self.app.bc.disconnect()
        self.bc_state = None
        self.refresh_basecamp()


class SpeciesTile(tk.Canvas):
    def __init__(self, parent, key, name, palette, command):
        S = kit.scale(parent)
        self.w, self.h = round(104 * S), round(96 * S)
        super().__init__(parent, width=self.w, height=self.h, bg=BG, highlightthickness=0, bd=0, cursor="hand2")
        self.key, self.name, self.palette = key, name, palette
        self.sel = self.hov = False
        self.bind("<Enter>", lambda e: self.set(hov=True))
        self.bind("<Leave>", lambda e: self.set(hov=False))
        self.bind("<ButtonRelease-1>", lambda e: command())
        self.draw()

    def set(self, **kw):
        for k, v in kw.items():
            setattr(self, k, v)
        self.draw()

    def draw(self):
        from .sprites import build_pet
        self.delete("all")
        S = kit.scale(self)
        p = kit.px_of(self, 2)
        border = ACCENT if self.sel else ("#cfc6e2" if self.hov else LINE)
        kit.framed_box(self, 0, 0, self.w, self.h, p, border, ACCENT_SOFT if self.sel else SURFACE)
        if self.sel:
            kit.framed_box(self, 1, 1, self.w - 1, self.h - 1, p, border, ACCENT_SOFT)
        g = build_pet("adult", self.palette, "happy" if (self.sel or self.hov) else "normal", 1, species=self.key)
        xs = [x for x, _ in g]
        ys = [y for _, y in g]
        sp = max(2, round(2.2 * S))
        w, h = (max(xs) - min(xs) + 1) * sp, (max(ys) - min(ys) + 1) * sp
        ox, oy = (self.w - w) / 2, round(8 * S) + (round(62 * S) - h)
        for (x, y), c in g.items():
            X, Y = ox + (x - min(xs)) * sp, oy + (y - min(ys)) * sp
            self.create_rectangle(X, Y, X + sp, Y + sp, fill=c, width=0)
        self.create_text(self.w / 2, self.h - round(13 * S), text=self.name,
                         font=kit.F["bold"] if self.sel else kit.F["small"], fill=INK)


class Swatch(tk.Canvas):
    def __init__(self, parent, color, command):
        S = kit.scale(parent)
        self.s = round(30 * S)
        super().__init__(parent, width=self.s, height=self.s, bg=BG, highlightthickness=0, bd=0, cursor="hand2")
        self.color = color
        self.sel = False
        self.bind("<ButtonRelease-1>", lambda e: command())
        self.draw()

    def set(self, sel):
        self.sel = sel
        self.draw()

    def draw(self):
        self.delete("all")
        p = kit.px_of(self, 2)
        s = self.s
        kit.framed_box(self, 0, 0, s, s, p, ACCENT if self.sel else LINE, SURFACE)
        m = p * 2 + (1 if self.sel else 0)
        if self.color == "auto":
            cols = ["#9be8c8", "#8fd3f0", "#ffd166", "#c3a6e8"]
            q = (s - 2 * m) / 2
            for i, c in enumerate(cols):
                x0, y0 = m + (i % 2) * q, m + (i // 2) * q
                self.create_rectangle(x0, y0, x0 + q, y0 + q, fill=c, width=0)
        else:
            pixel_box(self, m, m, s - m, s - m, p, PALETTES[self.color][0])
            self.create_rectangle(m, s - m - (s - 2 * m) / 3, s - m, s - m, fill=PALETTES[self.color][1], width=0)


# ------------------------------------------------------------------ timers
class TimersWindow(kit.Window):
    title = "Timers & alarms"
    icon = "hourglass"
    subtitle = "Countdowns show on a little sign above your pet."

    def build(self, f):
        app, S = self.app, self.S
        c = Card(f)
        c.pack(fill="x")
        b = c.body
        label(b, "Timer", "bold").pack(anchor="w")
        row = tk.Frame(b, bg=SURFACE)
        row.pack(fill="x", pady=(round(8 * S), 0))
        for m in (1, 5, 10, 15, 25, 30, 45, 60):
            Btn(row, f"{m}m", lambda m=m: app.start_timer(m * 60, self.tlabel.get().strip()), kind="soft",
                small=True).pack(side="left", padx=(0, round(4 * S)))
        row2 = tk.Frame(b, bg=SURFACE)
        row2.pack(fill="x", pady=(round(10 * S), 0))
        self.tdur = Field(row2, "", width=9, placeholder="25 / 1:30")
        self.tdur.pack(side="left")
        self.tlabel = Field(row2, "", width=16, placeholder="Label (optional)")
        self.tlabel.pack(side="left", padx=round(6 * S))
        Btn(row2, "Start", self.start_custom, kind="primary", small=True, icon="playtri").pack(side="left")
        Btn(row2, "Stopwatch", lambda: app.start_stopwatch(self.tlabel.get().strip()), small=True,
            icon="stopwatch").pack(side="left", padx=(round(6 * S), 0))
        self.tdur.entry.bind("<Return>", lambda e: self.start_custom())

        c2 = Card(f)
        c2.pack(fill="x", pady=(round(10 * S), 0))
        b2 = c2.body
        label(b2, "Alarm", "bold").pack(anchor="w")
        row3 = tk.Frame(b2, bg=SURFACE)
        row3.pack(fill="x", pady=(round(8 * S), 0))
        self.at = Field(row3, time.strftime("%H:%M", time.localtime(time.time() + 3600)), width=7)
        self.at.pack(side="left")
        self.alabel = Field(row3, "", width=16, placeholder="Label (optional)")
        self.alabel.pack(side="left", padx=round(6 * S))
        Btn(row3, "Add alarm", self.add_alarm, kind="primary", small=True, icon="bell").pack(side="left")
        self.rep = Chips(b2, REPEAT, "once")
        self.rep.pack(anchor="w", pady=(round(8 * S), 0))
        self.err = tk.Label(f, text="", font=kit.F["small"], fg=DANGER, bg=BG)
        self.err.pack(anchor="w")

        section(f, "Running", (6, 6))
        self.list = Scroller(f, height=round(230 * S), bg=BG)
        self.list.pack(fill="both", expand=True)
        self.sig = None
        self.rows = {}

    def start_custom(self):
        secs = parse_duration(self.tdur.get())
        if not secs:
            self.err.config(text="Try 25, 1:30, 90s or 2h 15m.")
            return
        self.err.config(text="")
        self.app.start_timer(secs, self.tlabel.get().strip())

    def add_alarm(self):
        hm = parse_clock(self.at.get())
        if not hm:
            self.err.config(text="Alarm time like 9:30, 17:45 or 5pm.")
            return
        self.err.config(text="")
        self.app.add_alarm(hm[0], hm[1], REPEAT_DAYS[self.rep.get()], self.alabel.get().strip())

    def act(self, iid, what):
        tm = self.app.timers
        {"pause": tm.toggle_pause, "reset": tm.reset, "delete": tm.remove}[what](iid)
        self.sig = None
        self.refresh()
        self.app.draw()

    def refresh(self):
        tm = self.app.timers
        now = time.time()
        order = {"timer": 0, "stopwatch": 1, "alarm": 2}
        items = sorted(tm.items, key=lambda i: (order[i["kind"]], i.get("end") or i.get("hh", 0) * 60 + i.get("mm", 0)))
        sig = [(i["id"], i.get("left") is None, i.get("running"), i.get("enabled"), bool(i.get("snooze")))
               for i in items]
        if sig != self.sig:
            self.sig = sig
            self.list.clear()
            self.rows = {}
            if not items:
                empty_state(self.list.inner, "Nothing running", "Start a timer or add an alarm above.")
            for it in items:
                self.rows[it["id"]] = self._row(it)
        for it in items:
            r = self.rows.get(it["id"])
            if r:
                self._paint(r, it, now)

    def _row(self, it):
        S = self.S
        card = Card(self.list.inner, pad=10)
        card.pack(fill="x", pady=(0, round(6 * S)))
        b = card.body
        cv = tk.Canvas(b, bg=SURFACE, highlightthickness=0, width=round(150 * S), height=round(22 * S))
        cv.pack(side="left")
        mid = tk.Frame(b, bg=SURFACE)
        mid.pack(side="left", fill="x", expand=True, padx=(round(8 * S), 0))
        name = it.get("label") or {"timer": "Timer", "stopwatch": "Stopwatch", "alarm": "Alarm"}[it["kind"]]
        label(mid, name, "bold").pack(anchor="w")
        sub = muted(mid, "")
        sub.pack(anchor="w")
        bar = tk.Canvas(mid, bg=SURFACE, highlightthickness=0, height=round(4 * S), width=round(120 * S))
        if it["kind"] == "timer":
            bar.pack(anchor="w", pady=(round(3 * S), 0))
        btns = tk.Frame(b, bg=SURFACE)
        btns.pack(side="right")
        paused = (it["kind"] == "timer" and it["left"] is not None) or \
                 (it["kind"] == "stopwatch" and not it["running"]) or (it["kind"] == "alarm" and not it["enabled"])
        IconBtn(btns, "playtri" if paused else "pause", lambda: self.act(it["id"], "pause")).pack(side="left")
        if it["kind"] != "alarm":
            IconBtn(btns, "reset", lambda: self.act(it["id"], "reset")).pack(side="left")
        IconBtn(btns, "trash", lambda: self.act(it["id"], "delete")).pack(side="left")
        return {"cv": cv, "sub": sub, "bar": bar}

    def _paint(self, r, it, now):
        tm, S = self.app.timers, self.S
        cv = r["cv"]
        cv.delete("all")
        p = kit.px_of(cv, 2)
        k = it["kind"]
        if k == "timer":
            t, icon = fmt_clock(tm.remaining(it, now)), "hourglass"
            sub = f"of {fmt_clock(it['dur'])}" + ("  ·  paused" if it["left"] is not None else "")
            frac = tm.remaining(it, now) / max(1, it["dur"])
            bar = r["bar"]
            bar.delete("all")
            w = round(120 * S)
            bar.create_rectangle(0, 0, w, round(4 * S), fill="#ece7f5", width=0)
            bar.create_rectangle(0, 0, w * frac, round(4 * S), fill=ACCENT, width=0)
        elif k == "stopwatch":
            t, icon = fmt_clock(tm.elapsed(it, now), False), "stopwatch"
            sub = "running" if it["running"] else "paused"
        else:
            t, icon = f"{it['hh']:02d}:{it['mm']:02d}", "bell"
            sub = describe_days(it["days"]) if it["enabled"] else "off"
            if it.get("snooze"):
                sub += f"  ·  snoozed {fmt_clock(it['snooze'] - now)}"
        x = draw_text(cv, 0, round(4 * S), ICON[icon], p, ACCENT) + 4 * p
        draw_text(cv, x, round(4 * S), t, p, INK)
        r["sub"].config(text=sub)


# ------------------------------------------------------------------ buddies
class BuddiesWindow(kit.Window):
    title = "Buddies"
    icon = "people"
    subtitle = "Pets on your office network show up here by themselves."

    def build(self, f):
        app, S = self.app, self.S
        c = Card(f)
        c.pack(fill="x")
        b = c.body
        r = SettingRow(b, "You appear as")
        r.pack(fill="x")
        Field(r.right, app.cfg["display_name"], width=16, on_commit=self._set_name).pack()
        kit.hline(b)
        r = SettingRow(b, "Do not disturb", "Calls are declined with “busy” and nothing rings.")
        r.pack(fill="x")
        self.dnd = Toggle(r.right, app.cfg["dnd"], app.set_dnd)
        self.dnd.pack()

        section(f, "Nearby")
        self.net_lbl = muted(f, "")
        self.net_lbl.pack(anchor="w", pady=(0, round(6 * S)))
        self.people = Scroller(f, height=round(150 * S), bg=BG)
        self.people.pack(fill="x")

        section(f, "Inbox")
        self.inbox = Scroller(f, height=round(180 * S), bg=BG)
        self.inbox.pack(fill="x")
        Btn(f, "Clear inbox", self.clear_inbox, kind="ghost", small=True, icon="trash").pack(
            anchor="e", pady=(round(6 * S), 0))
        self.psig = self.isig = None

    def _set_name(self, v):
        v = v.strip()[:24]
        if v and v != self.app.cfg["display_name"]:
            self.app.cfg["display_name"] = v
            self.app.cfg.save()
            self.app.net_rescan()

    def clear_inbox(self):
        self.app.inbox.clear()
        self.app.save_inbox()
        self.isig = None
        self.refresh()

    def refresh(self):
        app, S = self.app, self.S
        if self.dnd.get() != bool(app.cfg["dnd"]):
            self.dnd.set(app.cfg["dnd"])
        self.net_lbl.config(text=app.net_status())
        peers = app.net.online() if app.net else []
        psig = [(p["id"], p.get("name"), p.get("status"), p.get("species"), p.get("palette"), p.get("stage"))
                for p in peers]
        if psig != self.psig:
            self.psig = psig
            self.people.clear()
            if not peers:
                empty_state(self.people.inner, "No one nearby yet",
                            "Colleagues appear here automatically once they install Blip on the same network.",
                            ("Look again", app.net_rescan))
            for p in peers:
                self._person(p)
        inbox = list(reversed(app.inbox[-50:]))
        isig = [(e["ts"], e["text"]) for e in inbox]
        if isig != self.isig:
            self.isig = isig
            self.inbox.clear()
            if not inbox:
                muted(self.inbox.inner, "No messages yet.").pack(anchor="w", pady=round(6 * S))
            for e in inbox:
                self._msg(e)

    def _person(self, p):
        app, S = self.app, self.S
        card = Card(self.people.inner, pad=8)
        card.pack(fill="x", pady=(0, round(6 * S)))
        b = card.body
        pal = p.get("palette") if p.get("palette") in PALETTES else "gold"
        stage = p.get("stage") if p.get("stage") in ("baby", "child", "adult", "egg") else "adult"
        Avatar(b, p.get("species") or "blip", pal, stage, "normal", px=max(1, round(1.5 * S)), bg=SURFACE).pack(
            side="left")
        mid = tk.Frame(b, bg=SURFACE)
        mid.pack(side="left", fill="x", expand=True, padx=(round(10 * S), 0))
        label(mid, p.get("name", "?"), "bold").pack(anchor="w")
        busy = p.get("status") == "busy"
        st = tk.Frame(mid, bg=SURFACE)
        st.pack(anchor="w")
        dot = tk.Canvas(st, width=round(8 * S), height=round(8 * S), bg=SURFACE, highlightthickness=0)
        pixel_box(dot, 0, 0, round(8 * S), round(8 * S), 1, WARN if busy else OK)
        dot.pack(side="left", padx=(0, round(5 * S)))
        muted(st, ("Do not disturb" if busy else "Available") + (f"  ·  {p['ip']}" if p.get("ip") else "")).pack(
            side="left")
        btns = tk.Frame(b, bg=SURFACE)
        btns.pack(side="right")
        Btn(btns, "Call", lambda: app.call(p["id"]), kind="primary", small=True, icon="phone").pack(side="left")
        Btn(btns, "Message", lambda: app.compose(p["id"], parent=self.top), small=True, icon="mail").pack(
            side="left", padx=(round(6 * S), 0))

    def _msg(self, e):
        app, S = self.app, self.S
        row = tk.Frame(self.inbox.inner, bg=BG)
        row.pack(fill="x", pady=(0, round(4 * S)))
        mine = e["dir"] == "out"
        bubble = tk.Frame(row, bg=ACCENT_SOFT if mine else SURFACE, padx=round(10 * S), pady=round(6 * S),
                          highlightthickness=1, highlightbackground=ACCENT_SOFT if mine else LINE)
        bubble.pack(side="right" if mine else "left", anchor="e" if mine else "w")
        who = "You → " + e.get("name", "?") if mine else e.get("name", "?")
        top = tk.Frame(bubble, bg=bubble["bg"])
        top.pack(fill="x")
        tk.Label(top, text=who, font=kit.F["tiny"], fg=MUTED, bg=bubble["bg"]).pack(side="left")
        tk.Label(top, text=time.strftime("  %H:%M", time.localtime(e["ts"])), font=kit.F["tiny"], fg=MUTED,
                 bg=bubble["bg"]).pack(side="left")
        tk.Label(bubble, text=e["text"], font=kit.F["body"], fg=INK, bg=bubble["bg"], wraplength=round(330 * S),
                 justify="left", anchor="w").pack(anchor="w")
        if not mine and e.get("peer"):
            Btn(row, "Reply", lambda: app.compose(e["peer"], parent=self.top, name=e.get("name")), kind="ghost",
                small=True).pack(side="left", padx=(round(4 * S), 0))


# ------------------------------------------------------------------ basecamp tasks
class TasksWindow(kit.Window):
    title = "Basecamp"
    icon = "task"
    subtitle = "Your to-dos that are overdue or due today."

    def build(self, f):
        S = self.S
        bar = tk.Frame(f, bg=BG)
        bar.pack(fill="x")
        self.info = muted(bar, "")
        self.info.pack(side="left")
        Btn(bar, "Refresh", lambda: self.app.bc.refresh(), kind="ghost", small=True, icon="reset").pack(side="right")
        self.list = Scroller(f, height=round(380 * S), bg=BG)
        self.list.pack(fill="both", expand=True, pady=(round(8 * S), 0))
        self.list.cv.config(width=round(500 * S))
        self.sig = None

    def refresh(self):
        app = self.app
        bc = app.bc
        if bc.connected:
            when = time.strftime("%H:%M", time.localtime(bc.last_fetch)) if bc.last_fetch else "…"
            self.info.config(text=f"{app.cfg.get('bc_account_name', '')}  ·  updated {when}"
                                  + (f"  ·  {bc.last_error}" if bc.last_error else ""))
        else:
            self.info.config(text="Not connected")
        items = sorted(bc.items, key=lambda i: (i["scope"] != "overdue", i.get("due_on") or ""))
        sig = (bc.connected, bool(bc.last_fetch), [(i["id"], i["scope"]) for i in items])
        if sig == self.sig:
            return
        self.sig = sig
        self.list.clear()
        if not bc.connected:
            empty_state(self.list.inner, "Basecamp isn't connected",
                        "Sign in once and Blip reminds you about what's due.",
                        ("Connect Basecamp", lambda: app.open_window("settings", "basecamp")))
            return
        if not items and bc.last_fetch:
            empty_state(self.list.inner, "All clear!", "Nothing overdue or due today.")
            return
        for it in items:
            self._row(it)

    def _row(self, it):
        app, S = self.app, self.S
        card = Card(self.list.inner, pad=10)
        card.pack(fill="x", pady=(0, round(6 * S)))
        b = card.body
        over = it["scope"] == "overdue"
        badge = tk.Canvas(b, bg=SURFACE, highlightthickness=0, width=round(64 * S), height=round(20 * S))
        p = max(1, round(1 * S))
        txt = "OVERDUE" if over else "TODAY"
        col, fill = (DANGER, "#fdecee") if over else ("#b7791f", "#fdf3df")
        pixel_box(badge, 0, 0, round(64 * S), round(20 * S), kit.px_of(b, 2), fill)
        draw_text(badge, (round(64 * S) - text_width(txt, p)) / 2, (round(20 * S) - 7 * p) / 2, txt, p, col)
        badge.pack(side="left", anchor="n", pady=(round(2 * S), 0))
        mid = tk.Frame(b, bg=SURFACE)
        mid.pack(side="left", fill="x", expand=True, padx=(round(10 * S), round(6 * S)))
        label(mid, it["content"], "body", wraplength=round(280 * S)).pack(anchor="w")
        extra = it.get("project") or ""
        if it.get("due_on"):
            extra += ("  ·  " if extra else "") + f"due {it['due_on']}"
        muted(mid, extra).pack(anchor="w")
        btns = tk.Frame(b, bg=SURFACE)
        btns.pack(side="right", anchor="n")
        if it.get("type") in ("todo", "Todo", None):
            Btn(btns, "Done", lambda: (app.complete_task(it), setattr(self, "sig", None)), kind="soft", small=True,
                icon="check").pack(side="left")
        if it.get("url"):
            IconBtn(btns, "open", lambda: webbrowser.open(it["url"])).pack(side="left", padx=(round(4 * S), 0))
