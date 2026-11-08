---
name: workflow-implement
description: 用 workflow 工具编排多 agent 研究或实施波次时使用——分波设计、agent prompt 要素、串行收口、与并行兄弟会话的协调（窗口侦察/构造提交/撞号让号）。用户说「workflow 模式实施」「workflow 模式研究」「开 wave」时触发。
---

# Workflow 编排实施/研究（六波实战蒸馏）

## 步骤

1. **分波设计**：先盘点任务 → 按域分组（同域绝不并行——sched/kernel/user/docs/research 五域）；一波 2–3 路上限；实施波与研究波可混排（研究零冲突可多路）。当前兄弟会话占用域用 `git status --porcelain` + 文件 mtime 侦察（>10 分钟静默 + 转场迹象 = 窗口开）。
2. **agent prompt 五要素**（子代理不共享上下文，必须自包含）：① 仓库绝对路径与任务背景（含报告/提案的精确路径）；② 必读清单（域规则 + 现状代码，先读后动）；③ 实施步骤序列 + 明确的 blocked 条件（目标文件 M 即停）；④ 验证命令（独立 `CARGO_TARGET_DIR=/tmp/target-<名>` 避共享锁；QEMU 用 `timeout 25 qemu-system-aarch64 -machine virt -cpu cortex-a72 -m 128M -nographic -kernel <elf>` + grep 标记形态）；⑤ 禁令（git 操作归主会话；不动他人 M 文件；GPL 项目只借语义）。
3. **schema 返回**：`{status: done|blocked|failed|not-applicable, files_changed[], summary, risks}`——risks 要求含"重要偏差报告"（如门线已存在不加重复行的自主判断）。
4. **收口（主会话串行）**：`git status` 逐文件归属分离（我的 pathspec vs 兄弟 M）→ 三终检（check-docs --strict / docs_guard / 域测试）→ 分刀提交（每刀显式 pathspec，绝不 `git add -A`）。
5. **not-applicable 也是好产出**：可借鉴项否决走 rejected note（三层依据 + 重开条件）——「先裁后做」优先于硬做。

## 并行会话协调（最难的部分）

- **构造提交三基**（共笔文件追记：沿革/索引）：HEAD 基构造提交（`GIT_INDEX_FILE` 私有 index + `commit-tree` + `update-ref`）+ **工作树同步加节**（尾号位，兄弟下次 add 不抹）+ 真 index 修补（保兄弟暂存，`update-index --cacheinfo` index 基 blob）。
- **撞号**：编号类资产（沿革节号/ADR 号）先查库里**与工作树**双尾号；撞了让先落者、后到顺延（参考 #69/#70 vs SDK 预留 #75–#80 协调）。
- **同函数交叠**（两刀碰同一函数）：后刀守纪律 blocked + **留调和配方在 risks**；窗开后主会话执行配方（如 Σvruntime 账本调和 = 抬值经账点 + ktest 洗账两行）。
- **夹带零容忍**：兄弟的 M 文件永不进我的 pathspec；提交前 `git status` 复核。

## 边界

- 提交权只在主会话；agent 全部禁 git 写。
- 每波规模 ≤3 路、时长可控（深度研究单 agent 也可跑很久——耐心等 return）。
- 文档纪律全程生效（pre-commit 钩子会拦断链/lifecycle 不一致——拦了就修，别绕）。
