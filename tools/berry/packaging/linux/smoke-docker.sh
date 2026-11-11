#!/bin/sh
# Smoke-run the Linux AppDir inside an arm64 Linux container (OrbStack/Docker).
# QEMU TCG does the guest boot; the host binary is the real Linux ELF.
#
# Usage: smoke-docker.sh [APP_DIR] [OUT_DIR]
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd -P)
cd "$ROOT"
APP=${1:-out/BerryOS.AppDir-linux}
OUT=${2:-out/linux-smoke}
case "$APP" in /*) APP_ABS=$APP ;; *) APP_ABS=$ROOT/$APP ;; esac
case "$OUT" in /*) OUT_ABS=$OUT ;; *) OUT_ABS=$ROOT/$OUT ;; esac
[ -x "$APP_ABS/AppRun" ] || { echo "missing AppDir: $APP_ABS" >&2; exit 1; }
mkdir -p "$OUT_ABS"
command -v docker >/dev/null 2>&1 || { echo "docker not found" >&2; exit 1; }
SMOKE_IMG=${BERRY_SMOKE_IMAGE:-out/berryos-desktop.img}
case "$SMOKE_IMG" in /*) IMG_ABS=$SMOKE_IMG ;; *) IMG_ABS=$ROOT/$SMOKE_IMG ;; esac
[ -f "$IMG_ABS" ] || { echo "missing smoke image: $IMG_ABS" >&2; exit 1; }

docker run --rm --platform linux/arm64 \
    -v "$APP_ABS:/app:ro" -v "$ROOT/out:/images:ro" -v "$OUT_ABS:/out" \
    alpine:3.20 sh -c '
        set -eu
        apk add --no-cache qemu-system-aarch64 >/dev/null
        /app/AppRun doctor | tee /out/doctor.txt
        /app/AppRun shot --image /images/berryos-desktop.img --profile phone \
            --wait-marker "LAUNCH" --delay-ms 5000 --out /out/phone.png --timeout 300 --keep
        /app/AppRun shot --image /images/berryos-desktop.img --profile phone20 --memory 256 \
            --wait-marker "LAUNCH" --delay-ms 5000 --out /out/phone20.png --timeout 300 --keep
    '

file "$OUT_ABS/phone.png" "$OUT_ABS/phone20.png"
echo "smoke: $OUT_ABS/doctor.txt $OUT_ABS/phone.png $OUT_ABS/phone20.png"
