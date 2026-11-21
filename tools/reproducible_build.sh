#!/bin/bash
# reproducible_build.sh——G1 刀 4：确定性密封构建包装器。
#
# 用法：tools/reproducible_build.sh [just 目标...（默认 build）]
#
# 原理（生态闭环计划刀 4）：
#   1. SOURCE_DATE_EPOCH 钉时间戳（git log -1 --format=%ct——同一 commit
#      恒同值；未提交树回退 0=epoch）
#   2. RUSTFLAGS 注入 -C remap-path-width=4 + --remap-path-prefix（源码
#      路径→/berryos 抹平；debug 信息内绝对路径统一化）
#   3. CARGO_NET_OFFLINE=true（密封：网络不可达——已缓存依赖 lock 步进）
#   4. umask 0022 统一文件模式
#   5. TZ=UTC（时区敏感工具如 zip/tar）
#
# 验证：两台异构主机（不同目录/用户名）产出 SHA-256 一致（plan 验收线）。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# 1. SOURCE_DATE_EPOCH：同一 commit 恒同值。
if EPOCH=$(git -C "$ROOT" log -1 --format=%ct 2>/dev/null); then
  export SOURCE_DATE_EPOCH="$EPOCH"
else
  export SOURCE_DATE_EPOCH="0"
fi

# 2. 路径抹平：remap 调用方 PWD → /berryos。
#    注：不设 CARGO_TARGET_DIR（调用方自管隔离——berryos-dev 技能纪律）。
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix ${ROOT}=/berryos"

# 3. 密封网络。
export CARGO_NET_OFFLINE=true

# 4. 统一 umask 与时区。
umask 0022
export TZ=UTC

# 5. 统一 LC_ALL（排序/大小写折算确定性）。
export LC_ALL=C

echo "reproducible_build: SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH PWD→/berryos remapped OFFLINE=true TZ=UTC umask=0022"

# 6. 执行 just 目标（默认 build）。
TARGETS="${@:-build}"
exec just -f "$ROOT/justfile" -d "$ROOT" $TARGETS
