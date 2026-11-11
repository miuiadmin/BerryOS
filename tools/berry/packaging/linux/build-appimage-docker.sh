#!/bin/sh
# Build an aarch64 AppImage from an AppDir using an arm64 Ubuntu container
# (appimagetool itself is an AppImage; extract it, no host FUSE needed).
# Usage: build-appimage-docker.sh [APP_DIR] [OUT.AppImage]
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd -P)
cd "$ROOT"
APP=${1:-out/BerryOS.AppDir-linux}
OUT=${2:-out/BerryOS-aarch64.AppImage}
case "$APP" in /*) APP_ABS=$APP ;; *) APP_ABS=$ROOT/$APP ;; esac
case "$OUT" in /*) OUT_ABS=$OUT ;; *) OUT_ABS=$ROOT/$OUT ;; esac
[ -x "$APP_ABS/AppRun" ] || { echo "missing AppDir: $APP_ABS" >&2; exit 1; }
[ -f "$APP_ABS/berry.png" ] || { echo "AppDir missing berry.png icon" >&2; exit 1; }
command -v docker >/dev/null 2>&1 || { echo "docker not found" >&2; exit 1; }
OUT_DIR=$(dirname -- "$OUT_ABS")
OUT_NAME=$(basename -- "$OUT_ABS")
mkdir -p "$OUT_DIR"

docker run --rm --platform linux/arm64 \
    -v "$APP_ABS:/app:ro" -v "$OUT_DIR:/out" \
    ubuntu:24.04 bash -c "
        set -e
        apt-get update -qq
        DEBIAN_FRONTEND=noninteractive apt-get install -y -qq curl file squashfs-tools libfuse2 libglib2.0-0 >/dev/null
        curl -sL https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-aarch64.AppImage -o /tmp/appimagetool
        chmod +x /tmp/appimagetool
        cd /tmp
        /tmp/appimagetool --appimage-extract >/dev/null
        rm -rf /tmp/app; cp -a /app /tmp/app; chmod -R u+w /tmp/app
        ARCH=aarch64 ./squashfs-root/AppRun /tmp/app /out/$OUT_NAME
    "
file "$OUT_ABS"
echo "appimage: $OUT_ABS"
