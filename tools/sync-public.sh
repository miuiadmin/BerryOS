#!/usr/bin/env bash
# sync-public.sh —— BerryOS 双仓单向镜像导出（私密主仓 → 公开镜像）
#
# 模型（2026-10-24 裁决）：
#   私密主仓（origin，全量 + 全部历史）= 唯一开发现场
#   公开仓  （public 远端，白名单子集，fresh 历史）= 发布门面
#
# 纪律：
#   1. 单向导出——公开仓的改动永不反向合并回主仓
#   2. 白名单制——不在清单上的一律不进公开仓（默认拒绝）
#   3. 导出源 = git archive HEAD（只含在册文件，gitignored 产物天然不进）
#   4. 首次使用：目标仓为 fresh init（见脚本尾部提示）；此后每次同步 = 普通提交叠加
#
# 用法：tools/sync-public.sh <公开仓本地克隆路径> [提交说明]
set -euo pipefail

SRC=$(cd "$(dirname "$0")/.." && pwd)
DEST=${1:?用法: sync-public.sh <公开仓本地克隆路径> [提交说明]}
MSG=${2:-"mirror: sync from private @ $(git -C "$SRC" rev-parse --short HEAD)"}

[ -d "$DEST/.git" ] || { echo "ERR: $DEST 不是 git 仓库（首次请先 git init 并接好 public 远端）" >&2; exit 1; }

# ---------- 白名单（顶层条目；默认拒绝制） ----------
WHITELIST=(
  kernel mm sched ipc arch bootloader drivers fs net user tools
  docs
  .github .cargo
  justfile Cargo.toml Cargo.lock LICENSE
  README.md README.zh-CN.md .gitignore
)

# docs/ 内部排除——内部过程档只住私密仓（复盘/审查/裁决/沿革/盘点）
DOCS_EXCLUDE=(
  阶段沿革.md
  战役复盘-2026-10.md
  审查报告-2026-10-15-2b.md
  审查报告-2026-10-16-3b3.md
  战略裁决-2026-10-15.md
  Phase4收官盘点.md
  P6收官盘点.md
)

# ---------- 导出 HEAD 快照 ----------
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
git -C "$SRC" archive HEAD | tar -x -C "$TMP"

# ---------- 逐条目同步（白名单内 rsync --delete，保证删除传播） ----------
for item in "${WHITELIST[@]}"; do
  if [ -d "$TMP/$item" ]; then
    # 源带尾斜杠 = 拷内容（非整目录塞入）：macOS openrsync 对不存在目标
    # 的「rsync -a src dst」是嵌套语义（首版实踩：14 顶层全变 kernel/kernel），
    # GNU rsync 是落位语义——内容形式在两者下行为一致且幂等。仅对目录。
    mkdir -p "$DEST/$item"
    rsync -a --delete "$TMP/$item/" "$DEST/$item/"
  elif [ -f "$TMP/$item" ]; then
    cp -p "$TMP/$item" "$DEST/$item"
  elif [ -e "$DEST/$item" ]; then
    rm -rf "$DEST/$item"   # 主仓已删 → 镜像跟随删除
  fi
done

# docs 内部排除
for f in "${DOCS_EXCLUDE[@]}"; do
  rm -f "$DEST/docs/$f"
done

# 镜像侧导航注记（主仓无此文件——每次同步重写）
cat > "$DEST/docs/MIRROR.md" <<'EOF'
# 公开镜像说明

本仓库是 BerryOS 开发主仓的白名单镜像（单向导出，fresh 历史）。
部分中文设计文档（阶段沿革 / 复盘 / 审查报告 / 战略裁决 / 收官盘点）
属于内部过程记录，仅存于开发主仓，不在此镜像中——`docs/索引.md`
中对应行的链接在本镜像内不可达。核心设计文档全部在册。
EOF

# 清掉镜像侧白名单之外的顶层残留（防历史遗留泄漏）
for entry in "$DEST"/* "$DEST"/.[!.]*; do
  [ -e "$entry" ] || continue
  base=$(basename "$entry")
  [ "$base" = ".git" ] && continue   # 镜像仓自身元数据永不动（首版实踩：.[!.]* 连 .git 一起扬了）
  ok=0
  for item in "${WHITELIST[@]}"; do [ "$base" = "$item" ] && ok=1; done
  if [ $ok -eq 0 ]; then
    echo "镜像侧移除非白名单条目: $base"
    rm -rf "$entry"
  fi
done

# ---------- 提交 ----------
git -C "$DEST" add -A
if git -C "$DEST" diff --cached --quiet; then
  echo "无变化，跳过提交"
else
  git -C "$DEST" commit -m "$MSG"
  echo "已提交。推送到 public 远端请手动：git -C $DEST push public main"
fi
