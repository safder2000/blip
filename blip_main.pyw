import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

if "--selftest-https" in sys.argv:          # CI: the packaged app can reach GitHub over HTTPS
    from blip import updater  # noqa: E402
    info = updater.latest()
    print("https ok, latest release:", info and info["version"])
    sys.exit(0)

from blip.app import main  # noqa: E402

main()
