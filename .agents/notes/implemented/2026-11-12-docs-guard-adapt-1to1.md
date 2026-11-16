# Agent Note: docs_guard 适配纯 1:1 形态——撤行长律、桥契约换防

Status: implemented

## Problem

纯 1:1 终裁（[翻转笔记](2026-11-12-curia-25-pure-1to1-revert.md)）后 `tools/docs_guard.py` 四组红：G2（模板 AGENTS.md 行长超帽）、G3（6 条 BerryOS 内容探针断言已移除的旧 AGENTS.md）、G5/G7（断言已废弃的 `@AGENTS.md` 指针文件形态）。CI（ci.yml/ci-light.yml/justfile）必跑此工具——不改则 CI 永红，真回归被噪声淹没。工具是 BerryOS 自有件（`tools/`，不属 21 件 Curia 载荷），适配不违反 1:1。

## Decision

- **G2 撤**：AGENTS.md 行长纪律随旧 BerryOS 单源形态移除——模板归上游所有、本地不编辑，行长执法无对象。
- **G3 AGENTS 探针 6 串换 1 串**：`.agents/README.md`（Curia 契约锚——规格指针必须在位，上游模板演进中稳定不变）；`docs/规则/` 的 10 条探针守仍在位的域文件，原样保留。
- **G5 换判据**：`@AGENTS.md` 裸行 → **CLAUDE.md 为指向 AGENTS.md 的符号链接**（上游 2.5 桥契约）。
- **G7 换判据**：CLAUDE 指针字符帽 → **技能双桥契约**（`.claude/skills` 与 `.codex/skills` 均 `→ ../.agents/skills`）；防抄正文由链接形态天然覆盖。G1/G4/G6/G8/G10 判据未动（全部仍守真实结构）。

## Alternatives considered

- **不改，接受 CI 永红**：被拒——永久红 CI 比无 CI 更糟（真失败被噪声淹没）。
- **整体退役 docs_guard（摘出 CI）**：被拒——G4（雷册锚点）/G8（迁出目标）/G10（索引三不变量）仍守活的 docs/ 结构，拆掉损失真实执法面。

## Consequences

- 实仓九组全绿 `PASS（0 警）`（G1 45 行/G3 11 串/G4 123 锚点/G6 2054 字符/G5+G7 三桥/…/G10 117 行）；CI 推送即回绿。
- 负例 5/5 实证判据真实生效（CLAUDE 真文件回潮、链接指错、技能桥缺失、真目录冒充、规格锚丢失——全部正确 FAIL），无假绿。
- 已知既有缺陷（不在本次范围）：G4 在雷册文件缺失时直接 traceback 崩溃而非报 FAIL——exit 仍非零、CI 仍拦，仅输出形态差；顺手可修，未修。
- 上游若改桥契约形态（如换链接目标），G5/G7 会先红提醒对拍——探针即对拍哨兵。

## Related

- [纯 1:1 翻转笔记](2026-11-12-curia-25-pure-1to1-revert.md) · [1:1 迁移笔记](2026-11-12-upgrade-curia-25-1to1.md)
