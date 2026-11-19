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
        for name in ("timers", "buddies", "tasks", "welcome", "settings", "notes"):
            w = a.open_window(name)
            a.root.update()
            if name == "settings":
                for page in ("pet", "general", "menu", "office", "basecamp"):
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
        a.notes.add("plain note")
        a.notes.add("with a reminder", time.time() + 3600)
        done = a.notes.add("finished")
        a.notes.complete(done["id"])
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
        labels = [r[2].get("label") for r in a.menu.rows]
        self.assertNotIn("Timers & alarms", labels)      # lives inside the Timer flyout now
        self.assertIn("Notes", labels)
        self.assertNotIn("Remind me…", labels)
        self.assertNotIn("Wander around", labels)              # off by default: less clutter
        a.menu.close()

    def test_menu_sections_can_be_hidden(self):
        a = self.app

        class E:
            x_root, y_root = 400, 300
        try:
            for key in ("care", "notes", "focus", "office", "basecamp", "dnd"):
                a.cfg.set_menu(key, False)
            a.on_menu(E())
            a.root.update()
            m = a.menu
            self.assertEqual(m.actions, [])
            self.assertEqual([r[2].get("label") for r in m.rows if r[2].get("label")], ["Settings", "Quit Blip"])
            m.close()
            a.cfg.set_menu("wander", True)
            a.on_menu(E())
            a.root.update()
            self.assertIn("Wander around", [r[2].get("label") for r in a.menu.rows])
            a.menu.close()
        finally:
            a.cfg["menu"] = {}

    def test_office_link_opens_buddies(self):
        a = self.app

        class E:
            x_root, y_root = 400, 300
        a.on_menu(E())
        a.root.update()
        m = a.menu
        y0, y1, it = next(r for r in m.rows if r[2].get("section") == "Office")
        x0, ly0, x1, ly1 = m.link_rect(y0, y1, it)

        class C:
            x, y = (x0 + x1) / 2, (ly0 + ly1) / 2
        self.assertEqual(m.hit(C.x, C.y)[0][0], "link")
        m.on_click(C())
        for _ in range(5):
            a.root.update()
            time.sleep(0.01)
        self.assertTrue(a.windows["buddies"].alive())
        a.windows["buddies"].close()

    def test_mac_menu_stays_open_without_focus_until_pointer_leaves(self):
        from unittest import mock
        from blip import menu as M
        a = self.app

        class E:
            x_root, y_root = 400, 300
        a.on_menu(E())
        a.root.update()
        m = a.menu
        m.t0 -= 5                                  # well past the old 1.5 s cut-off
        inside = (m.win.winfo_rootx() + 20, m.win.winfo_rooty() + 20)
        with mock.patch.object(M.sys, "platform", "darwin"),                 mock.patch.object(a.root, "focus_get", return_value=None),                 mock.patch.object(m.win, "focus_force"):
            with mock.patch.object(m.win, "winfo_pointerxy", return_value=inside):
                m._watch()
                self.assertFalse(m.closed)         # no focus on a Mac is normal: stay open
            with mock.patch.object(m.win, "winfo_pointerxy", return_value=(5, 5)):
                m._watch()
                self.assertFalse(m.closed)         # just left: give it a moment
                m.away_since -= 4
                m._watch()
                self.assertTrue(m.closed)          # away for a while: close
        a.menu = None

    def test_press_and_hold_opens_menu_but_drag_does_not(self):
        from blip import app as A
        a = self.app

        class Ev:
            x_root, y_root = int(a.x + 100), int(a.y + 120)
        if a.menu:
            a.menu.close()
        a.on_press(Ev())
        end = time.time() + A.HOLD_MS / 1000 + 0.4
        while time.time() < end:
            a.root.update()
            time.sleep(0.02)
        self.assertIsNotNone(a.menu)
        a.on_release(Ev())                     # letting go doesn't pat or close it
        self.assertIsNotNone(a.menu)
        a.menu.close()
        a.on_press(Ev())                       # press, then move: that's a drag, no menu

        class Moved:
            x_root, y_root = Ev.x_root + 40, Ev.y_root - 30
        a.on_drag(Moved())
        end = time.time() + A.HOLD_MS / 1000 + 0.4
        while time.time() < end:
            a.root.update()
            time.sleep(0.02)
        self.assertIsNone(a.menu)
        a.on_release(Moved())

    def test_mac_wheel_steps(self):
        from blip.kit import wheel_steps
        self.assertEqual(wheel_steps(-3, mac=True), 3)      # Mac: small numbers, used as they are
        self.assertEqual(wheel_steps(2, mac=True), -2)
        self.assertEqual(wheel_steps(-40, mac=True), 8)     # fast flick, capped
        self.assertEqual(wheel_steps(-120, mac=False), 1)   # Windows: one notch
        self.assertEqual(wheel_steps(240, mac=False), -2)
        self.assertEqual(wheel_steps(-30, mac=False), 1)    # precision touchpads send less than 120

    def test_menu_tools_row(self):
        a = self.app
        ran = []
        a.start_stopwatch = lambda *x: ran.append("sw")

        class E:
            x_root, y_root = 400, 300
        a.on_menu(E())
        a.root.update()
        m = a.menu
        y0, y1, it = next(r for r in m.rows if r[2].get("tools"))
        rects = m.tool_rects(y0, y1, it)
        self.assertEqual([t["label"] for t in it["tools"]], ["Timer", "Alarm", "Stopwatch"])

        class C:
            pass
        x0, ty0, x1, ty1, i = rects[0]
        C.x, C.y = (x0 + x1) / 2, (ty0 + ty1) / 2
        m.on_click(C())                                  # Timer opens its flyout
        a.root.update()
        self.assertIsNotNone(m.child)
        x0, ty0, x1, ty1, i = rects[2]
        C.x, C.y = (x0 + x1) / 2, (ty0 + ty1) / 2
        m.on_click(C())                                  # Stopwatch runs
        for _ in range(5):
            a.root.update()
            time.sleep(0.01)
        self.assertEqual(ran, ["sw"])
        del a.start_stopwatch

    def test_reminder_rings_and_done_clears(self):
        a = self.app
        n = a.notes.add("stand-up call", time.time() - 1)
        for x in a.notes.due():
            a.reminder_fired(x)
        key = f"note:{n['id']}"
        self.assertIn(key, a.alerts)
        a.cards.close(key)
        a.root.update()
        self.assertNotIn(key, a.alerts)
        a.notes.delete(n["id"])

    def test_menu_lists_people_with_buttons(self):
        a = self.app
        calls = []

        class FakeNet:
            def online(self):
                return [{"id": "p1", "name": "Ravi", "species": "kitty", "palette": "sky", "stage": "adult",
                         "via": "server"},
                        {"id": "p2", "name": "Asha", "status": "busy", "species": "bot"},
                        {"id": "p3", "name": "Egg", "stage": "egg", "palette": "baby"},          # just hatched
                        {"id": "p4", "name": "Odd", "stage": "titan", "species": "dragon", "palette": 7}]
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
            self.assertEqual([r[2]["label"] for r in people], ["Egg", "Odd", "Ravi", "Asha"])   # busy last
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
            self.assertEqual(calls, [("call", "p3")])
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
            self.assertTrue(a.cfg["theme_chosen"])
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


class ThemeDefaultTests(unittest.TestCase):
    def test_new_and_old_installs_follow_the_system(self):
        import json
        from pathlib import Path
        from blip.config import Config
        d = Path(tempfile.mkdtemp())
        self.assertEqual(Config(d / "new.json")["theme"], "system")
        (d / "old.json").write_text(json.dumps({"cfg_version": 2, "theme": "dark"}))
        self.assertEqual(Config(d / "old.json")["theme"], "system")         # v0.6.0 default, never picked
        (d / "picked.json").write_text(json.dumps({"cfg_version": 2, "theme": "light", "theme_chosen": True}))
        self.assertEqual(Config(d / "picked.json")["theme"], "light")

    def test_old_made_up_team_codes_are_dropped(self):
        import json
        from pathlib import Path
        from blip.config import Config
        d = Path(tempfile.mkdtemp())
        (d / "c.json").write_text(json.dumps({"cfg_version": 4, "server": "https://blip.example.com",
                                              "team": "c0ffee1234"}))
        c = Config(d / "c.json")
        self.assertEqual(c["team"], "")                 # now on the server's shared team
        self.assertEqual(c["server"], "https://blip.example.com")

    def test_follows_os_flip(self):
        from unittest import mock
        from blip import kit, plat
        a = APP
        a.cfg["theme"] = "system"
        with mock.patch.object(plat, "system_dark", return_value=False):
            a.follow_system_theme()
        self.assertEqual(kit.THEME, "light")
        with mock.patch.object(plat, "system_dark", return_value=True):
            a.follow_system_theme()
        self.assertEqual(kit.THEME, "dark")
        self.assertFalse(a.cfg.get("theme_chosen") and a.cfg["theme"] != "system")


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

    def throw(self, a, y, vx, vy, ball=False, lucky=False):
        a.st["stage"] = "adult"
        a.y, a.vx, a.vy = y, vx, vy
        a.airborne, a.chute, a.bounces = True, 0, 0
        a.ball, a.lucky = ball, lucky
        a.set_action("fall", 1e18)

    def test_short_drop_bounces(self):
        a = self.app
        g = a.ground()
        self.throw(a, g - 100 * a.S, 2 * a.S, 18 * a.S)
        seen = self.run_until_landed(a)
        self.assertFalse(seen["chute"])
        self.assertGreaterEqual(seen["bounces"], 1)
        self.assertEqual(a.y, g)
        self.assertEqual(a.action, "idle")

    def test_high_drop_no_parachute_unless_lucky(self):
        a = self.app
        g = a.ground()
        self.throw(a, max(a.work[1], g - 600 * a.S), 0, 0)
        self.assertFalse(self.run_until_landed(a)["chute"])

    def test_lucky_high_drop_opens_parachute_and_lands_softly(self):
        a = self.app
        g = a.ground()
        self.throw(a, max(a.work[1], g - 600 * a.S), 0, 0, lucky=True)
        seen = self.run_until_landed(a)
        self.assertTrue(seen["chute"])
        self.assertEqual(seen["bounces"], 0)
        self.assertEqual(a.chute, 0)

    def test_fast_throw_becomes_a_ball_ricochets_and_unfolds(self):
        a = self.app
        lo, hi = a.bounds()
        a.x = (lo + hi) / 2
        self.throw(a, a.ground() - 200 * a.S, 38 * a.S, -30 * a.S, ball=True)
        walls = 0
        for i in range(4000):
            lo, hi = a.bounds()
            before = a.vx
            a.fly(a.ground(), lo, hi)
            walls += (before > 0) != (a.vx > 0) and a.vx != 0
            if i % 7 == 0:
                a.draw()                      # spinning ball + trail renders
            if not a.ball:
                break
        self.assertFalse(a.ball)
        self.assertGreaterEqual(walls, 1)     # hit a screen edge
        self.assertGreaterEqual(a.bounces, 2)
        self.assertEqual(a.action, "unfold")
        self.assertEqual(a.y, a.ground())

    def test_release_speed_picks_ball_or_drop(self):
        a = self.app
        now = time.monotonic()

        class E:
            x_root, y_root = 500, 300
        for trail, want in (([(now - 0.05, 100, 300), (now - 0.005, 600, 200)], True),
                            ([(now - 0.05, 100, 300), (now - 0.005, 104, 302)], False)):
            a.drag = [0, 0, 0, 0, True]
            a.set_action("held", 1e18)
            a.trail = trail
            a.on_release(E())
            self.assertEqual(a.ball, want)
        a.ball = False
        a.set_action("idle", 1)

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
