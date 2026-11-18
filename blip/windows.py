"""Regular windows: timers & alarms, office buddies, Basecamp tasks, settings, pet status."""
import time
import tkinter as tk
import webbrowser
from tkinter import messagebox, simpledialog, ttk

from . import __version__
from .sprites import COL, ICON, draw_text
from .timers import EVERYDAY, WEEKDAYS, parse_clock, parse_duration

BG = "#fbf8ff"
INK = COL["out"]
REPEAT = {"Once": [], "Weekdays": WEEKDAYS, "Every day": EVERYDAY, "Weekends": [5, 6]}


def style(root):
    s = ttk.Style(root)
    try:
        s.theme_use("clam")
    except tk.TclError:
        pass
    s.configure(".", background=BG, foreground=INK, font=("Segoe UI", 9))
    s.configure("TButton", padding=(10, 4), background="#ffffff", bordercolor="#d9d0ea")
    s.map("TButton", background=[("active", "#efe8ff")])
    s.configure("Accent.TButton", background="#6b7fd7", foreground="#ffffff")
    s.map("Accent.TButton", background=[("active", "#5a6cc4")])
    s.configure("TEntry", fieldbackground="#ffffff", bordercolor="#d9d0ea")
    s.configure("TNotebook", background=BG)
    s.configure("TNotebook.Tab", padding=(10, 4))
    s.configure("Muted.TLabel", foreground="#6b6480")
    s.configure("TCheckbutton", background=BG)
    s.configure("TLabelframe", background=BG)
    s.configure("TLabelframe.Label", background=BG, foreground=INK)


class Window:
    """Base: a single-instance tool window with a pixel-font header."""
    title = "Blip"
    icon = "heart"

    def __init__(self, app):
        self.app = app
        S = app.S
        t = self.top = tk.Toplevel(app.root)
        t.title(f"Blip - {self.title}")
        t.configure(bg=BG)
        t.attributes("-topmost", True)
        t.protocol("WM_DELETE_WINDOW", self.close)
        p = max(2, round(2 * S))
        head = tk.Canvas(t, height=7 * p + 12 * p, bg="#6b7fd7", highlightthickness=0)
        head.pack(fill="x")
        x = 8 * p
        x += draw_text(head, x, 6 * p, ICON[self.icon], p, "#ffffff") + 4 * p
        draw_text(head, x, 6 * p, self.title.upper(), p, "#ffffff")
        self.body = ttk.Frame(t, padding=round(12 * S))
        self.body.pack(fill="both", expand=True)
        self.build(self.body)
        t.update_idletasks()
        l, tp, r, b = app.work
        w, h = t.winfo_reqwidth(), t.winfo_reqheight()
        x = int(min(max(app.x + app.W / 2 - w / 2, l + 10), r - w - 10))
        y = int(max(tp + 10, min(app.y - h - 10, b - h - 10)))
        t.geometry(f"+{x}+{y}")
        t.after(400, lambda: t.attributes("-topmost", False))
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
        self.refresh()
        self.top.after(1000, self._tick)

    def build(self, f):
        pass

    def refresh(self):
        pass

    def close(self):
        self.top.destroy()


# ------------------------------------------------------------------ timers
class TimersWindow(Window):
    title = "Timers & alarms"
    icon = "hourglass"

    def build(self, f):
        app = self.app
        q = ttk.LabelFrame(f, text=" Quick timer ", padding=8)
        q.pack(fill="x")
        row = ttk.Frame(q)
        row.pack(fill="x")
        for m in (1, 5, 10, 15, 25, 30, 45, 60):
            ttk.Button(row, text=f"{m}m", width=4,
                       command=lambda m=m: app.start_timer(m * 60, self.label.get().strip())).pack(side="left", padx=1)
        row2 = ttk.Frame(q)
        row2.pack(fill="x", pady=(6, 0))
        ttk.Label(row2, text="Time").pack(side="left")
        self.custom = ttk.Entry(row2, width=9)
        self.custom.pack(side="left", padx=(4, 8))
        self.custom.insert(0, "25")
        ttk.Label(row2, text="Label").pack(side="left")
        self.label = ttk.Entry(row2, width=16)
        self.label.pack(side="left", padx=4)
        ttk.Button(row2, text="Start", style="Accent.TButton", command=self.start_custom).pack(side="left", padx=2)
        ttk.Button(row2, text="Stopwatch", command=lambda: app.start_stopwatch(self.label.get().strip())).pack(
            side="left", padx=2)
        ttk.Label(q, text="Time: 25 = 25 min, 1:30 = 1 h 30, 90s, 2h 15m", style="Muted.TLabel").pack(anchor="w")

        a = ttk.LabelFrame(f, text=" New alarm ", padding=8)
        a.pack(fill="x", pady=(10, 0))
        ttk.Label(a, text="At").pack(side="left")
        self.at = ttk.Entry(a, width=8)
        self.at.pack(side="left", padx=(4, 8))
        self.at.insert(0, time.strftime("%H:%M", time.localtime(time.time() + 3600)))
        self.rep = ttk.Combobox(a, values=list(REPEAT), width=10, state="readonly")
        self.rep.set("Once")
        self.rep.pack(side="left")
        ttk.Label(a, text="Label").pack(side="left", padx=(8, 0))
        self.alabel = ttk.Entry(a, width=14)
        self.alabel.pack(side="left", padx=6)
        ttk.Button(a, text="Add alarm", style="Accent.TButton", command=self.add_alarm).pack(side="left")

        lst = ttk.LabelFrame(f, text=" Running ", padding=8)
        lst.pack(fill="both", expand=True, pady=(10, 0))
        self.lb = tk.Listbox(lst, height=8, width=52, activestyle="none", bd=0, highlightthickness=1,
                             highlightbackground="#d9d0ea", selectbackground="#6b7fd7", font=("Consolas", 10))
        self.lb.pack(fill="both", expand=True)
        self.lb.bind("<Double-Button-1>", lambda e: self.act("pause"))
        b = ttk.Frame(lst)
        b.pack(fill="x", pady=(6, 0))
        ttk.Button(b, text="Pause / resume / on-off", command=lambda: self.act("pause")).pack(side="left")
        ttk.Button(b, text="Reset", command=lambda: self.act("reset")).pack(side="left", padx=4)
        ttk.Button(b, text="Delete", command=lambda: self.act("delete")).pack(side="left")
        self.ids = []

    def start_custom(self):
        secs = parse_duration(self.custom.get())
        if not secs:
            messagebox.showerror("Blip", "Try 25, 1:30, 90s or 2h 15m.", parent=self.top)
            return
        self.app.start_timer(secs, self.label.get().strip())

    def add_alarm(self):
        hm = parse_clock(self.at.get())
        if not hm:
            messagebox.showerror("Blip", "Time like 9:30, 17:45 or 5pm.", parent=self.top)
            return
        self.app.add_alarm(hm[0], hm[1], REPEAT[self.rep.get()], self.alabel.get().strip())

    def act(self, what):
        sel = self.lb.curselection()
        if not sel or sel[0] >= len(self.ids):
            return
        iid = self.ids[sel[0]]
        tm = self.app.timers
        if what == "pause":
            tm.toggle_pause(iid)
        elif what == "reset":
            tm.reset(iid)
        else:
            tm.remove(iid)
        self.refresh()
        self.app.draw()

    def refresh(self):
        tm = self.app.timers
        now = time.time()
        order = {"timer": 0, "stopwatch": 1, "alarm": 2}
        items = sorted(tm.items, key=lambda i: (order[i["kind"]], i.get("end") or i.get("hh", 0) * 60 + i.get("mm", 0)))
        rows = [tm.describe(i, now) + (f"  - {i['label']}" if i.get("label") else "") for i in items]
        sel = self.lb.curselection()
        if rows != list(self.lb.get(0, "end")):
            self.lb.delete(0, "end")
            for r in rows:
                self.lb.insert("end", r)
            if sel and sel[0] < len(rows):
                self.lb.selection_set(sel[0])
        self.ids = [i["id"] for i in items]


# ------------------------------------------------------------------ buddies
class BuddiesWindow(Window):
    title = "Office buddies"
    icon = "phone"

    def build(self, f):
        app = self.app
        top = ttk.Frame(f)
        top.pack(fill="x")
        ttk.Label(top, text="Your name").pack(side="left")
        self.name = ttk.Entry(top, width=18)
        self.name.insert(0, app.cfg["display_name"])
        self.name.pack(side="left", padx=6)
        self.name.bind("<FocusOut>", lambda e: self.save_name())
        self.name.bind("<Return>", lambda e: self.save_name())
        self.dnd = tk.BooleanVar(value=app.cfg["dnd"])
        ttk.Checkbutton(top, text="Do not disturb", variable=self.dnd,
                        command=lambda: app.set_dnd(self.dnd.get())).pack(side="left", padx=8)

        pf = ttk.LabelFrame(f, text=" Pets nearby ", padding=8)
        pf.pack(fill="both", expand=True, pady=(10, 0))
        self.lb = tk.Listbox(pf, height=8, width=48, activestyle="none", bd=0, highlightthickness=1,
                             highlightbackground="#d9d0ea", selectbackground="#6b7fd7", font=("Segoe UI", 10))
        self.lb.pack(fill="both", expand=True)
        self.lb.bind("<Double-Button-1>", lambda e: self.call())
        b = ttk.Frame(pf)
        b.pack(fill="x", pady=(6, 0))
        ttk.Button(b, text="Call", style="Accent.TButton", command=self.call).pack(side="left")
        ttk.Button(b, text="Message", command=self.message).pack(side="left", padx=4)
        ttk.Button(b, text="Look again", command=app.net_rescan).pack(side="left")
        self.netlabel = ttk.Label(pf, style="Muted.TLabel")
        self.netlabel.pack(anchor="w", pady=(6, 0))

        inf = ttk.LabelFrame(f, text=" Inbox ", padding=8)
        inf.pack(fill="both", expand=True, pady=(10, 0))
        self.inbox = tk.Listbox(inf, height=7, width=48, activestyle="none", bd=0, highlightthickness=1,
                                highlightbackground="#d9d0ea", selectbackground="#6b7fd7", font=("Segoe UI", 9))
        self.inbox.pack(fill="both", expand=True)
        b2 = ttk.Frame(inf)
        b2.pack(fill="x", pady=(6, 0))
        ttk.Button(b2, text="Reply", command=self.reply).pack(side="left")
        ttk.Button(b2, text="Clear", command=self.clear_inbox).pack(side="left", padx=4)
        self.peer_ids = []
        self.inbox_rows = []

    def save_name(self):
        n = self.name.get().strip()[:24]
        if n and n != self.app.cfg["display_name"]:
            self.app.cfg["display_name"] = n
            self.app.cfg.save()
            self.app.net_rescan()

    def selected_peer(self):
        sel = self.lb.curselection()
        if not sel or sel[0] >= len(self.peer_ids):
            messagebox.showinfo("Blip", "Pick someone in the list first.", parent=self.top)
            return None
        return self.peer_ids[sel[0]]

    def call(self):
        pid = self.selected_peer()
        if pid:
            self.app.call(pid)

    def message(self):
        pid = self.selected_peer()
        if pid:
            self.app.compose(pid, parent=self.top)

    def reply(self):
        sel = self.inbox.curselection()
        if not sel or sel[0] >= len(self.inbox_rows):
            return
        e = self.inbox_rows[sel[0]]
        if e.get("peer"):
            self.app.compose(e["peer"], parent=self.top, name=e.get("name"))

    def clear_inbox(self):
        self.app.inbox.clear()
        self.app.save_inbox()
        self.refresh()

    def refresh(self):
        app = self.app
        peers = app.net.online() if app.net else []
        rows = []
        for p in peers:
            st = {"busy": "  (do not disturb)"}.get(p.get("status"), "")
            where = p.get("ip") or "via relay"
            rows.append(f"{p.get('name', '?')}{st}    {where}")
        if rows != list(self.lb.get(0, "end")):
            sel = self.lb.curselection()
            self.lb.delete(0, "end")
            for r in rows:
                self.lb.insert("end", r)
            if sel and sel[0] < len(rows):
                self.lb.selection_set(sel[0])
        self.peer_ids = [p["id"] for p in peers]
        self.netlabel.config(text=app.net_status())
        inbox = list(reversed(app.inbox[-60:]))
        irows = []
        for e in inbox:
            arrow = "←" if e["dir"] == "in" else "→"
            irows.append(f"{time.strftime('%H:%M', time.localtime(e['ts']))} {arrow} {e.get('name', '?')}: {e['text']}")
        if irows != list(self.inbox.get(0, "end")):
            self.inbox.delete(0, "end")
            for r in irows:
                self.inbox.insert("end", r)
        self.inbox_rows = inbox


# ------------------------------------------------------------------ basecamp
class TasksWindow(Window):
    title = "Basecamp tasks"
    icon = "task"

    def build(self, f):
        self.lb = tk.Listbox(f, height=12, width=60, activestyle="none", bd=0, highlightthickness=1,
                             highlightbackground="#d9d0ea", selectbackground="#6b7fd7", font=("Segoe UI", 10))
        self.lb.pack(fill="both", expand=True)
        self.lb.bind("<Double-Button-1>", lambda e: self.open())
        b = ttk.Frame(f)
        b.pack(fill="x", pady=(8, 0))
        ttk.Button(b, text="Mark done", style="Accent.TButton", command=self.done).pack(side="left")
        ttk.Button(b, text="Open in Basecamp", command=self.open).pack(side="left", padx=4)
        ttk.Button(b, text="Refresh", command=lambda: self.app.bc.refresh()).pack(side="left")
        self.info = ttk.Label(f, style="Muted.TLabel")
        self.info.pack(anchor="w", pady=(6, 0))
        self.items = []

    def sel(self):
        s = self.lb.curselection()
        return self.items[s[0]] if s and s[0] < len(self.items) else None

    def open(self):
        it = self.sel()
        if it and it.get("url"):
            webbrowser.open(it["url"])

    def done(self):
        it = self.sel()
        if it:
            self.app.complete_task(it)

    def refresh(self):
        bc = self.app.bc
        if not bc.connected:
            self.info.config(text="Not connected. Right-click the pet > Settings > Basecamp.")
            items = []
        else:
            items = sorted(bc.items, key=lambda i: (i["scope"] != "overdue", i.get("due_on") or ""))
            when = time.strftime("%H:%M", time.localtime(bc.last_fetch)) if bc.last_fetch else "never"
            err = f"  -  {bc.last_error}" if bc.last_error else ""
            self.info.config(text=f"{bc.cfg.get('bc_account_name', '')}  -  updated {when}{err}")
        rows = [("OVERDUE  " if i["scope"] == "overdue" else "today    ") + i["content"] +
                (f"   [{i['project']}]" if i.get("project") else "") for i in items]
        if rows != list(self.lb.get(0, "end")):
            self.lb.delete(0, "end")
            for r in rows:
                self.lb.insert("end", r)
        if not items and bc.connected and not bc.last_error and bc.last_fetch:
            self.lb.delete(0, "end")
            self.lb.insert("end", "Nothing due today. Nice!")
        self.items = items


# ------------------------------------------------------------------ settings
class SettingsWindow(Window):
    title = "Settings"
    icon = "task"

    def build(self, f):
        app, cfg = self.app, self.app.cfg
        nb = ttk.Notebook(f)
        nb.pack(fill="both", expand=True)

        # general
        g = ttk.Frame(nb, padding=10)
        nb.add(g, text="General")
        self.v_sound = tk.BooleanVar(value=cfg["sound"])
        self.v_upd = tk.BooleanVar(value=cfg["auto_update"])
        self.v_start = tk.BooleanVar(value=app.startup_enabled())
        ttk.Checkbutton(g, text="Sounds", variable=self.v_sound).pack(anchor="w")
        ttk.Checkbutton(g, text="Start Blip when I sign in to Windows", variable=self.v_start).pack(anchor="w")
        ttk.Checkbutton(g, text="Install updates automatically", variable=self.v_upd).pack(anchor="w")
        uf = ttk.Frame(g)
        uf.pack(fill="x", pady=(10, 0))
        ttk.Label(uf, text=f"Version {__version__}").pack(side="left")
        ttk.Button(uf, text="Check for updates", command=lambda: app.check_updates(manual=True)).pack(
            side="left", padx=8)

        # network
        n = ttk.Frame(nb, padding=10)
        nb.add(n, text="Office network")
        self.v_net = tk.BooleanVar(value=cfg["network"])
        ttk.Checkbutton(n, text="Find other pets on this network", variable=self.v_net).grid(
            row=0, column=0, columnspan=2, sticky="w")
        self.e_pass = self._field(n, 1, "Office passphrase", cfg["passphrase"], show="•")
        self.e_relay = self._field(n, 2, "Relay (host:port)", cfg["relay"])
        self.e_peers = self._field(n, 3, "Extra PCs (IPs)", cfg["manual_peers"])
        ttk.Label(n, style="Muted.TLabel", wraplength=340, justify="left",
                  text="Everyone who should see each other needs the same passphrase. "
                       "Relay and extra IPs are only needed if pets on other floors or Wi-Fi don't show up.").grid(
            row=4, column=0, columnspan=2, sticky="w", pady=(8, 0))

        # basecamp
        b = ttk.Frame(nb, padding=10)
        nb.add(b, text="Basecamp")
        self.bc_status = ttk.Label(b, wraplength=360, justify="left")
        self.bc_status.grid(row=0, column=0, columnspan=3, sticky="w")
        ttk.Label(b, text="Personal access token").grid(row=1, column=0, sticky="w", pady=(10, 0))
        self.e_tok = ttk.Entry(b, width=34, show="•")
        self.e_tok.grid(row=1, column=1, sticky="we", pady=(10, 0), padx=4)
        ttk.Button(b, text="Connect", command=self.bc_token).grid(row=1, column=2, pady=(10, 0))
        ttk.Separator(b).grid(row=2, column=0, columnspan=3, sticky="we", pady=10)
        ttk.Label(b, text="or sign in with an integration:", style="Muted.TLabel").grid(row=3, column=0, columnspan=3,
                                                                                          sticky="w")
        self.e_cid = self._field(b, 4, "Client ID", cfg["bc_client_id"])
        self.e_csec = self._field(b, 5, "Client secret", cfg["bc_client_secret"], show="•")
        ttk.Button(b, text="Sign in with browser", command=self.bc_oauth).grid(row=6, column=1, sticky="w", pady=4)
        ttk.Separator(b).grid(row=7, column=0, columnspan=3, sticky="we", pady=10)
        ttk.Label(b, text="Account").grid(row=8, column=0, sticky="w")
        self.acct = ttk.Combobox(b, state="readonly", width=30)
        self.acct.grid(row=8, column=1, sticky="w", padx=4)
        self.acct.bind("<<ComboboxSelected>>", self.bc_pick)
        self.e_morning = self._field(b, 9, "Daily summary at", cfg["bc_morning"])
        self.v_bc = tk.BooleanVar(value=cfg["bc_enabled"])
        ttk.Checkbutton(b, text="Remind me about due and overdue to-dos", variable=self.v_bc).grid(
            row=10, column=0, columnspan=2, sticky="w", pady=(6, 0))
        ttk.Button(b, text="Disconnect", command=self.bc_disconnect).grid(row=10, column=2)
        self.accounts = []

        bar = ttk.Frame(f)
        bar.pack(fill="x", pady=(10, 0))
        ttk.Button(bar, text="Save", style="Accent.TButton", command=self.save).pack(side="right")
        ttk.Button(bar, text="Cancel", command=self.close).pack(side="right", padx=6)

    def _field(self, parent, row, label, value, show=None):
        ttk.Label(parent, text=label).grid(row=row, column=0, sticky="w", pady=2)
        e = ttk.Entry(parent, width=34, show=show or "")
        e.insert(0, value or "")
        e.grid(row=row, column=1, sticky="we", pady=2, padx=4)
        return e

    def refresh(self):
        bc = self.app.bc
        if bc.connected:
            txt = f"Connected to {self.app.cfg.get('bc_account_name') or 'Basecamp'}."
        else:
            txt = "Not connected."
        if bc.last_error:
            txt += f"  ({bc.last_error})"
        self.bc_status.config(text=txt)

    def _connected(self, ok, msg, accts):
        if not self.alive():
            return
        self.refresh()
        if accts:
            self.accounts = accts
            self.acct["values"] = [a["name"] for a in accts]
            cur = next((a["name"] for a in accts if a["href"] == self.app.cfg.get("bc_account")), "")
            self.acct.set(cur)
        (messagebox.showinfo if ok else messagebox.showerror)("Blip", msg, parent=self.top)

    def bc_token(self):
        tok = self.e_tok.get().strip()
        if not tok:
            messagebox.showinfo("Blip", "Paste a Basecamp personal access token first.", parent=self.top)
            return
        self.app.bc.connect_token(tok, self._connected)

    def bc_oauth(self):
        cfg = self.app.cfg
        cfg["bc_client_id"], cfg["bc_client_secret"] = self.e_cid.get().strip(), self.e_csec.get().strip()
        cfg.save()
        self.app.bc.connect_oauth(self._connected)

    def bc_pick(self, _e=None):
        i = self.acct.current()
        if 0 <= i < len(self.accounts):
            a = self.accounts[i]
            self.app.bc.set_account(a["href"], a["name"])

    def bc_disconnect(self):
        self.app.bc.disconnect()
        self.refresh()

    def save(self):
        app, cfg = self.app, self.app.cfg
        morning = self.e_morning.get().strip()
        if morning and not parse_clock(morning):
            messagebox.showerror("Blip", "Daily summary time like 9:30.", parent=self.top)
            return
        net_before = (cfg["network"], cfg["passphrase"], cfg["relay"], cfg["manual_peers"])
        cfg.update(sound=self.v_sound.get(), auto_update=self.v_upd.get(), network=self.v_net.get(),
                   passphrase=self.e_pass.get().strip() or "blip-office", relay=self.e_relay.get().strip(),
                   manual_peers=self.e_peers.get().strip(), bc_morning=morning, bc_enabled=self.v_bc.get(),
                   bc_client_id=self.e_cid.get().strip(), bc_client_secret=self.e_csec.get().strip())
        cfg.save()
        app.set_startup(self.v_start.get())
        if net_before != (cfg["network"], cfg["passphrase"], cfg["relay"], cfg["manual_peers"]):
            app.restart_net()
        self.close()


# ------------------------------------------------------------------ pet status
class StatusWindow(Window):
    title = "Pet"
    icon = "heart"
    BARS = (("full", "Food"), ("happy", "Fun"), ("energy", "Energy"), ("hygiene", "Clean"), ("health", "Health"))

    def build(self, f):
        app = self.app
        S = app.S
        self.head = ttk.Label(f, font=("Segoe UI", 12, "bold"))
        self.head.pack(fill="x")
        self.sub = ttk.Label(f, style="Muted.TLabel")
        self.sub.pack(fill="x")
        self.bw = round(240 * S)
        self.row = round(24 * S)
        self.cv = tk.Canvas(f, width=self.bw, height=self.row * len(self.BARS) + 4, bg=BG, highlightthickness=0)
        self.cv.pack(pady=8)
        fr = ttk.Frame(f)
        fr.pack(fill="x")
        for i, (label, cmd) in enumerate((("Meal", lambda: app.feed("meal")), ("Snack", lambda: app.feed("snack")),
                                          ("Play", app.play), ("Clean", app.clean),
                                          ("Medicine", app.medicine), ("Sleep/Wake", app.toggle_sleep))):
            ttk.Button(fr, text=label, command=cmd).grid(row=i // 3, column=i % 3, sticky="ew", padx=2, pady=2)
        for c in range(3):
            fr.grid_columnconfigure(c, weight=1)

    def refresh(self):
        from .app import STAGE_NAMES, fmt_age
        st = self.app.st
        s = st["stats"]
        self.head.config(text=st["name"] + ("  (ghost)" if st["gone"] else ""))
        bits = [STAGE_NAMES.get(st["stage"], st["stage"]), "age " + fmt_age(st["age"]), f"gen {st['gen']}"]
        if st["asleep"]:
            bits.append("sleeping")
        if st["sick"]:
            bits.append("sick!")
        if st["poops"]:
            bits.append(f"{len(st['poops'])} poop" + ("s" if len(st["poops"]) > 1 else ""))
        self.sub.config(text="  ·  ".join(bits))
        cv, S = self.cv, self.app.S
        cv.delete("all")
        lab = round(60 * S)
        for i, (k, name) in enumerate(self.BARS):
            y = i * self.row + 2
            v = s[k]
            col = "#4cc38a" if v > 60 else "#f5a524" if v > 30 else "#e5484d"
            cv.create_text(0, y + self.row / 2 - 2, text=name, anchor="w", font=("Segoe UI", 9), fill=INK)
            x0, x1 = lab, self.bw - 2
            h = round(12 * S)
            yy = y + (self.row - h) / 2 - 2
            cv.create_rectangle(x0, yy, x1, yy + h, fill="#ece6f5", outline="")
            cv.create_rectangle(x0, yy, x0 + (x1 - x0) * v / 100, yy + h, fill=col, outline="")


def ask_text(app, title, prompt, parent=None, initial=""):
    return simpledialog.askstring(title, prompt, parent=parent or app.root, initialvalue=initial)
