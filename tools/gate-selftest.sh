#!/usr/bin/env bash
# gate-selftest.sh —— 门禁链负例自检（工程纪律刀阶段 3；规则/16 §1）。
# 合成 git 沙箱装真实 .githooks 钩子（pre-commit + prepare-commit-msg），
# 六腿全触发才绿：
#   a 坏头块文档 staged → pre-commit 拒
#   b 好文档 staged → pre-commit + prepare-commit-msg 全链过（印记被消费）
#   c 坏文档 staged + --no-verify → prepare-commit-msg 印记闸拒（--no-verify
#     不跳本钩子——这是整个分层设计的关键腿）
#   d merge 流程（同源分支）→ 免检过（git 本就不跑 pre-commit，印记缺席合法）
#   e 工作树修好/索引留毒 → 拒（存亡判据：旧「读工作树」版钩子在此形态放行）
#   f 兄弟未跟踪坏文档在场 + 自己的好文档 staged → 过（多会话不误伤——
#     未跟踪内容对快照不可见，既不满足也不挡住本提交）
# 改 .githooks/* 或 check-docs 检查面必跑（挂 just test-host；直接执行即可，
# 无参无旗标——selftest_guard 的 DECL 面不命中，接线行是纪律自觉位）。
set -u
REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
T=$(mktemp -d) || exit 2
trap 'rm -rf "$T"' EXIT
leg() { printf 'gate-selftest: %s\n' "$1"; }
die() { printf 'gate-selftest FAIL: %s\n' "$1" >&2; exit 1; }

# —— 沙箱：真实钩子 + 真实 check-docs + 最小合法文档面 ——
git init -q "$T/r" || exit 2
cd "$T/r" || exit 2
git config user.name gate-selftest
git config user.email gate-selftest@local
git config commit.gpgsign false
mkdir -p .githooks .agents/tools .agents/plans/mod1 docs
cp "$REPO/.githooks/pre-commit" "$REPO/.githooks/prepare-commit-msg" .githooks/
chmod +x .githooks/pre-commit .githooks/prepare-commit-msg
git config core.hooksPath .githooks
cp "$REPO/.agents/tools/check-docs.mjs" .agents/tools/
# docs/ 必须有被跟踪文件（空目录不进 git——checkout-index 不导出，显式面
# 报「路径不存在」）：自由知识无头块形态合法。
printf '# 沙箱自由知识\n\n一句话说明：gate-selftest 夹具。\n' > docs/free.md
cat > .agents/plans/mod1/design.md <<'EOF'
# mod1 总体设计

Type: spec
Status: active
Updated: 2026-11-21
Version: 1.0.0

一句话说明：最小合法模块设计文档（gate-selftest 夹具）。
EOF
git add .agents .githooks docs
git commit -qm init || die "沙箱 init 提交未过门（腿 b 前置——最小合法面应过）"

bad_doc() { mkdir -p .agents/plans/mod2; printf '# 断头\n\nType: spec\nStatus: active\n' > .agents/plans/mod2/design.md; }
good_doc() { mkdir -p .agents/plans/mod2; printf '# mod2 总体设计\n\nType: spec\nStatus: active\nUpdated: 2026-11-21\nVersion: 1.0.0\n\n一句话说明：合法。\n' > .agents/plans/mod2/design.md; }
clean_leg() { git reset -q --hard HEAD >/dev/null 2>&1; git clean -qfd -e .githooks >/dev/null 2>&1; rm -f "$(git rev-parse --git-path berryos-gate.imprint)"; }

# 腿 a：坏头块 staged → pre-commit 拒
bad_doc; git add .agents/plans/mod2/design.md
if git commit -qm a 2>/dev/null; then die "腿 a：坏文档 staged 未被拒"; fi
clean_leg; leg "腿 a 过：坏头块 staged 被拒"

# 腿 b：好文档 staged → 全链过 + 印记被消费
good_doc; git add .agents/plans/mod2/design.md
git commit -qm b || die "腿 b：好文档未过门"
[ -f "$(git rev-parse --git-path berryos-gate.imprint)" ] && die "腿 b：印记未被消费（消费即删失灵）"
clean_leg; leg "腿 b 过：好文档全链过 + 印记即耗"

# 腿 c：坏文档 + --no-verify → 印记闸拒（--no-verify 不跳 prepare-commit-msg）
bad_doc; git add .agents/plans/mod2/design.md
if git commit --no-verify -qm c 2>/dev/null; then die "腿 c：--no-verify 绕过未被印记闸拦"; fi
clean_leg; leg "腿 c 过：--no-verify 被印记闸拦"

# 腿 d：merge 流程免检过（主支名不假设——init.defaultBranch 各机不同）
BR=$(git symbolic-ref --short HEAD)
git checkout -qb side && echo s > s.txt && git add s.txt && git commit -qm s1
git checkout -q "$BR"
git merge --no-ff side -m d || die "腿 d：merge 免检未过"
clean_leg; leg "腿 d 过：merge 流程免检（无印记合法）"

# 腿 e：工作树修好/索引留毒 → 拒（存亡判据）
bad_doc; git add .agents/plans/mod2/design.md
good_doc   # 工作树改成好的——旧「读工作树」钩子形态在此放行
if git commit -qm e 2>/dev/null; then die "腿 e：索引留毒未被拒（存亡判据——快照语义失灵）"; fi
clean_leg; leg "腿 e 过：工作树修好/索引留毒被拒"

# 腿 f：兄弟未跟踪坏文档在场 + 自己的好文档 staged → 过（mod3 新文件——
# mod2 自腿 b 起已入 HEAD，重写同内容无差异不可作 staged 证物）
mkdir -p .agents/notes .agents/plans/mod3
printf '# 兄弟半成品\n\nType: note\n' > .agents/notes/sibling-wip.md
printf '# mod3 总体设计\n\nType: spec\nStatus: active\nUpdated: 2026-11-21\nVersion: 1.0.0\n\n一句话说明：合法。\n' > .agents/plans/mod3/design.md
git add .agents/plans/mod3/design.md
git commit -qm f || die "腿 f：兄弟未跟踪半成品误伤了好提交"
clean_leg; leg "腿 f 过：兄弟未跟踪半成品不误伤"

echo "gate-selftest: PASS（六腿：坏拒/好过+印记耗/no-verify 拦/merge 免检/索引留毒拒/未跟踪不误伤）"
