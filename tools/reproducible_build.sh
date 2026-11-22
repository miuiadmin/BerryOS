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

# ---------- --selftest（合成 git+just 垫片沙箱，零真构建零真仓副作用；改密封面必跑） ----------
# 入口卫兵 = 直跑判断（paths.sh 先例）：$1 为 --selftest 且非 sourced。
if [[ "${1:-}" == "--selftest" && "${BASH_SOURCE[0]}" == "$0" ]]; then
  SELF=$(mktemp -d "${TMPDIR:-/tmp}/berryos-repro-selftest.XXXXXX")
  cleanup() { rm -rf "$SELF"; }
  trap cleanup EXIT

  # 真件就地测（垫片拦截 git/exec 两出口——脚本本体与生产路径零改动）。
  REAL_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
  S="$0"

  mkdir -p "$SELF/bin" "$SELF/bin_nogit" "$SELF/bin_nojust"
  # 垫片 git：log 子命令 → 固定纪元；其余恒红。
  cat > "$SELF/bin/git" <<'SHIM'
#!/bin/sh
for a in "$@"; do
  [ "$a" = "log" ] && { echo 1700000000; exit 0; }
done
exit 1
SHIM
  chmod +x "$SELF/bin/git"
  printf '#!/bin/sh\nexit 128\n' > "$SELF/bin_nogit/git"
  chmod +x "$SELF/bin_nogit/git"
  # 垫片 just：回显 exec 落地视图（argv + 五密封环境面 + umask 观测）。
  cat > "$SELF/bin/just" <<'SHIM'
#!/bin/sh
echo "just-args:$*"
echo "SDE=${SOURCE_DATE_EPOCH-unset}"
echo "OFF=${CARGO_NET_OFFLINE-unset}"
echo "TZV=${TZ-unset}"
echo "LCV=${LC_ALL-unset}"
echo "RF=${RUSTFLAGS-unset}"
echo "UM=$(umask)"
SHIM
  chmod +x "$SELF/bin/just"
  cp "$SELF/bin/just" "$SELF/bin_nogit/just"
  cp "$SELF/bin/git" "$SELF/bin_nojust/git"

  legs=0; fails=0; RC=0
  leg() {  # leg <名> <期望rc> <锚串（换行分隔；空串=只判 rc）
    name="$1"; want="$2"; anchors="$3"
    legs=$((legs+1)); why=""
    [ "$RC" = "$want" ] || why="rc 期望 ${want} 实得 ${RC}"
    while IFS= read -r a; do
      [ -z "$a" ] && continue
      if [ -z "$why" ] && ! grep -qF -- "$a" "$SELF/out.log"; then
        why="输出缺锚「${a}」"
      fi
    done <<EOF
$anchors
EOF
    if [ -n "$why" ]; then
      fails=$((fails+1))
      echo "reproducible_build selftest 腿红：${name}（${why}）" >&2
      tail -8 "$SELF/out.log" >&2
    else
      echo "reproducible_build selftest 腿绿：${name}"
    fi
  }

  PB="$SELF/bin:/usr/bin:/bin"
  PNG="$SELF/bin_nogit:/usr/bin:/bin"
  PNJ="$SELF/bin_nojust:/usr/bin:/bin"

  # 腿 1：五密封面全席（垫片 git 出固定纪元 → exec 视图逐项对账）。
  RC=0
  ( PATH="$PB" bash "$S" ) >"$SELF/out.log" 2>&1 || RC=$?
  leg "五密封面全席（纪元/离线/umask/TZ/LC_ALL/remap/exec argv）" 0 \
"just-args:-f ${REAL_ROOT}/justfile -d ${REAL_ROOT} build
SDE=1700000000
OFF=true
TZV=UTC
LCV=C
UM=0022
reproducible_build: SOURCE_DATE_EPOCH=1700000000
--remap-path-prefix
${REAL_ROOT}=/berryos"

  # 腿 2：非 git 树回退 epoch 0（git 恒红 → 未提交树缺省分支）。
  RC=0
  ( PATH="$PNG" bash "$S" ) >"$SELF/out.log" 2>&1 || RC=$?
  leg "非 git 树回退 epoch 0" 0 "SDE=0
reproducible_build: SOURCE_DATE_EPOCH=0"

  # 腿 3：just 目标透传（多目标原序，缺省 build 之外）。
  RC=0
  ( PATH="$PB" bash "$S" alpha beta ) >"$SELF/out.log" 2>&1 || RC=$?
  leg "目标透传（多目标原序）" 0 \
    "just-args:-f ${REAL_ROOT}/justfile -d ${REAL_ROOT} alpha beta"

  # 腿 4：RUSTFLAGS 叠加不覆盖（调用方旗标保留 + remap 追加）。
  RC=0
  ( PATH="$PB" RUSTFLAGS="-Zkeep" bash "$S" ) >"$SELF/out.log" 2>&1 || RC=$?
  leg "RUSTFLAGS 叠加（调用方旗标保留）" 0 "RF=-Zkeep --remap-path-prefix"

  # 腿 5：just 缺席 loud 拒（密封包装不静默吞 exec 失败——127 透传；git 垫片在场纪元照常解析）。
  RC=0
  ( PATH="$PNJ" bash "$S" ) >"$SELF/out.log" 2>&1 || RC=$?
  leg "just 缺席 loud 拒（127）" 127 "reproducible_build: SOURCE_DATE_EPOCH=1700000000"

  echo "reproducible_build.sh --selftest: ${legs} 腿，${fails} 红"
  [ "$fails" -eq 0 ] || exit 1
  exit 0
fi

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
