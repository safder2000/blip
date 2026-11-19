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

if "--selftest-opus" in sys.argv:           # CI: libopus is bundled and works
    try:
        from blip import opus  # noqa: E402
        pkt = opus.Encoder().encode(bytes(640))
        ok = len(opus.Decoder().decode(pkt)) == 640
    except Exception as e:                  # no dialogs in a windowed build: just the exit code
        ok, pkt = False, str(e)
    print("opus ok" if ok else f"opus FAILED: {pkt}")
    sys.exit(0 if ok else 1)

from blip.app import main  # noqa: E402

main()
