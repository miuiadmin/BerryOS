#!/bin/sh
# BerryOS 钩子安装器（工程纪律刀阶段 3；docs/规则/16-工程纪律.md §1）——
# 新克隆第一步：bash .githooks/install.sh（AGENTS.md 本地追加第 10 条指定动作）。
# 作用：core.hooksPath 钉 .githooks（相对路径，克隆搬迁不断链）+ 全件可执行 + 自验。
# 注：core.hooksPath 是本机仓库配置（不随克隆走）——每台新机/新克隆各跑一次。
set -e
cd "$(dirname "$0")/.."
git config core.hooksPath .githooks
for h in pre-commit pre-push prepare-commit-msg reference-transaction; do
  chmod +x ".githooks/$h" 2>/dev/null || true
  [ -x ".githooks/$h" ] || { echo "install: FAIL .githooks/$h 缺席或不可执行" >&2; exit 1; }
done
echo "install: ok  core.hooksPath = $(git config core.hooksPath)"
echo "install: PASS——四钩在位（pre-commit 快照文档门 + 跨根闸 / prepare-commit-msg 印记闸 / pre-push main 守门 / reference-transaction ref 兜底，末者需 git >= 2.28）"
