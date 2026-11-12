# Agent Note: Curia 2.5.0 终态收口——载荷面维持 1:1，宿主三件与编排技能恢复

Status: implemented

## Problem

[1:1 纯上游迁移](2026-11-12-upgrade-curia-25-1to1.md)（616e30b4）把宿主项目文件与 Curia 载荷一并清除了：根 `AGENTS.md` 被上游通用模板整替（72→45 行）、`CLAUDE.md` 由 `@AGENTS.md` 指针改为符号链接、`docs/README.md` 裁决表被通用说明整替、`workflow-implement` 技能被删。后果三笔：① 本仓自有执法 `tools/docs_guard.py`（justfile `docs-guard` + ci.yml/ci-light.yml 必跑）G2/G3/G7 三红，下次 CI 必挂；② BerryOS 规则单一事实源失踪——域铁律路由表、维护合同、雷册分工、docs_guard 预算制度全失载体，后续所有 agent 会话失去本仓规则入口；③ 模板内容对本仓实态虚假陈述（如「做完的进 `.agents/research/`」，本仓实态调研住根 `research/`，裁决表在册）。

## Decision

- **载荷面维持 1:1 纯上游不动**：`.agents/` 下 19 件载荷（规范/GLOSSARY/检查器/各 README/上游三技能）与 616e30b4 保持逐字节一致——1:1 是用户在并行会话的明确裁定（已入库），纯载荷消除逐版重涂漂移有理，本研究会话的精装方案对其让位。
- **宿主三件自 725e5554 恢复**：根 `AGENTS.md`（72 行 BerryOS 规则源）、`CLAUDE.md`（`@AGENTS.md` 指针形态，与备份 `CLAUDE.md.pre-curia` 逐字节同）、`docs/README.md`（BerryOS 文档归属裁决表）。判据：它们是宿主项目文件而非 Curia 载荷——上游 `template/` 对应件是给采用方的**示例**，Curia 2.2–2.4 期间两者本就共存；恢复后 docs_guard 三红转绿（PASS），CI 保绿。
- **`workflow-implement` 技能自 8bb34983 整目录恢复**：技能面是用户内容（上游三件技能也只是示例），它是本仓「workflow 模式」多 agent 编排纪律的载体，用户工作法活跃在用。
- **--health 两态登记为本仓预期形态**：`AGENTS.md=partial`（机械判据要求上游模板三节，本仓规则源本就不取该结构——机械真·语义假）与 `CLAUDE.md=real-file-pointer`（上游显式识别的等价形态）；勿为取悦 health 重塑 AGENTS.md（会撞 docs_guard 预算与双规则源）。
- **本裁决为会话内裁决**：两次用户问询超时未获确认；回滚配方 = `git revert <本提交>` 即回 1:1 纯态。

## Alternatives considered

- **全面回滚 1:1 走精装重涂**（整体换 5 件真变 + 重涂特记/裁决表/GLOSSARY 领域节）：被拒——1:1 已是用户裁定且入库，载荷面纯上游的减漂移理由成立；宿主件恢复已闭环全部客观破坏，精装只多回涂涂层。
- **纯 1:1 不动 + 改 docs_guard 删探针适配**：被拒——等于拆本仓自有执法迁就模板，规则源真空与虚假陈述两笔债不解决。
- **不恢复 workflow-implement（按 1:1 笔记「上游技能面回三件」）**：被拒——1:1 面纪律的对象是 21 件载荷的逐字节一致，技能目录本是用户内容家（笔记亦留有「需要时重立」活口）；恢复成本一步且完全可逆。

## Consequences

- 双执法面全绿：docs_guard PASS（1 警：G6 AGENTS.md 3047/3400 字符超 3000 警线——恢复前常态，警不拦）；check-docs --strict 169 篇 0 告警（较 1:1 态 +1 = 技能回归）。
- `--suggest-standards` 8 篇候选是提示非指令：存量版本化文档（`名-v主.次.md`）不批量转 standard（裁决表在位，裁定继续有效）；照抄建议头块会触发组 1 文件名版本不一致（v1.4 vs 1.0.0）。
- `CLAUDE.md.pre-curia` 保留原地（未跟踪）：指针形态已恢复，备份内容与库内一致，确认后可手动删。
- GLOSSARY 领域词条与 BerryOS 特记按 1:1 裁定**不回涂**（基线 725e5554 可随时回溯；如需回另行裁决）。
- 后续 Curia 升级沿用本终态公式：载荷面整目录对拍上游，宿主件（AGENTS.md/CLAUDE.md/docs README）独立演进不随模板。

## Related

- [1:1 迁移笔记（前篇）](2026-11-12-upgrade-curia-25-1to1.md) · [2.4 升级笔记](2026-11-08-upgrade-curia-24.md) · [2.3 升级笔记](2026-11-06-upgrade-curia-23.md) · [adopt-curia 引入笔记](2026-11-06-adopt-curia-docs-workspace.md) · 上游 miuiadmin/curia-md @ curia-v2.5.0（0958ae1）
