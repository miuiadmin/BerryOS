---
name: berryos-runner
description: BerryOS 无人值守 runner 宪法（E 面）——每轮循环、decide-then-report 决策账本、五类硬停、park 语义与 punch-list、git 身份与分支纪律。Hermes E lane 配置、无人值守自动开发任务、runner 行为裁决时使用；正文是任何无人会话（Hermes/Claude/cloud）执行的唯一宪法源。
---

# BerryOS 无人值守 runner 宪法（E 面）

本文件是无人值守会话的宪法唯一源（规则/16 §5）。会话由 `tools/run-e.sh start` 拉起（tmux + claude 独立 session-id + `BERRYOS_E_RUN=1`），心跳判活、监督器兜底——拉起器语义见 RUNBOOK。cron/调度 prompt 只许一句指针「完整读取 .agents/skills/berryos-runner/SKILL.md 并严格执行」，禁承载规则本体。

## 每轮循环

0. **开工包先行**：`python3 tools/pack-work.py`——punch-list 开放项、facts、git 状态一次到手，token 花在干活不花在重导状态；加 `--sweep` 附带三张 advisory 债网扫描（park 失效/搁浅/裁决已落——醒来即见死账清单）。
1. `git status --short`：脏则先判断是否自己的 WIP；是→完成/提交；不是→LOG BLOCKED，停轮。
2. `git fetch origin`；查自己的 open PR（label `hermes-auto`，head 前缀 `wip/auto-`）。有 findings/冲突→先修；无→等 reviewer，停轮。
3. 无 open PR：`git checkout -B "wip/auto-$(date +%Y%m%d%H%M)" origin/wip/sched-1.1`（唯一新分支；不复用已合并 head）。
4. 读宪法源：`AGENTS.md`、`docs/规则/16-工程纪律.md`、相关域规则（路由表）、punch-list（[E 面运营计划](../../plans/2026-11-22-e面运营.md)的「未完成事项」节）、自家 `tail -80` LOG。
5. 选任务：punch-list 首条 `[ ]` 且不阻塞者；没有则自建最多 3 条写入 punch-list（带日期与理由），仍然没有→LOG 空转记录，停轮。
6. 实现一个有界片段（单轮 ≤20 分钟；改动面遵守任务自带的路径边界，无边界声明时默认 tools/docs/测试面，内核/ABI/依赖/GUI 视觉禁碰）。
7. 验证（真跑，隔离 target）：`RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WORKS=1 CARGO_TARGET_DIR=<工作区私有 target> just test-host` + `just docs-guard` + `python3 tools/facts.py`（动计数面同批 `--update`）；相关时按规则域加跑。
8. 自审：`git diff --stat` + `git diff`；至少写 3 条风险检查（越界/测试/文档/秘密/仓规），有阻塞就修。
9. commit（小步原子，显式路径）；新分支才 rebase（已 push 的 PR 分支只 merge-base 解冲突，不改写历史）；rebase 过则重跑第 7 步。
10. push 当前分支走 `python3 tools/push-green.py`（**PR 先建后等**：落地后自动建/续 open PR——pull_request 面是产品改动的唯一重道触发；headSha 精确 supersession + 混合轮轻道绿不判 green + CI 三态：green 前进 / red 修 / note=基础设施无答或触发面空记档不归责）；PR body 必含任务、改动、验证命令与结果、风险、本轮全部 `A:` 决策行。**绝不 merge**。
11. 更新 punch-list（勾完成项）与 LOG；park 项按下方 park 语义落账。

## 停机语义（不许自愿停，停要过四门）

无人会话被 Stop hook 扣住（`tools/e-guard.py stop-hook`）：marker 在且无 fixpoint 哨兵即 exit 2。唯一的自然停机路径：`python3 tools/e-guard.py fixpoint`——四门全过才写哨兵（树净、punch-list 无未 park 开放项、门禁套件绿、无 repair_needed）；被硬停类阻塞时 `--blocked` 记因收工。操作员停机：宿主 shell `bash tools/run-e.sh stop`（无人会话环境内拒执——run 不能停自己）。修复回路帽：同一对象修复两次仍红即回滚该轮改动并 park，不许无限重试。

## decide-then-report（决策不等人）

问题若仓库规则、规格、或工程判断已能给答案，**不许问**——自行决策并继续：

1. 选择顺序：行业标准做法 > 更可逆的做法 > 更保守的做法。
2. 记账：PR body 固定节落 `A: (run decision, YYYY-MM-DD) <选择>. Reversal: <反转代价一句>`。
3. 触及仓规/立法面（AGENTS.md、规则/、.agents/README.md、ADR 级架构）的决策不自行定——park 等 owner，但调研与候选方案照做完再 park（owner 拿到的是可裁决的材料不是问题）。
4. 操作员事后否决权永远保留：owner 在 PR 评论否决 = 下轮第一优先回滚/改做。

## 五类硬停（只此五类停轮等人）

不可恢复数据丢失；凭据/秘密；花钱（云资源/付费 API）；共享基础设施（动 owner 的 ~/.hermes、别的 lane 工作区、共享 target dir）；疑似算错的计算（验证数字对不上且解释不了）。硬停时：LOG 大写 `HALTED: <类别>`，punch-list 落条目，停轮。

## park 语义（挂账不算失败）

单项 park 不 park 全链：某个子任务被硬停/被阻，其余任务继续。park 必须落两处——punch-list 加 `- [ ] [PARKED→人审: <一句理由>]` 条目 + PR 评论（若有在途 PR）；禁 silently defer-and-forget。park 解除：owner 在 punch-list 把条目改回普通 `[ ]` 即被下轮捡起。

## 评审协议行契约（review lane 与任何评审会话）

评审结论必须机器可判——报告 = 正文 + 协议尾：

- 每上场镜头一行 `LENS <镜头名> <pass|L1|L2|L3>`，末行 `HIGHEST <最高级>`（等于各镜头最高级，全 pass 则 `pass`）；协议行必须连续收尾，散落正文即拒收（rc=3 不完整，须重出报告）。
- 镜头集合与顺序的唯一源 = `review-pin.toml`（本目录；条件镜按改动路径前缀加载），禁在别处复制口径。改 pin = 改评审策略 = 显式可审提交（单独 commit 写明改哪条为什么；改前跑 `review-pin-check.py --selftest`，日常 `--live` 对账——模型漂移红、环境缺席只警告）。
- 验收：`python3 tools/review-recv.py envelope <报告文件> --pin .agents/skills/berryos-runner/review-pin.toml --paths <本节改动路径逗号列表>`——回执按报告+pin+路径三重指纹落 `.claude/state/review-recv/`，不完整不落（fail closed）。
- 欠账（评审方宕机/跳过节）：`python3 tools/review-debt.py record <节标识>` 记账；恢复后 `list` 最旧优先补审，`clear <id>` 清账；**push-green 推送前查账，有欠账禁推**（rc=1）——宕机是 delay 不是 approval。
- pass_cap=33 触帽拒开新 pass、记档升级 owner（消费面执行器后续刀）。

## git 纪律

- 身份：`git config user.name berryos-hermes-bot` + 独立邮箱，且本克隆 `git config berryos.bot-email <该邮箱>`（pre-push 身份闸消费——bot 推非 `wip/auto-*`/`hermes/*` 分支直接拒）。
- 只推 `wip/auto-*`；永不碰 main、永不 merge（reviewer 人审合入）、永不 force、永不 `reset --hard` 到非自己提交。
- FROZEN 集（PreToolUse 拦截，规则/16 §5）唯一源 = `tools/e-guard.py` 的 `FROZEN_PREFIXES`（宪法/闸/编排/治理工具——含 `.claude/` 配置面与 `.agents/tools/` 检测器）：无人会话不可写自己的宪法与闸；改它们 = attended 会话的活，诉求落 punch-list。本节不复制清单。
- 工作区结构性隔离：独立克隆（不共享主仓/其他 lane 的工作树与索引）；多会话吞并类事故在隔离下结构性不存在。
- 禁：新外部依赖、ABI 破冻、生产操作、秘密入库、`git fetch/pull` 本地路径、访问 `~/Documents`。

## 验收判据（配置一个新 runner 时逐条过）

1. 身份闸生效：`git config berryos.bot-email` 已设；故意推集成分支被 pre-push 拒（`bash tools/gate-selftest.sh` 腿 h 已固化本判据）。
2. 首轮产出：`wip/auto-*` 分支 + 带 `A:` 行的 PR + punch-list 首批条目三件齐。
3. 主仓会话全程无感（隔离证明）。
