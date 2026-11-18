import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

if "--selftest-https" in sys.argv:          # CI: the packaged app can do HTTPS (certificates found)
    import urllib.error  # noqa: E402
    from blip import updater  # noqa: E402
    try:
        updater._get(updater.API)
        print("https ok")
    except urllib.error.HTTPError as e:         # GitHub answered (e.g. rate limit): TLS worked
        print("https ok, GitHub said", e.code)
    sys.exit(0)

from blip.app import main  # noqa: E402

main()
