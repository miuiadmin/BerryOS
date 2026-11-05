#!/bin/sh
# Cross-build the CLI host for Linux (musl static) on a macOS dev machine.
# This is a packaging helper, not a guest/kernel build.
#
# Usage:
#   tools/berry/packaging/linux/build-linux-binary.sh
#   BERRY_TARGET=x86_64-unknown-linux-musl ... build-linux-binary.sh
#
# Requires: rustup target add <target>; rust-lld is used as the linker.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd -P)
cd "$ROOT"

HOST_ARCH=$(uname -m 2>/dev/null || echo unknown)
case "${BERRY_TARGET:-}" in
    "") case "$HOST_ARCH" in
            arm64|aarch64) TARGET=aarch64-unknown-linux-musl ;;
            x86_64|amd64)  TARGET=x86_64-unknown-linux-musl ;;
            *) echo "unsupported host arch: $HOST_ARCH; set BERRY_TARGET" >&2; exit 1 ;;
        esac ;;
    *) TARGET=$BERRY_TARGET ;;
esac

RUSTC=$(command -v rustc 2>/dev/null || true)
[ -n "$RUSTC" ] || { echo "rustc not found" >&2; exit 1; }
if ! "$RUSTC" --print target-libdir --target "$TARGET" >/dev/null 2>&1; then
    echo "target $TARGET is not installed; run: rustup target add $TARGET" >&2
    exit 1
fi
SYSROOT=$("$RUSTC" --print sysroot)
RUSTLLD="$SYSROOT/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/rust-lld"
[ -x "$RUSTLLD" ] || { echo "rust-lld not found: $RUSTLLD" >&2; exit 1; }

# The musl target's self-contained libc has no libdl.a; static musl does not
# need it. Build an empty AArch64/x86_64 ELF object and let -ldl resolve.
STUB=$(mktemp -d "${TMPDIR:-/tmp}/berry-libdl.XXXXXX")
trap 'rm -rf "$STUB"' EXIT HUP INT TERM
cat > "$STUB/dl.rs" <<'RS'
#![no_std]
#[no_mangle]
pub extern "C" fn __berry_dl_stub() {}
RS
"$RUSTC" --target "$TARGET" --crate-type lib --emit=obj -O \
    "$STUB/dl.rs" -o "$STUB/dl.o" >/dev/null
ar rcs "$STUB/libdl.a" "$STUB/dl.o"

cd tools/berry
PATH="$HOME/.cargo/bin:$PATH" \
CARGO_HOME="$PWD/.cargo-home" \
CARGO_TARGET_DIR="$PWD/cargo-target-linux" \
RUSTFLAGS="-C linker=$RUSTLLD -C link-arg=-L$STUB" \
    cargo build --release --target "$TARGET"

BIN="$PWD/cargo-target-linux/$TARGET/release/berry"
file "$BIN"
echo "built: $BIN"
