#!/usr/bin/env bash
# sync-public.sh —— BerryOS 双仓单向镜像导出（私密主仓 → 公开镜像仓）
#
# 模型（2026-10-24 裁决；2026-11-11 形态适配重构）：
#   私密主仓（origin，全量 + 全部历史）= 唯一开发现场
#   公开仓  （public 远端，白名单子集，fresh 历史）= 发布门面
#
# 纪律：
#   1. 单向导出——公开仓的改动永不反向合并回主仓
#   2. 白名单制——不在清单上的一律不进公开仓（默认拒绝）
#   3. 源 = git archive HEAD（只含在册文件，gitignored 产物天然不进）
#   4. 形态适配：镜像不是搬运——CI/构建门/内部档三层随同步自动适配公开形态：
#       a. workflows → ubuntu-latest + 内网拓扑注释剥除
#       b. justfile docs-guard 解耦（公开仓构建不依赖内部档 AGENTS.md）
#       c. tools/ docs/ 内部过程档排净 + 出口安检推前必过
#
# 用法：tools/sync-public.sh <公开仓本地克隆路径> [提交说明]
#       tools/sync-public.sh --check-members   # workspace ⊆ 白名单对账（test-host 前置）
#       tools/sync-public.sh --selftest        # 门禁自身钉板（零镜像仓库、零网络）
set -euo pipefail

SRC=$(cd "$(dirname "$0")/.." && pwd)

MODE=sync
case "${1:-}" in
  --check-members) MODE=check-members ;;
  --selftest) MODE=selftest ;;
esac

# ---------- workspace members 对账门（同步律机器执法） ----------
# 根因（#153/#171/#227 三连 REJECT 判例族）：workspace 新增顶层 crate 而忘补
# WHITELIST = 公开仓 members 引用残缺 → 外部 cargo build 必炸。人工对账三连
# 漏后升机器门：members ⊆ WHITELIST 不满足即红且列差集（同步时自动跑；独立
# 跑 = `--check-members`，挂 test-host 前置防回归）。
check_members() {
  # members 目录可参数化（selftest 红分支腿用合成仓验）；白名单恒从本脚本
  # 自身解析（BASH_SOURCE 单源，不随参数漂移）。
  local members_dir="${1:-$SRC}"
  python3 - "${BASH_SOURCE[0]}" "$members_dir" <<'PYM'
import sys, tomllib, re
from pathlib import Path
script = Path(sys.argv[1]).read_text(encoding='utf-8')
src = Path(sys.argv[2])
m = re.search(r'WHITELIST=\((.*?)\n\)', script, re.S)
items = set()
for ln in m.group(1).splitlines():
    ln = ln.strip()
    if not ln or ln.startswith('#'):
        continue
    items.update(ln.split())
with open(src / 'Cargo.toml', 'rb') as f:
    ws = tomllib.load(f)
members = [x for x in ws.get('workspace', {}).get('members', []) if '*' not in x]
# 嵌套 member（如 arch/aarch64）：其顶层目录（arch）在白名单即覆盖——
# rsync 按顶层目录整目录导出，嵌套 crate 随目录进镜像
top = lambda x: x.split('/')[0]
missing = sorted(x for x in members if top(x) not in items)
if missing:
    print(f"sync-public: FAIL——workspace members 未入白名单（外部构建必炸）: {' '.join(missing)}", file=sys.stderr)
    sys.exit(1)
print(f"sync-public: members 对账 PASS（{len(members)} 顶层 member ⊆ 白名单）")
PYM
}

case "$MODE" in
  check-members)
    check_members
    exit 0
    ;;
  selftest)
    # ---------- --selftest（零网络、零真实镜像仓；合成 SRC/DEST 钉板） ----------
    # bcts-run --selftest 同款合成腿风格：合成 SRC = 本仓 worktree 不动，用
    # 「脚本副本 + 合成成员仓」驱动端到端主流程；逐腿断言形态。红分支腿验
    # 证门真咬（漏补白名单必红/泄漏串必拦）。注意 bash 3.2：中文后紧邻
    # $VAR 须花括号（shvarcheck 口径）。
    SELF=$(mktemp -d "${TMPDIR:-/tmp}/berryos-sync-selftest.XXXXXX")
    cleanup() { rm -rf "${SELF}"; }
    trap cleanup EXIT

    # 合成身份（CI hermetic 面，2026-11-16 判例：CI-light 第七门首跑暴露——
    # selftest runner 用户无 git 身份，端到端腿走主流程 commit 时
    # `fatal: empty ident name` rc=128）。export 经 leg 子 shell 传主流程；
    # 主流程 commit 语义不变（真实镜像提交仍用操作者身份，本 export 只在
    # --selftest 面内生效）。
    export GIT_AUTHOR_NAME=selftest GIT_AUTHOR_EMAIL=selftest@t
    export GIT_COMMITTER_NAME=selftest GIT_COMMITTER_EMAIL=selftest@t

    # 依赖预检（同判例：runner 缺 rsync 时腿组 rc=127 且指认散在 rsync 行）：
    # 缺件先红并点名，别让 21 腿逐个淹死在 127 里。
    for bin in git rsync python3; do
      command -v "$bin" >/dev/null 2>&1 || {
        echo "sync-public selftest: FAIL——环境缺 ${bin}（本自检需 git/rsync/python3）" >&2
        exit 1
      }
    done

    legs=0; fails=0
    ok()  { legs=$((legs+1)); echo "sync-public selftest 腿绿：$1"; }
    bad() { legs=$((legs+1)); fails=$((fails+1)); echo "sync-public selftest 腿红：$1（期望 rc=$2 实得 rc=$3）" >&2; }
    # leg <名> <期望rc> <cmd…>——命令跑在显式收集 rc 的子 shell（set -e 不杀
    # 框架：期望 rc=1 的红腿在 set -e 父进程里裸跑即自杀，selftest 首版实踩
    # （二版再犯：subshell 裸跑非 if 语境，want=1 腿实测 16 腿后静默死——
    # `|| rc=$?` 吞错位捕获是唯一安全形，if 语境豁免不适用于赋值侧）。
    leg() {
      local name="$1" want="$2"; shift 2
      local rc=0
      ( "$@" ) >"${SELF}/out.log" 2>&1 || rc=$?
      if [ "$rc" = "$want" ]; then ok "$name"; else bad "$name" "$want" "$rc"; tail -5 "${SELF}/out.log" >&2; fi
    }
    expect() {
      local name="$1"; shift
      if ( "$@" ) >"${SELF}/out.log" 2>&1; then ok "$name"; else rc=$?; bad "$name" 0 "$rc"; tail -5 "${SELF}/out.log" >&2; fi
    }

    # —— 合成 SRC（git 仓库 + 白名单条目 + 内部档 + 敏感 workflow）——
    MSRC="${SELF}/src"
    mkdir -p "${MSRC}/.github/workflows" "${MSRC}/kernel" "${MSRC}/user" "${MSRC}/wqmatch" "${MSRC}/tools" "${MSRC}/docs/规则"
    git -C "${MSRC}" init -q
    printf '[workspace]\nmembers = ["kernel", "wqmatch"]\n' > "${MSRC}/Cargo.toml"
    echo 'pub fn f() {}' > "${MSRC}/wqmatch/lib.rs"
    echo 'x' > "${MSRC}/kernel/lib.rs"
    echo 'x' > "${MSRC}/user/lib.rs"
    echo 'x' > "${MSRC}/tools/keep.txt"
    echo '# 公开面 workflow' > "${MSRC}/.github/workflows/sdk-publish.yml"
    printf 'name: ci\non: [push]\njobs:\n  b:\n    runs-on: [self-hosted, linux-x64, brew]\n    steps:\n      - run: echo hi\n      # 内网 netns 拓扑：unshare -Urn sh -c '\''ip link set lo up && exec just smoke'\''\n' > "${MSRC}/.github/workflows/ci.yml"
    echo '内部' > "${MSRC}/docs/阶段沿革.md"
    echo '内部' > "${MSRC}/docs/雷册-判例库.md"
    echo '内部' > "${MSRC}/docs/unsafe台账.md"
    echo '内部' > "${MSRC}/tools/evidence-g3-1d.md"
    echo '内部' > "${MSRC}/tools/bench_baseline_icount.notes.md"
    echo '内部' > "${MSRC}/docs/规则/01.md"
    echo 'AGENTS' > "${MSRC}/AGENTS.md"
    echo '设计' > "${MSRC}/docs/IPC设计-v1.0.md"
    printf 'docs-guard:\n    python3 tools/docs_guard.py\n\nbuild:\n    cargo build\n' > "${MSRC}/justfile"
    # 同步主流程用脚本副本：git archive HEAD 导出面 = 副本仓自身 HEAD
    cp "$0" "${MSRC}/tools/sync-public.sh" 2>/dev/null || cp "${BASH_SOURCE[0]}" "${MSRC}/tools/sync-public.sh"
    git -C "${MSRC}" add -A
    git -C "${MSRC}" -c user.email=selftest@t -c user.name=selftest commit -qm "synthetic base"

    SCRIPT="${MSRC}/tools/sync-public.sh"

    # —— 腿组 1：端到端同步形态（合成 DEST 含历史泄漏面）——
    SYNCED="${SELF}/dest"
    mkdir -p "${SYNCED}/internal-proc" "${SYNCED}/.github/workflows"
    git -C "${SYNCED}" init -q
    echo 'LEAK-OLD' > "${SYNCED}/internal-proc/x.md"
    printf 'name: lel\non: [push]\njobs:\n  b:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo ok\n      # 内部注记 berryos-bot 使用 netns\n' > "${SYNCED}/.github/workflows/leak.yml"
    git -C "${SYNCED}" add -A
    git -C "${SYNCED}" -c user.email=selftest@t -c user.name=selftest commit -qm "pre"

    leg "端到端同步 rc=0" 0 bash "${SCRIPT}" "${SYNCED}" "selftest sync"
    expect "docs 内部档排净（沿革/雷册/台账）" test ! -e "${SYNCED}/docs/阶段沿革.md"
    expect "雷册排净" test ! -e "${SYNCED}/docs/雷册-判例库.md"
    expect "unsafe 台账排净" test ! -e "${SYNCED}/docs/unsafe台账.md"
    expect "docs/规则 整目录排净" test ! -e "${SYNCED}/docs/规则"
    expect "tools 内部档排净（evidence/notes）" test ! -e "${SYNCED}/tools/evidence-g3-1d.md"
    expect "notes.md 全剔面" test ! -e "${SYNCED}/tools/bench_baseline_icount.notes.md"
    expect "AGENTS.md 不入镜像" test ! -e "${SYNCED}/AGENTS.md"
    expect "sync-public.sh 自身不入镜像" test ! -e "${SYNCED}/tools/sync-public.sh"
    expect "内部 CI ci.yml 剥除" test ! -e "${SYNCED}/.github/workflows/ci.yml"
    expect "公开面 workflow 保留" test -e "${SYNCED}/.github/workflows/sdk-publish.yml"
    expect "白名单外顶层残留移除" test ! -e "${SYNCED}/internal-proc"
    expect "设计文档保留" test -e "${SYNCED}/docs/IPC设计-v1.0.md"
    expect "justfile docs-guard 降级 no-op" grep -q "public mirror: docs-guard skipped" "${SYNCED}/justfile"
    expect "镜像导航 MIRROR.md 在场" test -e "${SYNCED}/docs/MIRROR.md"
    expect "同步提交落盘（DEST git log）" git -C "${SYNCED}" log --oneline --grep="selftest sync"

    # —— 腿组 2：出口安检真咬（SRC 白名单文件带敏感串 → 同步必拦 rc=1）——
    # 真泄漏场景 = 敏感串进白名单文件（rsync 会原样拷入镜像，安检在提交前拦）；
    # 植入 DEST 的文件会被 rsync --delete 先删——不是安检面的对象。
    MSRC_LEAK="${SELF}/src-leak"
    cp -R "${MSRC}" "${MSRC_LEAK}"
    rm -rf "${MSRC_LEAK}/.git"
    git -C "${MSRC_LEAK}" init -q
    echo '服务器 69.165.x.x 部署拓扑' > "${MSRC_LEAK}/docs/部署设计-v1.0.md"
    git -C "${MSRC_LEAK}" add -A
    git -C "${MSRC_LEAK}" -c user.email=selftest@t -c user.name=selftest commit -qm "plant leak in src"
    LEAKDEST="${SELF}/dest-leak"
    mkdir -p "${LEAKDEST}"
    git -C "${LEAKDEST}" init -q
    leg "安检拦截泄漏串 rc=1（SRC 白名单文件带 69.165）" 1 bash "${MSRC_LEAK}/tools/sync-public.sh" "${LEAKDEST}" "leak probe"
    expect "拦截现场留在盘上（供人工定性，未提交）" test -e "${LEAKDEST}/docs/部署设计-v1.0.md"
    expect "拦截后零提交（git log 无 leak probe）" test -z "$(git -C "${LEAKDEST}" log --oneline 2>/dev/null | grep "leak probe" || true)"

    # —— 腿组 3：members 对账门红分支（合成仓漏补 ghostface 必红）——
    MSRC_BAD="${SELF}/src-bad"
    cp -R "${MSRC}" "${MSRC_BAD}"
    rm -rf "${MSRC_BAD}/.git"
    git -C "${MSRC_BAD}" init -q
    python3 -c "
import sys
p='${MSRC_BAD}/Cargo.toml'
s=open(p).read()
open(p,'w').write(s.replace('members = [\"kernel\", \"wqmatch\"]', 'members = [\"kernel\", \"wqmatch\", \"ghostface\"]'))
"
    git -C "${MSRC_BAD}" add -A
    git -C "${MSRC_BAD}" -c user.email=selftest@t -c user.name=selftest commit -qm "ghost member"
    BADDEST="${SELF}/dest-bad"
    mkdir -p "${BADDEST}"
    git -C "${BADDEST}" init -q
    leg "members 门红分支（ghostface 漏补必 rc=1）" 1 bash "${MSRC_BAD}/tools/sync-public.sh" "${BADDEST}" "bad probe"

    # —— 腿组 4：独立入口（真仓 --check-members 必绿）——
    leg "--check-members rc=0（真仓）" 0 bash "${BASH_SOURCE[0]}" --check-members

    if [ "${fails}" -gt 0 ]; then
      echo "sync-public: FAIL——selftest ${fails}/${legs} 腿红" >&2
      exit 1
    fi
    echo "sync-public: selftest ${legs}/${legs} 腿绿（端到端形态 16 + 安检拦泄漏 3 + members 门红分支 1 + 独立入口 1；零网络零真实镜像仓）"
    exit 0
    ;;
esac

DEST=${1:?用法: sync-public.sh <公开仓本地克隆路径> [提交说明]}
MSG=${2:-"mirror: sync from private @ $(git -C "$SRC" rev-parse --short HEAD)"}

[ -d "$DEST/.git" ] || { echo "ERR: $DEST 不是 git 仓库（首次请先 git init 并接好 public 远端）" >&2; exit 1; }

# ---------- 白名单（顶层条目；默认拒绝制） ----------
# ⚠ 同步律：workspace members 新增顶层 crate 时必须同步补这里
# （漏补 = 公开仓 workspace 残缺，外部构建必炸）。
WHITELIST=(
  kernel mm sched ipc arch bootloader drivers fs net user tools
  # w3 产线外迁 crate（2026-11-11 补账对齐 workspace members；本轮回放对齐
  # +lkface/trapface 两件（#292/#296 重放合入）——白名单漂移由 --check-members
  # 机器门执法（漏补 = 红且列差集），不再靠人工对账）
  bandface chanface dtbface dpgface emface evface frzface hdlface
  hdrface hofface intface iprofile itally piface polface ringface
  safeck schedface sliceface sysvface telface utrace
  vmoledger vslotface wqmatch wsrcface lkface trapface heapface
  svcface
  # rootface 补账（2026-11-15 #305 本件净增量重放时白名单笔未随刀——本刀补登）
  rootface docs
  # fwcfgface/susface 补账（2026-11-16 本刀实捕：#299/#307 合入后白名单
  # 笔未随刀——check-members 门被 base 重写剥落故两连漏无人咬；接线重放
  # 即红，红分支实捕定谳）；memface 三连实捕（2026-11-16 本刀：#381 第86跳
  # merge 解冲突时白名单笔被剥，CI light sync-public --check-members 腿红
  # run 43809528251 实锤——判例固化：解 merge 冲突后必核白名单笔未剥）
  fwcfgface susface memface
  # arch/aarch64 补账（2026-11-19 镜像同步实捕：51f6d8fdc 追平笔带入
  # workspace members 而白名单笔未随刀——check-members 门红分支实捕定谳，
  # fwcfgface/balface/psface 判例同款纪律）
  arch/aarch64
  # balface 补账（2026-11-16 本刀重放接线实捕：W3-Q82 #273 首开件白名单
  # 笔未随刀——check-members 门红分支实捕定谳，同 fwcfgface 判例）
  balface
  # psface 补账（2026-11-16 本刀 rebase 进 #362 后门再咬：W3-Q72 合入后
  # 白名单笔未随刀——接线恢复后门第一次真执法实捕）
  psface
  # wakeface 补账（2026-11-16 本刀 merge 进 #365 后门三咬：W3-Q66 合入后
  # 白名单笔未随刀——check-members 三连实捕定谳）
  wakeface
  # tickface 补笔（2026-11-16 本刀随 W3-Q31 净增量重放带刀：白名单笔随刀，
  # check-members 门先验——判例 #368 fwcfgface/susface/psface 同款纪律）
  tickface
  # billface 补账（2026-11-16 本刀 W3-Q76 净增量重放随刀：净增量重放必带
  # 白名单笔，#368 fwcfgface/susface/psface 判例同款）
  billface
  # numface 补账（2026-11-16 本刀实捕：W3-Q83 净增量重放时白名单笔未随刀
  # ——check-members 门咬，billface/fwcfgface/psface/wakeface 判例同款）
  numface
  # dmface/aokface 补账（2026-11-16 本刀净增量重放随刀：dmface 为 W3-Q84
  # 首开件白名单笔随刀补登（fwcfgface/balface 同判例，先登防红）；aokface
  # 为 base 债清偿——base 848a0fb35 树内 aokface/ 在、justfile 已消费而
  # members/白名单双漏登，base tip test-host 必红，随刀一并清偿）
  dmface aokface
  # wsrcface 补账（2026-11-16 净增量重放随刀：W3-Q28 首开件白名单笔
  # 随刀补登——fwcfgface/dmface 同判例，先登防 check-members 红）
  wsrcface
  # heapface 补账（2026-11-15 #301 合入后机器门首咬：新增顶层 crate 忘补
  # 白名单即 test-host 红——check-members 门执法生效的第一次实捕）
  .github .cargo
  # rust-toolchain.toml：编译器轴钉版（SDK-02 拍板①）——「同输入同字节」
  # 契约的公开面组成部分；ci.yml 显式 toolchain 同值引用它。
  rust-toolchain.toml
  justfile Cargo.toml Cargo.lock LICENSE
  README.md README.zh-CN.md .gitignore
)

# ---------- docs/ 内部排除（默认拒绝制的定向补丁） ----------
# 内部过程档只住私密仓（复盘/审查/裁决/沿革/盘点/流水线内部面）
DOCS_EXCLUDE=(
  阶段沿革.md
  战役复盘-2026-10.md
  审查报告-2026-10-15-2b.md
  审查报告-2026-10-16-3b3.md
  战略裁决-2026-10-15.md
  Phase4收官盘点.md
  P6收官盘点.md
  # 2026-11-11 出口安检抓获：流水线内部面（服务器 IP/产能口径/雷册/判例索引/架构决策）
  并行开发轨道-v1.0.md
  开发体验.md
  架构决策.md
  雷册-判例库.md
  判例索引.md
  # 2026-11-15 回放补漏：CI 对拍产物/治理内部面（判例引用 unsafe 出现点登记——
  # 判例库族内部档；C2c 挂账移交材料；W2 索引运维注记）
  unsafe台账.md
  挂账-C2c深坑件.md
  挂账-C2c深坑件
  索引双源核对清单-v0.2.md
)

# ---------- tools/ 内部排除 ----------
# 内部过程证据档/基线笔记——非公开工具链组成（默认拒绝制下的定向补丁）
TOOLS_EXCLUDE=(
  evidence-g3-1d.md
  evidence-g3-3.md
  evidence-g3-4a.md
  evidence-g3-4b.md
  evidence-g3-5.md
  evidence-g3-5c.md
  evidence-g3-5d.md
  evidence-g3-5recon.md
  evidence-g3-6.md
)

# ---------- 导出 HEAD 快照 ----------
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
git -C "$SRC" archive HEAD | tar -x -C "$TMP"

# ---------- 逐条目同步（白名单内 rsync --delete，保证删除传播） ----------
for item in "${WHITELIST[@]}"; do
  if [ -d "$TMP/$item" ]; then
    # 源带尾斜杠 = 拷内容（非整目录塞入）：macOS /usr/bin/rsync（openrsync）对不存在目标
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

# ---------- docs/tools 内部档剔除 ----------
# rm -rf（非 -f）：排除表条目可能是目录（挂账-C2c深坑件 = 移交材料 + uu/
# 源码归档子目录）——selftest 端到端实咬：rm -f 对目录静默无效，整目录
# （含内部源码归档）留在镜像 = 泄漏面。排除条目一律按可递归删除处理。
for f in "${DOCS_EXCLUDE[@]}"; do rm -rf "$DEST/docs/$f"; done
for f in "${TOOLS_EXCLUDE[@]}"; do rm -rf "$DEST/tools/$f"; done
# tools 下内部笔记类：*.notes.md 全剔（门禁基线文本保留）
find "$DEST/tools" -maxdepth 1 -name '*.notes.md' -delete 2>/dev/null || true
# docs/规则/ 整目录剔（域铁律——设计思路本体，内部档）
rm -rf "$DEST/docs/规则"

# ---------- 形态适配 a：workflows → 公开形态 ----------
# 内部 CI 双道（ci.yml/ci-light.yml）不入镜像：其形态假设内部 runner（工具链
# 预装 fail-fast、QEMU/just 全家无安装步）——换 ubuntu-latest 必红，门面仓挂
# 永久红 CI 不如不挂。公开面保留 sdk-publish.yml（crates.io 发布链，本就
# ubuntu-latest + workflow_dispatch 休眠态）。CI 住在开发主仓。
# unsafe-ledger-autofix.yml 同律不入（2026-11-20 镜像同步出口安检实捕：
# 内部 autofix 流水线——push 触发 wip/sched-1.1 + hermes-auto label +
# 内部 runner PATH，随 .github 白名单 rsync 带入后安检 LEAK 拦截于提交前；
# 形态剥除律：内部流水线 workflow 一律剥除，非形态适配——与 ci 双道同律）。
WF_DIR="$DEST/.github/workflows"
rm -f "$WF_DIR/ci.yml" "$WF_DIR/ci-light.yml" "$WF_DIR/unsafe-ledger-autofix.yml"
for wf in "$WF_DIR"/*.yml; do
  [ -e "$wf" ] || continue
  # runner 面换公开形态（余量防御：任何残留 self-hosted 标签）
  sed -i '' 's/runs-on: \[self-hosted, linux-x64, brew\]/runs-on: ubuntu-latest/' "$wf" 2>/dev/null \
    || sed -i 's/runs-on: \[self-hosted, linux-x64, brew\]/runs-on: ubuntu-latest/' "$wf"
  # 2) 内网注释块（ogent/netns/unshare/18080/REVIEW.md/hermes-auto/berryos-bot/自托管）剥除：
  #  awk 块过滤——连续 # 注释行成缓冲，块内命中黑名单词 → 整段丢弃
  python3 - "$wf" <<'PYEOF'
import sys, re
p = sys.argv[1]
lines = open(p, encoding='utf-8').read().splitlines(keepends=True)
BLACK = re.compile(r'ogent|netns|unshare|18080|REVIEW\.md|hermes-auto|berryos-bot|自托管|self-hosted')
out, buf = [], []
for ln in lines:
    if ln.lstrip().startswith('#'):
        buf.append(ln)
    else:
        if buf and any(BLACK.search(b) for b in buf):
            pass  # 整段注释丢弃
        else:
            out.extend(buf)
        buf = []
        out.append(ln)
if buf and not any(BLACK.search(b) for b in buf):
    out.extend(buf)
open(p, 'w', encoding='utf-8').write(''.join(out))
# netns wrap 步剥除（公开仓 ubuntu-latest 无内网端口冲突约束）
s = ''.join(out)
# 通用形态剥除：unshare -Urn sh -c '<cmd>' → <cmd>（引号内原命令直出）
import re as _re
s = _re.sub(r"unshare -Urn sh -c '([^']*)'", lambda m: m.group(1), s)
s = s.replace('ip link set lo up && exec ', '')
open(p, 'w', encoding='utf-8').write(s)
PYEOF
done
# sync-public.sh 自身不入公开仓（镜像操作手册 = 内部件）
rm -f "$DEST/tools/sync-public.sh"

# ---------- 形态适配 b：justfile docs-guard 解耦 ----------
# 公开仓构建不依赖 AGENTS.md（内部档不在镜像）——docs-guard 前置门在镜像侧降级 no-op
# （justfile 缺席的最小镜像形态跳过——合成腿/裁剪镜像不炸）
if [ -f "$DEST/justfile" ]; then
python3 - "$DEST/justfile" <<'PYEOF2'
import sys
p = sys.argv[1]
s = open(p, encoding='utf-8').read()
old = """docs-guard:
    python3 tools/docs_guard.py"""
new = """# 公开镜像形态：docs-guard 依赖 AGENTS.md（内部档不在镜像）——降级 no-op，
# 构建/CI 前置门回到 cargo/just 本体（AGENTS 预算门是内部治理，不约束外部贡献者）
docs-guard:
    @echo "public mirror: docs-guard skipped (internal governance doc absent)" """
if old in s:
    s = s.replace(old, new, 1)
    open(p, 'w', encoding='utf-8').write(s)
    print('justfile: docs-guard → no-op（公开形态）')
else:
    print('justfile: docs-guard 定义未匹配（可能已适配）——人工复核')
PYEOF2
fi

# ---------- 镜像侧导航注记（主仓无此文件——每次同步重写） ----------
cat > "$DEST/docs/MIRROR.md" <<'EOF'
# 公开镜像说明

本仓库是 BerryOS 开发主仓的白名单镜像（单向导出，fresh 历史）。
部分中文设计文档（阶段沿革 / 复盘 / 审查报告 / 战略裁决 / 收官盘点 /
规则 / 架构决策 ADR 全文 / 雷册判例库）属于内部过程记录，仅存于开发
主仓，不在此镜像中——`docs/索引.md` 中对应行的链接在本镜像内不可达。
核心设计文档全部在册。
CI 形态差异：内部 CI 双道（ci.yml/ci-light.yml）依赖内部 runner 设施，不入
本镜像；公开面保留 sdk-publish.yml（crates.io 发布链）。构建入口 `just build`
/ `just smoke` 与上游同源（需 QEMU 与 Rust 工具链，见 README）。
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

# ---------- workspace members 对账门（同步律机器执法） ----------
# 根因（#153/#171/#227 三连 REJECT 判例族）：workspace 新增顶层 crate 而忘补
# WHITELIST = 公开仓 members 引用残缺 → 外部 cargo build 必炸。人工对账三连
# 漏后升机器门：members ⊆ WHITELIST 不满足即红且列差集（同步时自动跑；独立
# 跑 = `--check-members`，挂 test-host 前置防回归）。
check_members() {
  python3 - "$SRC" <<'PYM'
import sys, tomllib
from pathlib import Path
src = Path(sys.argv[1])
script = (src / 'tools' / 'sync-public.sh').read_text(encoding='utf-8')
import re
m = re.search(r'WHITELIST=\((.*?)\n\)', script, re.S)
items = set()
for ln in m.group(1).splitlines():
    ln = ln.strip()
    if not ln or ln.startswith('#'):
        continue
    items.update(ln.split())
with open(src / 'Cargo.toml', 'rb') as f:
    ws = tomllib.load(f)
members = [x for x in ws.get('workspace', {}).get('members', []) if '*' not in x]
missing = sorted(x for x in members if x not in items)
if missing:
    print(f"sync-public: FAIL——workspace members 未入白名单（外部构建必炸）: {' '.join(missing)}", file=sys.stderr)
    sys.exit(1)
print(f"sync-public: members 对账 PASS（{len(members)} 顶层 member ⊆ 白名单）")
PYM
}

# ---------- 出口安检（推前必过） ----------
echo "==== 出口安检 ===="
check_members
LEAK=0
# 1) 敏感串全树扫（IP/内网服务/流水线内部语汇）——工作树面（rg --no-ignore
#    等价）：git grep 只扫索引内已提交内容，rsync 后未 add 的新文件不在扫描
#    面（selftest 实咬：植入泄漏文件 rc=0 漏放行）；用 find+grep 扫全树文件
#    实体，锁文件豁免同 v2。
while IFS= read -r hit; do
  echo "LEAK: $hit"
  LEAK=1
done < <(cd "$DEST" && find . -type f -not -path './.git/*' -not -name '*.lock' -print0 | xargs -0 grep -lE '69\.165|ogent-hub|berryos-bot|hermes-auto|REVIEW\.md' 2>/dev/null || true)
# 2) 内部档在场核验
for f in AGENTS.md CLAUDE.md; do
  [ -e "$DEST/$f" ] && { echo "LEAK: $f 在场"; LEAK=1; }
done
for d in docs/规则 docs/雷册-判例库.md docs/架构决策.md docs/并行开发轨道-v1.0.md; do
  [ -e "$DEST/$d" ] && { echo "LEAK: $d 在场"; LEAK=1; }
done
if [ $LEAK -eq 1 ]; then
  echo "ERR: 出口安检未过——先修排除面/形态适配再推" >&2
  exit 1
fi
echo "出口安检 PASS"

# ---------- 提交 ----------
git -C "$DEST" add -A
if git -C "$DEST" diff --cached --quiet; then
  echo "无变化，跳过提交"
else
  git -C "$DEST" commit -m "$MSG"
  echo "已提交。推送到 public 远端请手动：git -C $DEST push public main"
fi
