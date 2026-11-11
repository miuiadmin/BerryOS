#!/bin/sh
# Pack a validated AppDir into a portable Linux tarball.
# Usage:
#   build-tarball.sh [APP_DIR] [OUT.tar.gz]
# Defaults:
#   APP_DIR = out/BerryOS.AppDir-linux
#   OUT     = out/BerryOS-linux-<arch>.tar.gz
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd -P)
cd "$ROOT"

APP=${1:-out/BerryOS.AppDir-linux}
OUT=${2:-}

die() { printf 'error: %s\n' "$*" >&2; exit 1; }
[ -x "$APP/AppRun" ] || die "missing AppDir/AppRun: $APP (run build-appdir.sh first)"
[ -x "$APP/usr/bin/berry" ] || die "missing AppDir/usr/bin/berry: $APP"
[ -f "$APP/usr/share/berryos/berryos.img" ] || die "missing berryos.img: $APP"
[ -f "$APP/usr/share/berryos/AAVMF_CODE.fd" ] || die "missing AAVMF_CODE.fd: $APP"

BIN_KIND=$(file -b "$APP/usr/bin/berry" 2>/dev/null || true)
case "$BIN_KIND" in
    *aarch64*) ARCH=aarch64 ;;
    *x86-64*)  ARCH=x86_64 ;;
    *)         ARCH=unknown ;;
esac
[ -n "$OUT" ] || OUT="out/BerryOS-linux-$ARCH.tar.gz"
mkdir -p "$(dirname -- "$OUT")"

# Validate before packing (ELF required).
if [ -x tools/berry/packaging/linux/check-appdir.sh ]; then
    tools/berry/packaging/linux/check-appdir.sh --require-elf --quiet "$APP" || \
        die "AppDir validation failed (use check-appdir.sh for details)"
fi

tar -C "$APP" -czf "$OUT" .
SIZE=$(wc -c < "$OUT" | tr -d '[:space:]')
if command -v shasum >/dev/null 2>&1; then
    SHA=$(shasum -a 256 "$OUT" | awk '{print $1}')
else
    SHA=$(sha256sum "$OUT" | awk '{print $1}')
fi
printf 'packaged: %s (%s bytes, sha256=%s)\n' "$OUT" "$SIZE" "$SHA"
