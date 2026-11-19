import datetime as dt
import tempfile
import unittest
from pathlib import Path

from blip.notes import Notes, describe_when, parse_when

NOW = dt.datetime(2026, 11, 19, 14, 0).timestamp()


def at(d, h, m):
    return dt.datetime(2026, 11, d, h, m).timestamp()


class ParseWhenTests(unittest.TestCase):
    def test_forms(self):
        self.assertEqual(parse_when("in 20m", NOW), NOW + 1200)
        self.assertEqual(parse_when("45", NOW), NOW + 2700)
        self.assertEqual(parse_when("1h 30m", NOW), NOW + 5400)
        self.assertEqual(parse_when("5pm", NOW), at(19, 17, 0))
        self.assertEqual(parse_when("17:30", NOW), at(19, 17, 30))
        self.assertEqual(parse_when("at 9", NOW), at(20, 9, 0))        # 9:00 already passed today
        self.assertEqual(parse_when("tomorrow", NOW), at(20, 9, 0))
        self.assertEqual(parse_when("tomorrow 10", NOW), at(20, 10, 0))
        self.assertEqual(parse_when("today 4pm", NOW), at(19, 16, 0))
        for bad in ("", "banana", "today 9am", "25:99"):
            self.assertIsNone(parse_when(bad, NOW), bad)

    def test_describe(self):
        self.assertEqual(describe_when(NOW + 1200, NOW), "in 20 min")
        self.assertEqual(describe_when(at(19, 17, 30), NOW), "today 17:30")
        self.assertEqual(describe_when(at(20, 9, 0), NOW), "tomorrow 09:00")


class NotesTests(unittest.TestCase):
    def test_reminders_fire_once_snooze_and_persist(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "notes.json"
            n = Notes(path)
            plain = n.add("buy milk")
            r = n.add("call the client", NOW + 600)
            self.assertEqual([x["id"] for x in n.ordered()], [r["id"], plain["id"]])
            self.assertEqual(n.due(NOW), [])
            fired = n.due(NOW + 601)
            self.assertEqual([x["id"] for x in fired], [r["id"]])
            self.assertEqual(n.due(NOW + 700), [])                    # only once
            n.snooze(r["id"], 10, now=NOW + 700)
            self.assertEqual([x["id"] for x in n.due(NOW + 1301)], [r["id"]])
            n.complete(plain["id"])
            again = Notes(path)                                        # saved to disk
            self.assertEqual(len(again.items), 2)
            self.assertTrue(again.get(plain["id"])["done"])
            again.clear_done()
            self.assertEqual([x["text"] for x in again.items], ["call the client"])
            again.set_reminder(r["id"], None)
            self.assertEqual(again.pending(), [])


if __name__ == "__main__":
    unittest.main()
