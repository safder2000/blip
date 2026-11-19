"""Write blip/_build.py for a release build: version + optional Basecamp integration creds
(taken from env vars BC_CLIENT_ID / BC_CLIENT_SECRET, i.e. GitHub repository secrets).

If the two secrets were saved the wrong way round, Launchpad tells us which one is the
client ID and we swap them. Nothing secret is ever printed."""
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path


def launchpad_knows(cid):
    url = "https://launchpad.37signals.com/authorization/new?" + urllib.parse.urlencode(
        {"response_type": "code", "client_id": cid, "redirect_uri": "http://localhost:47615/callback"})
    try:
        urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "Blip build"}), timeout=20)
        return True
    except urllib.error.HTTPError as e:
        return "Client not found" not in e.read().decode(errors="replace")
    except OSError:
        return None   # can't tell (offline)


version = sys.argv[1].lstrip("v")
cid = os.environ.get("BC_CLIENT_ID", "").strip()
secret = os.environ.get("BC_CLIENT_SECRET", "").strip()
note = ""
if cid and secret:
    if launchpad_knows(cid) is False and launchpad_knows(secret) is True:
        cid, secret = secret, cid
        note = " (the two secrets were swapped; fixed)"
out = Path(__file__).resolve().parent.parent / "blip" / "_build.py"
out.write_text(
    f"VERSION = {version!r}\n"
    f"BC_CLIENT_ID = {cid!r}\n"
    f"BC_CLIENT_SECRET = {secret!r}\n",
    encoding="utf-8")
# the Rust app: workspace version, and the integration as build-time env for cargo (never printed)
cargo = Path(__file__).resolve().parent.parent / "rust" / "Cargo.toml"
if cargo.exists():
    import re
    text = cargo.read_text(encoding="utf-8")
    text = re.sub(r'(\[workspace\.package\][^\[]*?version = ")[^"]*(")', lambda m: m.group(1) + version + m.group(2), text, count=1)
    cargo.write_text(text, encoding="utf-8")
env_file = os.environ.get("GITHUB_ENV")
if env_file and cid and secret:
    with open(env_file, "a", encoding="utf-8") as f:
        f.write(f"BLIP_BC_CLIENT_ID={cid}\nBLIP_BC_CLIENT_SECRET={secret}\n")
print("stamped", version, "- Basecamp integration built in:", ("yes" + note) if (cid and secret) else "no")
