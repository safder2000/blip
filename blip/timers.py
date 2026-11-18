"""Countdown timers, stopwatches and clock alarms (model only, no UI)."""
import datetime as dt
import math
import re
import time
import uuid

from .config import read_json, write_json

DAY_NAMES = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
WEEKDAYS = [0, 1, 2, 3, 4]
EVERYDAY = list(range(7))


def fmt_clock(secs, up=True):
    """Seconds as MM:SS or H:MM:SS. Countdowns round up, stopwatches (up=False) round down."""
    secs = max(0, int(math.ceil(secs - 1e-6) if up else math.floor(secs)))
    h, m, s = secs // 3600, secs % 3600 // 60, secs % 60
    return f"{h}:{m:02d}:{s:02d}" if h else f"{m:02d}:{s:02d}"


def parse_duration(text):
    """'25' -> 25 min, '1:30' -> 1 h 30 min, '90s', '2h', '1h 20m', '45 min' -> seconds. None if invalid."""
    t = text.strip().lower()
    if not t:
        return None
    if re.fullmatch(r"\d+(\.\d+)?", t):
        return int(float(t) * 60) or None
    m = re.fullmatch(r"(\d+):(\d{1,2})(?::(\d{1,2}))?", t)
    if m:
        a, b, c = int(m[1]), int(m[2]), m[3]
        return (a * 3600 + b * 60 + int(c)) if c is not None else (a * 3600 + b * 60) or None
    total, pos = 0, 0
    for m in re.finditer(r"(\d+(?:\.\d+)?)\s*(h|hr|hrs|hours?|m|min|mins|minutes?|s|sec|secs|seconds?)\b", t):
        if t[pos:m.start()].strip():
            return None
        n, u = float(m[1]), m[2][0]
        total += n * {"h": 3600, "m": 60, "s": 1}[u]
        pos = m.end()
    if pos == 0 or t[pos:].strip():
        return None
    return int(total) or None


def parse_clock(text):
    """'9:30', '09:30', '9.30', '930', '5pm', '5:15 pm' -> (h, m) or None."""
    t = text.strip().lower().replace(" ", "")
    m = re.fullmatch(r"(\d{1,2})(?:[:.]?(\d{2}))?(am|pm)?", t)
    if not m:
        return None
    h, mi, ap = int(m[1]), int(m[2] or 0), m[3]
    if ap:
        if not 1 <= h <= 12:
            return None
        h = h % 12 + (12 if ap == "pm" else 0)
    if not (0 <= h < 24 and 0 <= mi < 60):
        return None
    return h, mi


def next_fire(hh, mm, days, now):
    """Next timestamp after `now` at hh:mm on one of `days` (weekday numbers; empty = any day)."""
    base = dt.datetime.fromtimestamp(now).replace(hour=hh, minute=mm, second=0, microsecond=0)
    for add in range(0, 8):
        cand = base + dt.timedelta(days=add)
        if cand.timestamp() > now and (not days or cand.weekday() in days):
            return cand.timestamp()
    return None


def describe_days(days):
    if not days:
        return "once"
    s = sorted(days)
    if s == EVERYDAY:
        return "every day"
    if s == WEEKDAYS:
        return "weekdays"
    return ", ".join(DAY_NAMES[d] for d in s)


class Timers:
    def __init__(self, path):
        self.path = path
        self.items = read_json(path, [])

    def save(self):
        write_json(self.path, self.items)

    def get(self, iid):
        return next((i for i in self.items if i["id"] == iid), None)

    # ------------------------------------------------ create
    def add_timer(self, secs, label="", now=None):
        now = time.time() if now is None else now
        it = {"id": uuid.uuid4().hex[:10], "kind": "timer", "label": label, "dur": secs,
              "end": now + secs, "left": None}
        self.items.append(it)
        self.save()
        return it

    def add_stopwatch(self, label="", now=None):
        it = {"id": uuid.uuid4().hex[:10], "kind": "stopwatch", "label": label,
              "start": time.time() if now is None else now, "acc": 0.0, "running": True}
        self.items.append(it)
        self.save()
        return it

    def add_alarm(self, hh, mm, days=(), label="", now=None):
        now = time.time() if now is None else now
        it = {"id": uuid.uuid4().hex[:10], "kind": "alarm", "label": label, "hh": hh, "mm": mm,
              "days": sorted(set(days)), "enabled": True, "next": next_fire(hh, mm, days, now), "snooze": None}
        self.items.append(it)
        self.save()
        return it

    # ------------------------------------------------ queries
    def remaining(self, it, now=None):
        now = time.time() if now is None else now
        return it["left"] if it["left"] is not None else max(0.0, it["end"] - now)

    def elapsed(self, it, now=None):
        now = time.time() if now is None else now
        return it["acc"] + (now - it["start"] if it["running"] else 0.0)

    def describe(self, it, now=None):
        now = time.time() if now is None else now
        k = it["kind"]
        if k == "timer":
            state = " (paused)" if it["left"] is not None else ""
            return f"Timer {fmt_clock(self.remaining(it, now))}{state}"
        if k == "stopwatch":
            return f"Stopwatch {fmt_clock(self.elapsed(it, now), False)}" + ("" if it["running"] else " (paused)")
        when = f"{it['hh']:02d}:{it['mm']:02d}"
        st = describe_days(it["days"]) if it["enabled"] else "off"
        if it.get("snooze"):
            st += ", snoozed"
        return f"Alarm {when} ({st})"

    def display(self, now=None):
        """(icon, text, item) for the pixel sign over the pet, or None."""
        now = time.time() if now is None else now
        timers = [i for i in self.items if i["kind"] == "timer"]
        running = sorted((i for i in timers if i["left"] is None), key=lambda i: i["end"])
        paused = [i for i in timers if i["left"] is not None]
        sws = [i for i in self.items if i["kind"] == "stopwatch"]
        if running:
            return "hourglass", fmt_clock(self.remaining(running[0], now)), running[0]
        if paused:
            return "hourglass", fmt_clock(paused[0]["left"]), paused[0]
        if sws:
            sw = next((s for s in sws if s["running"]), sws[0])
            return "stopwatch", fmt_clock(self.elapsed(sw, now), False), sw
        snoozed = [i for i in self.items if i["kind"] == "alarm" and i.get("snooze")]
        if snoozed:
            a = min(snoozed, key=lambda i: i["snooze"])
            return "bell", fmt_clock(a["snooze"] - now), a
        return None

    def active_count(self):
        return sum(1 for i in self.items if i["kind"] in ("timer", "stopwatch"))

    # ------------------------------------------------ edits
    def toggle_pause(self, iid, now=None):
        now = time.time() if now is None else now
        it = self.get(iid)
        if not it:
            return
        if it["kind"] == "timer":
            if it["left"] is None:
                it["left"] = max(0.0, it["end"] - now)
            else:
                it["end"], it["left"] = now + it["left"], None
        elif it["kind"] == "stopwatch":
            if it["running"]:
                it["acc"] += now - it["start"]
                it["running"] = False
            else:
                it["start"], it["running"] = now, True
        elif it["kind"] == "alarm":
            it["enabled"] = not it["enabled"]
            it["snooze"] = None
            it["next"] = next_fire(it["hh"], it["mm"], it["days"], now) if it["enabled"] else None
        self.save()

    def reset(self, iid, now=None):
        now = time.time() if now is None else now
        it = self.get(iid)
        if it and it["kind"] == "stopwatch":
            it.update(start=now, acc=0.0)
        elif it and it["kind"] == "timer":
            it.update(end=now + it["dur"], left=None)
        self.save()

    def remove(self, iid):
        self.items = [i for i in self.items if i["id"] != iid]
        self.save()

    def snooze(self, iid, mins, now=None):
        now = time.time() if now is None else now
        it = self.get(iid)
        if it and it["kind"] == "alarm":
            it["snooze"] = now + mins * 60
            self.save()
        elif it is None or it["kind"] == "timer":
            label = (it or {}).get("label", "")
            self.add_timer(mins * 60, label, now)

    # ------------------------------------------------ tick
    def tick(self, now=None):
        """Advance; return the items that just went off. Finished timers are removed."""
        now = time.time() if now is None else now
        fired, keep, changed = [], [], False
        for it in self.items:
            k = it["kind"]
            if k == "timer" and it["left"] is None and it["end"] <= now:
                fired.append(it)
                changed = True
                continue
            if k == "alarm":
                if it.get("snooze") and it["snooze"] <= now:
                    it["snooze"] = None
                    fired.append(it)
                    changed = True
                elif it["enabled"] and it.get("next") and it["next"] <= now:
                    fired.append(it)
                    changed = True
                    if it["days"]:
                        it["next"] = next_fire(it["hh"], it["mm"], it["days"], now)
                    else:
                        it["enabled"], it["next"] = False, None
                elif it["enabled"] and not it.get("next"):
                    it["next"] = next_fire(it["hh"], it["mm"], it["days"], now)
                    changed = True
            keep.append(it)
        self.items = keep
        if changed:
            self.save()
        return fired
