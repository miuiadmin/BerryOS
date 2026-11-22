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
- 挂账处置（同日）：① ABI 文档 §计数/§1.1 注记已两轮同步（S3-a 进程级 + S3-b 嵌套 depth 已落；表行由 abi.toml 生成不手改）。② `implemented` 翻 true 的精确条件 = 0x3E handler 落 SINGLE_THREAD（当前 options 恒 0 直 `-ENOTSUP`，库里注释明示留 S3-b）——落齐后翻值 + note 同改 + `abi-check`。③ 沿革撞号整理（窗关后执行）：库里现为双（八十一）+ 顺次八十二–八十五；方案 = 兄弟五节顺移 +1（S3-b 嵌套→八十二、bug 修→八十三、S4-a→八十四、S5-a→八十五、G4 收官→八十六），Curia 节保持八十一；窗开期间任何基的改号都会被兄弟下一次保存/整文件 add 覆盖（§A133 工作树非事实源），必须等沿革活动窗关闭。
兑现注记（2026-11-17）：②已落——abi.toml 0x3E 行 `implemented = true`（:579-583，173a070e9 S3-b 单线程冻结刀随 handler 落地翻值），上文「options 恒 0 直 `-ENOTSUP`」为翻值前时点表述；③已落——沿革撞号经后续重写收口，process_suspend/freeze 相关条目现位 docs/阶段沿革.md:3456/:3471。

## Related

- [adopt-curia 笔记](2026-11-06-adopt-curia-docs-workspace.md) · `docs/系统调用ABI设计-v1.0.md` §计数 · `docs/调点调度-v1.1.md`
