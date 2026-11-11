#!/bin/sh
# Build a portable Linux AppDir for the BerryOS device shell.
#
# QEMU stays external (GPLv2); the AppDir ships berry + berryos.img + AAVMF.
# Usage:
#   tools/berry/packaging/linux/build-appdir.sh [APP_DIR]
#
# Environment overrides:
#   BERRY_BIN=...        host release binary (default: <target>/release/berry)
#   BERRY_TARGET=...     Cargo target triple (default: rustc host triple)
#   BERRY_IMAGE=...      device image (default: out/berryos.img)
#   AAVMF_CODE=...       AAVMF/edk2 code pflash (default: auto-detect)
#   APPIMAGETOOL=...     appimagetool executable (default: PATH lookup)
#   BERRY_APPIMAGE=...   AppImage output (default: out/BerryOS-<arch>.AppImage)
#   BERRY_ARCH=...       AppImage ARCH override (x86_64/aarch64/armhf/i686)
#   BERRY_ALLOW_NO_FIRMWARE=1  build without AAVMF (non-bootable; checker fails)
#   BERRY_FORCE=1        replace an existing non-AppDir directory
set -eu

usage() {
    cat <<'EOF'
Usage: tools/berry/packaging/linux/build-appdir.sh [APP_DIR]

Build an AppDir (default: out/BerryOS.AppDir) from the host release binary,
out/berryos.img, and AAVMF code. QEMU is never bundled.

Environment:
  BERRY_BIN        berry release binary (default: tools/berry/cargo-target/<target>/release/berry)
  BERRY_TARGET     Cargo target triple (default: rustc host triple)
  BERRY_IMAGE      device image (default: out/berryos.img)
  AAVMF_CODE       AAVMF/edk2 code pflash (default: auto-detect)
  APPIMAGETOOL     appimagetool path (default: PATH lookup; optional)
  BERRY_APPIMAGE   AppImage output (default: out/BerryOS-<arch>.AppImage)
  BERRY_ARCH       AppImage ARCH override (x86_64/aarch64/armhf/i686)
  BERRY_ALLOW_NO_FIRMWARE=1  allow a non-bootable AppDir without AAVMF
  BERRY_FORCE=1    replace an existing directory that is not an AppDir

Relative BERRY_BIN / BERRY_IMAGE / AAVMF_CODE paths are resolved from the
repository root.
EOF
}

die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

warn() {
    printf 'warning: %s\n' "$*" >&2
}

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd -P) || {
    printf 'error: cannot resolve repository root from %s\n' "$0" >&2
    exit 1
}
[ -f "$ROOT/tools/berry/Cargo.toml" ] || \
    die "not a BerryOS checkout (missing tools/berry/Cargo.toml): $ROOT"
INVOKE_DIR=$(pwd -P)
cd "$ROOT"

APP=
ENDOPTS=0
while [ $# -gt 0 ]; do
    if [ "$ENDOPTS" = 1 ]; then
        [ -z "$APP" ] || die "too many arguments: $1"
        APP=$1
    else
        case "$1" in
            -h|--help)
                usage
                exit 0
                ;;
            --)
                ENDOPTS=1
                ;;
            -*)
                die "unknown option: $1 (try --help)"
                ;;
            *)
                [ -z "$APP" ] || die "too many arguments: $1"
                APP=$1
                ;;
        esac
    fi
    shift
done
APP=${APP:-out/BerryOS.AppDir}

# Never rm -rf an arbitrary path (/, $HOME, the repo root, ...).
APP_BASE=$(basename -- "$APP")
case "$APP_BASE" in
    ''|.|..|/) die "refusing unsafe AppDir path: $APP" ;;
esac
APP_PARENT=$(dirname -- "$APP")
mkdir -p -- "$APP_PARENT" || die "cannot create AppDir parent: $APP_PARENT"
APP_ABS=$(CDPATH= cd -- "$APP_PARENT" && pwd -P)/$APP_BASE
case "$APP_ABS" in
    /|//|"$ROOT"|"$INVOKE_DIR")
        die "refusing unsafe AppDir path: $APP_ABS"
        ;;
esac
if [ -n "${HOME:-}" ] && [ "$APP_ABS" = "$HOME" ]; then
    die "refusing unsafe AppDir path: $APP_ABS"
fi
if [ -e "$APP_ABS" ] && [ ! -d "$APP_ABS" ]; then
    die "AppDir path exists and is not a directory: $APP_ABS"
fi
if [ -d "$APP_ABS" ] && [ -n "$(ls -A "$APP_ABS" 2>/dev/null || true)" ]; then
    if [ ! -f "$APP_ABS/AppRun" ] && [ ! -f "$APP_ABS/usr/bin/berry" ]; then
        [ "${BERRY_FORCE:-0}" = 1 ] || die \
            "refusing to replace a directory that is not an AppDir: $APP_ABS (set BERRY_FORCE=1 to override)"
    fi
fi

# Host target: BERRY_TARGET, else rustc's host triple. BERRY_BIN alone is enough
# on machines without rustc; the AppImage arch is inferred from `file` below.
TARGET=${BERRY_TARGET:-}
RUSTC=$(command -v rustc 2>/dev/null || true)
if [ -z "$RUSTC" ] && [ -n "${HOME:-}" ] && [ -x "$HOME/.cargo/bin/rustc" ]; then
    RUSTC=$HOME/.cargo/bin/rustc
fi
if [ -z "$TARGET" ] && [ -n "$RUSTC" ]; then
    TARGET=$("$RUSTC" -vV 2>/dev/null | sed -n 's/^host: //p')
fi
if [ -n "$TARGET" ]; then
    case "$TARGET" in
        *-*-*) ;;
        *) die "invalid BERRY_TARGET (expected a Cargo triple): $TARGET" ;;
    esac
fi
if [ -z "${BERRY_BIN:-}" ] && [ -z "$TARGET" ]; then
    if [ -n "$RUSTC" ]; then
        die "cannot parse the host target from 'rustc -vV'
  set BERRY_TARGET=<triple> or BERRY_BIN=/path/to/berry"
    else
        die "rustc not found and BERRY_BIN is unset
  install Rust from https://rustup.rs
  or set BERRY_TARGET=<triple> and/or BERRY_BIN=/path/to/berry"
    fi
fi
BIN=${BERRY_BIN:-tools/berry/cargo-target/$TARGET/release/berry}

[ -e "$BIN" ] || die "missing release binary: $BIN
  build it: just berry-build
  or set BERRY_BIN=/path/to/berry"
[ -f "$BIN" ] || die "binary is not a regular file: $BIN"
[ -s "$BIN" ] || die "binary is empty: $BIN"
[ -x "$BIN" ] || die "binary is not executable: $BIN
  chmod +x '$BIN' or rebuild with: just berry-build"

IMG=${BERRY_IMAGE:-out/berryos.img}
[ -f "$IMG" ] || die "missing device image: $IMG
  build it: just image
  or set BERRY_IMAGE=/path/to/berryos.img"
[ -r "$IMG" ] || die "device image is not readable: $IMG"
[ -s "$IMG" ] || die "device image is empty: $IMG"

size_of() {
    wc -c < "$1" | tr -d '[:space:]'
}

detect_arch() {
    case "$1" in
        x86_64-*|amd64-*) printf 'x86_64\n' ;;
        aarch64-*|arm64-*) printf 'aarch64\n' ;;
        armv7*-*|arm-*-gnueabihf|arm-*-musleabihf) printf 'armhf\n' ;;
        i?86-*) printf 'i686\n' ;;
        *) printf 'unknown\n' ;;
    esac
}

ARCH=${BERRY_ARCH:-$(detect_arch "$TARGET")}
BIN_IS_ELF=unknown
if command -v file >/dev/null 2>&1; then
    BIN_KIND=$(file -b "$BIN" 2>/dev/null || true)
    case "$BIN_KIND" in
        *ELF*)
            BIN_IS_ELF=1
            ;;
        *)
            BIN_IS_ELF=0
            warn "binary is not a Linux ELF: $BIN_KIND
  this AppDir is host-native and is not runnable on Linux;
  rebuild with BERRY_TARGET=<linux-triple> BERRY_BIN=<linux release binary>"
            ;;
    esac
    if [ "$ARCH" = unknown ]; then
        case "$BIN_KIND" in
            *aarch64*|*ARM64*|*arm64*) ARCH=aarch64 ;;
            *x86-64*|*x86_64*) ARCH=x86_64 ;;
            *ARM,\ EABI*|*armhf*) ARCH=armhf ;;
            *Intel\ 80386*|*i386*) ARCH=i686 ;;
        esac
    fi
fi
case "$ARCH" in
    x86_64|aarch64|armhf|i686) ;;
    *) warn "unknown AppImage ARCH '$ARCH'; set BERRY_ARCH or appimagetool may fail" ;;
esac

# AAVMF/edk2 code search. An explicit AAVMF_CODE is authoritative: a typo is
# an error, not a silent fallback to a system firmware that would not ship.
find_firmware() {
    for c in \
        /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
        /usr/local/share/qemu/edk2-aarch64-code.fd \
        /opt/local/share/qemu/edk2-aarch64-code.fd \
        /usr/share/qemu/edk2-aarch64-code.fd \
        /usr/share/AAVMF/AAVMF_CODE.fd \
        /usr/share/qemu-efi-aarch64/QEMU_EFI.fd \
        /usr/share/edk2/aarch64/QEMU_CODE.fd \
        /usr/share/edk2/aarch64/QEMU_EFI.fd \
        /usr/share/edk2/aarch64/QEMU_EFI-pflash.raw \
        out/AAVMF_CODE.fd \
        tools/berry/firmware/AAVMF_CODE.fd; do
        if [ -f "$c" ]; then
            printf '%s\n' "$c"
            return 0
        fi
    done

    QEMU=${BERRY_QEMU:-}
    if [ -z "$QEMU" ]; then
        QEMU=$(command -v qemu-system-aarch64 2>/dev/null || true)
    fi
    if [ -n "$QEMU" ]; then
        if command -v "$QEMU" >/dev/null 2>&1; then
            QEMU=$(command -v "$QEMU")
        fi
        if [ -x "$QEMU" ]; then
            # Derive the install prefix from the QEMU path without launching
            # QEMU: covers Homebrew, /usr/local, /usr, and Nix store prefixes.
            QEMU_PREFIX=$(CDPATH= cd -- "$(dirname -- "$QEMU")/.." 2>/dev/null && pwd -P) || QEMU_PREFIX=
            if [ -n "$QEMU_PREFIX" ]; then
                for d in \
                    "$QEMU_PREFIX/share/qemu" \
                    "$QEMU_PREFIX/share/AAVMF" \
                    "$QEMU_PREFIX/share/qemu-efi-aarch64" \
                    "$QEMU_PREFIX/share/edk2/aarch64"; do
                    for f in \
                        edk2-aarch64-code.fd \
                        AAVMF_CODE.fd \
                        QEMU_EFI.fd \
                        QEMU_CODE.fd \
                        QEMU_EFI-pflash.raw; do
                        if [ -f "$d/$f" ]; then
                            printf '%s\n' "$d/$f"
                            return 0
                        fi
                    done
                done
            fi
        fi
    fi
    return 1
}

if [ -n "${AAVMF_CODE:-}" ]; then
    FW=$AAVMF_CODE
    [ -f "$FW" ] || die "AAVMF_CODE does not point to a file: $FW
  install qemu-efi-aarch64 / edk2-aarch64, or unset AAVMF_CODE to auto-detect"
else
    FW=$(find_firmware || true)
fi
if [ -z "$FW" ]; then
    if [ "${BERRY_ALLOW_NO_FIRMWARE:-0}" = 1 ]; then
        warn "AAVMF firmware not found; building a non-bootable AppDir
  check-appdir.sh will report the missing AAVMF_CODE.fd"
    else
        die "AAVMF/edk2 firmware not found
  Debian/Ubuntu: sudo apt install qemu-efi-aarch64
  Fedora:        sudo dnf install edk2-aarch64
  Arch:          sudo pacman -S edk2-aarch64
  macOS:         brew install qemu
  or set AAVMF_CODE=/path/to/edk2-aarch64-code.fd
  (BERRY_ALLOW_NO_FIRMWARE=1 builds a non-bootable AppDir)"
    fi
fi
if [ -n "$FW" ]; then
    [ -r "$FW" ] || die "AAVMF file is not readable: $FW"
    FW_SIZE=$(size_of "$FW")
    [ "$FW_SIZE" -gt 0 ] || die "AAVMF file is empty: $FW"
    if [ "$FW_SIZE" -lt 1048576 ]; then
        warn "AAVMF file is unusually small ($FW_SIZE bytes): $FW"
    fi
fi

DESKTOP_SRC=tools/berry/packaging/linux/berry.desktop
[ -f "$DESKTOP_SRC" ] || die "missing desktop file: $DESKTOP_SRC"

umask 022
rm -rf "$APP_ABS"
mkdir -p \
    "$APP_ABS/usr/bin" \
    "$APP_ABS/usr/share/berryos" \
    "$APP_ABS/usr/share/applications" || die "cannot create AppDir tree: $APP_ABS"

cp -f "$BIN" "$APP_ABS/usr/bin/berry" || die "failed to copy binary: $BIN"
chmod 0755 "$APP_ABS/usr/bin/berry"
[ "$(size_of "$APP_ABS/usr/bin/berry")" = "$(size_of "$BIN")" ] || \
    die "short copy of binary: $BIN"

cp -f "$IMG" "$APP_ABS/usr/share/berryos/berryos.img" || die "failed to copy image: $IMG"
chmod 0644 "$APP_ABS/usr/share/berryos/berryos.img"
[ "$(size_of "$APP_ABS/usr/share/berryos/berryos.img")" = "$(size_of "$IMG")" ] || \
    die "short copy of image: $IMG"

if [ -n "$FW" ]; then
    cp -f "$FW" "$APP_ABS/usr/share/berryos/AAVMF_CODE.fd" || die "failed to copy AAVMF: $FW"
    chmod 0644 "$APP_ABS/usr/share/berryos/AAVMF_CODE.fd"
    [ "$(size_of "$APP_ABS/usr/share/berryos/AAVMF_CODE.fd")" = "$(size_of "$FW")" ] || \
        die "short copy of AAVMF: $FW"
fi

cp -f "$DESKTOP_SRC" "$APP_ABS/usr/share/applications/berry.desktop" || \
    die "failed to copy desktop entry"
# appimagetool also looks for a desktop entry at the AppDir root.
cp -f "$DESKTOP_SRC" "$APP_ABS/berry.desktop" || die "failed to copy root desktop entry"
ICON_SRC=tools/berry/packaging/linux/berry.png
[ -f "$ICON_SRC" ] || die "missing icon: $ICON_SRC"
cp -f "$ICON_SRC" "$APP_ABS/berry.png" || die "failed to copy AppImage icon"
mkdir -p "$APP_ABS/usr/share/icons/hicolor/256x256/apps"
cp -f "$ICON_SRC" "$APP_ABS/usr/share/icons/hicolor/256x256/apps/berry.png" || \
    die "failed to copy hicolor icon"
chmod 0644 "$APP_ABS/usr/share/applications/berry.desktop" "$APP_ABS/berry.desktop" \
    "$APP_ABS/berry.png" "$APP_ABS/usr/share/icons/hicolor/256x256/apps/berry.png"

cat > "$APP_ABS/AppRun" <<'RUN'
#!/bin/sh
# Generated by tools/berry/packaging/linux/build-appdir.sh.
# Keep this POSIX sh: AppImage executes it on the target Linux host.
set -eu
if [ -n "${APPDIR:-}" ] && [ -x "$APPDIR/usr/bin/berry" ]; then
    HERE=$(CDPATH= cd -- "$APPDIR" && pwd) || {
        echo "BerryOS: cannot enter APPDIR: $APPDIR" >&2
        exit 1
    }
else
    SELF=$0
    if command -v readlink >/dev/null 2>&1; then
        RESOLVED=$(readlink -f -- "$SELF" 2>/dev/null || true)
        if [ -n "$RESOLVED" ]; then
            SELF=$RESOLVED
        fi
    fi
    HERE=$(CDPATH= cd -- "$(dirname -- "$SELF")" && pwd) || {
        echo "BerryOS: cannot locate AppDir from $SELF" >&2
        exit 1
    }
fi
BERRY="$HERE/usr/bin/berry"
if [ ! -x "$BERRY" ]; then
    echo "BerryOS: missing executable: $BERRY" >&2
    exit 1
fi
if [ -z "${BERRY_IMAGE:-}" ]; then
    BERRY_IMAGE="$HERE/usr/share/berryos/berryos.img"
    export BERRY_IMAGE
fi
if [ ! -f "$BERRY_IMAGE" ]; then
    echo "BerryOS: missing device image: $BERRY_IMAGE" >&2
    echo "Rebuild the AppDir with build-appdir.sh, or set BERRY_IMAGE." >&2
    exit 1
fi
if [ -z "${AAVMF_CODE:-}" ] && [ -f "$HERE/usr/share/berryos/AAVMF_CODE.fd" ]; then
    AAVMF_CODE="$HERE/usr/share/berryos/AAVMF_CODE.fd"
    export AAVMF_CODE
fi
exec "$BERRY" "$@"
RUN
chmod 0755 "$APP_ABS/AppRun"

chmod 0755 \
    "$APP_ABS" \
    "$APP_ABS/usr" \
    "$APP_ABS/usr/bin" \
    "$APP_ABS/usr/share" \
    "$APP_ABS/usr/share/berryos" \
    "$APP_ABS/usr/share/applications"

TOOL=${APPIMAGETOOL:-}
if [ -n "$TOOL" ]; then
    case "$TOOL" in
        */*)
            [ -x "$TOOL" ] || die "APPIMAGETOOL is not executable: $TOOL"
            ;;
        *)
            TOOL=$(command -v "$TOOL" 2>/dev/null || true)
            [ -n "$TOOL" ] || die "APPIMAGETOOL not found on PATH: ${APPIMAGETOOL}"
            ;;
    esac
else
    TOOL=$(command -v appimagetool 2>/dev/null || true)
fi

printf 'packaged AppDir: %s\n' "$APP_ABS"
printf '  binary: %s (%s bytes)\n' "$APP_ABS/usr/bin/berry" "$(size_of "$APP_ABS/usr/bin/berry")"
printf '  image : %s (%s bytes)\n' "$APP_ABS/usr/share/berryos/berryos.img" "$(size_of "$APP_ABS/usr/share/berryos/berryos.img")"
if [ -n "$FW" ]; then
    printf '  AAVMF : %s (%s bytes, from %s)\n' \
        "$APP_ABS/usr/share/berryos/AAVMF_CODE.fd" \
        "$(size_of "$APP_ABS/usr/share/berryos/AAVMF_CODE.fd")" "$FW"
else
    printf '  AAVMF : missing (non-bootable AppDir)\n'
fi
printf '  desktop: %s\n' "$APP_ABS/usr/share/applications/berry.desktop"
printf '  AppRun : %s\n' "$APP_ABS/AppRun"

if [ -n "$TOOL" ] && [ "$BIN_IS_ELF" != 0 ]; then
    OUT_APPIMAGE=${BERRY_APPIMAGE:-out/BerryOS-$ARCH.AppImage}
    APPIMAGE_PARENT=$(dirname -- "$OUT_APPIMAGE")
    mkdir -p -- "$APPIMAGE_PARENT" || die "cannot create AppImage output dir: $APPIMAGE_PARENT"
    printf 'running: %s %s %s (ARCH=%s)\n' "$TOOL" "$APP_ABS" "$OUT_APPIMAGE" "$ARCH"
    if ARCH="$ARCH" "$TOOL" "$APP_ABS" "$OUT_APPIMAGE"; then
        [ -f "$OUT_APPIMAGE" ] || die "appimagetool reported success but output is missing: $OUT_APPIMAGE"
        printf 'packaged AppImage: %s (%s bytes)\n' "$OUT_APPIMAGE" "$(size_of "$OUT_APPIMAGE")"
    else
        printf 'error: appimagetool failed; the AppDir is still usable: %s\n' "$APP_ABS" >&2
        exit 1
    fi
elif [ -n "$TOOL" ]; then
    printf 'skip AppImage: bundled binary is not a Linux ELF; rebuild for Linux first\n'
else
    printf 'hint: appimagetool not found; install it or set APPIMAGETOOL=/path/to/appimagetool to build an AppImage\n'
fi
