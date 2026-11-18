#!/bin/bash
# Install or update Blip on a Mac (Apple Silicon). Paste this in Terminal:
#   curl -fsSL https://raw.githubusercontent.com/safder2000/blip/main/install-mac.sh | bash
set -euo pipefail
URL="https://github.com/safder2000/blip/releases/latest/download/Blip-mac-arm64.zip"
if [ "$(uname -m)" != "arm64" ]; then
  echo "Sorry - Blip for Mac needs an Apple Silicon Mac (M1 or newer)."; exit 1
fi
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
echo "Downloading Blip..."
curl -fsSL "$URL" -o "$TMP/Blip.zip"
curl -fsSL "$URL.sha256" -o "$TMP/Blip.zip.sha256"
want="$(cut -d' ' -f1 "$TMP/Blip.zip.sha256")"
got="$(shasum -a 256 "$TMP/Blip.zip" | cut -d' ' -f1)"
if [ "$want" != "$got" ]; then echo "The download looks damaged - please try again."; exit 1; fi
DEST="/Applications"
if [ ! -w "$DEST" ]; then DEST="$HOME/Applications"; mkdir -p "$DEST"; fi
/usr/bin/ditto -x -k "$TMP/Blip.zip" "$TMP/x"
osascript -e 'tell application "Blip" to quit' >/dev/null 2>&1 || true
sleep 1
rm -rf "$DEST/Blip.app"
mv "$TMP/x/Blip.app" "$DEST/Blip.app"
/usr/bin/xattr -dr com.apple.quarantine "$DEST/Blip.app" 2>/dev/null || true
open "$DEST/Blip.app"
echo "Blip is installed in $DEST and running. It keeps itself up to date from now on."
