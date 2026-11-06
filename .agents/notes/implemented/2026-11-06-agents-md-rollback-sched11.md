# Agent Note: S2a/S3-a 拍板后不回写 AGENTS.md 的核对结论

Status: implemented

## Problem

2026-11-06 S2a（带内 EEVDF）/ S3-a（进程 freeze）/ S3-b（pager_refs）相继拍板落地（c58b529），仓主质疑：改了约定为何不回写 AGENTS.md（维护合同：改约定 → 回写本文件）？

## Decision

核对后**不回写**——拍板内容全落 sched-1.1 刀面，不在 AGENTS.md 载荷面（v1 冻结常量 + 铁律路由）内：

- `syscall ~30–45（v1 35 在册）` 是 **v1 面口径**，0x3E `process_suspend` 属 ABI 单源（`docs/系统调用ABI设计-v1.0.md` §计数）的「1.1 增 4（0x3C–0x3F）」面，不计入 v1 的 35；「已越过 ~30–45 预算带」指全表总数 55（39 在册 + v2 增 11 + v3 增 5），v1 面未越带。
- EEVDF/freeze/pager_refs 是 1.1 面机制：契约 → `docs/调点调度-v1.1.md`、号位登记 → ABI 文档 1.1 节、流水 → `docs/阶段沿革.md`、判例 → 雷册 §A132——拍板刀 c58b529 已全部履行。
- 判据：AGENTS.md 只载「跨版本稳定的冻结常量 + 路由」；某刀面的在途契约/实现进度由该面契约文档与沿革承载，入口不镜像（否则每刀都要动入口，预算 3000/3400 也装不下）。

## Alternatives considered

- **把 1.1 面（EEVDF/freeze/36 号等）写进 AGENTS.md 冻结常量节**：被拒——冻结常量是 v1 跨版本口径；1.1 在途细节写进入口违反「入口只载当前状态 + 铁律路由」，且预算零余量。
- **更新调度行为 1.1 已落**：被拒——刀序进度属路线图/沿革面，入口的「当前站 → 路线图」路由已覆盖。

## Consequences

- AGENTS.md 保持 3000/3400 零警不动；1.1 面事实由调点调度-v1.1 / ABI 文档 / 沿革三处承载（已在库）。
- 同类问题的判据可检索本篇：先问「拍板落在哪个面——v1 冻结常量还是某刀面契约」，后者不进入口。
- 挂账（拍板刀收尾义务，非入口义务）：ABI 文档 §计数「0x3C–0x3F S1 在册未实现，S2/S3 落语义」子句已随 S3-a 部分过时（0x3E 已实现），abi-check 只对拍表不对文档——sched-1.1 收尾刀应同步该行实现态。

## Related

- [adopt-curia 笔记](2026-11-06-adopt-curia-docs-workspace.md) · `docs/系统调用ABI设计-v1.0.md` §计数 · `docs/调点调度-v1.1.md`
