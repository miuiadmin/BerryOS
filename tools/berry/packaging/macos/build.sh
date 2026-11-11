#!/bin/sh
# Build a self-contained macOS .app for the BerryOS device shell.
# QEMU is NOT bundled (GPLv2, separate process); install it with `brew install qemu`.
# Usage: tools/berry/packaging/macos/build.sh [out/BerryOS.app]
set -eu
ROOT=$(cd "$(dirname "$0")/../../../.." && pwd)
cd "$ROOT"
HOST=$(rustc -vV | sed -n 's/^host: //p')
BIN="tools/berry/cargo-target/$HOST/release/berry"
test -x "$BIN" || { echo "missing $BIN — run: just berry-build"; exit 1; }
IMG=${BERRY_IMAGE:-out/berryos.img}
test -f "$IMG" || { echo "missing $IMG — run: just image, or set BERRY_IMAGE"; exit 1; }

APP=${1:-out/BerryOS.app}
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/berry"
chmod +x "$APP/Contents/MacOS/berry"
cp "$IMG" "$APP/Contents/Resources/berryos.img"
cp LICENSE "$APP/Contents/Resources/LICENSE"

FW=${AAVMF_CODE:-}
if [ -z "$FW" ]; then
  for c in \
    /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
    /usr/local/share/qemu/edk2-aarch64-code.fd \
    /usr/share/qemu/edk2-aarch64-code.fd \
    /usr/share/AAVMF/AAVMF_CODE.fd; do
    if [ -f "$c" ]; then FW=$c; break; fi
  done
fi
if [ -n "$FW" ] && [ -f "$FW" ]; then
  cp "$FW" "$APP/Contents/Resources/AAVMF_CODE.fd"
else
  echo "warning: AAVMF code not found; set AAVMF_CODE or install qemu" >&2
fi

VERSION=$("$BIN" --version | awk '{print $2}')
BUNDLE_ID=${BERRY_BUNDLE_ID:-dev.berryos.shell}
sed -e "s/@VERSION@/$VERSION/g" -e "s/@BUNDLE_ID@/$BUNDLE_ID/g" \
  tools/berry/packaging/macos/Info.plist > "$APP/Contents/Info.plist"

if command -v codesign >/dev/null 2>&1; then
  codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || \
    echo "warning: ad-hoc codesign failed" >&2
fi
echo "packaged: $APP"
