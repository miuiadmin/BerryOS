#!/bin/sh
# Build a portable Linux AppDir for the BerryOS device shell.
# QEMU stays external (GPLv2); the AppDir ships berry + image + AAVMF.
# Usage: tools/berry/packaging/linux/build-appdir.sh [out/BerryOS.AppDir]
set -eu
ROOT=$(cd "$(dirname "$0")/../../../.." && pwd)
cd "$ROOT"
HOST=$(rustc -vV | sed -n 's/^host: //p')
BIN="tools/berry/cargo-target/$HOST/release/berry"
IMG=${BERRY_IMAGE:-out/berryos.img}
APP=${1:-out/BerryOS.AppDir}

test -x "$BIN" || { echo "missing $BIN — run: just berry-build" >&2; exit 1; }
test -f "$IMG" || { echo "missing $IMG — run: just image, or set BERRY_IMAGE" >&2; exit 1; }

rm -rf "$APP"
mkdir -p "$APP/usr/bin" "$APP/usr/share/berryos" "$APP/usr/share/applications"
cp "$BIN" "$APP/usr/bin/berry"
cp "$IMG" "$APP/usr/share/berryos/berryos.img"
chmod +x "$APP/usr/bin/berry"

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
  cp "$FW" "$APP/usr/share/berryos/AAVMF_CODE.fd"
else
  echo "warning: AAVMF code not found; set AAVMF_CODE" >&2
fi

cp tools/berry/packaging/linux/berry.desktop "$APP/usr/share/applications/berry.desktop"
cat > "$APP/AppRun" <<'RUN'
#!/bin/sh
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
export BERRY_IMAGE="$HERE/usr/share/berryos/berryos.img"
export AAVMF_CODE="$HERE/usr/share/berryos/AAVMF_CODE.fd"
exec "$HERE/usr/bin/berry" "$@"
RUN
chmod +x "$APP/AppRun"

if command -v appimagetool >/dev/null 2>&1; then
  appimagetool "$APP" out/BerryOS-x86_64.AppImage
  echo "packaged: out/BerryOS-x86_64.AppImage"
else
  echo "packaged AppDir: $APP (install appimagetool to produce an AppImage)"
fi
