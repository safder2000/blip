"""Blip - the pet window and everything it does."""
import math
import queue
import random
import sys
import time
import traceback
import tkinter as tk
import uuid
from pathlib import Path

from . import __version__
from .basecamp import Basecamp
from .config import Config, data_dir, frozen, read_json, write_json
from .net import Net
from .sound import Sound
from .sprites import build_ball
from .sprites import (BANG, COL, COLORS, FOOD, GH, GW, HEART, ICON, NOTE, PALETTES, POOP, SPARK, SPECIES, ZED,
                      build_egg, build_pet, draw_text, text_width)
from .timers import Timers, describe_days, fmt_clock, parse_clock, parse_duration
from .notes import Notes, describe_when, parse_when
from .ui import KEY, CardStack, pixel_box
from . import kit
from . import updater as upd
from . import windows as W
from .menu import PopMenu
from .tray import Tray
from . import voice
from . import plat
from .plat import work_area_at


def startup_args():
    if frozen():
        return [sys.executable]
    exe = Path(sys.executable)
    if plat.WIN and exe.with_name("pythonw.exe").exists():
        exe = exe.with_name("pythonw.exe")
    return [str(exe), str(Path(__file__).resolve().parent.parent / "blip_main.pyw")]

FAST = 60.0 if "--fast" in sys.argv else 1.0
HOUR = 3600.0
CHUTE_CHANCE = 0.1     # a dropped pet now and then pops a parachute: a little surprise
EGG_TIME = 90
BABY_END = 12 * HOUR
CHILD_END = 48 * HOUR
STAGE_NAMES = {"egg": "Egg", "baby": "Baby", "child": "Kid", "adult": "Grown-up"}
CALL_RING_SECS = 60
ANSWERS = {"coming": "On my way!", "2min": "Give me 2 minutes.", "busy": "Busy right now - I'll get back to you."}


# ---------------------------------------------------------------- pet state
def fresh_state(gen=1, name="Blip"):
    return {"name": name, "gen": gen, "stage": "egg", "palette": "baby", "age": 0.0,
            "stats": {"full": 80.0, "happy": 80.0, "energy": 90.0, "hygiene": 100.0, "health": 100.0},
            "sick": False, "asleep": False, "poops": [], "next_poop": None, "snacks": 0.0,
            "care_good": 0.0, "care_total": 0.0, "gone": False, "x": None,
            "wander": True, "wall": time.time(), "bc_seen": {}, "bc_summary_day": "",
            "species": "blip", "color": "auto"}


def load_state(path):
    st = read_json(path, None)
    base = fresh_state()
    if not isinstance(st, dict):
        return base
    stats = {**base["stats"], **st.get("stats", {})}
    base.update(st)
    base["stats"] = stats
    return base


CRIES = {"blip": ("blip", "bloo"), "kitty": ("mew", "nya"), "bunny": ("pip", "puu"), "bear": ("bwuh", "rawr"),
         "pup": ("arf", "wuf"), "chick": ("piyo", "cheep"), "bot": ("beep", "boop")}


def cry(species, mood="happy"):
    """A little Pokemon-style line in the pet's own language."""
    a, b = CRIES.get(species, CRIES["blip"])
    A = a.capitalize()
    forms = {
        "happy": [f"{A}!", f"{A} {a}!", f"{b}~!", f"{A}~"],
        "love": [f"{a}~ \u2665", f"{b} {b}~ \u2665", f"\u2665 {A}!"],
        "ask": [f"{A}?", f"{a} {a}?", f"{b}?"],
        "sad": [f"{a}...", f"{b}...", f"...{a}"],
        "no": [f"{A}! {A}!", f"{b}!!", f"nn~ {a}!"],
        "angry": [f"{A.upper()}!!", f"{b.upper()}!"],
        "sleepy": [f"{a}... zz", "zzz...", f"{b}~ zz"],
        "music": [f"\u266a {a}~", f"{b} \u266a", f"\u266a\u266a {a}"],
        "oops": [f"{a}...!", f"{A}?!"],
        "wow": [f"{A}!!", f"{A}~!!", f"{b} {b}!!"],
        "dizzy": [f"{a}~ @_@", "@_@", f"{b}... @"],
    }
    return random.choice(forms.get(mood, forms["happy"]))


def wrap_words(text, width):
    lines, cur = [], ""
    for word in text.split():
        while len(word) > width:
            if cur:
                lines.append(cur)
                cur = ""
            lines.append(word[:width])
            word = word[width:]
        if not cur:
            cur = word
        elif len(cur) + 1 + len(word) <= width:
            cur += " " + word
        else:
            lines.append(cur)
            cur = word
    if cur:
        lines.append(cur)
    return lines or [""]


def fmt_age(sec):
    sec = int(sec)
    d, h, m = sec // 86400, sec % 86400 // 3600, sec % 3600 // 60
    if d:
        return f"{d}d {h}h"
    if h:
        return f"{h}h {m}m"
    return f"{m}m"


# ---------------------------------------------------------------- app
class App:
    def __init__(self, data=None, start_services=True):
        plat.dpi_aware()
        self.data = Path(data) if data else data_dir()
        self.cfg = Config(self.data / "config.json")
        self.root = root = tk.Tk()
        root.report_callback_exception = self._on_error
        root.title("Blip")
        kit.init(root)
        kit.set_theme(self.cfg["theme"])
        W.style(root)
        self.update_state = None
        self.menu = None
        st0 = read_json(self.data / "state.json", {}) or {}
        sx = st0.get("x") if isinstance(st0.get("x"), (int, float)) else 10
        S = self.S = plat.use_scale(root, sx + 100, root.winfo_screenheight() - 60 if sx == 10 else 400)
        self.P = max(3, round(4 * S))
        self.bub_h = round(48 * S)
        self.W = max(GW * self.P, round(200 * S))
        self.ox = (self.W - GW * self.P) // 2
        self.H = self.bub_h + GH * self.P
        root.overrideredirect(True)
        root.attributes("-topmost", True)
        plat.make_transparent(root)
        if plat.MAC:      # Dock icon click brings the pet back; app menu > Settings
            root.createcommand("::tk::mac::ReopenApplication", self.reopen)
            root.createcommand("::tk::mac::ShowPreferences", lambda: self.open_window("settings"))
            root.createcommand("::tk::mac::Quit", self.quit)
        self.cv = tk.Canvas(root, width=self.W, height=self.H, bg=KEY, highlightthickness=0, bd=0)
        self.cv.pack()
        self.font = kit.F["small"]

        self.state_path = self.data / "state.json"
        self.st = load_state(self.state_path)
        self.surprise_look()
        self.timers = Timers(self.data / "timers.json")
        self.notes = Notes(self.data / "notes.json")
        self.inbox = read_json(self.data / "inbox.json", [])
        self.sound = Sound(self.data, lambda: self.cfg["sound"], lambda: self.cfg.get("volume", 60))
        self.cards = CardStack(self)
        self.events = queue.Queue()
        self.bc = Basecamp(self.cfg, self.post)
        self.bc.on_items = self.on_bc_items
        self.updater = upd.Updater(__version__, self.post)
        self.net = None
        self.net_error = None
        self.calls_in, self.calls_out = {}, {}
        self.alerts = {}
        self.voice = self.voice_call = None
        self.windows = {}
        self.bc_snooze_until = 0.0
        self.last_bc_nag = time.monotonic()
        self.refresh_icon()

        self.frame = 0
        self.action, self.action_until = "idle", 0.0
        self.dir = random.choice((-1, 1))
        self.vy, self.airborne, self.land = 0.0, False, 0
        self.vx, self.chute, self.bounces, self.trail = 0.0, 0, 0, []
        self.ball, self.spin, self.lucky, self.ptick = False, 0.0, False, 0
        self.phase = 0.0
        self.food, self.food_t0 = None, 0.0
        self.particles = []
        self.bubble, self.bubble_until, self.bubble_icon = "", 0.0, None
        self.board_text, self.board_until = "", 0.0
        self.happy_until = 0.0
        self.last_pat = 0.0
        self.last_nag = time.monotonic()
        self.last_save = time.monotonic()
        self.top_row = 14
        self.drag = None
        self.poop_wins = []
        self.ring_target = None
        self.var_wander = tk.BooleanVar(value=self.st["wander"])
        self.var_dnd = tk.BooleanVar(value=self.cfg["dnd"])

        l, t, r, b = work_area_at(root, 10, 10)
        x = self.st.get("x")
        if x is None:
            x = r - self.W - 140 * S
        self.x = float(x)
        self.work = work_area_at(root, self.x + self.W / 2, b - 5)
        lo, hi = self.bounds()
        self.x = min(max(self.x, lo), hi)
        self.y = float(self.work[3] - self.H)
        root.geometry(f"{self.W}x{self.H}+{int(self.x)}+{int(self.y)}")

        cv = self.cv
        cv.bind("<ButtonPress-1>", self.on_press)
        cv.bind("<B1-Motion>", self.on_drag)
        cv.bind("<ButtonRelease-1>", self.on_release)
        cv.bind("<Button-3>", self.on_menu)

        for px in self.st["poops"]:
            self._poop_window(px)
        gap = time.time() - self.st["wall"]
        self.st["wall"] = time.time()
        if self.st["gone"]:
            self.set_action("gone", 1e18)
        elif gap > 120:
            self.catch_up(gap)
        if self.st["stage"] == "egg" and not self.st["gone"]:
            self.st["age"] < 5 and self.board("A new egg!", 5)
        if self.st["asleep"]:
            self.set_action("sleep", 1e18)
        self.check_growth()

        self.physics()
        self.anim()
        root.after(1000, self.sim)
        root.after(4000, self.follow_system_theme)
        root.after(100, self.pump)
        self.tray = None
        self.temp_shown = False
        self.voice_ok = voice.available()
        if not self.st.get("visible", True):
            self._set_visible(False)
        if start_services:
            self.start_net()
            self.tray = Tray(self)
            root.after(int(random.uniform(60, 120) * 1000), self._update_loop)
            root.after(15000, self._bc_loop)
            if not self.cfg.get("named"):
                root.after(1200, lambda: self.open_window("welcome"))

    # ------------------------------------------------ plumbing
    def post(self, fn, *args):
        """Run fn(*args) on the UI thread (safe to call from any thread)."""
        self.events.put((fn, args))

    def pump(self):
        try:
            while True:
                fn, args = self.events.get_nowait()
                try:
                    fn(*args)
                except Exception:
                    self._on_error(*sys.exc_info())
        except queue.Empty:
            pass
        self.root.after(100, self.pump)

    def _on_error(self, *exc):
        tb = "".join(traceback.format_exception(*exc))
        if exc[0] is AttributeError and "deletecommand" in tb:
            return      # Tk cleaning up an after() callback of a window that already closed: harmless
        try:
            with open(self.data / "error.log", "a", encoding="utf-8") as fh:
                fh.write(time.ctime() + f" v{__version__}\n" + "".join(traceback.format_exception(*exc)) + "\n")
        except Exception:
            pass
        traceback.print_exception(*exc)

    def bounds(self):
        l, t, r, b = self.work
        pad = self.ox + 6 * self.P
        return l - pad, r - self.W + pad

    def ground(self):
        return self.work[3] - self.H

    def say(self, text, secs=4.0, icon=None):
        """Low level: put text (and an optional pixel icon) in the speech bubble."""
        self.bubble, self.bubble_until, self.bubble_icon = text, time.monotonic() + secs, icon

    def chirp(self, mood="happy", icon=None, secs=3.0):
        """The pet 'talks' in its own little language, plus an icon for what it wants."""
        if self.st["gone"]:
            self.say("...", secs)
        elif self.st["stage"] != "egg":
            self.say(cry(self.species(), mood), secs, icon)

    def board(self, text, secs=3.5):
        """Real information (sent, timer set, do not disturb...) goes on a little signboard."""
        self.board_text, self.board_until = text, time.monotonic() + secs

    def set_action(self, a, secs):
        self.action, self.action_until = a, time.monotonic() + secs

    def spawn(self, kind, n=1):
        for _ in range(n):
            if kind == "heart":
                self.particles.append([kind, 16 + random.randint(-7, 3), self.top_row - 3, 7])
            elif kind == "spark":
                self.particles.append([kind, random.randint(4, 26), random.randint(self.top_row - 2, GH - 4), 3])
            elif kind == "z":
                self.particles.append([kind, 19, self.top_row - 2, 6])
            elif kind == "bang":
                self.particles.append([kind, random.choice((5, 8, 23, 26)), self.top_row - 1, 3])
            elif kind == "note":
                self.particles.append([kind, random.randint(4, 26), self.top_row - 2, 5])

    def hatched(self):
        return not self.st["gone"] and self.st["stage"] != "egg"

    def palette(self):
        st = self.st
        if st["gone"]:
            return "ghost"
        if st["sick"]:
            return "sick"
        if st.get("color", "auto") != "auto" and st.get("color") in PALETTES:
            return st["color"]
        return {"baby": "baby", "child": "child"}.get(st["stage"], st["palette"])

    def species(self):
        return self.st.get("species") if self.st.get("species") in SPECIES else "blip"

    def mood_word(self):
        st, s = self.st, self.st["stats"]
        if st["gone"]:
            return "on the pet planet"
        if st["stage"] == "egg":
            return "still an egg"
        if st["asleep"]:
            return "sleeping"
        if st["sick"]:
            return "feeling sick"
        low = min(("hungry", s["full"]), ("bored", s["happy"]), ("tired", s["energy"]), ("messy", s["hygiene"]),
                  key=lambda kv: kv[1])
        if low[1] < 25:
            return low[0]
        return "happy" if s["happy"] > 60 else "okay"

    def mood(self):
        st, a = self.st, self.action
        if st["gone"]:
            return "normal"
        if a == "held":
            return "held"
        if a == "unfold":
            return "happy"
        if a == "fall" and self.airborne:
            return "happy" if self.chute else "held"
        if a == "ring":
            return "alert"
        if self.voice and self.voice.level_out > 0.12 and self.frame % 2:
            return "eat"
        if st["asleep"]:
            return "sleep"
        if st["sick"]:
            return "sick"
        if a == "eat":
            return "eat"
        if a == "play" or time.monotonic() < self.happy_until:
            return "happy"
        s = st["stats"]
        if min(s["full"], s["happy"], s["hygiene"]) < 20 or s["energy"] < 10:
            return "sad"
        return "normal"

    def busy_or_unable(self, what):
        st = self.st
        if st["gone"]:
            self.chirp()
            return True
        if st["stage"] == "egg":
            self.board("Still an egg")
            return True
        if st["asleep"] and what != "wake":
            self.chirp("sleepy")
            return True
        if self.action in ("eat", "held", "ring"):
            return True
        return False

    # ------------------------------------------------ alerts (ringing)
    def start_alert(self, key, sound="ring", dash=False):
        self.alerts[key] = sound
        self.sound.play(sound, loop=True)
        if not self.st.get("visible", True) and not self.temp_shown:
            self.temp_shown = True          # hidden pets pop out while something rings
            self._set_visible(True)
        if self.st["asleep"] and not self.st["gone"]:
            self.st["asleep"] = False
        if dash:
            l, t, r, b = self.work
            self.ring_target = (l + r) / 2 - self.W / 2
        if self.hatched() and self.action != "held":
            self.food = None
            self.set_action("ring", 1e18)
            self.jump()

    def stop_alert(self, key):
        if self.alerts.pop(key, None) is None:
            return
        if self.alerts:
            self.sound.play(list(self.alerts.values())[-1], loop=True)
        else:
            self.sound.stop()
            self.ring_target = None
            if self.action == "ring":
                self.choose()
            if self.temp_shown and not self.st.get("visible", True):
                self.temp_shown = False
                self.root.after(1500, lambda: self.st.get("visible", True) or self._set_visible(False))

    # ------------------------------------------------ loops
    def physics(self):
        a = self.action
        lo, hi = self.bounds()
        ground = self.ground()
        jitter = 0
        if a == "held":
            pass
        elif a == "gone":
            self.phase += 0.04
            target = ground - 30 * self.S + math.sin(self.phase) * 6 * self.S
            self.y += (target - self.y) * 0.05
        elif a == "fall" and (self.airborne or self.ball or self.y < ground - 0.5):
            self.airborne = True
            self.fly(ground, lo, hi)
        elif self.airborne or self.y < ground - 0.5:
            self.airborne = True
            self.chute = 0
            self.vy += 0.9 * self.S
            self.y += self.vy
            if a == "play":
                self.x += self.dir * 2.0 * self.S
            if self.y >= ground:
                self.y, self.vy, self.airborne, self.land = ground, 0.0, False, 2
                if a == "fall":
                    self.set_action("idle", 1.5)
                elif a == "play" and time.monotonic() < self.action_until:
                    self.jump()
        else:
            self.y = ground
            if abs(self.vx) > 0.3 * self.S:      # skid after a throw
                self.x += self.vx
                self.vx *= 0.8
            else:
                self.vx = 0.0
            if a == "walk":
                self.x += self.dir * (0.7 if self.st["sick"] else 1.5) * self.S
            elif a == "play" and time.monotonic() < self.action_until:
                self.jump()
            elif a == "ring":
                if self.ring_target is not None and abs(self.ring_target - self.x) > 4:
                    step = 9 * self.S
                    self.dir = 1 if self.ring_target > self.x else -1
                    self.x += max(-step, min(step, self.ring_target - self.x))
                else:
                    jitter = (2 if self.frame % 2 else -2) * self.S
                    if random.random() < 0.02:
                        self.jump()
        if a != "held":
            if self.x < lo:
                self.x, self.dir = lo, 1
            elif self.x > hi:
                self.x, self.dir = hi, -1
        self.root.geometry(f"+{int(self.x + jitter)}+{int(self.y)}")
        if self.ball:
            self.ptick += 1
            if self.ptick % 2 == 0:
                self.draw()
        self.root.after(30, self.physics)

    def fly(self, ground, lo, hi):
        """A thrown or dropped pet: gravity, bounces off the floor and the screen edges, and a
        parachute when it falls from high up."""
        S = self.S
        top = self.work[1] - self.bub_h
        if self.ball:
            self.roll(ground, lo, hi)
            return
        if self.chute:
            self.chute += 1
            self.vy += (1.7 * S - self.vy) * 0.12       # drift down slowly
            self.vx *= 0.96
            self.x += self.vx + math.sin(self.chute / 10) * 0.8 * S
        else:
            self.vy += 0.9 * S
            self.vx *= 0.995
            self.x += self.vx
            if self.lucky and self.vy > 3 * S and ground - self.y > 190 * S and not self.bounces:
                self.chute = 1
                self.bubble_until = 0       # the canopy needs the space above the head
        self.y += self.vy
        if self.y < top:
            self.y, self.vy = top, abs(self.vy) * 0.4
        if self.x < lo:
            self.x, self.vx = lo, abs(self.vx) * 0.6
        elif self.x > hi:
            self.x, self.vx = hi, -abs(self.vx) * 0.6
        if self.y < ground:
            return
        self.y = ground
        self.land = 2
        if not self.chute and self.vy > 5 * S and self.bounces < 5:
            self.vy = -self.vy * 0.48
            self.vx *= 0.75
            self.bounces += 1
            if self.bounces == 1 and self.vy < -9 * S:
                self.spawn("spark", 2)
            return
        soft, hard = bool(self.chute), self.bounces >= 2
        self.vy, self.airborne, self.chute = 0.0, False, 0
        self.set_action("idle", 1.5)
        if soft:
            self.happy_until = time.monotonic() + 2
            self.chirp("happy", None, 2)
        elif hard and self.hatched():
            self.chirp("dizzy", None, 2.5)

    BALL_SPEED = 14       # px per tick (at 100% scale) that turns a throw into a ball

    def roll(self, ground, lo, hi):
        """Curled-up ball: ricochets off the screen edges, the top and the taskbar, then rolls
        to a stop and unfolds."""
        S, P = self.S, self.P
        top = self.work[1] - self.bub_h - 12 * P
        self.vy += 0.75 * S
        self.vx *= 0.998
        self.x += self.vx
        self.y += self.vy
        self.spin += self.vx / (7.5 * P)
        hit = False
        if self.x < lo:
            self.x, self.vx, hit = lo, abs(self.vx) * 0.78, True
        elif self.x > hi:
            self.x, self.vx, hit = hi, -abs(self.vx) * 0.78, True
        if self.y < top:
            self.y, self.vy, hit = top, abs(self.vy) * 0.7, True
        if hit and math.hypot(self.vx, self.vy) > 12 * S:
            self.spawn("spark", 2)
        if self.y < ground:
            return
        self.y = ground
        if self.vy > 2.5 * S:
            self.vy = -self.vy * 0.62
            self.vx *= 0.92
            self.bounces += 1
            self.land = 1
            return
        self.vy = 0.0
        self.vx *= 0.955                       # rolling along the taskbar
        if abs(self.vx) < 0.5 * S:
            self.vx = 0.0
            self.ball, self.airborne = False, False
            self.land = 3
            self.set_action("unfold", 0.6)
            self.draw()

    def jump(self):
        self.vy = -8.5 * self.S
        self.airborne = True
        if self.action == "play" and random.random() < 0.3:
            self.dir = -self.dir

    def choose(self):
        st = self.st
        if self.alerts and self.hatched():
            self.set_action("ring", 1e18)
        elif st["gone"]:
            self.set_action("gone", 1e18)
        elif st["asleep"]:
            self.set_action("sleep", 1e18)
        elif st["stage"] != "egg" and st["wander"] and random.random() < (0.35 if st["sick"] else 0.55):
            self.dir = random.choice((-1, 1))
            self.set_action("walk", random.uniform(2, 6))
        else:
            self.set_action("idle", random.uniform(2, 7))

    def anim(self):
        self.frame += 1
        now = time.monotonic()
        a = self.action
        if a in ("idle", "walk") and now >= self.action_until:
            self.choose()
        elif a == "eat" and now >= self.action_until:
            self.finish_eat()
        elif a == "play" and now >= self.action_until and not self.airborne:
            self.set_action("idle", 2)
        elif a == "unfold" and now >= self.action_until:
            self.set_action("idle", 1.5)
            if self.hatched():
                if self.bounces >= 3:
                    self.chirp("dizzy", None, 2.5)
                else:
                    self.chirp("wow", None, 2)
                    self.happy_until = now + 2
        if self.st["asleep"] and self.frame % 5 == 0:
            self.spawn("z")
        if a == "ring" and self.frame % 2 == 0:
            self.spawn("bang")
        if self.land:
            self.land -= 1
        for p in self.particles:
            p[2] -= 1
            p[3] -= 1
            if p[0] == "z" and self.frame % 2:
                p[1] += 1
        self.particles = [p for p in self.particles if p[3] > 0]
        self.draw()
        self.root.after(250, self.anim)

    def follow_system_theme(self):
        """With 'Match system', switch when Windows / macOS flips between light and dark."""
        if self.cfg.get("theme") == "system":
            want = "dark" if plat.system_dark() else "light"
            if want != kit.THEME:
                self.apply_theme("system", chosen=False)
        self.root.after(10000 if plat.MAC else 4000, self.follow_system_theme)

    def sim(self):
        now = time.time()
        gap = max(0.0, now - self.st["wall"])
        self.st["wall"] = now
        if gap > 120:
            self.catch_up(gap)
        else:
            self.step(gap * FAST)
        if self.action != "held":
            self.work = work_area_at(self.root, self.x + self.W / 2, self.y + self.H - 2)
        for it in self.timers.tick(now):
            self.timer_fired(it)
        for n in self.notes.due(now):
            self.reminder_fired(n)
        self.nag()
        if self.tray and self.frame % 40 < 4:
            n = len(self.net.online()) if self.net else 0
            self.tray.set_title(f"Blip - {self.st['name']} is {self.mood_word()}"
                                + (f"  ·  {n} nearby" if n else "") + ("  ·  do not disturb" if self.cfg["dnd"] else ""))
        if time.monotonic() - self.last_save > 30:
            self.save()
        self.root.after(1000, self.sim)

    # ------------------------------------------------ simulation (pet needs)
    def step(self, dt):
        st = self.st
        s = st["stats"]
        if st["gone"]:
            return
        st["age"] += dt
        if st["stage"] == "egg":
            if st["age"] >= EGG_TIME:
                self.hatch()
            return
        h = dt / HOUR
        if st["asleep"]:
            s["full"] -= 4 * h
            s["happy"] -= 1 * h
            s["energy"] += 30 * h
            s["hygiene"] -= 1 * h
        else:
            s["full"] -= 10 * h
            s["happy"] -= 8 * h
            s["energy"] -= 7 * h
            s["hygiene"] -= (2 + 6 * len(st["poops"])) * h
        if st["sick"]:
            s["health"] -= 6 * h
            s["happy"] -= 4 * h
        if s["full"] <= 0:
            s["health"] -= 15 * h
        if s["hygiene"] < 15:
            s["health"] -= 6 * h
        if s["happy"] <= 0:
            s["health"] -= 4 * h
        if not st["sick"] and s["full"] > 40 and s["happy"] > 30 and s["hygiene"] > 40:
            s["health"] += 6 * h
        st["snacks"] = max(0.0, st["snacks"] - h)
        self.clamp()

        if not st["sick"] and (s["hygiene"] < 25 or s["full"] < 15 or st["snacks"] > 3) \
                and random.random() < 0.6 * h:
            st["sick"] = True
            self.chirp("sad", "pill", 5)
        if st["next_poop"] is not None and st["age"] >= st["next_poop"]:
            st["next_poop"] = None
            self.add_poop()
        if not st["asleep"] and s["energy"] < 8 and self.action in ("idle", "walk"):
            st["asleep"] = True
            self.set_action("sleep", 1e18)
            self.chirp("sleepy", "moon")
        if st["asleep"] and s["energy"] >= 100:
            st["asleep"] = False
            self.chirp("happy")
            self.choose()
        if st["stage"] in ("baby", "child"):
            st["care_total"] += dt
            if min(s["full"], s["happy"], s["hygiene"]) > 35 and not st["sick"]:
                st["care_good"] += dt
        self.check_growth()
        if s["health"] <= 0:
            self.leave()

    def catch_up(self, gap):
        """Time passed while Blip was closed: gentler decay, never fatal."""
        st = self.st
        s = st["stats"]
        if st["gone"]:
            return
        st["age"] += gap
        if st["stage"] == "egg":
            if st["age"] >= EGG_TIME:
                self.hatch()
            return
        h = min(gap, 24 * HOUR) / HOUR
        floor = 15.0
        if st["asleep"]:
            s["energy"] += 30 * h
            rates = (("full", 1.5), ("happy", 0.5), ("hygiene", 0.5))
        else:
            rates = (("full", 3.3), ("happy", 2.7), ("energy", 2.3), ("hygiene", 1.0))
        for k, rate in rates:
            s[k] = max(min(s[k], floor), s[k] - rate * h)
        self.clamp()
        if st["next_poop"] is not None and st["age"] >= st["next_poop"]:
            st["next_poop"] = None
            self.add_poop()
        if st["asleep"] and s["energy"] >= 100:
            st["asleep"] = False
        if st["stage"] in ("baby", "child"):
            st["care_total"] += gap
            st["care_good"] += gap * 0.5
        self.check_growth()
        if gap > 600:
            self.chirp("love", "heart", 5)
            self.happy_until = time.monotonic() + 3

    def clamp(self):
        s = self.st["stats"]
        for k in s:
            s[k] = min(100.0, max(0.0, s[k]))

    def check_growth(self):
        st = self.st
        if st["gone"]:
            return
        if st["stage"] == "baby" and st["age"] >= BABY_END:
            st["stage"] = "child"
            self.grew()
        if st["stage"] == "child" and st["age"] >= CHILD_END:
            ratio = st["care_good"] / max(1.0, st["care_total"])
            st["palette"] = "gold" if ratio > 0.75 else "sky" if ratio > 0.45 else "plum"
            st["stage"] = "adult"
            self.grew()

    def grew(self):
        self.spawn("spark", 8)
        self.chirp("wow")
        self.board("Grew up!", 5)
        self.sound.play("done")
        self.happy_until = time.monotonic() + 3
        self.save()

    def hatch(self):
        st = self.st
        st["stage"] = "baby"
        st["age"] = max(st["age"], EGG_TIME)
        self.spawn("spark", 8)
        self.chirp("wow", "heart", 5)
        self.sound.play("done")
        self.happy_until = time.monotonic() + 4
        self.set_action("idle", 2)
        self.save()

    def leave(self):
        st = self.st
        st.update(gone=True, asleep=False, sick=False)
        for win, _ in self.poop_wins:
            win.destroy()
        self.poop_wins, st["poops"] = [], []
        self.set_action("gone", 1e18)
        self.sound.play("sad")
        self.say("...", 12)
        self.cards.show("Goodbye", f"{st['name']} floated off to the pet planet. Right-click and hatch a new egg any time.", [("New egg", self.new_egg), ("OK", lambda: None)], kind="info", icon="heart")
        self.save()

    def nag(self):
        st = self.st
        now = time.monotonic()
        if st["gone"] or st["asleep"] or st["stage"] == "egg" or now < self.bubble_until or self.alerts:
            return
        overdue = [i for i in self.bc.items if i["scope"] == "overdue"]
        if overdue and self.cfg["bc_enabled"] and now - self.last_bc_nag > 45 * 60 \
                and time.time() > self.bc_snooze_until:
            self.last_bc_nag = now
            self.board(f"{len(overdue)} task{'s' if len(overdue) > 1 else ''} overdue", 5)
            return
        if now - self.last_nag < 30:
            return
        s = st["stats"]
        if st["sick"]:
            self.chirp("sad", "pill")
        elif len(st["poops"]) >= 3:
            self.chirp("no", "clean")
        else:
            needs = [(s["full"], "ask", "meal"), (s["happy"], "ask", "play"),
                     (s["energy"], "sleepy", "moon"), (s["hygiene"], "sad", "clean")]
            val, mood, icon = min(needs, key=lambda n: n[0])
            if val < 25:
                self.chirp(mood, icon)
            elif random.random() < 0.06:
                self.chirp("music")
            else:
                return
        self.last_nag = now

    # ------------------------------------------------ care actions
    def feed(self, kind):
        if self.busy_or_unable("feed"):
            return
        st, s = self.st, self.st["stats"]
        if kind == "meal":
            if s["full"] >= 92:
                self.chirp("no", "meal")
                return
            s["full"] += 35
            s["happy"] += 3
            if st["next_poop"] is None:
                st["next_poop"] = st["age"] + random.uniform(25, 70) * 60
        else:
            s["full"] += 10
            s["happy"] += 15
            st["snacks"] += 1
            if st["snacks"] > 3:
                s["health"] -= 6
            if st["next_poop"] is None and random.random() < 0.3:
                st["next_poop"] = st["age"] + random.uniform(20, 60) * 60
        self.clamp()
        self.food, self.food_t0 = kind, time.monotonic()
        self.set_action("eat", 2.5)

    def finish_eat(self):
        kind, self.food = self.food, None
        if kind == "pill":
            self.chirp("happy", "heart")
        elif kind == "snack" and self.st["snacks"] > 3:
            self.chirp("sad", "pill")
        else:
            self.chirp("love")
            self.happy_until = time.monotonic() + 2
            self.spawn("heart")
        self.set_action("idle", 2)

    def play(self):
        if self.busy_or_unable("play"):
            return
        s = self.st["stats"]
        if self.st["sick"]:
            self.chirp("no", "pill")
            return
        if s["energy"] < 15:
            self.chirp("no", "moon")
            return
        if s["full"] < 10:
            self.chirp("no", "meal")
            return
        s["happy"] += 20
        s["energy"] -= 10
        s["full"] -= 5
        s["hygiene"] -= 3
        self.clamp()
        self.set_action("play", 3.5)
        self.jump()
        self.spawn("heart", 2)
        self.sound.play("pop")
        self.chirp("wow")

    def clean(self):
        st = self.st
        if st["gone"]:
            return
        n = len(st["poops"])
        for win, _ in self.poop_wins:
            win.destroy()
        self.poop_wins, st["poops"] = [], []
        was = st["stats"]["hygiene"]
        st["stats"]["hygiene"] = 100.0
        self.spawn("spark", 6)
        self.chirp("happy" if n or was < 70 else "ask")

    def medicine(self):
        if self.busy_or_unable("medicine"):
            return
        st = self.st
        if not st["sick"]:
            self.chirp("no", "pill")
            st["stats"]["happy"] -= 5
            self.clamp()
            return
        st["sick"] = False
        st["stats"]["health"] += 25
        self.clamp()
        self.food, self.food_t0 = "pill", time.monotonic()
        self.set_action("eat", 2.0)

    def toggle_sleep(self):
        st = self.st
        if self.busy_or_unable("wake"):
            return
        if st["asleep"]:
            st["asleep"] = False
            self.chirp("sleepy" if st["stats"]["energy"] < 40 else "happy")
            self.choose()
        else:
            if st["stats"]["energy"] > 85:
                self.chirp("no", "moon")
                return
            st["asleep"] = True
            self.set_action("sleep", 1e18)
            self.chirp("sleepy", "moon")

    def pat(self):
        st = self.st
        now = time.monotonic()
        if self.alerts:
            self.open_cards_front()
            return
        if st["gone"]:
            self.chirp()
            return
        if st["stage"] == "egg":
            self.land = 2
            return
        if st["asleep"]:
            self.chirp("sleepy")
            return
        self.happy_until = now + 1.5
        self.spawn("heart")
        if now - self.last_pat > 2:
            st["stats"]["happy"] += 4
            self.clamp()
        self.last_pat = now
        if random.random() < 0.35:
            self.chirp("love", None, 2)

    def open_cards_front(self):
        for c in self.cards.cards:
            try:
                c.win.lift()
            except tk.TclError:
                pass

    def new_egg(self):
        st = self.st
        for win, _ in self.poop_wins:
            win.destroy()
        self.poop_wins = []
        self.st = fresh_state(st["gen"] + 1, st["name"])
        self.surprise_look()
        self.st["x"], self.st["wander"] = int(self.x), st["wander"]
        self.set_action("idle", 2)
        self.board("A new egg!", 5)
        self.save()

    # ------------------------------------------------ poops
    def add_poop(self):
        st = self.st
        if len(st["poops"]) >= 6:
            st["stats"]["hygiene"] -= 10
            self.clamp()
            return
        l, t, r, b = self.work
        px = self.x + self.W / 2 - self.dir * 10 * self.P
        px = min(max(px, l + 6 * self.P), r - 6 * self.P)
        st["poops"].append(px)
        st["stats"]["hygiene"] -= 5
        self.clamp()
        self._poop_window(px)
        self.chirp("oops")

    def _poop_window(self, px):
        P = self.P
        w, h = 8 * P, len(POOP) * P
        l, t, r, b = work_area_at(self.root, px, self.work[3] - 2)
        win = tk.Toplevel(self.root)
        win.overrideredirect(True)
        win.attributes("-topmost", True)
        plat.make_transparent(win)
        c = tk.Canvas(win, width=w, height=h, bg=KEY, highlightthickness=0, bd=0, cursor="hand2")
        c.pack()
        for y, row in enumerate(POOP):
            for x, ch in enumerate(row):
                if ch != ".":
                    c.create_rectangle(x * P, y * P, (x + 1) * P, (y + 1) * P, width=0,
                                       fill=COL["out"] if ch == "o" else COL["poop"])
        win.geometry(f"{w}x{h}+{int(px - w / 2)}+{int(b - h)}")
        entry = (win, px)
        c.bind("<Button-1>", lambda e: self.clean_one(entry))
        self.poop_wins.append(entry)
        self.root.lift()

    def clean_one(self, entry):
        win, px = entry
        if entry in self.poop_wins:
            self.poop_wins.remove(entry)
        if px in self.st["poops"]:
            self.st["poops"].remove(px)
        win.destroy()
        self.st["stats"]["hygiene"] += 8
        self.clamp()
        self.spawn("spark", 2)

    # ------------------------------------------------ timers & alarms
    def start_timer(self, secs, label=""):
        it = self.timers.add_timer(secs, label)
        self.board(f"Timer {fmt_clock(secs)}", 3)
        self.sound.play("pop")
        self.draw()
        return it

    def start_stopwatch(self, label=""):
        self.timers.add_stopwatch(label)
        self.board("Stopwatch on", 3)
        self.sound.play("pop")
        self.draw()

    def add_alarm(self, hh, mm, days, label=""):
        self.timers.add_alarm(hh, mm, days, label)
        self.board(f"Alarm {hh:02d}:{mm:02d} {describe_days(days)}", 4)
        self.sound.play("pop")

    def ask_timer(self):
        txt = W.ask_text(self, "New timer", "How long? (25, 1:30, 90s, 2h 15m)", initial="25")
        if txt is None:
            return
        secs = parse_duration(txt)
        if not secs:
            self.board("Try 25 or 1:30")
            return
        self.start_timer(secs)

    def ask_alarm(self):
        txt = W.ask_text(self, "New alarm", "Ring at what time today? (e.g. 13:00 or 5pm)",
                         initial=time.strftime("%H:%M", time.localtime(time.time() + 3600)))
        if txt is None:
            return
        hm = parse_clock(txt)
        if not hm:
            self.board("Try 25 or 1:30")
            return
        self.add_alarm(hm[0], hm[1], [])

    # ------------------------------------------------ notes & reminders
    def quick_reminder(self):
        got = W.ask_reminder(self)
        if not got:
            return
        text, ts = got
        self.notes.add(text, ts)
        self.board("Reminder " + describe_when(ts) if ts else "Noted!", 4)
        self.chirp("happy", "note")
        w = self.windows.get("notes")
        if w and w.alive():
            w.refresh()

    def reminder_fired(self, n):
        key = f"note:{n['id']}"
        nid = n["id"]

        def done():
            self.notes.complete(nid)
            self._notes_changed()

        def snooze(mins):
            self.notes.snooze(nid, mins)
            self._notes_changed()
        self.start_alert(key, "alarm")
        self.board("Reminder!", 6)
        self.cards.show("Reminder", n["text"], [("Done", done), ("10 min", lambda: snooze(10)),
                                                 ("1 hour", lambda: snooze(60))],
                        kind="task", icon="bell", key=key, timeout=300, on_close=lambda why: self.stop_alert(key))
        self._notes_changed()

    def _notes_changed(self):
        w = self.windows.get("notes")
        if w and w.alive():
            w.refresh()

    def timer_fired(self, it):
        key = f"tm:{it['id']}"
        label = it.get("label") or ""
        if it["kind"] == "timer":
            title, icon = "Time's up!", "hourglass"
            body = (f"{label}\n" if label else "") + f"Your {fmt_clock(it['dur'])} timer is done."
            buttons = [("Stop", lambda: None),
                       ("+5 min", lambda: (self.start_timer(5 * 60, label), None)[1])]
        else:
            title, icon = "Alarm", "bell"
            body = (f"{label}\n" if label else "") + f"It's {it['hh']:02d}:{it['mm']:02d}."
            iid = it["id"]
            buttons = [("Stop", lambda: None),
                       ("Snooze 5", lambda: (self.timers.snooze(iid, 5), None)[1]),
                       ("Snooze 10", lambda: (self.timers.snooze(iid, 10), None)[1])]
        self.start_alert(key, "alarm")
        self.cards.show(title, body, buttons, kind="alarm" if it["kind"] == "alarm" else "timer", icon=icon,
                        key=key, timeout=180, on_close=lambda why: self.stop_alert(key))

    # ------------------------------------------------ office network
    def profile(self):
        return {"name": self.cfg["display_name"], "status": "busy" if self.cfg["dnd"] else "available",
                "stage": self.st["stage"], "palette": self.palette(), "species": self.species(),
                "voice": bool(getattr(self, "voice_ok", False)), "opus": "opus" in voice.codecs(),
                "ver": __version__}

    def refresh_icon(self):
        """Window icons show the current pet."""
        try:
            pal = self.palette() if self.palette() in PALETTES else "gold"
            self._icon = kit.pet_photo(self.root, self.species(), pal, 32)
            self._icon_big = kit.pet_photo(self.root, self.species(), pal, 64)
            self.root.iconphoto(True, self._icon_big, self._icon)
        except tk.TclError:
            pass

    def surprise_look(self):
        """New pets get a random look, so every desk's pet is different from day one.
        Pets whose owner already picked a look keep it."""
        st = self.st
        if st.get("look_chosen"):
            return
        if st.get("species", "blip") == "blip" and st.get("color", "auto") == "auto":
            st["species"] = random.choice(list(SPECIES))
            st["color"] = random.choice([c for c in COLORS if c != "auto"])
        st["look_chosen"] = True

    def set_look(self, species=None, color=None):
        self.st["look_chosen"] = True
        if species:
            self.st["species"] = species
        if color:
            self.st["color"] = color
        self.refresh_icon()
        if self.tray:
            self.tray.set_look(self.species(), self.palette())
        self.draw()
        self.net_rescan()
        self.save()

    def start_net(self):
        self.net_error = None
        if not self.cfg["network"]:
            self.net = None
            return
        peers = [p for p in self.cfg["manual_peers"].replace(";", ",").split(",")]
        server = self.cfg.get("server", "")
        if server and not self.cfg.get("team"):
            from .link import new_team_code
            self.cfg["team"] = new_team_code()
            self.cfg.save()
        self.net = Net(self.cfg["node_id"], self.profile, self.cfg["passphrase"],
                       lambda kind, d: self.post(self.on_net, kind, d),
                       server=server, team=self.cfg.get("team", ""), manual_peers=peers)
        self.net.on_voice = self._voice_packet
        if not self.net.start():
            self.net_error = self.net.error
            self.net = None

    def restart_net(self):
        if self.net:
            self.net.stop()
        self.start_net()

    def net_rescan(self):
        if self.net:
            self.net.announce_now()

    def net_status(self):
        if not self.cfg["network"]:
            return "Office network is off (Settings)."
        if not self.net:
            return f"Network problem: {self.net_error}"
        n = len(self.net.online())
        state, _ = self.net.server_state()
        srv = {"connected": "  - online", "connecting": "  - connecting to server",
               "error": "  - server not reachable"}.get(state, "")
        return f"{n} pet{'s' if n != 1 else ''} nearby{srv}"

    def set_dnd(self, on):
        self.cfg["dnd"] = bool(on)
        self.cfg.save()
        self.var_dnd.set(self.cfg["dnd"])
        self.net_rescan()
        self.board("DND on" if on else "DND off", 3)
        if self.tray:
            self.tray.refresh_menu()

    def log_inbox(self, direction, peer, name, text):
        self.inbox.append({"ts": time.time(), "dir": direction, "peer": peer, "name": name, "text": text[:500]})
        self.inbox = self.inbox[-100:]
        self.save_inbox()

    def save_inbox(self):
        write_json(self.data / "inbox.json", self.inbox)

    def peer_name(self, pid, fallback="Someone"):
        p = self.net.peer(pid) if self.net else None
        return (p or {}).get("name") or fallback

    def on_net(self, kind, d):
        if kind == "peers" or not d:
            return
        src, body = d["from"], d.get("body") or {}
        name = self.peer_name(src)
        if kind == "call":
            self.incoming_call(src, name, str(body.get("call_id", ""))[:40])
        elif kind == "call_cancel":
            cid = str(body.get("call_id", ""))
            if cid in self.calls_in:
                self.cards.close(f"in:{cid}", why="cancel")
        elif kind == "call_reply":
            self.call_answered(src, name, str(body.get("call_id", "")), str(body.get("text", ""))[:300],
                               body.get("voice"))
        elif kind == "voice_end":
            if self.voice_call and self.voice_call["cid"] == str(body.get("call_id", "")):
                self.end_voice(notify=False, why="They hung up")
        elif kind == "msg":
            self.incoming_msg(src, name, str(body.get("text", ""))[:500])

    def call(self, pid):
        if not self.net:
            self.board("Network off")
            return
        name = self.peer_name(pid)
        cid = uuid.uuid4().hex[:12]
        self.calls_out[cid] = {"peer": pid, "name": name}
        key = f"out:{cid}"

        def closed(why):
            info = self.calls_out.pop(cid, None)
            if info and why in ("button", "x", "timeout") and self.net:
                self.net.send(pid, "call_cancel", {"call_id": cid})
            if info and why == "timeout":
                self.cards.show("No answer", f"{name} didn't pick up.",
                                [("Message", lambda: self.compose(pid, name=name)), ("OK", lambda: None)],
                                kind="call", icon="phone", timeout=30)

        self.cards.show(f"Calling {name}", f"Ringing {name}'s Blip...", [("Hang up", lambda: None)],
                        kind="call", icon="phone", key=key, timeout=CALL_RING_SECS + 5, on_close=closed)
        self.log_inbox("out", pid, name, "\U0001F4DE called")
        if self.hatched():
            self.jump()

        def sent(ok):
            if not ok:
                self.post(self._call_failed, cid, name)
        self.net.send(pid, "call", {"call_id": cid}, sent)

    def _call_failed(self, cid, name):
        if self.calls_out.pop(cid, None) is not None:
            self.cards.close(f"out:{cid}")
            self.cards.show("Can't reach", f"Couldn't reach {name}'s Blip. They may be offline.", [("OK", lambda: None)],
                            kind="call", icon="phone", timeout=20)

    def incoming_call(self, src, name, cid):
        self.log_inbox("in", src, name, "\U0001F4DE called you")
        if self.cfg["dnd"]:
            self.net.send(src, "call_reply", {"call_id": cid, "text": "Busy right now (do not disturb)."})
            self.cards.show("Missed call", f"{name} called while you were on do not disturb.",
                            [("Call back", lambda: self.call(src)), ("OK", lambda: None)],
                            kind="call", icon="phone", key=f"missed:{src}")
            return
        key = f"in:{cid}"
        self.calls_in[cid] = {"peer": src, "name": name}

        def answer(code):
            def go():
                if code == "voice":
                    self.calls_in.pop(cid, None)
                    self.answer_voice(src, name, cid)
                    return
                text = ANSWERS.get(code)
                if code == "custom":
                    text = W.ask_text(self, f"Reply to {name}", "Your reply:")
                    if not text:
                        return True
                self.net.send(src, "call_reply", {"call_id": cid, "text": text[:300]})
                self.log_inbox("out", src, name, text)
                self.calls_in.pop(cid, None)
                if self.hatched():
                    self.happy_until = time.monotonic() + 2
            return go

        def closed(why):
            self.stop_alert(key)
            if self.calls_in.pop(cid, None) is not None and why in ("timeout", "cancel"):
                self.cards.show("Missed call", f"{name} called you.",
                                [("Call back", lambda: self.call(src)), ("OK", lambda: None)],
                                kind="call", icon="phone", key=f"missed:{src}")

        self.start_alert(key, "ring", dash=True)
        buttons = [("Coming", answer("coming")), ("2 min", answer("2min")), ("Busy", answer("busy")),
                   ("Reply...", answer("custom"))]
        if self.can_voice(src):
            buttons.insert(0, ("Talk", answer("voice")))
        self.cards.show("Incoming call", f"{name} is calling you!", buttons,
                        kind="call", icon="phone", key=key, timeout=CALL_RING_SECS, on_close=closed)

    def call_answered(self, src, name, cid, text, voice_info=None):
        if self.calls_out.pop(cid, None) is None:
            return
        self.cards.close(f"out:{cid}")
        if isinstance(voice_info, dict) and self.voice_ok and not self.voice:
            p = self.net.peer(src) if self.net else None
            try:
                nonce = str(voice_info["nonce"])
                bytes.fromhex(nonce)
                port = int(voice_info.get("port") or 0)
            except (KeyError, TypeError, ValueError):
                nonce = port = None
            codec = voice_info.get("codec") if voice_info.get("codec") in voice.codecs() else "pcm"
            if nonce and voice_info.get("route") == "server" and p:
                self.start_voice(src, name, cid, 0, nonce, None, route="server", codec=codec)
                return
            if nonce and p and p.get("ip") and port:
                self.start_voice(src, name, cid, 0, nonce, (p["ip"], port))
                return
        self.log_inbox("in", src, name, text)
        self.sound.play("ding")
        self.cards.show(name, text or "answered", [("OK", lambda: None), ("Message", lambda: self.compose(src, name=name))],
                        kind="call", icon="phone", timeout=45)
        if self.hatched():
            self.happy_until = time.monotonic() + 2
            self.jump()

    # ------------------------------------------------ voice calls
    def can_voice(self, pid):
        p = self.net.peer(pid) if self.net else None
        return bool(self.voice_ok and not self.voice and p and p.get("voice")
                    and p.get("via") in ("lan", "server"))

    def answer_voice(self, src, name, cid):
        nonce = voice.new_nonce()
        p = self.net.peer(src) or {}
        if p.get("via") == "server":
            codec = "opus" if p.get("opus") and "opus" in voice.codecs() else "pcm"
            sess = self.start_voice(src, name, cid, 1, nonce, None, route="server", codec=codec)
            info = {"nonce": nonce, "route": "server", "codec": codec}
        else:
            sess = self.start_voice(src, name, cid, 1, nonce, None)
            info = {"port": sess.port if sess else 0, "nonce": nonce}
        if sess:
            self.net.send(src, "call_reply", {"call_id": cid, "text": "Talking", "voice": info})

    def _voice_packet(self, src, packet):
        """Call audio from the server (network thread): straight into the session."""
        sess, info = self.voice, self.voice_call
        if sess and info and info["peer"] == src:
            sess.feed(packet)

    def start_voice(self, pid, name, cid, role, nonce, peer_addr, route="lan", codec="pcm"):
        net = self.net
        keys = voice.session_keys(net.tkey if route == "server" else net.key, cid, nonce)
        transport = (lambda pkt: net.server_voice(pid, pkt)) if route == "server" else None
        try:
            sess = voice.VoiceSession(keys, role, peer_addr, on_end=lambda why: self.post(self._voice_lost, cid),
                                      transport=transport, codec=codec)
        except RuntimeError:             # codec failed to load: fall back to raw audio
            sess = voice.VoiceSession(keys, role, peer_addr, on_end=lambda why: self.post(self._voice_lost, cid),
                                      transport=transport)
        try:
            sess.start()
        except OSError as e:
            self.cards.show("Voice", f"Couldn't start the call audio: {e}", [("OK", lambda: None)], kind="call",
                            icon="phone", timeout=20)
            return None
        self.voice, self.voice_call = sess, {"cid": cid, "peer": pid, "name": name}
        self.log_inbox("in" if role else "out", pid, name, "\U0001F3A7 voice call")
        if self.hatched():
            self.happy_until = time.monotonic() + 2
        note = "\nCouldn't open your microphone: " + sess.error if sess.error else \
            "\nTip: a headset stops echo."

        def mute():
            if self.voice:
                self.voice.muted = not self.voice.muted
                self._voice_paint()
            return True
        self.cards.show(f"Talking with {name}", "00:00" + note, [("Hang up", self.end_voice), ("Mute", mute)],
                        kind="call", icon="phone", key="voice",
                        on_close=lambda why: why == "x" and self.end_voice())
        self._voice_note = note
        self._voice_tick(sess)
        return sess

    def _voice_paint(self):
        c = self.cards.get("voice")
        if c and self.voice:
            t = int(self.voice.seconds())
            state = "  \u00b7  muted" if self.voice.muted else ""
            if not self.voice.received and t > 4:
                state += "  \u00b7  connecting\u2026"
            c.set_body(f"{t // 60:02d}:{t % 60:02d}{state}{self._voice_note}")

    def _voice_tick(self, sess):
        if self.voice is not sess:
            return
        self._voice_paint()
        self.root.after(1000, lambda: self._voice_tick(sess))

    def _voice_lost(self, cid):
        if self.voice_call and self.voice_call["cid"] == cid:
            self.end_voice(notify=True, why="Lost the connection")

    def end_voice(self, notify=True, why=None):
        sess, info = self.voice, self.voice_call
        self.voice = self.voice_call = None
        if sess:
            sess.stop()
        if info and notify and self.net:
            self.net.send(info["peer"], "voice_end", {"call_id": info["cid"]})
        self.cards.close("voice")
        if info:
            secs = int(sess.seconds()) if sess else 0
            self.board(why or f"Call ended {secs // 60}:{secs % 60:02d}", 4)

    def incoming_msg(self, src, name, text):
        self.log_inbox("in", src, name, text)
        if not self.cfg["dnd"]:
            self.sound.play("ding")
        if self.hatched() and self.action in ("idle", "walk"):
            self.jump()
        self.cards.show("Message", f"{name}: {text}",
                        [("Reply", lambda: self.compose(src, name=name)), ("OK", lambda: None)],
                        kind="msg", icon="mail")

    def compose(self, pid, parent=None, name=None):
        if not self.net:
            self.board("Network off")
            return
        name = name or self.peer_name(pid)
        text = W.ask_text(self, f"Message {name}", f"Message to {name}:", parent=parent)
        if not text or not text.strip():
            return
        text = text.strip()[:500]

        def sent(ok):
            self.post(self.board, "Sent!" if ok else f"Can't reach {name}", 3)
        self.log_inbox("out", pid, name, text)
        self.net.send(pid, "msg", {"text": text}, sent)

    # ------------------------------------------------ basecamp
    def _bc_loop(self):
        if self.bc.connected and self.cfg["bc_enabled"]:
            self.bc.refresh()
        self.root.after(15 * 60 * 1000, self._bc_loop)

    def on_bc_items(self, items, error):
        win = self.windows.get("tasks")
        if win and win.alive():
            win.refresh()
        if error or not self.cfg["bc_enabled"] or time.time() < self.bc_snooze_until:
            return
        today = time.strftime("%Y-%m-%d")
        seen = {k: v for k, v in self.st.get("bc_seen", {}).items() if v == today}
        new = [i for i in items if str(i["id"]) not in seen]
        morning = parse_clock(self.cfg.get("bc_morning") or "") or (9, 30)
        now_hm = time.localtime()[3:5]
        summary_due = self.st.get("bc_summary_day") != today and now_hm >= morning
        show = items if summary_due else new
        if show:
            self.st["bc_summary_day"] = today if summary_due else self.st.get("bc_summary_day", "")
            for i in items:
                seen[str(i["id"])] = today
            self.bc_card(show, summary_due)
        self.st["bc_seen"] = seen

    def bc_card(self, items, summary):
        over = [i for i in items if i["scope"] == "overdue"]
        today = [i for i in items if i["scope"] != "overdue"]
        lines = []
        for i in (over + today)[:5]:
            tag = "OVERDUE" if i["scope"] == "overdue" else "today"
            lines.append(f"• {i['content']}  ({tag}{', ' + i['project'] if i.get('project') else ''})")
        if len(items) > 5:
            lines.append(f"+ {len(items) - 5} more")
        head = []
        if over:
            head.append(f"{len(over)} overdue")
        if today:
            head.append(f"{len(today)} due today")
        title = "Your Basecamp day" if summary else "Basecamp"
        self.sound.play("ding")
        if self.hatched():
            self.jump()

        def snooze():
            self.bc_snooze_until = time.time() + 3600
            self.root.after(3600 * 1000 + 5000, lambda: self.bc_card(self.bc.items, False) if self.bc.items else None)
        self.cards.show(title, ", ".join(head) + ":\n" + "\n".join(lines),
                        [("Show all", lambda: self.open_window("tasks")), ("Snooze 1h", snooze), ("OK", lambda: None)],
                        kind="task", icon="task", key="bc")

    def connect_basecamp(self):
        """One click: open the Basecamp approval page in the browser and finish in the background."""
        if not all(self.bc.client_creds()):
            self.open_window("settings", "basecamp")
            return
        self.board("Opening Basecamp", 4)

        def done(ok, msg, accts):
            self.cards.show("Basecamp", msg + (". Blip will remind you about what's due." if ok else ""),
                            [("Show my tasks", lambda: self.open_window("tasks")), ("OK", lambda: None)] if ok
                            else [("OK", lambda: None)], kind="task", icon="task", timeout=30)
            for name in ("tasks", "settings"):
                w = self.windows.get(name)
                if w and w.alive():
                    for attr in ("sig", "bc_state"):
                        if hasattr(w, attr):
                            setattr(w, attr, None)
                    w.refresh()
        self.bc.connect_oauth(done)

    def complete_task(self, item):
        if item.get("type") not in ("todo", "Todo", None):
            self.board("Only to-dos")
            return

        def done(ok, err):
            if ok:
                self.sound.play("done")
                self.spawn("spark", 6)
                self.happy_until = time.monotonic() + 3
                self.board("\x05 Done!", 3)
                self.chirp("wow")
                self.st["stats"]["happy"] += 5
                self.clamp()
            else:
                self.board("Couldn't tick it", 4)
            win = self.windows.get("tasks")
            if win and win.alive():
                win.refresh()
        self.bc.complete(item, done)

    # ------------------------------------------------ updates
    def _update_loop(self):
        self.check_updates(manual=False)
        self.root.after(int(6 * 3600 * 1000 + random.uniform(0, 1800) * 1000), self._update_loop)

    def check_updates(self, manual=False, quiet=False):
        """manual: the user asked. quiet: Settings shows the result inline, so no card."""
        self.update_state = ("checking", "Checking for updates…")
        self.updater.check(lambda info, err: self.on_update_info(info, err, manual, quiet))
        if manual and not quiet:
            self.board("Checking...", 3)

    def on_update_info(self, info, err, manual, quiet=False):
        loud = manual and not quiet
        if err:
            self.update_state = ("error", "Couldn't reach GitHub. Check your internet and try again.")
            if loud:
                self.cards.show("Update check", f"Couldn't check for updates.\n{err}", [("OK", lambda: None)],
                                kind="update", icon="task", timeout=30)
            return
        self.cfg["last_update_check"] = time.time()
        self.cfg.save()
        if not info:
            self.update_state = ("ok", "You're up to date")
            if loud:
                self.cards.show("Up to date", f"You have the latest Blip (v{__version__}).", [("OK", lambda: None)],
                                kind="update", icon="check", timeout=20)
            return
        self.update_state = ("new", f"Blip {info['version']} is available")
        if frozen() and (self.cfg["auto_update"] or manual):
            self.install_update(info)
        elif frozen():
            self.cards.show("Update ready", f"Blip v{info['version']} is out.",
                            [("Update now", lambda: self.install_update(info)), ("Later", lambda: None)],
                            kind="update", icon="task", key="update")
        else:
            self.update_state = ("new", f"Blip {info['version']} is out (running from source: git pull)")
            if not quiet:
                self.cards.show("Update", f"Blip v{info['version']} is out (you're running from source: git pull).",
                                [("OK", lambda: None)], kind="update", icon="task", key="update", timeout=60)

    def install_update(self, info):
        if self.alerts or self.calls_out or self.calls_in or self.voice:
            self.update_state = ("new", f"Blip {info['version']} installs after your call")
            self.root.after(10 * 60 * 1000, lambda: self.install_update(info))
            return
        self.update_state = ("installing", f"Installing Blip {info['version']}… Blip restarts by itself")
        self.board(f"Updating v{info['version']}", 30)
        self.updater.fetch(info, self.on_update_ready)

    def on_update_ready(self, path, err):
        if err:
            self.cards.show("Update failed", f"Couldn't download the update.\n{err}", [("OK", lambda: None)],
                            kind="update", icon="task", timeout=60)
            return
        self.save()
        if self.net:
            self.net.stop()
        if self.tray:
            self.tray.stop()
        upd.launch_installer(path)
        self.root.after(300, self.root.destroy)

    # ------------------------------------------------ startup registry
    def startup_enabled(self):
        return plat.startup_enabled()

    def set_startup(self, on):
        if FAST > 1:
            return
        try:
            plat.set_startup(on, startup_args())
        except OSError as e:
            self.board("Couldn't change", 3)
            self._on_error(type(e), e, e.__traceback__)

    def reopen(self):
        """macOS: the Dock icon was clicked."""
        if not self.st.get("visible", True):
            self.toggle_visible()
        self.root.after(50, self.menu_at_cursor)

    # ------------------------------------------------ drawing
    def draw(self):
        cv, st, P, oy = self.cv, self.st, self.P, self.bub_h
        cv.delete("px")
        if self.ball:
            egg = st["stage"] == "egg"
            cols = (COL["egg"], COL["egg2"], COL["spot"]) if egg else PALETTES[self.palette()]
            g = build_ball(cols, self.spin, face=not egg, squash=0.25 if self.land else 0.0)
            self.add_trail(g)
        elif st["stage"] == "egg" and not st["gone"]:
            g = build_egg(self.frame, min(1.0, st["age"] / EGG_TIME))
        else:
            a = self.action
            facing = self.dir if a in ("walk", "play", "eat") or (a == "ring" and self.ring_target) else 0
            if a == "walk":
                squash = self.frame % 2 == 0
            elif st["asleep"]:
                squash = self.frame % 8 < 4
            elif a == "ring":
                squash = self.frame % 2 == 0
            else:
                squash = self.frame % 6 == 0
            squash = squash or self.land > 0
            mood = self.mood()
            blink = mood in ("normal", "sad") and random.random() < 0.08
            g = build_pet(st["stage"], self.palette(), mood, self.frame, facing, squash, blink, self.species())
        self.top_row = min(y for _, y in g)

        if self.food and self.action == "eat":
            prog = min(1.0, (time.monotonic() - self.food_t0) / 2.2)
            cells = FOOD[self.food]
            edible = [c for c in cells if c[2] != "bowl"]
            keep = edible[int(len(edible) * prog):] + [c for c in cells if c[2] == "bowl"]
            fx = 26 if self.dir > 0 else 1
            for x, y, c in keep:
                g[(fx + x, GH - 4 + y)] = COL[c]
        for kind, x, y, _ in self.particles:
            cells, col = {"heart": (HEART, COL["heart"]), "z": (ZED, COL["z"]), "spark": (SPARK, COL["spark"]),
                          "bang": (BANG, COL["alert"]), "note": (NOTE, COL["z"])}[kind]
            for dx, dy in cells:
                if 0 <= x + dx < GW:
                    g[(x + dx, y + dy)] = col
        if self.chute and self.action == "fall":
            self.add_chute(g)
        for (x, y), c in g.items():
            if y * P + oy >= 0:
                cv.create_rectangle(self.ox + x * P, oy + y * P, self.ox + (x + 1) * P, oy + (y + 1) * P,
                                    fill=c, width=0, tags="px")
        cv.delete("bub")
        if self.board_text and time.monotonic() < self.board_until:
            self.draw_board()
        elif self.bubble and time.monotonic() < self.bubble_until:
            self.draw_bubble()
        else:
            self.draw_sign()

    def add_trail(self, g):
        """Rocket flame and smoke puffs behind a fast ball."""
        v = math.hypot(self.vx, self.vy)
        if v < 8 * self.S:
            return
        ux, uy = -self.vx / v, -self.vy / v
        cx, cy = 16, GH - 8.5
        xmin, xmax = -self.ox // self.P, (self.W - self.ox) // self.P - 1
        ymin = -self.bub_h // self.P
        n = 2 + min(3, int(v / (8 * self.S)))
        for k in range(n):
            d = 9 + 3.2 * k
            size = 3 if k < 2 else 2
            col = ("#fff1a8", "#ffb347", "#ff6b3d", "#cfc8dc", "#b3abc4")[min(k, 4)]
            jx = random.uniform(-0.8, 0.8) * k * 0.4
            jy = random.uniform(-0.8, 0.8) * k * 0.4
            px, py = int(cx + ux * d - uy * jx), int(cy + uy * d + ux * jy)
            for dx in range(size):
                for dy in range(size):
                    x, y = px + dx - size // 2, py + dy - size // 2
                    if xmin <= x <= xmax and ymin <= y < GH:
                        g.setdefault((x, y), col)

    def add_chute(self, g):
        """A striped pixel parachute above the pet's head, swaying a little."""
        t = self.top_row
        sway = round(math.sin(self.chute / 10) * 1.5)
        cx = GW // 2 + sway
        top = t - 11
        widths = [6, 10, 12, 13, 13]
        red, cream, ink = "#ef5b6b", "#fff4e6", COL["out"]
        for i, hw in enumerate(widths):
            y = top + i
            for x in range(cx - hw, cx + hw):
                edge = x in (cx - hw, cx + hw - 1) or i == 0
                stripe = ((x - cx + 13) // 4) % 2
                g[(x, y)] = ink if edge else (red if stripe else cream)
        y = top + len(widths)
        for x in range(cx - 13, cx + 13):       # scalloped hem
            if (x - cx) % 4 in (0, 1):
                g[(x, y)] = ink
        # lines from the hem down to the pet's shoulders
        for sx, ex in ((cx - 12, GW // 2 - 6), (cx - 4, GW // 2 - 3), (cx + 3, GW // 2 + 2), (cx + 11, GW // 2 + 5)):
            y0, y1 = y + 1, t + 2
            for k in range(y1 - y0):
                x = round(sx + (ex - sx) * k / max(1, y1 - y0 - 1))
                g.setdefault((x, y0 + k), "#9a90b0")

    def draw_sign(self):
        disp = self.timers.display()
        if not disp:
            return
        icon, text, it = disp
        cv, S = self.cv, self.S
        px = max(1, round(2 * S))
        full = ICON[icon] + " " + text
        tw = text_width(full, px)
        w, h = tw + 8 * px, 7 * px + 6 * px
        x0 = int(self.W / 2 - w / 2)
        y1 = int(self.bub_h + (self.top_row - 1) * self.P - 2 * px)
        y0 = y1 - h
        paused = (it["kind"] == "timer" and it["left"] is not None) or (it["kind"] == "stopwatch" and not it["running"])
        urgent = it["kind"] == "timer" and it["left"] is None and self.timers.remaining(it) <= 10
        fill = "#fff4c2" if paused else ("#ffd6dc" if urgent and self.frame % 2 else "#e9fbe9")
        pixel_box(cv, x0, y0, x0 + w, y1, px, COL["out"], ("bub", "sign"))
        pixel_box(cv, x0 + px, y0 + px, x0 + w - px, y1 - px, px, fill, ("bub", "sign"))
        if not (paused and self.frame % 4 == 0):
            draw_text(cv, x0 + 4 * px, y0 + 3 * px, full, px, COL["out"], ("bub", "sign"))
        n = self.timers.active_count()
        if n > 1:
            draw_text(cv, x0 + w + 2 * px, y0, f"+{n - 1}", px, COL["out"], ("bub", "sign"))

    def draw_board(self):
        cv, S = self.cv, self.S
        px = max(1, round(2 * S))
        tags = ("bub", "board")
        maxc = max(6, int((self.W - 16 * px) // (6 * px)))
        lines = wrap_words(self.board_text.upper(), maxc)[:3]
        lh = 9 * px
        tw = max(text_width(ln, px) for ln in lines)
        w, h = tw + 10 * px, len(lines) * lh + 5 * px
        x0 = int(self.W / 2 - w / 2)
        y1 = int(self.bub_h + (self.top_row - 2) * self.P - 3 * px)
        y1 = max(y1, h + 1)
        y0 = y1 - h
        wood, cream = "#8b5a2b", "#fff1d0"
        cv.create_rectangle(self.W / 2 - px, y1, self.W / 2 + px, y1 + 4 * px, fill=wood, width=0, tags=tags)
        pixel_box(cv, x0, y0, x0 + w, y1, px, wood, tags)
        pixel_box(cv, x0 + px, y0 + px, x0 + w - px, y1 - px, px, cream, tags)
        for i, ln in enumerate(lines):
            draw_text(cv, int(self.W / 2 - text_width(ln, px) / 2), y0 + 3 * px + i * lh, ln, px, COL["out"], tags)

    def draw_bubble(self):
        cv, S = self.cv, self.S
        cx = self.W / 2
        ty = self.bub_h + (self.top_row - 3) * self.P - 8 * S
        ty = max(ty, 30 * S)
        ip = max(1, round(2 * S))
        iw = (5 * ip + round(5 * S)) if self.bubble_icon else 0
        t = cv.create_text(cx - iw / 2, ty, text=self.bubble, font=self.font, fill=COL["out"],
                           anchor="s", width=round(150 * S), justify="center", tags="bub")
        x1, y1, x2, y2 = cv.bbox(t)
        x1, y1, x2, y2 = x1 - 6 * S, y1 - 3 * S, x2 + 6 * S + iw, y2 + 3 * S
        if y1 < 1:
            cv.move(t, 0, 1 - y1)
            y2 += 1 - y1
            y1 = 1
        lw = max(1, round(S))
        cv.create_polygon(cx - 4 * S, y2 - lw, cx + 4 * S, y2 - lw, cx, y2 + 6 * S,
                          fill=COL["white"], outline=COL["out"], width=lw, tags="bub")
        cv.create_rectangle(x1, y1, x2, y2, fill=COL["white"], outline=COL["out"], width=lw, tags="bub")
        cv.create_rectangle(cx - 4 * S + lw, y2 - lw, cx + 4 * S - lw + 1, y2 + 1,
                            fill=COL["white"], width=0, tags="bub")
        cv.tag_raise(t)
        if self.bubble_icon:
            draw_text(cv, x2 - 6 * S - 5 * ip, (y1 + y2) / 2 - 3.5 * ip, ICON[self.bubble_icon], ip,
                      COL["z"], "bub")

    # ------------------------------------------------ input
    def on_press(self, e):
        if self.menu:
            self.menu.close()
        if "board" in self.cv.gettags("current"):
            self.board_until = 0
            self.drag = None
            return
        if "sign" in self.cv.gettags("current"):
            self.drag = None
            self.open_window("timers")
            return
        self.drag = [e.x_root - self.x, e.y_root - self.y, e.x_root, e.y_root, False]
        self.trail = []

    def on_drag(self, e):
        d = self.drag
        if not d:
            return
        if not d[4] and abs(e.x_root - d[2]) + abs(e.y_root - d[3]) > 5:
            d[4] = True
            if self.st["asleep"] and not self.st["gone"]:
                self.st["asleep"] = False
                self.st["stats"]["happy"] -= 5
                self.clamp()
                self.chirp("angry")
            if self.action != "gone":
                self.ball, self.chute = False, 0
                self.set_action("held", 1e18)
                self.food = None
                self.ring_target = None
        if d[4]:
            self.x, self.y = e.x_root - d[0], e.y_root - d[1]
            self.trail = (self.trail + [(time.monotonic(), e.x_root, e.y_root)])[-8:]

    def on_release(self, e):
        d, self.drag = self.drag, None
        if not d:
            return
        if d[4]:
            self.work = work_area_at(self.root, e.x_root, e.y_root)
            lo, hi = self.bounds()
            self.x = min(max(self.x, lo), hi)
            if self.action == "held":
                self.vx, self.vy = self.throw_speed()
                self.airborne = True
                self.chute, self.bounces = 0, 0
                self.ball = math.hypot(self.vx, self.vy) > self.BALL_SPEED * self.S
                self.lucky = not self.ball and random.random() < CHUTE_CHANCE
                self.set_action("fall", 1e18)
                if self.alerts:
                    self.root.after(1500, lambda: self.alerts and self.set_action("ring", 1e18))
            self.cards.layout()
            self.save()
        else:
            self.pat()

    def throw_speed(self):
        """Mouse speed over the last ~0.1 s, in pixels per physics tick (30 ms)."""
        now = time.monotonic()
        pts = [t for t in self.trail if now - t[0] < 0.12]
        if len(pts) < 2 or now - pts[-1][0] > 0.08:
            return 0.0, 0.0
        dt = max(0.016, pts[-1][0] - pts[0][0])
        lim = 40 * self.S
        vx = (pts[-1][1] - pts[0][1]) / dt * 0.03
        vy = (pts[-1][2] - pts[0][2]) / dt * 0.03
        return max(-lim, min(lim, vx)), max(-lim, min(lim, vy))

    def on_menu(self, e):
        if self.menu:
            self.menu.close()
        st, s = self.st, self.st["stats"]
        alive = not st["gone"]
        hatched = self.hatched()
        n = len(st["poops"])
        header = {
            "name": st["name"], "species": self.species(), "palette": self.palette(),
            "stage": "egg" if st["stage"] == "egg" and not st["gone"] else (st["stage"] if st["stage"] != "egg" else "baby"),
            "mood": self.mood() if self.mood() in ("happy", "sad", "sleep", "sick") else "normal",
            "status": f"{STAGE_NAMES.get(st['stage'], '')}  \u00b7  {self.mood_word()}",
            "bars": [("meal", s["full"], "#f0a020"), ("play", s["happy"], "#ef5b8b"), ("moon", s["energy"], "#6b7fd7")],
            "command": lambda: self.open_window("settings", "pet"),
        }
        actions = [
            ("meal", "Feed a meal", lambda: self.feed("meal"), hatched, "Feed"),
            ("snack", "Give a snack", lambda: self.feed("snack"), hatched, "Snack"),
            ("play", "Play", self.play, hatched, "Play"),
            ("clean", "Clean up", self.clean, alive, f"Clean ({n})" if n else "Clean"),
            ("pill", "Medicine", self.medicine, hatched, "Meds"),
            ("moon", "Sleep", self.toggle_sleep, hatched, "Wake" if st["asleep"] else "Sleep"),
        ]

        n_active = self.timers.active_count()

        def timer_menu():
            items = [{"label": f"{m} minutes" if m > 1 else "1 minute", "icon": "hourglass",
                      "command": (lambda sec=m * 60: self.start_timer(sec))} for m in (1, 5, 10, 15, 25, 30, 45, 60)]
            return items + [{"label": "Other…", "icon": "plus", "command": self.ask_timer},
                            {"sep": True},
                            {"label": "All timers & alarms", "icon": "gear", "hint": str(n_active or ""),
                             "command": lambda: self.open_window("timers")}]

        peers = sorted(self.net.online() if self.net else [], key=lambda p: (p.get("status") == "busy",
                                                                             (p.get("name") or "").lower()))
        due = len(self.bc.items)
        pending = len(self.notes.pending())
        open_notes = sum(1 for n in self.notes.items if not n.get("done"))
        items = [
            {"section": "Notes"},
            {"label": "Remind me…", "icon": "bell", "command": self.quick_reminder},
            {"label": "Notes & reminders", "icon": "note", "command": lambda: self.open_window("notes"),
             "hint": f"{pending} set" if pending else (str(open_notes) if open_notes else "")},
            {"tools": [{"icon": "hourglass", "label": "Timer", "submenu": timer_menu,
                        "badge": str(n_active) if n_active else ""},
                       {"icon": "bell", "label": "Alarm", "command": self.ask_alarm},
                       {"icon": "stopwatch", "label": "Stopwatch", "command": self.start_stopwatch}]},
            {"section": f"Office - {len(peers)} nearby" if peers else "Office"},
        ]
        SHOW = 5
        for p in peers[:SHOW]:
            pid = p["id"]
            items.append({"person": p, "label": p.get("name", "?"),
                          "command": (lambda pid=pid: self.compose(pid)),
                          "buttons": [("phone", (lambda pid=pid: self.call(pid)), True, "Call"),
                                      ("chat", (lambda pid=pid: self.compose(pid)), True, "Message")]})
        if not peers:
            items.append({"label": "Nobody nearby yet · look again", "icon": "reset", "command": self.net_rescan})
        items += [
            {"label": "Everyone & inbox" if len(peers) > SHOW else "Buddies & inbox", "icon": "people",
             "command": lambda: self.open_window("buddies"),
             "hint": f"+{len(peers) - SHOW}" if len(peers) > SHOW else ""},
            {"label": "Basecamp tasks", "icon": "task", "command": lambda: self.open_window("tasks"),
             "hint": str(due or "")} if self.bc.connected else
            {"label": "Connect Basecamp", "icon": "task", "command": self.connect_basecamp},
            {"label": "Do not disturb", "icon": "dnd", "check": bool(self.cfg["dnd"]),
             "command": lambda: self.set_dnd(not self.cfg["dnd"])},
            {"sep": True},
            {"label": "Wander around", "icon": "walk", "check": bool(st["wander"]),
             "command": lambda: self.set_wander(not self.st["wander"])},
            {"label": "Settings", "icon": "gear", "command": lambda: self.open_window("settings")},
        ]
        if st["gone"]:
            items.append({"label": "Hatch a new egg", "icon": "heart", "command": self.new_egg})
        items.append({"label": "Quit Blip", "icon": "power", "command": self.quit})
        self.menu = PopMenu(self, items, e.x_root + 4, e.y_root - 20, header=header, actions=actions)

    def set_wander(self, on):
        self.st["wander"] = bool(on)
        self.var_wander.set(self.st["wander"])
        if not on and self.action == "walk":
            self.set_action("idle", 3)

    def open_window(self, name, page=None):
        if name == "status":
            name, page = "settings", "pet"
        w = self.windows.get(name)
        if w and w.alive():
            w.top.deiconify()
            w.top.lift()
            w.top.focus_force()
            if page and hasattr(w, "show"):
                w.show(page)
            return w
        cls = {"timers": W.TimersWindow, "buddies": W.BuddiesWindow, "tasks": W.TasksWindow,
               "notes": W.NotesWindow,
               "settings": W.SettingsWindow, "welcome": W.WelcomeWindow}[name]
        self.windows[name] = w = cls(self)
        if page and hasattr(w, "show"):
            w.show(page)
        if name == "tasks" and self.bc.connected:
            self.bc.refresh()
        return w

    def apply_theme(self, name, chosen=True):
        """Switch dark/light/system and redraw the open windows in the new colours."""
        self.cfg["theme"] = name
        if chosen:
            self.cfg["theme_chosen"] = True
        self.cfg.save()
        kit.set_theme(name)
        W.style(self.root)
        if self.menu:
            self.menu.close()
        for key, w in list(self.windows.items()):
            if w and w.alive() and key != "welcome":
                page = getattr(w, "page", None)
                w.close()
                self.open_window(key, page)

    def toggle_wander(self):
        self.st["wander"] = self.var_wander.get()
        if not self.st["wander"] and self.action == "walk":
            self.set_action("idle", 3)

    def rename(self):
        name = W.ask_text(self, "Rename", "Pet name:", initial=self.st["name"])
        if name and name.strip():
            self.st["name"] = name.strip()[:16]
            self.chirp("happy", "heart")
            self.save()

    def save(self):
        self.st["x"] = int(self.x)
        self.st["wall"] = time.time()
        try:
            write_json(self.state_path, self.st)
        except Exception:
            self._on_error(*sys.exc_info())
        self.last_save = time.monotonic()

    def quit(self):
        if self.voice:
            self.end_voice()
        self.save()
        self.sound.stop()
        if self.net:
            self.net.stop()
        if self.tray:
            self.tray.stop()
        self.root.destroy()

    # ------------------------------------------------ tray helpers
    def work_at(self, x, y):
        return work_area_at(self.root, x, y)

    def menu_at_cursor(self):
        class E:
            pass
        e = E()
        e.x_root, e.y_root = self.root.winfo_pointerxy()
        e.x_root -= round(130 * self.S)
        e.y_root -= round(40 * self.S)
        self.on_menu(e)

    def _set_visible(self, on):
        wins = [self.root] + [w for w, _ in self.poop_wins]
        for w in wins:
            try:
                (w.deiconify if on else w.withdraw)()
            except tk.TclError:
                pass
        if on:
            self.root.attributes("-topmost", True)

    def toggle_visible(self):
        on = not self.st.get("visible", True)
        self.st["visible"] = on
        self.temp_shown = False
        self._set_visible(on)
        if on:
            self.chirp("happy")
        self.save()
        if self.tray:
            self.tray.refresh_menu()


def main():
    data = None
    if FAST > 1:
        data = data_dir() / "fast"
    if not plat.single_instance("BlipDesktopPet" + ("Fast" if FAST > 1 else "")):
        return
    app = App(data)
    app.root.mainloop()
