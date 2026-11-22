# Agent Note: Curia 2.2 → 2.3 同步与两条本地化裁决

Status: implemented

## Problem

上游 curia-md 升 2.3（bcd73bc，五提交）：新增取代链三律（链终结在现行版本 / 现行态口径 / 一主题一现行承载点）、模块设计固定形态（`plans/<模块>/design.md` + `Version:` 字段）、目录名放开中文（kebab-case 或纯中文二选一）、检索纪律「用前先解析状态」与配方 6–8。fork-and-own 无 upgrade，须人工同步并裁决与本仓制度的冲突。

## Decision

- **合并面 = 两文件**：`check-docs.mjs` 整体换 2.3 版（新增取代链成环/断开/超 16 跳/终点现行态校验、SEMVER、中文目录正则）；`.agents/README.md` 规范全文换 2.3 版 + 保留头部 BerryOS 特记。其余载荷件（write-doc/GLOSSARY/notes 目录指令/research/docs README）上游 2.2→2.3 未变，本地定制原样保留。
- **本地化裁决一（模块设计固定形态不启用）**：本仓模块总体设计 = `docs/` 版本化设计文档（`名-v主.次.md`，60+ 份在册，索引详表即「知道模块名一步找到它」的等价物）；若启用 `plans/<模块>/design.md` 则同主题双现行承载点，违反 2.3 自己的一主题一承载点律。裁决落 `docs/README.md` 裁决表与规范全文特记。
- **本地化裁决二（中文目录名自然生效）**：`.agents/` 子目录两种形态取一，检查器 2.3 已支持，无存量影响。

## Alternatives considered

- **启用 `plans/<模块>/design.md` 并迁移模块设计**：被拒——60+ 份版本化文档在 `docs/`，迁移即存量清理运动（违反存量只读纳管），双轨期两处都声称现行；版本化命名已承载同等演进语义（`Version:` 字段语义并入 `名-v主.次.md`）。
- **对空仓重跑 init 再全量对拷**：被拒——init 不覆盖已存在文件，对本地定制多的树产生杂音；按文件级 diff 精确合并更准（实际差异恰只有两文件）。

## Consequences

- 检查器 2.3 首跑默认面 110 篇 0 告警（取代链校验现无 superseded 文档空过）；docs_guard 不受影响。
- 未来翻转 `superseded` 时遵守链三律：`Superseded-by:` 终结在现行态、翻状态同批改引用、一主题只留一份现行——检查器会执法成环/断链/终点退役态。
- 检索纪律升级：rg 命中先读头块 `Status:` 再当依据（`superseded` 追链到终点）；配方 6–8 见规范全文「链接与检索」。
- `Version:` 字段仅模块设计文档要求——本仓不启用该形态，字段不出现即不判。

## Related

- [adopt-curia 引入笔记](2026-11-06-adopt-curia-docs-workspace.md) · 上游 `miuiadmin/curia-md` @ bcd73bc（2.3.0）
