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
( cd "$OUT" && zip -qr ../BerryOS-win64.zip . )
ls -la "$OUT"
echo "packaged: $OUT and out/BerryOS-win64.zip"
