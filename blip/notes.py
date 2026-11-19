"""Notes with optional reminders (model only, no UI)."""
import datetime as dt
import re
import time
import uuid

from .config import read_json, write_json
from .timers import parse_clock, parse_duration


def parse_when(text, now=None):
    """When a reminder should go off, from what people type. Timestamp or None.

    'in 20m', '20m', '1h 30m', '45' (minutes)       -> from now
    '5pm', '17:30', 'at 9', '9.15am'                 -> next time the clock shows it
    'tomorrow', 'tomorrow 10', 'today 4pm'           -> that day (tomorrow alone = 9:00)
    """
    now = time.time() if now is None else now
    t = (text or "").strip().lower()
    if not t:
        return None
    base = dt.datetime.fromtimestamp(now)
    m = re.fullmatch(r"(today|tomorrow|tmrw|tmr)\s*(?:at\s*)?(.*)", t)
    if m:
        day = base + dt.timedelta(days=0 if m[1] == "today" else 1)
        hm = parse_clock(m[2]) if m[2].strip() else (9, 0)
        if not hm:
            return None
        ts = day.replace(hour=hm[0], minute=hm[1], second=0, microsecond=0).timestamp()
        return ts if ts > now else None
    if t.startswith("in "):
        secs = parse_duration(t[3:])
        return now + secs if secs else None
    clockish = t.startswith("at ") or re.search(r"(am|pm)$", t) or re.fullmatch(r"\d{1,2}[:.]\d{2}", t)
    if clockish:
        hm = parse_clock(t[3:] if t.startswith("at ") else t)
        if not hm:
            return None
        ts = base.replace(hour=hm[0], minute=hm[1], second=0, microsecond=0).timestamp()
        return ts if ts > now else ts + 86400
    secs = parse_duration(t)
    return now + secs if secs else None


def describe_when(ts, now=None):
    """'in 25 min', 'today 17:30', 'tomorrow 09:00', 'Mon 24 Nov 09:00'."""
    now = time.time() if now is None else now
    d, n = dt.datetime.fromtimestamp(ts), dt.datetime.fromtimestamp(now)
    left = ts - now
    if 0 <= left < 3600:
        return f"in {max(1, round(left / 60))} min"
    hm = d.strftime("%H:%M")
    days = (d.date() - n.date()).days
    if days == 0:
        return f"today {hm}"
    if days == 1:
        return f"tomorrow {hm}"
    if days == -1:
        return f"yesterday {hm}"
    return d.strftime("%a %d %b ") + hm


class Notes:
    def __init__(self, path):
        self.path = path
        data = read_json(path, [])
        self.items = [n for n in data if isinstance(n, dict) and n.get("id")] if isinstance(data, list) else []

    def save(self):
        write_json(self.path, self.items)

    def get(self, nid):
        return next((n for n in self.items if n["id"] == nid), None)

    def add(self, text, remind=None):
        n = {"id": uuid.uuid4().hex[:10], "text": text.strip()[:2000], "created": time.time(),
             "remind": remind, "fired": False, "done": False}
        self.items.append(n)
        self.save()
        return n

    def edit(self, nid, text):
        n = self.get(nid)
        if n and text.strip():
            n["text"] = text.strip()[:2000]
            self.save()

    def set_reminder(self, nid, ts):
        n = self.get(nid)
        if n:
            n["remind"], n["fired"] = ts, False
            if ts:
                n["done"] = False
            self.save()

    def snooze(self, nid, minutes, now=None):
        now = time.time() if now is None else now
        self.set_reminder(nid, now + minutes * 60)

    def complete(self, nid, done=True):
        n = self.get(nid)
        if n:
            n["done"] = bool(done)
            self.save()

    def delete(self, nid):
        self.items = [n for n in self.items if n["id"] != nid]
        self.save()

    def clear_done(self):
        self.items = [n for n in self.items if not n.get("done")]
        self.save()

    def due(self, now=None):
        """Reminders whose time has come; each is returned once."""
        now = time.time() if now is None else now
        out = [n for n in self.items if n.get("remind") and not n.get("fired") and not n.get("done")
               and n["remind"] <= now]
        for n in out:
            n["fired"] = True
        if out:
            self.save()
        return out

    def pending(self):
        return [n for n in self.items if n.get("remind") and not n.get("fired") and not n.get("done")]

    def ordered(self):
        """Upcoming reminders first (soonest first), then other open notes (newest first), then done."""
        def key(n):
            if n.get("done"):
                return (2, -n["created"])
            if n.get("remind") and not n.get("fired"):
                return (0, n["remind"])
            return (1, -n["created"])
        return sorted(self.items, key=key)
