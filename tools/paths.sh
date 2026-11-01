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
