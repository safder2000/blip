# PyInstaller spec for Blip.app (macOS). Built by .github/workflows/release.yml on an Apple Silicon runner:
#   VERSION=1.2.3 pyinstaller --noconfirm --clean packaging/blip_mac.spec
import os

ROOT = os.path.abspath(os.path.join(SPECPATH, ".."))
VERSION = os.environ.get("VERSION", "0.0.0")

OPUS = os.path.join(ROOT, "vendor", "libopus.0.dylib")      # built by the workflow
a = Analysis([os.path.join(ROOT, "blip_main.pyw")], pathex=[ROOT], hiddenimports=["sounddevice", "certifi"],
             binaries=[(OPUS, ".")] if os.path.exists(OPUS) else [])
pyz = PYZ(a.pure)
exe = EXE(pyz, a.scripts, [], exclude_binaries=True, name="Blip", console=False, argv_emulation=False)
coll = COLLECT(exe, a.binaries, a.datas, name="Blip")
app = BUNDLE(
    coll,
    name="Blip.app",
    icon=os.path.join(ROOT, "assets", "blip.icns"),
    bundle_identifier="com.safder2000.blip",
    info_plist={
        "CFBundleName": "Blip",
        "CFBundleDisplayName": "Blip",
        "CFBundleShortVersionString": VERSION,
        "CFBundleVersion": VERSION,
        "LSMinimumSystemVersion": "11.0",
        "NSHighResolutionCapable": True,
        "NSMicrophoneUsageDescription": "Blip uses your microphone for voice calls with colleagues' pets.",
        "NSLocalNetworkUsageDescription": "Blip finds your colleagues' pets on the office network for calls and messages.",
    },
)
