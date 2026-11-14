#!/bin/sh
# Assemble a Windows distribution folder from cross-built berry.exe + image + AAVMF.
# Usage: assemble.sh [OUT_DIR]
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd -P)
cd "$ROOT"
OUT=${1:-out/BerryOS-win64}
EXE=${BERRY_EXE:-out/berry.exe}
GUI=${BERRY_GUI_EXE:-out/berry-shell.exe}
IMG=${BERRY_IMAGE:-out/berryos.img}
FW=${AAVMF_CODE:-}
if [ -z "$FW" ]; then
    for c in /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
             /usr/local/share/qemu/edk2-aarch64-code.fd \
             /usr/share/qemu/edk2-aarch64-code.fd; do
        [ -f "$c" ] && { FW=$c; break; }
    done
fi
[ -f "$EXE" ] || { echo "missing $EXE (run build-cross-docker.sh)" >&2; exit 1; }
[ -f "$GUI" ] || { echo "missing $GUI (run build-cross-docker.sh)" >&2; exit 1; }
[ -f "$IMG" ] || { echo "missing $IMG" >&2; exit 1; }
[ -n "$FW" ] && [ -f "$FW" ] || { echo "AAVMF not found; set AAVMF_CODE" >&2; exit 1; }

rm -rf "$OUT"
mkdir -p "$OUT"
cp "$EXE" "$OUT/berry.exe"
cp "$GUI" "$OUT/berry-shell.exe"
cp "$IMG" "$OUT/berryos.img"
cp "$FW" "$OUT/AAVMF_CODE.fd"

# Vendored noVNC web assets for `berry vnc web` (wave 3). web_roots() finds
# <exe-dir>/web next to berry.exe, i.e. this exact spot. License compliance:
# NOTICE.md, novnc/LICENSE.txt, novnc/AUTHORS and novnc/vendor/pako/LICENSE
# all ride along inside <package>/web/ (copied wholesale from tools/berry/web).
WEB_SRC=tools/berry/web
[ -f "$WEB_SRC/novnc/vnc.html" ] || { echo "missing $WEB_SRC/novnc/vnc.html" >&2; exit 1; }
rm -rf "$OUT/web"
cp -R "$WEB_SRC" "$OUT/web"
# Normalize modes (cp -R keeps the checkout's file modes; e.g. NOTICE.md at
# 0600 would ship unreadable to anyone but the builder after unzip on unix).
chmod -R a+rX "$OUT/web"
for f in NOTICE.md novnc/vnc.html novnc/LICENSE.txt novnc/AUTHORS novnc/vendor/pako/LICENSE; do
  [ -f "$OUT/web/$f" ] || { echo "web assets incomplete: missing $f" >&2; exit 1; }
done

# Third-party notices for the statically linked host-tool dependencies
# (wave 6): single source under packaging/, copied into every platform
# package — the Windows seat is the package root next to berry.exe
# (the zip is rebuilt from $OUT wholesale, so the notice rides along).
TPN_SRC=tools/berry/packaging/THIRD-PARTY-NOTICES.txt
[ -f "$TPN_SRC" ] || { echo "missing third-party notices source: $TPN_SRC" >&2; exit 1; }
cp "$TPN_SRC" "$OUT/THIRD-PARTY-NOTICES.txt"
chmod 0644 "$OUT/THIRD-PARTY-NOTICES.txt"

cat > "$OUT/run-berry.cmd" <<'CMD'
@echo off
setlocal
set BERRY_IMAGE=%~dp0berryos.img
set AAVMF_CODE=%~dp0AAVMF_CODE.fd
"%~dp0berry.exe" shell %*
CMD
cat > "$OUT/README.txt" <<'TXT'
BerryOS device shell (Windows)
Double-click berry-shell.exe for the GUI (no console).
CLI: berry.exe <command> (console), or run-berry.cmd.
QEMU is external (GPLv2): put qemu-system-aarch64.exe on PATH or set BERRY_QEMU.
TXT
# Idempotent zip: delete the old archive first — `zip -qr` alone would keep
# stale entries from a previous run inside the existing zip. ZIP must be
# absolute: the zip runs after `cd "$OUT"`, a relative path would resolve
# against the package dir itself.
ZIP=$(CDPATH= cd -- "$(dirname -- "$OUT")" && pwd -P)/BerryOS-win64.zip
rm -f "$ZIP"
( cd "$OUT" && zip -qr "$ZIP" . )
ls -la "$OUT"
echo "packaged: $OUT and $ZIP"
