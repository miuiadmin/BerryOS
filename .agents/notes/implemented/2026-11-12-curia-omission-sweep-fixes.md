# Agent Note: 纯 1:1 收官遗漏清查——活文档旧形态残留 20 处修复刀

Status: implemented

## Problem

清查波（workflow 单 agent + 主会话定向 grep 交叉）扫出：机器面/载荷/接线/桥/笔记链全干净，但**活文档面仍把旧 BerryOS 单源架构当现行事实**——约 20 处失准，其中最要害是三条「回写 AGENTS.md」活指令（规则/README 维护合同、规则 13 第 7 条与 checklist——照做即破 1:1 载荷纯度）、路线图 v1.1 死链两处、以及把已移除的归属裁决表当现存权威的引据四处。

## Decision

- **规则/README.md 一刀重构（9 处）**：新叙事 = BerryOS 规则正文的家 = `docs/规则/` + `docs/命令速览.md` + `docs/架构决策.md`；AGENTS.md 归 Curia 上游模板（零回写），CLAUDE.md 符号链接桥；结构表换五层现形态（含 G5/G7 桥契约行）；维护合同四条改述（改约定 → 架构决策新 ADR + 域文件；执法清单对齐 docs_guard 适配后八门）；「身份与原则」「冻结常量」死节引用改指架构决策/纲领；路线图 v1.1→v1.2 两处。
- **随刀七件（11 处）**：索引.md 头注与 README/命令速览/规则三行、命令速览 docs-guard 描述与回写指令、规则 13 第 7 条与 checklist、雷册来源段补时点演进注 + 维护契约括号更新现形态（三处一动契约本体不动）、判例索引半句、版本化提案加「裁决表已移除勿援引」时点注、ci.yml:44 门禁注释正述。
- **垃圾清理**：tools/__pycache__（本轮单测产物）、/tmp 全部 curia-250*/curia-audit/neg* 残留、`git worktree prune` 清 curia-bridge.WA9z 悬空登记（18→16；berryos-g43c1 正经树不动）。

## Alternatives considered

- **只清查不修（残留留 owner）**：被拒——「回写 AGENTS.md」是活指令，下一个读规则 13 的 agent 会照做并破坏 1:1；修是刚需不是润色。
- **连 GUI-8 侦察快照、absorb-2026os 路线图计划文档一起改**：被拒——时点快照/时点计划文档，改写违时点语义；等下次实质触及再更新。

## Consequences

- 双执法终态：docs_guard PASS（0 警）、check-docs --strict 170 篇 0 告警；规则 13 的 G3 探针串（排除 8 问/Apache-2.0）与 G4 锚点行未受改动影响。
- 至此活文档面零「回写 AGENTS.md」指令、零旧指针/行长律/裁决表现行引用；已知保留：G4 traceback 缺陷（笔记在案）、GUI-8 与 absorb 路线图两处时点快照（低优先）。
- 清查波环境注记：主树分支 wip/sched-1.1，并行会话提交会叠在 Curia 面之上（c671fe1b 后已有兄弟轮十五提交）——对拍/回溯勿按固定 HEAD 假设。

## Related

- [纯 1:1 翻转笔记](2026-11-12-curia-25-pure-1to1-revert.md) · [docs_guard 适配笔记](2026-11-12-docs-guard-adapt-1to1.md) · [1:1 迁移笔记](2026-11-12-upgrade-curia-25-1to1.md)
