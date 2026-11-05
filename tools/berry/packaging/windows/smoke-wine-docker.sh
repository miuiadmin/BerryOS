#!/bin/sh
# Smoke berry.exe under Wine in an amd64 Linux container (not real Windows).
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd -P)
cd "$ROOT"
EXE=${1:-out/berry.exe}
case "$EXE" in /*) EXE_ABS=$EXE ;; *) EXE_ABS=$ROOT/$EXE ;; esac
[ -f "$EXE_ABS" ] || { echo "missing $EXE_ABS" >&2; exit 1; }
command -v docker >/dev/null 2>&1 || { echo "docker not found" >&2; exit 1; }
docker run --rm --platform linux/amd64 -v "$EXE_ABS:/berry.exe:ro" ubuntu:24.04 bash -c '
    set -e
    apt-get update -qq
    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq wine64 >/dev/null
    export WINEPREFIX=/tmp/wine WINEDEBUG=-all
    /usr/lib/wine/wine64 /berry.exe --version
'
