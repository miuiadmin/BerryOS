#!/usr/bin/env bash
# gate-selftest.sh —— 门禁链负例自检（工程纪律刀阶段 3/4/6；规则/16 §1、§3、§5）。
# 合成 git 沙箱装真实 .githooks 钩子（pre-commit + prepare-commit-msg + pre-push），
# 八腿全触发才绿：
#   a 坏头块文档 staged → pre-commit 拒
#   b 好文档 staged（facts 同批刷新）→ pre-commit + prepare-commit-msg 全链
#     过（印记被消费）
#   c 坏文档 staged + --no-verify → prepare-commit-msg 印记闸拒（--no-verify
#     不跳本钩子——这是整个分层设计的关键腿）
#   d merge 流程（同源分支）→ 免检过（git 本就不跑 pre-commit，印记缺席合法）
#   e 工作树修好/索引留毒 → 拒（存亡判据：旧「读工作树」版钩子在此放行）
#   f 兄弟未跟踪坏文档在场 + 自己的好文档 staged → 过（多会话不误伤——
#     未跟踪内容对快照不可见，既不满足也不挡住本提交）
#   g 好文档 staged 但 facts 制品未同批刷新 → 快照门拒（--verify-facts
#     管道腿：计数漂移在提交点被拦，刷新后同内容过门）
# 改 .githooks/* 或 check-docs 检查面必跑（挂 just test-host；直接执行即可，
# 无参无旗标——selftest_guard 的 DECL 面不命中，接线行是纪律自觉位）。
set -u
REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
T=$(mktemp -d) || exit 2
trap 'rm -rf "$T"' EXIT
leg() { printf 'gate-selftest: %s\n' "$1"; }
die() { printf 'gate-selftest FAIL: %s\n' "$1" >&2; exit 1; }

# —— 沙箱：真实钩子 + 真实 check-docs + 最小合法文档面 + facts 制品 ——
# 全段剥 GIT_* 环境（审计 2026-11-22：外层 pre-commit 的 GIT_INDEX_FILE 会串进
# 沙箱的 git 调用导致 config 写错仓库——控制面门子进程里跑 gate-selftest 必红）
(
unset GIT_INDEX_FILE GIT_DIR GIT_WORK_TREE GIT_QUARANTINE_PATH 2>/dev/null
git init -q "$T/r" || exit 2
cd "$T/r" || exit 2
git config user.name gate-selftest
git config user.email gate-selftest@local
git config commit.gpgsign false
mkdir -p .githooks .agents/tools .agents/plans/mod1 docs tools
cp "$REPO/.githooks/pre-commit" "$REPO/.githooks/prepare-commit-msg" .githooks/
chmod +x .githooks/pre-commit .githooks/prepare-commit-msg
git config core.hooksPath .githooks
cp "$REPO/.agents/tools/check-docs.mjs" .agents/tools/
# 控制面条件测试门依赖件（pre-commit 尾段调用；清单+门本体随沙箱走）。
# gate-selftest.sh 本体用 exit-0 stub 替代（审计 r2）：真件进沙箱=自指无限递归
# （沙箱 commit→门→前缀测试 gate-selftest→再造沙箱…）；stub 断递归且不缺件，
# 沙箱内前缀级测试对 .githooks/ 空转可接受（真门已在外层真仓 staged 上跑过）。
mkdir -p tools
cp "$REPO/tools/control-plane-gate.sh" "$REPO/tools/control-plane.txt" tools/
printf '#!/bin/sh\n# gate-selftest 沙箱 stub（防递归占位）\nexit 0\n' > tools/gate-selftest.sh
chmod +x tools/gate-selftest.sh
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
# facts 制品（快照门 --verify-facts 消费；数字随腿滚动——模拟「动计数面
# 的同一提交带刷」纪律）。set_facts <docs_md> <design_md>
set_facts() {
  printf 'docs_md=%s\ndesign_md=%s\nstamped_designs=0\n' "$1" "$2" > tools/facts.txt
  git add tools/facts.txt
}
set_facts 2 1
git add .agents .githooks docs tools/control-plane-gate.sh tools/control-plane.txt tools/gate-selftest.sh
git commit -qm init || die "沙箱 init 提交未过门（最小合法面应过）"

bad_doc() { mkdir -p .agents/plans/mod2; printf '# 断头\n\nType: spec\nStatus: active\n' > .agents/plans/mod2/design.md; }
good_doc() { mkdir -p .agents/plans/mod2; printf '# mod2 总体设计\n\nType: spec\nStatus: active\nUpdated: 2026-11-21\nVersion: 1.0.0\n\n一句话说明：合法。\n' > .agents/plans/mod2/design.md; }
clean_leg() { git reset -q --hard HEAD >/dev/null 2>&1; git clean -qfd -e .githooks >/dev/null 2>&1; rm -f "$(git rev-parse --git-path berryos-gate.imprint)"; }

# 腿 a：坏头块 staged → pre-commit 拒
bad_doc; git add .agents/plans/mod2/design.md
if git commit -qm a 2>/dev/null; then die "腿 a：坏文档 staged 未被拒"; fi
clean_leg; leg "腿 a 过：坏头块 staged 被拒"

# 腿 b：好文档 staged（facts 同批刷新到 3/2）→ 全链过 + 印记被消费
good_doc; git add .agents/plans/mod2/design.md
set_facts 3 2
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

# 腿 f：兄弟未跟踪坏文档在场 + 自己的好文档 staged → 过
mkdir -p .agents/notes .agents/plans/mod3
printf '# 兄弟半成品\n\nType: note\n' > .agents/notes/sibling-wip.md
printf '# mod3 总体设计\n\nType: spec\nStatus: active\nUpdated: 2026-11-21\nVersion: 1.0.0\n\n一句话说明：合法。\n' > .agents/plans/mod3/design.md
git add .agents/plans/mod3/design.md
set_facts 4 3
git commit -qm f || die "腿 f：兄弟未跟踪半成品误伤了好提交"
clean_leg; leg "腿 f 过：兄弟未跟踪半成品不误伤"

# 腿 g：好文档 staged 但 facts 制品未同批刷新 → 快照门拒；刷新后同内容过
mkdir -p .agents/plans/mod4
printf '# mod4 总体设计\n\nType: spec\nStatus: active\nUpdated: 2026-11-21\nVersion: 1.0.0\n\n一句话说明：合法。\n' > .agents/plans/mod4/design.md
git add .agents/plans/mod4/design.md
if git commit -qm g 2>/dev/null; then die "腿 g：facts 漂移未被快照门拦"; fi
set_facts 5 4
git commit -qm g2 || die "腿 g：facts 刷新后同内容未过门"
clean_leg; leg "腿 g 过：facts 漂移快照门拒 + 刷新后过"

# 腿 g2：tools-only staged 的控制面门腿——清单内治理件（零 docs staged）提交，
# 控制面门必须调起且其自检红即拒（审计 r2：上轮 P0-2 半修的补腿——前缀修复的
# 端到端实证）。夹具：临时造 selftest exit 1 的假 e-guard.py 覆盖后还原。
cp .agents/tools/check-docs.mjs /tmp/cd-mjs.bak 2>/dev/null || true
mkdir -p tools
printf 'import sys\nsys.exit(1)\n' > /tmp/fake-guard.py
cp tools/e-guard.py /tmp/eg-bak.py
cp /tmp/fake-guard.py tools/e-guard.py
git add tools/e-guard.py
if git commit -qm g2 2>/dev/null; then
  cp /tmp/eg-bak.py tools/e-guard.py && git add tools/e-guard.py && git commit -qm g2-fix || true
  die "腿 g2：tools-only 红件未被控制面门拦（半修复发）"
fi
cp /tmp/eg-bak.py tools/e-guard.py
clean_leg; leg "腿 g2 过：tools-only 红件被控制面门拦"

# 腿 h：bot 身份闸（push 层）——bot 提交推集成分支拒、推 wip/auto-* 过
cp "$REPO/.githooks/pre-push" .githooks/ && chmod +x .githooks/pre-push
git add .githooks/pre-push && git commit -qm h0 || die "腿 h：pre-push 装载提交未过门"
# 注意：此处在沙箱仓配（-C 指向）而非主仓——2026-11-22 实抓泄漏（写进主仓
# 导致身份闸拦截了主仓自己的推送）。修复：只在沙箱内配。
sha=$(git rev-parse HEAD); z=0000000000000000000000000000000000000000
git config berryos.bot-email gate-selftest@local
if echo "refs/heads/$BR $sha refs/heads/$BR $z" | env -u GIT_INDEX_FILE -u GIT_DIR .githooks/pre-push origin u 2>/dev/null; then
  die "腿 h：bot 身份推集成分支未被拒"
fi
# 审计 r2 两条负腿：显式 refspec 把 bot 提交送往集成分支/main——落点闸必须拒
if echo "refs/heads/wip/auto-t $sha refs/heads/$BR $z" | env -u GIT_INDEX_FILE -u GIT_DIR .githooks/pre-push origin u 2>/dev/null; then
  die "腿 h：refspec 落点旁路（auto→集成分支）未被拒"
fi
if echo "refs/heads/wip/auto-t $sha refs/heads/main $z" | env -u GIT_INDEX_FILE -u GIT_DIR .githooks/pre-push origin u 2>/dev/null; then
  die "腿 h：refspec 落点旁路（auto→main）未被拒"
fi
echo "refs/heads/wip/auto-t $sha refs/heads/wip/auto-t $z" | env -u GIT_INDEX_FILE -u GIT_DIR .githooks/pre-push origin u \
  || die "腿 h：bot 身份推 wip/auto-* 被误拦"
git config --unset berryos.bot-email 2>/dev/null || true
leg "腿 h 过：bot 推集成分支拒 / wip/auto-* 过"

)  # 沙箱 subshell 结束——恢复外层环境
echo "gate-selftest: PASS（八腿：坏拒/好过+印记耗/no-verify 拦/merge 免检/索引留毒拒/未跟踪不误伤/facts 漂移门拒/bot 身份 push 闸）"
