import hashlib
import http.server
import tempfile
import threading
import unittest
from pathlib import Path
from unittest import mock

from blip import basecamp, updater
from blip.config import Config
from blip.sprites import FONT, ICON, PALETTES, build_egg, build_pet, text_cells


class SpriteTests(unittest.TestCase):
    def test_all_moods_build(self):
        for stage in ("baby", "child", "adult"):
            for pal in PALETTES:
                for mood in ("normal", "happy", "sad", "eat", "sleep", "sick", "held", "alert"):
                    for f in range(4):
                        g = build_pet(stage, pal, mood, f, facing=f % 3 - 1, squash=f % 2 == 0)
                        self.assertTrue(g)
        self.assertTrue(build_egg(3, 0.9))

    def test_font_covers_ui_text(self):
        for ch in "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 :.!?-+/'(),%#<>&_=@":
            self.assertIn(ch, FONT)
        for v in ICON.values():
            self.assertIn(v, FONT)
        self.assertTrue(text_cells("Time's up!"))


class FakeBC:
    """Stands in for basecamp.api(): records calls and returns canned responses."""

    def __init__(self):
        self.calls = []

    def __call__(self, method, url, token=None, form=None, **kw):
        self.calls.append((method, url, token, form))
        if url == basecamp.API_AUTH:
            return 200, {"identity": {}, "accounts": [
                {"product": "bcx", "href": "https://old", "name": "Old"},
                {"product": "bc3", "href": "https://3.basecampapi.com/42", "name": "Acme"}]}
        if "/my/assignments/due.json" in url:
            scope = url.split("scope=")[1]
            return 200, [{"id": 7 if scope == "overdue" else 8, "content": f"{scope} thing", "type": "todo",
                          "due_on": "2026-11-18", "bucket": {"id": 1, "name": "Proj"}, "app_url": "u"}]
        if url.endswith("/authorization/token"):
            return 200, {"access_token": "new", "refresh_token": "r2", "expires_in": 1209600}
        if url.endswith("/completion.json"):
            return 204, None
        raise AssertionError(url)


class BasecampTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.cfg = Config(Path(self.tmp.name) / "c.json")
        self.ran = []
        self.bc = basecamp.Basecamp(self.cfg, lambda fn, *a: self.ran.append((fn, a)) or fn(*a))
        self.fake = FakeBC()
        self.p = mock.patch.object(basecamp, "api", self.fake)
        self.p.start()

    def tearDown(self):
        self.p.stop()
        self.tmp.cleanup()

    def test_connect_with_token_then_fetch_and_complete(self):
        done = threading.Event()
        got = []
        self.bc.on_items = lambda items, err: (got.append((items, err)), done.set())
        self.bc.connect_token(" pat-123 ", lambda ok, msg, accts: None)
        self.assertTrue(done.wait(5))
        self.assertEqual(self.cfg["bc_account"], "https://3.basecampapi.com/42")
        self.assertEqual(self.cfg["bc_token"], "pat-123")
        items, err = got[-1]
        self.assertIsNone(err)
        self.assertEqual([(i["id"], i["scope"], i["project"]) for i in items],
                         [(7, "overdue", "Proj"), (8, "due_today", "Proj")])
        ev = threading.Event()
        self.bc.complete(items[0], lambda ok, e: ev.set())
        self.assertTrue(ev.wait(5))
        self.assertEqual(self.fake.calls[-1][:3], ("POST", "https://3.basecampapi.com/42/todos/7/completion.json",
                                                   "pat-123"))
        self.assertEqual([i["id"] for i in self.bc.items], [8])

    def test_token_refresh_near_expiry(self):
        import time
        self.cfg.update(bc_token="old", bc_refresh="r1", bc_expires=time.time() + 3600,
                        bc_account="https://3.basecampapi.com/42", bc_client_id="cid", bc_client_secret="sec")
        self.bc.fetch_due()
        method, url, _, form = self.fake.calls[0]
        self.assertTrue(url.endswith("/authorization/token"))
        self.assertEqual(form["grant_type"], "refresh_token")
        self.assertEqual(self.cfg["bc_token"], "new")
        self.assertEqual(self.fake.calls[1][2], "new")


class Files(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a):
        pass


class UpdaterTests(unittest.TestCase):
    def test_versions(self):
        self.assertTrue(updater.is_newer("0.2.0", "0.1.9"))
        self.assertTrue(updater.is_newer("v1.0", "0.9.9"))
        self.assertFalse(updater.is_newer("0.1.0", "0.1.0"))
        self.assertTrue(updater.is_newer("0.1.0", "0.0.0-dev"))

    def test_pick_assets(self):
        rel = {"assets": [{"name": "notes.txt"}, {"name": "BlipSetup-1.0.0.exe.sha256"},
                          {"name": "BlipSetup-1.0.0.exe"}]}
        exe, sha = updater.pick_assets(rel)
        self.assertEqual(exe["name"], "BlipSetup-1.0.0.exe")
        self.assertEqual(sha["name"], "BlipSetup-1.0.0.exe.sha256")

    def test_download_checks_hash(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / "BlipSetup-9.exe").write_bytes(b"fake installer")
            good = hashlib.sha256(b"fake installer").hexdigest()
            (d / "good.sha256").write_text(f"{good}  BlipSetup-9.exe")
            (d / "bad.sha256").write_text("0" * 64)
            srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0),
                                                  lambda *a: Files(*a, directory=str(d)))
            threading.Thread(target=srv.serve_forever, daemon=True).start()
            base = f"http://127.0.0.1:{srv.server_port}"
            try:
                info = {"exe_url": f"{base}/BlipSetup-9.exe", "exe_name": "BlipSetup-9.exe",
                        "sha_url": f"{base}/good.sha256"}
                out = updater.download(info, d / "out")
                self.assertEqual(out.read_bytes(), b"fake installer")
                info["sha_url"] = f"{base}/bad.sha256"
                with self.assertRaises(ValueError):
                    updater.download(info, d / "out2")
            finally:
                srv.shutdown()


if __name__ == "__main__":
    unittest.main()
