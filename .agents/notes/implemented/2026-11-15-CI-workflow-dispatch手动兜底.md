# CI workflow_dispatch 手动兜底通道

Status: implemented

## Problem

2026-11-15 #325（波3 主件）遭遇分支事件黑洞：push×5/close-reopen×3/删分支重建/换 base 往返/contents API 提交全通道零 run 创建，同期他分支事件正常。彼时两道 CI（ci.yml 重道 + ci-light.yml 轻道）均无 workflow_dispatch——黑洞期对自家 head 零手动取证通道，只能开全新分支+全新 PR 重放替（#356）绕路。同日 unsafe-ledger-autofix 的 403 断点（GITHUB_TOKEN 禁开 PR）也暴露同一形态：自动化链断在仓设置面，无手动兜底即依赖 lane 代偿。

## Decision

两道 CI 补 `workflow_dispatch:`（无参形态——不经 paths 过滤，对任意分支 head 可手动点火）。要点：

- dispatch 的 run 不吃 push/pull_request 的 paths/paths-ignore 过滤——黑洞分支也能拿到门禁信号
- 只兜底不常态用：正常流仍走事件触发（dispatch 点火不产生 PR checks 关联，取证用）
- 仓 Actions 权限同步放行 `can_approve_pull_request_reviews=true`（autofix 开 PR 步 403 根治——GITHUB_TOKEN 默认 read + 禁开 PR 是 GitHub 仓级默认，私有仓自托管 runner 生态放行无风险面）

## Alternatives considered

- 只靠重放替（#356 判例）：有效但重——开新 PR 重排队重审，且黑洞常持续数小时
- repository_dispatch（API 侧触发）：需 client payload 与 workflow 改造，且仍依赖 API 调用方权限——比 UI/gh 一键 dispatch 重

## Consequences

- 事件黑洞期 3 分钟内可对任意 head 点火取证（vs 重放替整链路 ~1 小时）
- dispatch 是无过滤面——重道 dispatch 会跑全套 QEMU（~20min 占串行位），取证优先轻道
- autofix 403 断点自愈后，孤儿分支代偿（如 #363）成为历史形态
