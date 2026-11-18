"""Write blip/_build.py for a release build: version + optional Basecamp integration creds
(taken from env vars BC_CLIENT_ID / BC_CLIENT_SECRET, i.e. GitHub repository secrets)."""
import os
import sys
from pathlib import Path

version = sys.argv[1].lstrip("v")
out = Path(__file__).resolve().parent.parent / "blip" / "_build.py"
out.write_text(
    f"VERSION = {version!r}\n"
    f"BC_CLIENT_ID = {os.environ.get('BC_CLIENT_ID', '')!r}\n"
    f"BC_CLIENT_SECRET = {os.environ.get('BC_CLIENT_SECRET', '')!r}\n",
    encoding="utf-8")
print("stamped", version)
