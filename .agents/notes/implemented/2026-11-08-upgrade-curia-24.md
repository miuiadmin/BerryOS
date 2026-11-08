# Agent Note: Curia 2.3 → 2.4 同步与 docs standard 落地裁决

Status: implemented

## Problem

上游 curia-md 升 2.4.0（f614d55，2.3 后五提交）：新增第 10 类 `standard`——`docs/` 下的现行标准与契约（`Type: standard` + `Status: current/historical/superseded` + `Version: MAJOR.MINOR.PATCH`，文件名版本须为头块前缀）；检查器扩三处（`.agents/` 或 `docs/` 任一即合法根锚的 docs-only 模式、组 1 standard 头块/semver/文件名版本一致性、组 2 家归属跨 docs、组 3 现行依据纳入 docs current standard、第三归档区 `docs/archive/`）；存量纳管补第 7 条（存量设计规范默认自由知识，不批量补头块）。fork-and-own 无 upgrade 通道，须人工同步并裁决与本仓 `docs/` 版本化制度（`名-v主.次.md`，60+ 份在册）的关系。

## Decision

- **合并面按文件级 diff 精确落**（实测差异恰为上游 2.3.0→2.4.0 的 10 个载荷件）：`check-docs.mjs` 整体换 2.4（本地与 2.3.0 逐字节同，零定制损失）；`.agents/README.md` 规范全文换 2.4 + BerryOS 特记重涂（2.4 口径：10 类 + standard 裁决摘要）；GLOSSARY 以 2.4 为底重织本地四词条（现行态补 `standard` 的 `current`、承载点律补跨两根口径）+ BerryOS 领域节；archive/plans/skills 家 README 与 prose-standard 整体换（本地零定制）；write-doc 换 2.4 + 特记升级（9→10 类、standard 归属指向裁决表）。上游未变的载荷件（notes/AGENTS、notes/README、research/README、write-note）本地保留；`ADOPTION.md`/`INSTALL.md`/`init --diff` 属上游仓件非载荷，不取。
- **standard 制度落地裁决（存量不迁、新标准启用）**：存量版本化文档（`名-v主.次.md`）= 自由知识，不批量补 `Type: standard`（上游存量纳管第 7 条明文此形态合规，下次实质修改时再补）；**新的跨模块现行标准**（写作排版/编码约定/UI/API 契约这类需「现行第几版」的）走 `Type: standard` 固定形态，享受取代链 + Version 执法。同主题永不双轨：已用 `名-v主.次` 的主题原地演进继续旧制。裁决落 `docs/README.md` 裁决表（新增行）、特记、write-doc 特记三处。
- **`docs/archive/` 第三归档区不启用**：本目录冷历史仍走既有制度（原地冻结或 `docs/归档/`，索引维护规则 5）；上游明文该区可选、其他名字按普通主题目录处理不强制迁移。

## Alternatives considered

- **存量 60+ 版本化文档批量补 `Type: standard` 头块**：被拒——违反存量只读纳管（不为合规批量追改）；且 semver `Version:` 与 `名-v主.次` 文件名版本双语义冗余，文件名版本一致性判据反成新摩擦面。
- **防御性关闭 standard（不启用新形态）**：被拒——沿 2.3 模块设计启用先例（「彻底接入，不防御性关闭」）；机制落地 + 存量不迁移本就无双轨风险，关闭只损失执法面。
- **重跑 init 全量对拷**：被拒——本地定制面广（特记/裁决表/GLOSSARY 领域节/notes AGENTS 分工），init 不覆盖已存在文件、对拷产杂音；文件级 diff 更准（上游差异面恰与本地定制面可完全分离）。

## Consequences

- 换装前后检查器对默认面均 131 篇 0 告警（换装前 dry-run 亦 0——2.4 判据对存量树无回溯命中）；负例三发验证判据真实生效：组 1 拦坏 semver、组 2 拦 `docs/` 下非 standard 类型、组 3 拦 current standard 链 superseded（docs-only 模式注记正常）。
- 根锚放宽（`.agents/` 或 `docs/` 任一即合法）对本地无行为变化（两根都在）；本仓在别的子目录裸跑检查器仍会正确拒（退出码 2）。
- 未来写跨模块标准走 standard 骨架（背景/适用范围/契约/版本与兼容/适用边界），升版原地改 + `Version` 递增；翻转 `superseded` 遵守链三律，standard 现行依据引用旧规则会被组 3 拦。
- `名-v主.次` 与 `standard` 两制并存以「存量主题/新跨模块标准」分界，与 2.3 模块设计固定形态同一并存模式；AGENTS.md 维护合同的版本化条款不动。

## Related

- [2.3 升级笔记（前篇）](2026-11-06-upgrade-curia-23.md) · [模块设计启用笔记](2026-11-06-curia23-module-design-enable.md) · [adopt-curia 引入笔记](2026-11-06-adopt-curia-docs-workspace.md) · 上游 `miuiadmin/curia-md` @ f614d55（curia-v2.4.0）
