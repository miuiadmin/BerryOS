#!/bin/sh
# Run the AppImage through its FUSE runtime inside an arm64 Ubuntu container.
# Requires Docker with /dev/fuse + SYS_ADMIN (OrbStack supports this).
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd -P)
cd "$ROOT"
APP=${1:-out/BerryOS-aarch64.AppImage}
case "$APP" in /*) APP_ABS=$APP ;; *) APP_ABS=$ROOT/$APP ;; esac
[ -f "$APP_ABS" ] || { echo "missing AppImage: $APP_ABS" >&2; exit 1; }
command -v docker >/dev/null 2>&1 || { echo "docker not found" >&2; exit 1; }

docker run --rm --platform linux/arm64 \
    --cap-add SYS_ADMIN --device /dev/fuse \
    -v "$APP_ABS:/app.AppImage:ro" ubuntu:24.04 bash -c '
        set -e
        apt-get update -qq
        DEBIAN_FRONTEND=noninteractive apt-get install -y -qq libfuse2 >/dev/null
        /app.AppImage --version
        /app.AppImage doctor 2>&1 | head -8
    '
