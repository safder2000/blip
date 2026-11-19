#!/bin/bash
# Build Blip.app around the Rust binary (run by .github/workflows/release.yml on macOS):
#   packaging/make_mac_app.sh <version> <path to blip binary> <libopus.0.dylib or ""> <out dir>
set -euo pipefail
VER="$1"; BIN="$2"; OPUS="${3:-}"; OUT="${4:-dist}"
APP="$OUT/Blip.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Frameworks"
cp "$BIN" "$APP/Contents/MacOS/Blip"
chmod +x "$APP/Contents/MacOS/Blip"
if [ -n "$OPUS" ] && [ -f "$OPUS" ]; then cp "$OPUS" "$APP/Contents/Frameworks/libopus.0.dylib"; fi
if [ -f assets/blip.icns ]; then cp assets/blip.icns "$APP/Contents/Resources/blip.icns"; fi
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>Blip</string>
  <key>CFBundleIdentifier</key><string>com.safder2000.blip</string>
  <key>CFBundleName</key><string>Blip</string>
  <key>CFBundleDisplayName</key><string>Blip</string>
  <key>CFBundleIconFile</key><string>blip</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VER}</string>
  <key>CFBundleVersion</key><string>${VER}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSUIElement</key><true/>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSMicrophoneUsageDescription</key><string>Blip uses your microphone for voice calls with colleagues' pets.</string>
  <key>NSLocalNetworkUsageDescription</key><string>Blip finds your colleagues' pets on the office network for calls and messages.</string>
</dict>
</plist>
PLIST
# ad-hoc signature for the whole bundle (Apple Silicon needs one; also seals Info.plist)
codesign --force --deep --sign - "$APP"
codesign --verify --deep "$APP"
echo "built $APP ($VER)"
