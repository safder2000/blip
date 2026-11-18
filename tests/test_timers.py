import datetime as dt
import tempfile
import unittest
from pathlib import Path

from blip.timers import Timers, fmt_clock, next_fire, parse_clock, parse_duration


class ParseTests(unittest.TestCase):
    def test_durations(self):
        cases = {"25": 1500, "1:30": 5400, "0:45": 2700, "90s": 90, "2h": 7200, "1h 20m": 4800,
                 "45 min": 2700, "1.5": 90, "10 seconds": 10, "1:02:03": 3723}
        for text, want in cases.items():
            self.assertEqual(parse_duration(text), want, text)
        for bad in ("", "abc", "5x", "0", "h", "1h x"):
            self.assertIsNone(parse_duration(bad), bad)

    def test_clock(self):
        self.assertEqual(parse_clock("9:30"), (9, 30))
        self.assertEqual(parse_clock("17.45"), (17, 45))
        self.assertEqual(parse_clock("5pm"), (17, 0))
        self.assertEqual(parse_clock("12am"), (0, 0))
        self.assertEqual(parse_clock("12:15 pm"), (12, 15))
        self.assertEqual(parse_clock("930"), (9, 30))
        for bad in ("25:00", "9:75", "13pm", "noon"):
            self.assertIsNone(parse_clock(bad), bad)

    def test_fmt(self):
        self.assertEqual(fmt_clock(65), "01:05")
        self.assertEqual(fmt_clock(3725), "1:02:05")
        self.assertEqual(fmt_clock(0.2), "00:01")
        self.assertEqual(fmt_clock(59.9, up=False), "00:59")


class NextFireTests(unittest.TestCase):
    def test_later_today_and_tomorrow(self):
        now = dt.datetime(2026, 11, 18, 10, 0).timestamp()          # Wednesday
        self.assertEqual(dt.datetime.fromtimestamp(next_fire(11, 0, [], now)), dt.datetime(2026, 11, 18, 11, 0))
        self.assertEqual(dt.datetime.fromtimestamp(next_fire(9, 0, [], now)), dt.datetime(2026, 11, 19, 9, 0))

    def test_weekdays_skip_weekend(self):
        fri = dt.datetime(2026, 11, 20, 18, 0).timestamp()
        nxt = dt.datetime.fromtimestamp(next_fire(9, 0, [0, 1, 2, 3, 4], fri))
        self.assertEqual(nxt, dt.datetime(2026, 11, 23, 9, 0))   # Monday


class TimersTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.path = Path(self.tmp.name) / "t.json"

    def tearDown(self):
        self.tmp.cleanup()

    def test_timer_fires_once_and_persists(self):
        t = Timers(self.path)
        it = t.add_timer(60, "tea", now=1000)
        self.assertEqual(t.display(now=1010)[1], "00:50")
        self.assertEqual(Timers(self.path).get(it["id"])["label"], "tea")
        self.assertEqual(t.tick(now=1059), [])
        fired = t.tick(now=1060)
        self.assertEqual([f["id"] for f in fired], [it["id"]])
        self.assertEqual(t.items, [])
        self.assertIsNone(t.display(now=1061))

    def test_pause_resume(self):
        t = Timers(self.path)
        it = t.add_timer(100, now=0)
        t.toggle_pause(it["id"], now=30)
        self.assertEqual(t.tick(now=500), [])
        self.assertEqual(t.remaining(t.get(it["id"]), now=500), 70)
        t.toggle_pause(it["id"], now=500)
        self.assertEqual(t.tick(now=569), [])
        self.assertEqual(len(t.tick(now=570)), 1)

    def test_stopwatch(self):
        t = Timers(self.path)
        sw = t.add_stopwatch(now=0)
        self.assertEqual(t.display(now=75.5)[:2], ("stopwatch", "01:15"))
        t.toggle_pause(sw["id"], now=100)
        self.assertEqual(t.elapsed(t.get(sw["id"]), now=900), 100)
        t.reset(sw["id"], now=900)
        self.assertEqual(t.elapsed(t.get(sw["id"]), now=900), 0)

    def test_alarm_once_and_repeat_and_snooze(self):
        t = Timers(self.path)
        now = dt.datetime(2026, 11, 18, 8, 0).timestamp()
        once = t.add_alarm(9, 0, [], "standup", now=now)
        daily = t.add_alarm(9, 0, list(range(7)), now=now)
        at9 = dt.datetime(2026, 11, 18, 9, 0).timestamp()
        fired = {f["id"] for f in t.tick(now=at9)}
        self.assertEqual(fired, {once["id"], daily["id"]})
        self.assertFalse(t.get(once["id"])["enabled"])
        self.assertEqual(dt.datetime.fromtimestamp(t.get(daily["id"])["next"]), dt.datetime(2026, 11, 19, 9, 0))
        t.snooze(once["id"], 5, now=at9)
        self.assertEqual(t.tick(now=at9 + 299), [])
        self.assertEqual([f["id"] for f in t.tick(now=at9 + 300)], [once["id"]])
        self.assertEqual(t.tick(now=at9 + 400), [])

    def test_timer_snooze_makes_new_timer(self):
        t = Timers(self.path)
        it = t.add_timer(10, "pasta", now=0)
        t.tick(now=10)
        t.snooze(it["id"], 5, now=10)
        self.assertEqual(len(t.items), 1)
        self.assertEqual(t.items[0]["label"], "")   # original was removed when it fired
        self.assertEqual(t.remaining(t.items[0], now=10), 300)


if __name__ == "__main__":
    unittest.main()
