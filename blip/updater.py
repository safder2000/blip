"""Self-update from GitHub Releases.

A release carries BlipSetup-<ver>.exe and BlipSetup-<ver>.exe.sha256. When a newer
version is out, the installed app downloads the setup, checks the SHA-256, runs it
silently and quits; the installer starts the new Blip when it is done.
"""
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import threading
import urllib.request
from pathlib import Path

REPO = "safder2000/blip"
API = f"https://api.github.com/repos/{REPO}/releases/latest"
UA = "Blip-updater (https://github.com/safder2000/blip)"


def parse_version(s):
    nums = [int(x) for x in re.findall(r"\d+", s or "")[:3]]
    return tuple(nums + [0] * (3 - len(nums)))


def is_newer(remote, local):
    return parse_version(remote) > parse_version(local)


def _get(url, accept="application/vnd.github+json", timeout=30):
    req = urllib.request.Request(url, headers={"User-Agent": UA, "Accept": accept})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()


def pick_assets(release):
    exe = sha = None
    for a in release.get("assets", []):
        n = a.get("name", "")
        if n.lower().startswith("blipsetup") and n.lower().endswith(".exe"):
            exe = a
        elif n.lower().endswith(".exe.sha256"):
            sha = a
    return exe, sha


def latest():
    """{'version', 'exe_url', 'exe_name', 'sha_url', 'notes', 'page'} or None."""
    rel = json.loads(_get(API))
    exe, sha = pick_assets(rel)
    if not exe:
        return None
    return {"version": rel.get("tag_name", "").lstrip("v"), "exe_url": exe["browser_download_url"],
            "exe_name": exe["name"], "sha_url": sha["browser_download_url"] if sha else None,
            "notes": rel.get("body") or "", "page": rel.get("html_url", "")}


def download(info, folder=None):
    folder = Path(folder or tempfile.gettempdir()) / "blip-update"
    folder.mkdir(parents=True, exist_ok=True)
    path = folder / info["exe_name"]
    data = _get(info["exe_url"], accept="application/octet-stream", timeout=300)
    if not info.get("sha_url"):
        raise ValueError("release has no checksum file")
    want = _get(info["sha_url"], accept="application/octet-stream").decode().split()[0].lower()
    got = hashlib.sha256(data).hexdigest()
    if got != want:
        raise ValueError("download checksum mismatch")
    path.write_bytes(data)
    return path


def launch_installer(path):
    flags = 0
    if sys.platform == "win32":
        flags = 0x00000008 | 0x00000200   # DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
    subprocess.Popen([str(path), "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS"],
                     creationflags=flags, close_fds=True, cwd=os.path.dirname(str(path)))


class Updater:
    def __init__(self, current, post):
        self.current = current
        self.post = post
        self.busy = False
        self.available = None

    def check(self, on_result):
        """on_result(info_or_None, error_or_None) on the UI thread."""
        if self.busy:
            return
        self.busy = True

        def work():
            try:
                info = latest()
                newer = info if info and is_newer(info["version"], self.current) else None
                self.available = newer
                self.post(on_result, newer, None)
            except Exception as e:
                self.post(on_result, None, str(e))
            finally:
                self.busy = False
        threading.Thread(target=work, daemon=True).start()

    def fetch(self, info, on_ready):
        """Download + verify; on_ready(path_or_None, error_or_None) on the UI thread."""
        def work():
            try:
                self.post(on_ready, download(info), None)
            except Exception as e:
                self.post(on_ready, None, str(e))
        threading.Thread(target=work, daemon=True).start()
