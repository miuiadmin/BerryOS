#!/bin/sh
# Cross-build berry.exe from a macOS arm64 host using an amd64 Linux container
# (OrbStack/Rosetta) + mingw-w64. QEMU stays external.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd -P)
cd "$ROOT"
command -v docker >/dev/null 2>&1 || { echo "docker not found" >&2; exit 1; }

docker run --rm --platform linux/amd64 \
    -v "$ROOT/tools/berry:/src" -v "$ROOT/out:/out" \
    rust:1.97-slim bash -c '
        set -e
        apt-get update -qq
        DEBIAN_FRONTEND=noninteractive apt-get install -y -qq gcc-mingw-w64-x86-64 file >/dev/null
        rustup target add x86_64-pc-windows-gnu
        cd /src
        CARGO_TARGET_DIR=/tmp/target cargo build --release --target x86_64-pc-windows-gnu
        cp /tmp/target/x86_64-pc-windows-gnu/release/berry.exe /out/berry.exe
        CARGO_TARGET_DIR=/tmp/target cargo build --release --target x86_64-pc-windows-gnu --features gui
        cp /tmp/target/x86_64-pc-windows-gnu/release/berry.exe /out/berry-shell.exe
        file /out/berry.exe /out/berry-shell.exe
    '
file out/berry.exe out/berry-shell.exe
echo "built: out/berry.exe (console) + out/berry-shell.exe (gui)"
