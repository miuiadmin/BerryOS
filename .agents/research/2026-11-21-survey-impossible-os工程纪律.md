# impossible-os 工程纪律调研

Type: survey
Status: current
Updated: 2026-11-21

一句话说明：对 rizonesoft/impossible-os（单人 Derick Payne + Claude Code 编排的从零 x86-64 OS，GPL-3.0-only，2026-03 建仓）做六维工程纪律调研——规划体系、测试纪律、文档事实单源、构建工具链、开发过程取证、代码库组织——结论直接催生 BerryOS 工程纪律升级六面契约（[纪律升级总体设计](../plans/纪律升级/design.md)）。

## 问题

BerryOS 同为「少量人类 + AI 编排」的从零 OS 项目，impossible-os 用 8.5 个月 6815 提交做到真机可启动带桌面（自写 C 约 41.5 万行、3760 测试套件/17663 断言点、19 域 235 个规划文件 2753 节）——它的工程纪律里哪些是 BerryOS 可借鉴、哪些是它独有的执行条件、哪些不值得学。

## 方法与来源

六维并行只读研究（每维一 agent，全部结论带仓库内证据路径、关键数字亲手复核）。研究对象本地克隆：`/Users/murongcheng/Documents/code/impossible-os`（blob-filtered 全历史）；上游 https://github.com/rizonesoft/impossible-os 。调研日 2026-11-21，star 469。

## 结论

### ① 三根支柱（总判断）

它不是「代码写得漂亮」，而是把三条原则贯彻到病态程度：**一切声明皆派生物**（README 徽章、测试数字、体量统计、GitHub About 全部脚本生成，手写即违规）；**一切门禁带事故墓志铭**（每道门头部注释写催生它的事故——日期+实测数字，例如「裸 --amend 曾吞兄弟 13 个文件含 527 行在途代码」钉死 --only 强制）；**一切状态机器可判**（「做完」= 节级双戳，调度 oracle 只读表格行与戳记、从不读散文）。

### ② 机器可读规划语料（差距最大的一面）

todo/ 树 19 域 235 文件 2753 节，节级统一模板，README 明言唯一统治理由「a section must be implementable by a session that starts with no conversation context」。稳定地址法（D01 T13 §9，编号永不复用）；依赖活在 Implementation Order 表的 Depends On 列（环/悬空依赖 fail todo-plan.py --check）；完成 = 双戳（Verified: 日期|commit|N/N items|测试计数 + Quality reviewed: 审查明细，实测全树 944/919 个）；Deferred 带 XREF 与 RESOLVED 回执。反黑洞法条：往已盖戳节塞裸 checkbox = 塞进调度器看不见的黑洞（实测曾积累 117 项，当日清零）。仓库 4808 个提交动过 todo/——规划树本身是最高频变更物。

### ③ 数字自证链

3760 套件/17663 断言由脚本从源码重算、全绿后自动回写 git 跟踪的 coverage.md、刻意不写时间戳（数字变才有 diff）；调研侧用独立方法复核完全吻合（register 行 3763 减 runner 3 行 = 3760；去注释断言计数恰 17663）。COUNT.md（自写 97.9 万行/vendored 47.6 万行分账）由 post-commit 钩子全自动重生成。BerryOS 侧先例：bench icount 基线（git 跟踪、无时间戳、TOL=0.0）与 bench 回执（serial_sha256）已是同形态。

### ④ 门禁防绕过学

门判暂存区不判工作树（checkout-index 物化，「未提交的页面既不能满足也不能挡住一个提交」）；门路径强制清空 SKIP_LINT_* 环境变量（2026-08-08 审计发现四个检查被环境变量静默关闭而门照样绿——「principly visible is not a gate」）；prepare-commit-msg 用 pid 印记拒 --no-verify；钩子 amend 必须 --only 限定路径。构建门期望值从源现场推导（BSS 地板从 linker script 现场 grep，防第四份定义漂移；UKI 内嵌核 objcopy 逐字节 cmp）。BerryOS 对照：这正是多会话「只精确暂存自己路径」人律缺的机器对偶。

### ⑤ 测试面三独特件

smoke 矩阵 29 腿（TCG/KVM×1/2CPU 四引擎腿 + 25 条定向故障注入——坏 NVRAM 跨靴状态机、降级 init 仍须到提示符、双 panic；存在理由：一个 SYS_EXEC 竞态在 KVM 下绿着活了 297 个提交直到 TCG 腿抓到）；断言计数稳定性（「总数自己会动」当一等观测量，截断 trace 是硬错误——「完美吻合的截断子集比不报告更糟」）；测试代码禁碰面表格由编辑时 hook 强制（3 起事故在案）。BerryOS 对照：bcts 冻结标记集回放已是计数稳定性的同类物。

### ⑥ 无人值守 run 制度

tmux 内单会话 + Stop-hook 守卫 + 心跳重启的 fixpoint runner（整趟扫过零进展才停）；「一节一推、推到绿」；answers.md 决策账本（run decision + Reversal cost，操作员对前 30 问全部采纳 run 建议）；五类硬停白名单；overnight-todo.md 人机 punch-list（禁 silently defer-and-forget）；评审欠账制（「An outage is a delay, never an approval」）。另核实：Hermes 侧已有 BerryOS 四 lane 舰队在跑（w1/w2/w3 各 10 分钟 + review 3 分钟，独立克隆 + `.hermes-loop/LOOP.md` 宪法 + PR label `hermes-auto`），W1 已 691 轮——E 面是收编不是从零建。

### ⑦ 值得记录的其余件（本期不采纳）

跨模型评审 pin（Claude 写/GPT-6.2 评，one route no fallback）；docs 覆盖率棘轮（covers= 指令 + 只缩不涨基线，235/235 清零）；检测器的检查器（禁正则解析 HTML/URL + 必触发坏样例 fixture——「a detector that goes quiet is an ERROR」）；每条工具排除项附一次全树试跑的度量否决记录；空目录文档法（shim/ 空但 README 写满为什么空与恢复路径）。

### ⑧ 落点

owner 裁决四面全上（双戳/门禁与数字自证/判据单源/E 面），实施方案见[纪律升级总体设计](../plans/纪律升级/design.md)，域铁律正文见[规则/16](../../docs/规则/16-工程纪律.md)；排除清单：跨模型评审 pin、docs 覆盖棘轮、站点漂移检查、OS Comparison 表、loopx。48 条 findings 全量（含证据路径）存会话档案，本调研收口结论面。
