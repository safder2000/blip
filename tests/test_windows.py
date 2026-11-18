"""Open every window in its empty and its busy state, so a layout bug can't crash one."""
import tempfile
import time
import unittest

from blip.app import App


class WindowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp()
        cls.app = App(cls.tmp, start_services=False)

    @classmethod
    def tearDownClass(cls):
        cls.app.root.destroy()

    def open_all(self):
        a = self.app
        for name in ("timers", "buddies", "tasks", "welcome", "settings"):
            w = a.open_window(name)
            a.root.update()
            if name == "settings":
                for page in ("pet", "general", "office", "basecamp"):
                    w.show(page)
                    w.refresh()
                    a.root.update()
            w.refresh()
            w.close()

    def test_empty(self):
        self.app.bc.items = []
        self.open_all()

    def test_busy(self):
        a = self.app
        a.timers.add_timer(60, "t")
        a.timers.add_stopwatch()
        a.timers.add_alarm(9, 0, [0, 1])
        a.inbox.append({"ts": time.time(), "dir": "in", "peer": "x", "name": "Ravi", "text": "hi"})
        a.cfg.update(bc_token="t", bc_account="https://x", bc_account_name="Acme")
        a.bc.last_fetch = time.time()
        a.bc.items = [{"id": 1, "type": "todo", "content": "Thing", "due_on": None, "project": "P",
                       "scope": "overdue", "url": "u"}]
        self.open_all()

    def test_menu(self):
        a = self.app

        class E:
            x_root, y_root = 400, 400
        a.on_menu(E())
        a.root.update()
        self.assertIsNotNone(a.menu)
        a.menu.open_sub(next(r[2] for r in a.menu.rows if r[2].get("label") == "Timer"))
        a.root.update()
        a.menu.close()


if __name__ == "__main__":
    unittest.main()
