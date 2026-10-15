#!/usr/bin/env bash
# 内核产物路径单一解析点（审查报告清理项：justfile / smoke.sh / CI 三处各复制一份
# cargo metadata 管线，漂移面 ×3——改这里一处即可）。
#
# 用法（须在仓库根，与 cargo 一致）：
#   工具链/paths.sh kernel          打印 debug 内核 ELF 路径
#   工具链/paths.sh kernel-release  打印 release 内核 ELF 路径
#   工具链/paths.sh target-dir      打印 cargo target 目录
#   source 工具链/paths.sh          定义 berryos_target_dir / berryos_kernel 函数
#
# target 目录可能被全局 ~/.cargo/config.toml 的 target-dir 覆盖（本机指向
# ~/.cargo-target），故动态解析不硬编码；CARGO_TARGET_DIR 环境变量同样被尊重。

# 非交互 shell 常缺 ~/.cargo/bin（与 justfile 开头同病），补上再找 cargo。
if ! command -v cargo >/dev/null 2>&1; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

berryos_target_dir() {
  cargo metadata --no-deps --format-version=1 2>/dev/null \
    | grep -o '"target_directory":"[^"]*"' | cut -d'"' -f4
}

# $1 = debug|release（默认 debug）。
berryos_kernel() {
  echo "$(berryos_target_dir)/aarch64-unknown-none/${1:-debug}/bos-kernel"
}

# 直接执行时按参数出路径；被 source 时不做任何事（只定义函数）。
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  case "${1:-}" in
    kernel)         berryos_kernel debug ;;
    kernel-release) berryos_kernel release ;;
    target-dir)     berryos_target_dir ;;
    *) echo "usage: $0 {kernel|kernel-release|target-dir}" >&2; exit 2 ;;
  esac
fi
