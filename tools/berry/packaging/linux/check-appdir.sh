#!/bin/sh
# Structural checks for a BerryOS Linux AppDir.
#
# This never boots a Linux guest and never executes the bundled berry binary.
# Usage:
#   tools/berry/packaging/linux/check-appdir.sh [options] [APP_DIR]
#
# Options:
#   --file        print `file` classification for bundled artifacts
#   --sha256      print SHA-256 for bundled artifacts (reads image + firmware)
#   --require-elf fail when usr/bin/berry is not a Linux ELF
#   -q, --quiet   only warnings, failures, and the summary
#   -h, --help    show this help
#
# Exit status: 0 = no failures, 1 = structural failure, 2 = usage error.
set -eu

usage() {
    cat <<'EOF'
Usage: tools/berry/packaging/linux/check-appdir.sh [options] [APP_DIR]

Check an AppDir (default: out/BerryOS.AppDir) without booting a guest.

Options:
  --file        print `file` classification for bundled artifacts
  --sha256      print SHA-256 for bundled artifacts
  --require-elf fail unless usr/bin/berry is a Linux ELF
  -q, --quiet   only warnings, failures, and the summary
  -h, --help    show this help

Checks: AppRun, usr/bin/berry, berryos.img, AAVMF_CODE.fd, the desktop entry,
permissions, AppRun environment variables, and the Exec/TryExec fields.
EOF
}

OK=0
WARN=0
FAIL=0
QUIET=0

pass() {
    OK=$((OK + 1))
    if [ "$QUIET" != 1 ]; then
        printf 'ok   %s\n' "$*"
    fi
}

warn() {
    WARN=$((WARN + 1))
    printf 'warn %s\n' "$*"
}

fail() {
    FAIL=$((FAIL + 1))
    printf 'FAIL %s\n' "$*"
}

APP=
SHOW_FILE=0
SHOW_SHA=0
REQUIRE_ELF=0
ENDOPTS=0
while [ $# -gt 0 ]; do
    if [ "$ENDOPTS" = 1 ]; then
        [ -z "$APP" ] || {
            printf 'check-appdir: too many arguments: %s\n' "$1" >&2
            exit 2
        }
        APP=$1
    else
        case "$1" in
            -h|--help)
                usage
                exit 0
                ;;
            --file)
                SHOW_FILE=1
                ;;
            --sha256)
                SHOW_SHA=1
                ;;
            --require-elf)
                REQUIRE_ELF=1
                ;;
            -q|--quiet)
                QUIET=1
                ;;
            --)
                ENDOPTS=1
                ;;
            -*)
                printf 'check-appdir: unknown option: %s\n' "$1" >&2
                usage >&2
                exit 2
                ;;
            *)
                [ -z "$APP" ] || {
                    printf 'check-appdir: too many arguments: %s\n' "$1" >&2
                    exit 2
                }
                APP=$1
                ;;
        esac
    fi
    shift
done

APP=${APP:-out/BerryOS.AppDir}
if [ ! -e "$APP" ]; then
    printf 'FAIL AppDir not found: %s\n' "$APP" >&2
    exit 1
fi
if [ ! -d "$APP" ]; then
    printf 'FAIL AppDir is not a directory: %s\n' "$APP" >&2
    exit 1
fi
APP=$(CDPATH= cd -- "$APP" && pwd -P) || {
    printf 'FAIL cannot enter AppDir: %s\n' "$APP" >&2
    exit 1
}

mode_of() {
    if stat -c '%a' "$1" >/dev/null 2>&1; then
        stat -c '%a' "$1"
    else
        stat -f '%Lp' "$1"
    fi
}

size_of() {
    wc -c < "$1" | tr -d '[:space:]'
}

rel_path() {
    printf '%s' "${1#"$APP"/}"
}

check_dir() {
    _path=$1
    _rel=$(rel_path "$_path")
    if [ ! -d "$_path" ]; then
        fail "directory missing: $_rel"
        return 0
    fi
    if [ ! -r "$_path" ] || [ ! -x "$_path" ]; then
        fail "directory is not readable/executable: $_rel"
        return 0
    fi
    pass "directory present: $_rel"
    return 0
}

check_file() {
    _path=$1
    _label=$2
    _need_x=$3
    _rel=$(rel_path "$_path")
    if [ ! -e "$_path" ]; then
        fail "$_label missing: $_rel"
        return 1
    fi
    if [ -L "$_path" ]; then
        fail "$_label is a symlink, expected a regular file: $_rel"
        return 1
    fi
    if [ ! -f "$_path" ]; then
        fail "$_label is not a regular file: $_rel"
        return 1
    fi
    if [ ! -r "$_path" ]; then
        fail "$_label is not readable: $_rel"
        return 1
    fi
    _size=$(size_of "$_path")
    if [ "$_size" -eq 0 ]; then
        fail "$_label is empty: $_rel"
        return 1
    fi
    _mode=$(mode_of "$_path")
    if [ "$_need_x" = 1 ]; then
        if [ ! -x "$_path" ]; then
            fail "$_label is not executable (mode $_mode): $_rel"
            return 1
        fi
    else
        if [ ! -r "$_path" ]; then
            fail "$_label is not readable (mode $_mode): $_rel"
            return 1
        fi
    fi
    pass "$_label present: $_rel (mode $_mode, $_size bytes)"
    return 0
}

hash_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        return 1
    fi
}

show_file() {
    _path=$1
    if [ -f "$_path" ]; then
        printf 'file %-38s %s\n' "$(rel_path "$_path")" "$(file -b "$_path" 2>/dev/null || printf 'unknown')"
    fi
}

show_sha() {
    _path=$1
    if [ -f "$_path" ]; then
        _hash=$(hash_file "$_path" 2>/dev/null || true)
        if [ -n "$_hash" ]; then
            printf 'sha256 %s  %s\n' "$_hash" "$(rel_path "$_path")"
        else
            warn "cannot compute SHA-256 for $(rel_path "$_path") (need sha256sum or shasum)"
        fi
    fi
}

printf 'AppDir: %s\n' "$APP"
if [ -r "$APP" ] && [ -x "$APP" ]; then
    pass "AppDir is readable and executable"
else
    fail "AppDir is not readable/executable"
fi

check_dir "$APP/usr"
check_dir "$APP/usr/bin"
check_dir "$APP/usr/share"
check_dir "$APP/usr/share/berryos"
check_dir "$APP/usr/share/applications"

APP_RUN=$APP/AppRun
APP_RUN_OK=0
if check_file "$APP_RUN" "AppRun" 1; then
    APP_RUN_OK=1
fi
if [ "$APP_RUN_OK" = 1 ]; then
    if sh -n "$APP_RUN" 2>/dev/null; then
        pass "AppRun shell syntax"
    else
        fail "AppRun has a shell syntax error"
    fi
    _first=$(sed -n '1p' "$APP_RUN")
    case "$_first" in
        '#!/bin/sh'|'#!/usr/bin/env sh')
            pass "AppRun shebang: $_first"
            ;;
        *)
            warn "AppRun shebang is not POSIX sh: $_first"
            ;;
    esac
    if grep -q 'BERRY_IMAGE' "$APP_RUN"; then
        pass "AppRun sets BERRY_IMAGE"
    else
        fail "AppRun does not set BERRY_IMAGE"
    fi
    if grep -q 'AAVMF_CODE' "$APP_RUN"; then
        pass "AppRun sets AAVMF_CODE"
    else
        fail "AppRun does not set AAVMF_CODE"
    fi
    if grep -q 'berryos.img' "$APP_RUN"; then
        pass "AppRun points at berryos.img"
    else
        warn "AppRun does not mention berryos.img"
    fi
    if grep -q 'usr/bin/berry' "$APP_RUN" && grep -q 'exec ' "$APP_RUN"; then
        pass "AppRun execs usr/bin/berry"
    else
        fail "AppRun does not exec usr/bin/berry"
    fi
fi

BERRY=$APP/usr/bin/berry
BERRY_OK=0
if check_file "$BERRY" "berry binary" 1; then
    BERRY_OK=1
fi
if [ "$BERRY_OK" = 1 ] && command -v file >/dev/null 2>&1; then
    _kind=$(file -b "$BERRY" 2>/dev/null || true)
    case "$_kind" in
        *ELF*)
            pass "berry binary is a Linux ELF: $_kind"
            ;;
        *)
            if [ "$REQUIRE_ELF" = 1 ]; then
                fail "berry binary is not a Linux ELF: $_kind"
            else
                warn "berry binary is not a Linux ELF: $_kind (use --require-elf to enforce)"
            fi
            ;;
    esac
fi

IMG=$APP/usr/share/berryos/berryos.img
if check_file "$IMG" "device image" 0; then
    _img_size=$(size_of "$IMG")
    if [ "$_img_size" -lt 1048576 ]; then
        warn "device image is smaller than 1 MiB ($_img_size bytes)"
    fi
fi

FW=$APP/usr/share/berryos/AAVMF_CODE.fd
if check_file "$FW" "AAVMF code" 0; then
    _fw_size=$(size_of "$FW")
    if [ "$_fw_size" -lt 1048576 ]; then
        warn "AAVMF code is smaller than 1 MiB ($_fw_size bytes)"
    fi
fi

DESKTOP=$APP/usr/share/applications/berry.desktop
if [ ! -f "$DESKTOP" ]; then
    for _candidate in "$APP"/*.desktop; do
        if [ -f "$_candidate" ]; then
            DESKTOP=$_candidate
            break
        fi
    done
fi
DESKTOP_OK=0
if check_file "$DESKTOP" "desktop entry" 0; then
    DESKTOP_OK=1
    case "$DESKTOP" in
        "$APP/usr/share/applications/"*)
            ;;
        *)
            warn "desktop entry is not at usr/share/applications/berry.desktop: $(rel_path "$DESKTOP")"
            ;;
    esac
fi

if [ "$DESKTOP_OK" = 1 ]; then
    if grep -q '^\[Desktop Entry\]' "$DESKTOP"; then
        pass "desktop entry has [Desktop Entry]"
    else
        fail "desktop entry is missing [Desktop Entry]"
    fi
    if grep -Eq '^Type[[:space:]]*=[[:space:]]*Application[[:space:]]*$' "$DESKTOP"; then
        pass "desktop entry Type=Application"
    else
        fail "desktop entry Type is not Application"
    fi
    if grep -Eq '^Name[[:space:]]*=[[:space:]]*[^[:space:]]' "$DESKTOP"; then
        pass "desktop entry has a Name"
    else
        fail "desktop entry is missing Name"
    fi

    _exec_count=$(awk '/^[[:space:]]*Exec[[:space:]]*=/{n++} END{print n+0}' "$DESKTOP")
    if [ "$_exec_count" -eq 1 ]; then
        pass "desktop entry has exactly one Exec"
    else
        fail "desktop entry must have exactly one Exec (found $_exec_count)"
    fi

    _exec_val=$(awk '/^[[:space:]]*Exec[[:space:]]*=/{sub(/^[^=]*=[[:space:]]*/,""); print; exit}' "$DESKTOP")
    if [ -z "$_exec_val" ]; then
        fail "desktop entry Exec is empty"
    else
        _exec_cmd=$(printf '%s\n' "$_exec_val" | awk '{print $1}')
        case "$_exec_cmd" in
            \"*\")
                _exec_cmd=${_exec_cmd#\"}
                _exec_cmd=${_exec_cmd%\"}
                ;;
            \'*\')
                _exec_cmd=${_exec_cmd#\'}
                _exec_cmd=${_exec_cmd%\'}
                ;;
        esac

        _bad_codes=$(printf '%s\n' "$_exec_val" | awk '{
            for (i = 1; i <= length($0); i++) {
                if (substr($0, i, 1) == "%") {
                    c = substr($0, i + 1, 1)
                    if (c == "") {
                        printf "%% "
                    } else if (c !~ /[fFuUdDnNickvm%]/) {
                        printf "%s ", c
                    }
                    i++
                }
            }
        }')
        if [ -n "$_bad_codes" ]; then
            warn "desktop entry Exec has unknown field code(s): $_bad_codes"
        fi

        case "$_exec_cmd" in
            '')
                fail "desktop entry Exec command is empty"
                ;;
            /*)
                fail "desktop entry Exec must not use an absolute path: $_exec_cmd"
                ;;
            *..*)
                fail "desktop entry Exec escapes the AppDir: $_exec_cmd"
                ;;
            */*)
                _exec_path="$APP/$_exec_cmd"
                if [ ! -e "$_exec_path" ]; then
                    fail "desktop entry Exec target is missing in AppDir: $_exec_cmd"
                elif [ ! -f "$_exec_path" ]; then
                    fail "desktop entry Exec target is not a file: $_exec_cmd"
                elif [ ! -x "$_exec_path" ]; then
                    fail "desktop entry Exec target is not executable: $_exec_cmd"
                else
                    pass "desktop entry Exec resolves inside AppDir: $_exec_cmd"
                fi
                ;;
            berry)
                if [ -x "$BERRY" ]; then
                    pass "desktop entry Exec=berry resolves to usr/bin/berry"
                else
                    fail "desktop entry Exec=berry but usr/bin/berry is not executable"
                fi
                ;;
            *)
                fail "desktop entry Exec is not the bundled berry binary: $_exec_cmd (use Exec=berry)"
                ;;
        esac
    fi

    if grep -Eq '^Terminal[[:space:]]*=[[:space:]]*false[[:space:]]*$' "$DESKTOP"; then
        pass "desktop entry Terminal=false"
    elif grep -Eq '^Terminal[[:space:]]*=' "$DESKTOP"; then
        warn "desktop entry Terminal is not false"
    else
        warn "desktop entry is missing Terminal=false"
    fi

    if grep -Eq '^Categories[[:space:]]*=[[:space:]]*[^[:space:]]' "$DESKTOP"; then
        pass "desktop entry has Categories"
    else
        warn "desktop entry is missing Categories"
    fi

    _tryexec=$(awk '/^[[:space:]]*TryExec[[:space:]]*=/{sub(/^[^=]*=[[:space:]]*/,""); print; exit}' "$DESKTOP")
    if [ -n "$_tryexec" ]; then
        case "$_tryexec" in
            berry)
                if [ -x "$BERRY" ]; then
                    pass "desktop entry TryExec=berry resolves to usr/bin/berry"
                else
                    fail "desktop entry TryExec=berry but usr/bin/berry is not executable"
                fi
                ;;
            /*)
                fail "desktop entry TryExec must not be absolute: $_tryexec"
                ;;
            *..*)
                fail "desktop entry TryExec escapes the AppDir: $_tryexec"
                ;;
            */*)
                if [ -x "$APP/$_tryexec" ]; then
                    pass "desktop entry TryExec resolves inside AppDir: $_tryexec"
                else
                    fail "desktop entry TryExec target is missing or not executable: $_tryexec"
                fi
                ;;
            *)
                warn "desktop entry TryExec does not resolve to bundled berry: $_tryexec"
                ;;
        esac
    fi
fi

if [ "$SHOW_FILE" = 1 ]; then
    if command -v file >/dev/null 2>&1; then
        show_file "$APP_RUN"
        show_file "$BERRY"
        show_file "$IMG"
        show_file "$FW"
        show_file "$DESKTOP"
    else
        warn "file command not found; cannot classify artifacts"
    fi
fi

if [ "$SHOW_SHA" = 1 ]; then
    show_sha "$APP_RUN"
    show_sha "$BERRY"
    show_sha "$IMG"
    show_sha "$FW"
    show_sha "$DESKTOP"
fi

if command -v du >/dev/null 2>&1; then
    printf 'size: %s\n' "$(du -sh "$APP" 2>/dev/null | awk '{print $1}')"
fi
printf '\nsummary: %d ok, %d warning(s), %d failure(s)\n' "$OK" "$WARN" "$FAIL"

if [ "$FAIL" -ne 0 ]; then
    exit 1
fi
exit 0
