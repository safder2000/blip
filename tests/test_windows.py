"""Open every window in its empty and its busy state, so a layout bug can't crash one."""
import tempfile
import time
import unittest

from blip.app import App

APP = None


def setUpModule():
    global APP
    APP = App(tempfile.mkdtemp(), start_services=False)


def tearDownModule():
    APP.root.destroy()


class WindowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = APP

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
        labels = [r[2].get("label") for r in a.menu.rows]
        self.assertNotIn("Timers & alarms", labels)      # lives inside the Timer flyout now
        a.menu.close()

    def test_menu_lists_people_with_buttons(self):
        a = self.app
        calls = []

        class FakeNet:
            def online(self):
                return [{"id": "p1", "name": "Ravi", "species": "kitty", "palette": "sky", "stage": "adult"},
                        {"id": "p2", "name": "Asha", "status": "busy", "species": "bot"}]
        a.net = FakeNet()
        a.call = lambda pid: calls.append(("call", pid))
        a.compose = lambda pid, **k: calls.append(("msg", pid))
        try:
            class E:
                x_root, y_root = 400, 300
            a.on_menu(E())
            a.root.update()
            m = a.menu
            people = [r for r in m.rows if r[2].get("person")]
            self.assertEqual([r[2]["label"] for r in people], ["Ravi", "Asha"])     # busy people last
            y0, y1, it = people[0]
            bx0, by0, bx1, by1, i = next(b for b in m.person_buttons(y0, y1, it) if it["buttons"][b[4]][0] == "phone")
            key, obj = m.hit((bx0 + bx1) / 2, (y0 + y1) / 2)
            self.assertEqual(key[0], "btn")

            class C:
                x, y = (bx0 + bx1) / 2, (y0 + y1) / 2
            m.on_click(C())
            for _ in range(5):
                a.root.update()
                time.sleep(0.01)
            self.assertEqual(calls, [("call", "p1")])
        finally:
            a.net = None
            if a.menu:
                a.menu.close()

    def test_both_themes_and_live_switch(self):
        a = self.app
        try:
            for theme in ("light", "dark", "system"):
                a.apply_theme(theme)
                self.open_all()

                class E:
                    x_root, y_root = 400, 400
                a.on_menu(E())
                a.root.update()
                a.menu.close()
            w = a.open_window("settings", "general")
            a.root.update()
            a.apply_theme("light")          # rebuilds the open window
            a.root.update()
            self.assertTrue(a.windows["settings"].alive())
            self.assertEqual(a.windows["settings"].page, "general")
            self.assertEqual(a.cfg["theme"], "light")
            a.windows["settings"].close()
        finally:
            a.apply_theme("dark")

    def test_update_status_inline(self):
        a = self.app
        w = a.open_window("settings", "general")
        a.root.update()
        a.update_state = ("checking", "Checking for updates")
        w.refresh()
        self.assertFalse(w.upd_btn.enabled)
        a.update_state = None
        a.on_update_info(None, None, manual=True, quiet=True)
        w.refresh()
        self.assertTrue(w.upd_btn.enabled)
        self.assertIn("up to date", w.upd_label.cget("text"))
        self.assertIn("just now", w.upd_label.cget("text"))
        self.assertFalse(a.cards.cards if hasattr(a.cards, "cards") else [])   # no pop-up card
        a.on_update_info(None, "offline", manual=True, quiet=True)
        w.refresh()
        self.assertIn("Couldn't", w.upd_label.cget("text"))
        w.close()


class PhysicsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = APP

    def run_until_landed(self, a, steps=2000):
        seen = {"chute": False, "bounces": 0}
        for _ in range(steps):
            lo, hi = a.bounds()
            a.fly(a.ground(), lo, hi)
            seen["chute"] |= bool(a.chute)
            seen["bounces"] = max(seen["bounces"], a.bounces)
            if not a.airborne:
                return seen
        self.fail("never landed")

    def throw(self, a, y, vx, vy):
        a.st["stage"] = "adult"
        a.y, a.vx, a.vy = y, vx, vy
        a.airborne, a.chute, a.bounces = True, 0, 0
        a.set_action("fall", 1e18)

    def test_short_hard_throw_bounces(self):
        a = self.app
        g = a.ground()
        self.throw(a, g - 100 * a.S, 6 * a.S, 18 * a.S)
        seen = self.run_until_landed(a)
        self.assertFalse(seen["chute"])
        self.assertGreaterEqual(seen["bounces"], 1)
        self.assertEqual(a.y, g)
        self.assertEqual(a.action, "idle")

    def test_high_drop_opens_parachute_and_lands_softly(self):
        a = self.app
        g = a.ground()
        top = a.work[1]
        self.throw(a, max(top, g - 600 * a.S), 0, 0)
        seen = self.run_until_landed(a)
        self.assertTrue(seen["chute"])
        self.assertEqual(seen["bounces"], 0)
        self.assertEqual(a.chute, 0)

    def test_parachute_draws(self):
        a = self.app
        self.throw(a, a.ground() - 400 * a.S, 0, 0)
        a.chute = 5
        a.draw()
        a.root.update()
        a.chute = 0
        a.set_action("idle", 1)

    def test_throw_speed_from_mouse_trail(self):
        a = self.app
        now = time.monotonic()
        a.trail = [(now - 0.05, 100, 500), (now - 0.01, 140, 480)]
        vx, vy = a.throw_speed()
        self.assertGreater(vx, 0)
        self.assertLess(vy, 0)
        a.trail = [(now - 0.5, 100, 500), (now - 0.4, 300, 500)]   # mouse stopped before letting go
        self.assertEqual(a.throw_speed(), (0.0, 0.0))


if __name__ == "__main__":
    unittest.main()
