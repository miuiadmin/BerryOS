# impossible-os 自动化开发深研（第二轮）

Type: survey
Status: current
Updated: 2026-11-22

一句话说明：对 rizonesoft/impossible-os 的自动化开发内脏做第二轮六维深研（runner 本体/fixpoint 循环/推绿链/评审流水线/Claude 配置治理/治理演进——每维精读真码而非文档），结论直接支撑 BerryOS 自动化开发二期（[E 面运营计划](../plans/2026-11-22-e面运营.md)后继刀）的差距表与落地配方；第一轮面级调研见[前一份 survey](2026-11-21-survey-impossible-os工程纪律.md)。

## 问题

第一轮确认了它「有什么」（overnight runner、一节一推、跨模型评审、决策账本）；本轮回答「怎么转的」：进程怎么活、循环怎么选活、推送怎么等绿、评审怎么记账、AI 配置面怎么治理、治理本身怎么演进——以及 BerryOS（Hermes 四 lane + E 面宪法 + 门禁全家桶）与它的机制差在哪、什么可平移。

## 方法与来源

六维并行深研（每维一 agent，读真码优先、文档对照、不一致点名；量化数字从代码读出）。对象本地克隆 `/Users/murongcheng/Documents/code/impossible-os`（已滚至 2026-11-22 上游）；48 条 findings 全量含证据路径存会话档案，本文收口机制总图与配方面。

## 结论

### ① 自动化开发的总图（它为什么搞得动）

单入口拉起器（`scripts/run.sh` 157 行：验工具链 → skill_denies 权限预检 → 写 marker → tmux 起 run 会话 + 心跳会话；launchd/oneshoot 自恢复）拉起**唯一** Claude 会话：独立 session-id（重启接自己上下文，不接操作员的）、`OVERNIGHT_SEQUENCER_RUN=1` 环境判别（一切钩子凭它区分无人/有人）、400K 自动压缩 +「盘即内存」（cursor/git/TODO 三件套，技能每压缩后重读）。会话**不许自停**（Stop hook exit 2，唯一放行是机器验证的 fixpoint 哨兵——哨兵全仓一个写点、写前过四道闸防伪造）也**不许自拆**（FROZEN 八文件 + PreToolUse 反作弊封条）。心跳会话 300s 一拍：以会话私有 transcript mtime 判活、闲 45 分钟送一句 nudge、死会话按 300×2^n 退避（封顶 1h）`--resume` 自己。supervisor 只做两件事且**永不许成为停 run 的原因**：3 小时零产品产出把 machinery 自愈提交整批 revert 成一个提交；6 小时零 ship 写 repair-needed 换诊断 nudge。

每轮干活六拍：advance-work 拼开工包（oracle 三态分类算出的下一节 + section-pack 定向材料 + 上轮 checkpoint + preflight 树哈希戳）→ 实现 → 本地门 → 一节一推（后台化 push、`@{u}..HEAD` 空为落地判据）→ ci-wait 等绿（20s 拍、45 分钟未启动/3 小时未完双熔断、exit 124 重发、supersession 防假警）→ 回执与 rollover。评审是旁路流水线：pin 文件唯一事实源（dispatcher 对照 ~/.codex/config.toml 硬校验）→ 契约化 prompt → 回执是 LENS/HIGHEST 协议行（机器验收，缺行 rc=3）→ 宕机建模为债（记账→禁推→恢复后最旧优先补审）。治理自身：事故→带测量数字的条款 + 同 commit 挂 hook 三层立法；控制面 manifest（25 前缀命中即跑 98 个测试）；machinery-undo 防无限打磨；承认 30/30 橡皮图章后立法 run 自决 + operator 否决永远赢。

五条设计公理（全部实测教训内嵌代码注释）：一切状态在盘上、会话可牺牲；一切「过没过」是机器可判凭据不是叙述；一切闸门 fail-direction 明文（基础设施无答=note 不=红）；一切规则带事故日期与数字；治理自身也被治理（FROZEN/log-only 先行/可删除立法）。

### ② BerryOS 差距表（它有而我们没有/更弱）

| # | 机制差 | 它的形态 | BerryOS 现状 |
|---|---|---|---|
| 1 | 拉起器+心跳+监督外壳 | run.sh/heartbeat/supervisor 三件套，会话死活有人管 | Hermes lane 只有 cron 到点开新轮；进程死了没人接、只磨工具没人管 |
| 2 | 会话不许自停 | Stop hook 三角 + fixpoint 哨兵 | E 面宪法是 prompt 级，会话想停零阻力 |
| 3 | 开工包拼装 | advance-work + section-pack（醒来零重定向） | lane 每 10m 重读 LOOP.md/git log 自行重定向 |
| 4 | 推到绿机器链 | ci-wait 双熔断 + supersession + 内容寻址回执 | 「永不 merge」形态天然缺等绿半边；无回执缓存 |
| 5 | 评审协议回执 | LENS/HIGHEST 行 + pin 校验 + 欠账五件套 | review lane 产出自然语言 PR 评论，机器不可判 |
| 6 | 两张债网 + 决策回流 | stale_parks/stranded_deferrals/awaiting-answer 自动重开 | punch-list 纯 markdown，答案落盘无机器读回 |
| 7 | FROZEN + 控制面 manifest | 宪法/钩子 run 不可写，改控制面必跑其测试 | runner 会话可写自己的宪法与钩子，改了没人测 |
| 8 | 配置面登记审计 | MANIFEST 五向漂移 + 63 hook 全事件 + 预算观测 | .claude 面接近空白，提醒无预算无跟随率 |

### ③ 落地配方（按优先级）

**P0（自动化二期骨干）**：① `tools/run-e.sh` 拉起器（marker + tmux + launchd 自启 + `BERRYOS_E_RUN=1` 判别 + stop 对该环境拒执）；② 心跳（transcript mtime 判活/300×2^n 退避 resume/最小 nudge）+ supervisor（machinery 前缀清单=宪法 skill/.githooks/编排脚本；3h 零产品产出整批 revert；6h 零 ship 告警）；③ Stop-hook 守卫（punch-list 空 + 门禁绿 + CLI 复核才许写放行哨兵）；④ 开工包拼装器（facts/双戳/punch-list → JSON）；⑤ 一节一推链（后台 push + 落地判据 + ci-wait 等价含 supersession）。**P1（凭据层）**：⑥ 评审协议行回执 + pin；⑦ receipts 内容寻址（装 icount/构建证物）；⑧ run-artifact 信封。**P2（治理加固）**：⑨ FROZEN 集 + 控制面 manifest 条件测试门；⑩ 两张债网 + 决策账本回流；⑪ .claude MANIFEST + 预算观测；⑫ agent-replay 换模型闸（远期）。每条的详细配方与代码证据在会话档案 48 findings 中。

### ④ 不必抄的

单会话单树形态（Hermes 多 lane 是更适BerryOS 的骨架，缺的是上述层不是形态）；他们的 todo-plan 依赖图全量（我们 punch-list + plans 双戳够用，先做 oracle 读 punch-list 再议图）；CLAUDE.md 506 行巨型宪法（我们 AGENTS.md 轻注入 + 规则/16 分域 + skills 按需加载的形态上下文成本更优——他们也在外移历史降成本）。
