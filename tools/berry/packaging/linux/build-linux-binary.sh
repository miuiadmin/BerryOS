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

# Cross-C for ring (VNC wave 5: rustls's ring backend compiles C). On a
# macOS host there is no musl cross-gcc; Apple clang + -ffreestanding
# covers ring's freestanding crypto sources, with one shim needed:
# clang's builtin stddef.h #include_next's unconditionally for the musl
# branch and there is no musl sysroot to catch it — a self-contained
# stddef.h placed in an -isystem dir ahead of the builtin resolves it.
# (stdint.h needs no shim: its include_next is guarded by
# __STDC_HOSTED__, which -ffreestanding clears.)
CC_ENV="CC_${TARGET//-/_}"
CFLAGS_ENV="CFLAGS_${TARGET//-/_}"
if [ -z "$(eval "printf %s \"\${$CC_ENV:-}\"")" ] && command -v clang >/dev/null 2>&1; then
    cat > "$STUB/stddef.h" <<'H'
#pragma once
typedef __SIZE_TYPE__ size_t;
typedef __PTRDIFF_TYPE__ ptrdiff_t;
#ifndef NULL
#define NULL ((void *)0)
#endif
#define offsetof(t, member) __builtin_offsetof(t, member)
typedef struct {
    long long __berry_align_ll;
    long double __berry_align_ld;
} max_align_t;
H
    mkdir -p "$STUB/shim"
    mv "$STUB/stddef.h" "$STUB/shim/stddef.h"
    export CC_aarch64_unknown_linux_musl="$(command -v clang)" \
           CC_x86_64_unknown_linux_musl="$(command -v clang)"
    export CFLAGS_aarch64_unknown_linux_musl="--target=aarch64-unknown-linux-musl -ffreestanding -isystem $STUB/shim" \
           CFLAGS_x86_64_unknown_linux_musl="--target=x86_64-unknown-linux-musl -ffreestanding -isystem $STUB/shim"
fi

cd tools/berry
PATH="$HOME/.cargo/bin:$PATH" \
CARGO_HOME="$PWD/.cargo-home" \
CARGO_TARGET_DIR="$PWD/cargo-target-linux" \
RUSTFLAGS="-C linker=$RUSTLLD -C link-arg=-L$STUB" \
    cargo build --release --target "$TARGET"

BIN="$PWD/cargo-target-linux/$TARGET/release/berry"
file "$BIN"
echo "built: $BIN"
