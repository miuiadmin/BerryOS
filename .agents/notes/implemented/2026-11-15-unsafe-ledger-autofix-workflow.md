# unsafe 台账步红自动重产回流（unsafe-ledger-autofix workflow）

Status: implemented

## Problem

unsafe 台账对拍步（ci.yml/ci-light.yml `--check` 执法）的复发红面：owner 直推 kernel/ 改源不带台账重产——判例族 #167/#175/#220/#281 系七连发，每次都走「lane 巡检发现 run 红 → 手工重产 → 开 fix PR」的全人工段。QUEUE W1-Q14 挂账根治件：「ci-light 台账步红自动开 fix PR 的 workflow」。

## Decision

新增 `.github/workflows/unsafe-ledger-autofix.yml`（独立 workflow，不改 ci-light 本体）：

- **触发面 = push@wip/sched-1.1 无 paths 过滤**：audit 纯 python3 秒级；kernel-only 推送不触发 ci-light（其 paths 面 = docs/tools/md）但同样漂移台账——探测覆盖面必须比 ci-light 宽，否则漏检 owner 直推的主形态。
- **红即自动重产 → 证绿闭环 → 单一专用分支 `unsafe-autofix/ledger` force push 去重 → `actions/github-script@v7` REST 开/复用 fix PR（label hermes-auto 进 reviewer 队列）**。去重两道：固定分支名原地更新；开 PR 前按 head 查 open 命中即复用。
- **推前证绿闭环**：重产后复跑 `--check`，红（= 重产器自身 bug）拦下不推。
- **非漂移红拦下**：对拍红但重产零 diff（marker 块手改越界/结构性坏）→ `::error` 人工修，不自动开 PR——防止机器覆盖人工裁决。
- **PR 面**：GITHUB_TOKEN 发起事件不递归触发 workflow（GitHub 防环生语义），fix PR 验证走 ci-light 常规道（合入 base 后 workflow 对新 push 再跑 `--check` 终验）。
- 台账 prose 头部同步一行指引（marker 块外，prose 面合法改动）。

## Alternatives considered

- 在 ci-light.yml 台账步挂 `if: failure()` 后续 job（否：GITHUB_TOKEN 触发的 run 里 push 需 `permissions: contents: write` 会改变 ci-light 全部 run 的 token 权限面，且失败 run 中开 PR 的动作污染轻道语义——轻道是纯探测面，回流动作独立成件互不连坐）。
- `workflow_run` 监听 ci-light 完成（否：默认分支 = main，workflow_run 只在默认分支的 workflow 文件上触发——本仓开发主干是 wip/sched-1.1，文件合入 main 前永不被触发；同理 GITHUB_TOKEN 权限边界更绕）。
- 本地 pre-push 钩子扩展（已存在 #182 系——只罩本 lane 推送，owner 直推面裸奔，恰是七连发基因所在）。
- 定时轮询（否：红面即时性差，且每次轮询都要跑全量 audit）。

## Consequences

- 验证（本机实跑，macOS host）：YAML 合法性 ruby YAML 解析过 + write_file 内建校验过；红分支实证——kernel/src/main.rs 注入 `pub unsafe fn w1_autofix_probe() {}` 后 `--check` EXIT=1 FAIL 面正确（密度总览 + 登记表双块首差异行均指注入点）；重产 995 条 → `git diff --quiet` 非空检测（DIFF_EMPTY=1）→ 还原后 `--check` 绿；重产 diff 形态 = 6+/5− 仅 marker 块内行（prose 面零改）。绿基线全套：`--check` 994 条在册绿 + docs_guard PASS + check-docs --strict 245 篇 0 告。
- 首次实战检验面：下次 owner 直推漏带重产时本 workflow 自动开 PR——零人工介入即根治件生效的直接证据。
- runner 面零新依赖：python3 + git + actions/checkout + actions/github-script 均既有面（sdk-publish.yml 已用显式 permissions 先例；仓库 actions 权限 allowed_actions=all 实查）。
- 残余风险：①runner 离线时 workflow 不跑（探测缺口与 CI 常规面同构）；②`unsafe-autofix/ledger` 分支被人工动过时 force push 覆盖（分支语义 = 机器自有，body 已注明）；③开 PR 失败（网络/权限）时 run 红 + `::error` 留痕，人工兜底面不变。
- 首轮生效待观察：本 PR 合入 base 后的第一次 push 即触发首跑（预期绿面早退——当前台账在册 994 对拍绿）。

## Related

[unsafe 台账](../../../docs/unsafe台账.md)（prose 头部指引同批更新）；判例族 #167/#175/#220 重产史 + QUEUE W1-Q14 根治件挂账（`.hermes-loop/QUEUE.md`，lane 内部件）；本地 pre-push 台账守卫钩子（PR #182，tools/hooks/——推送侧防线，本件是 CI 侧自动回流，互补成对）。
