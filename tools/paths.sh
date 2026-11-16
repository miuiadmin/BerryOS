#!/usr/bin/env bash
# 内核产物路径单一解析点（审查报告清理项：justfile / smoke.sh / CI 三处各复制一份
# cargo metadata 管线，漂移面 ×3——改这里一处即可）。
#
# 用法（须在仓库根，与 cargo 一致）：
#   tools/paths.sh kernel          打印 debug 内核 ELF 路径
#   tools/paths.sh kernel-release  打印 release 内核 ELF 路径
#   tools/paths.sh target-dir      打印 cargo target 目录
#   source tools/paths.sh          定义 berryos_target_dir / berryos_kernel 函数
#
# target 目录可能被全局 ~/.cargo/config.toml 的 target-dir 覆盖（本机指向
# ~/.cargo-target），故动态解析不硬编码；CARGO_TARGET_DIR 环境变量同样被尊重。

# ---------- --selftest（合成垫片沙箱，零真仓零真工具链；改解析/拒面/CLI 形态必跑） ----------
# 入口卫兵 = 直跑判断（BASH_SOURCE == $0）：sourced 时 $1 属于调用者
# （bcts-run.sh/smoke.sh `source paths.sh --selftest` 会把调用者的 --selftest
# 穿透进来——sourced 形态只定义函数，永不入自检）。
if [[ "${1:-}" == "--selftest" && "${BASH_SOURCE[0]}" == "$0" ]]; then
  SELF=$(mktemp -d "${TMPDIR:-/tmp}/berryos-paths-selftest.XXXXXX")
  cleanup() { rm -rf "$SELF"; }
  trap cleanup EXIT
  mkdir -p "$SELF/bin" "$SELF/bin_silent" "$SELF/bin_onlycargo" "$SELF/bin_nolld" \
           "$SELF/sysroot/lib/rustlib/fakehost/bin" "$SELF/home"
  S="$0"

  # 垫片 cargo：只答 metadata 一问（target_directory 单字段 JSON 面）。
  cat > "$SELF/bin/cargo" <<'SHIM'
#!/bin/sh
[ "$1" = metadata ] && { printf '%s\n' '{"target_directory":"/fake/target"}'; exit 0; }
exit 1
SHIM
  chmod +x "$SELF/bin/cargo"
  cp "$SELF/bin/cargo" "$SELF/bin_onlycargo/cargo"
  printf '#!/bin/sh\nexit 1\n' > "$SELF/bin_silent/cargo"
  chmod +x "$SELF/bin_silent/cargo"

  # 垫片 rustc：sysroot 指沙箱内绝对路径（rust-lld 存在/缺席两树）。
  cat > "$SELF/bin/rustc" <<SHIM
#!/bin/sh
if [ "\$1" = "--print" ] && [ "\$2" = sysroot ]; then printf '%s\n' "$SELF/sysroot"; exit 0; fi
[ "\$1" = -vV ] && { printf 'host: fakehost\n'; exit 0; }
exit 1
SHIM
  chmod +x "$SELF/bin/rustc"
  cat > "$SELF/bin_nolld/rustc" <<SHIM
#!/bin/sh
if [ "\$1" = "--print" ] && [ "\$2" = sysroot ]; then printf '%s\n' "$SELF/sysroot_empty"; exit 0; fi
[ "\$1" = -vV ] && { printf 'host: fakehost\n'; exit 0; }
exit 1
SHIM
  chmod +x "$SELF/bin_nolld/rustc"
  printf '#!/bin/sh\n' > "$SELF/sysroot/lib/rustlib/fakehost/bin/rust-lld"
  chmod +x "$SELF/sysroot/lib/rustlib/fakehost/bin/rust-lld"

  legs=0; fails=0
  leg() {  # leg <名> <PATH> <期望rc> <锚面:stdout|stderr|-> <锚> <参数…>
    name="$1"; p="$2"; want="$3"; face="$4"; pat="$5"; shift 5
    rc=0
    ( PATH="$p" HOME="$SELF/home" bash "$S" "$@" ) >"$SELF/out.log" 2>&1 || rc=$?
    legs=$((legs+1)); why=""
    [ "$rc" = "$want" ] || why="rc 期望 ${want} 实得 ${rc}"
    if [ -z "$why" ] && [ "$face" != "-" ] && ! grep -qF "$pat" "$SELF/out.log"; then
      why="输出缺锚「${pat}」"
    fi
    if [ -n "$why" ]; then
      fails=$((fails+1)); echo "paths.sh selftest 腿红：${name}（${why}）" >&2
      tail -4 "$SELF/out.log" >&2
    else
      echo "paths.sh selftest 腿绿：${name}"
    fi
  }

  PB="$SELF/bin:/usr/bin:/bin"
  PSL="$SELF/bin_silent:/usr/bin:/bin"
  PNO="$SELF/bin_onlycargo:/usr/bin:/bin"
  PNL="$SELF/bin_nolld:/usr/bin:/bin"

  leg "target-dir 解析" "$PB" 0 stdout "/fake/target" target-dir
  leg "kernel 组装（debug 默认档）" "$PB" 0 stdout "/fake/target/aarch64-unknown-none/debug/berry-kernel" kernel
  leg "kernel-release 组装" "$PB" 0 stdout "/fake/target/aarch64-unknown-none/release/berry-kernel" kernel-release
  leg "cargo 垫片哑拒（metadata 空手）" "$PSL" 1 stderr "cargo 不可用" target-dir
  leg "cargo 缺席拒（HOME 沙箱化——PATH 自足修补无可拾真件）" "/usr/bin:/bin" 1 stderr "cargo 不可用" target-dir
  leg "rust-lld 解析（sysroot/host 组装）" "$PB" 0 stdout "lib/rustlib/fakehost/bin/rust-lld" rust-lld
  leg "rust-lld 缺席拒" "$PNL" 1 stderr "rust-lld 不存在" rust-lld
  leg "rustc 缺席拒" "$PNO" 1 stderr "rustc 不可用" rust-lld
  leg "未知子命令 usage 拒" "$PB" 2 stderr "usage" bogus-subcommand

  # source 形态：只定义函数零副作用（CLI 卫兵不触发），三函数齐且无杂音。
  rc=0
  ( PATH="$PB" HOME="$SELF/home" bash -c \
    'source "$1" && printf "%s\n" "$(declare -F berryos_target_dir berryos_kernel berryos_rust_lld | wc -l | tr -d " ")"' \
    _ "$S" ) >"$SELF/out.log" 2>&1 || rc=$?
  legs=$((legs+1)); why=""
  [ "$rc" = 0 ] || why="rc 期望 0 实得 ${rc}"
  if [ -z "$why" ] && [ "$(cat "$SELF/out.log")" != 3 ]; then
    why="函数计数期望 3 实得 $(cat "$SELF/out.log")"
  fi
  if [ -n "$why" ]; then
    fails=$((fails+1)); echo "paths.sh selftest 腿红：source 形态三函数（${why}）" >&2
    tail -4 "$SELF/out.log" >&2
  else
    echo "paths.sh selftest 腿绿：source 形态三函数"
  fi

  echo "paths.sh --selftest: ${legs} 腿，${fails} 红"
  [ "$fails" -eq 0 ] || exit 1
  exit 0
fi

# 非交互 shell 常缺 ~/.cargo/bin（与 justfile 开头同病），补上再找 cargo。
if ! command -v cargo >/dev/null 2>&1; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

berryos_target_dir() {
  local td
  td="$(cargo metadata --no-deps --format-version=1 2>/dev/null \
    | grep -o '"target_directory":"[^"]*"' | cut -d'"' -f4)"
  if [[ -z "$td" ]]; then
    # cargo 不可用/异常配置：不吐残缺根路径，凭退出码让下游感知（3b3 审查 M4）。
    echo "paths.sh: cargo 不可用或无法解析 target 目录" >&2
    return 1
  fi
  printf '%s\n' "$td"
}

# $1 = debug|release（默认 debug）。
berryos_kernel() {
  local td
  td="$(berryos_target_dir)" || return 1
  echo "$td/aarch64-unknown-none/${1:-debug}/berry-kernel"
}

# rust-lld 随 rustup 走（sysroot 内）。justfile 属性背引号（RUST_LLD :=）在
# recipe 环境外求值、无 `export PATH :=` 修补（justfile :8-9 注释自载的 gotcha
# ——GUI-1 段① just image 实踩：/lib/rustlib//bin/rust-lld），故经本脚本解析。
berryos_rust_lld() {
  local sysroot host lld
  sysroot="$(rustc --print sysroot 2>/dev/null)" \
    || { echo "paths.sh: rustc 不可用（PATH 自足修补后仍失败）" >&2; return 1; }
  host="$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')"
  lld="$sysroot/lib/rustlib/$host/bin/rust-lld"
  [[ -x "$lld" ]] || { echo "paths.sh: rust-lld 不存在：$lld" >&2; return 1; }
  printf '%s\n' "$lld"
}

# 直接执行时按参数出路径；被 source 时不做任何事（只定义函数）。
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  case "${1:-}" in
    kernel)         berryos_kernel debug ;;
    kernel-release) berryos_kernel release ;;
    target-dir)     berryos_target_dir ;;
    rust-lld)       berryos_rust_lld ;;
    *) echo "usage: $0 {kernel|kernel-release|target-dir|rust-lld}" >&2; exit 2 ;;
  esac
fi
