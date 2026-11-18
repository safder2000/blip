"""Paths, settings and small JSON persistence helpers."""
import json
import os
import shutil
import sys
import uuid
from pathlib import Path


def data_dir():
    override = os.environ.get("BLIP_DATA")
    if override:
        d = Path(override)
    else:
        if sys.platform == "darwin":
            d = Path.home() / "Library" / "Application Support" / "Blip"
        else:
            d = Path(os.environ.get("APPDATA") or Path.home()) / "Blip"
        old = d.parent / "pcpet"
        if not d.exists() and (old / "state.json").exists():   # keep a pet from the first prototype
            d.mkdir(parents=True, exist_ok=True)
            shutil.copy2(old / "state.json", d / "state.json")
    d.mkdir(parents=True, exist_ok=True)
    return d


def read_json(path, default):
    try:
        return json.loads(Path(path).read_text("utf-8"))
    except Exception:
        return default


def write_json(path, data):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(data, indent=1), "utf-8")
    os.replace(tmp, path)


def _default_name():
    from .plat import user_name
    return user_name()[:20] or "Someone"


DEFAULTS = {
    "display_name": None,          # filled with the Windows user name
    "node_id": None,               # random id for this PC on the office network
    "passphrase": "blip-office",   # pets only see pets with the same passphrase
    "network": True,
    "dnd": False,
    "relay": "",                   # optional "host:port" of a relay (blip.relay) when broadcast is blocked
    "manual_peers": "",            # optional comma-separated IPs to say hello to directly
    "sound": False,                # quiet by default; Settings > General > Sounds
    "volume": 60,
    "auto_update": True,
    "theme": "dark",               # dark | light | system
    "last_update_check": 0,
    "bc_token": "",
    "bc_refresh": "",
    "bc_expires": 0,
    "bc_client_id": "",
    "bc_client_secret": "",
    "bc_account": "",              # API base href, e.g. https://3.basecampapi.com/123
    "bc_account_name": "",
    "bc_morning": "09:30",
    "bc_enabled": True,
}


class Config(dict):
    def __init__(self, path):
        super().__init__(DEFAULTS)
        self.path = Path(path)
        self.update(read_json(self.path, {}))
        changed = False
        if not self.get("display_name"):
            self["display_name"] = _default_name()
            changed = True
        if not self.get("node_id"):
            self["node_id"] = uuid.uuid4().hex
            changed = True
        if self.get("cfg_version", 1) < 2:   # v0.4: sounds became opt-in, for existing installs too
            self["sound"] = False
            self["cfg_version"] = 2
            changed = True
        if changed:
            self.save()

    def save(self):
        write_json(self.path, dict(self))


def frozen():
    return bool(getattr(sys, "frozen", False))
