# Agent Note: scheduler 95 轮会话知识传承——设计决策、踩坑、方法论与遗留

Status: implemented

## Problem

95 轮 workflow 会话（沿革五十轮起至九十五轮终局，约 2026-11-11 至 11-14）收官：调度器 v1.0→v1.4 全线、性能优化 15 项、安全修复 8 项、路线图 14/21 关闭。经验教训散落 `docs/阶段沿革.md` 末段约 100 条（5132 行起、共 6000 行），详史可检索性差——下一代会话无法低成本继承「为什么这么做」与「坑在哪」。本篇把可传承的决策因果浓缩为单一入口，时点快照以路线图与基线文件为权威。

## Decision

沉淀五节传承面：十个关键设计决策（含否决方案）、五个最大的坑、性能优化方法论、遗留事项指南、六件分析工具用法。数字均为时点值，复核以 [系统优化路线图-v1.0](../../../docs/系统优化路线图-v1.0.md) 与 `tools/sched_baseline.json` 为准。

### D1 SLEEPQ per-key 链选 64 桶 hash 而非 tree 或全双挂（0fffcbfd，七十六轮）

- 同键兄弟链（`key_prev`/`key_next`）只是第二挂点：deadline 主序与超时扫描仍走唯一 SLEEPQ 单队列，`KEY_HEADS` 64 桶 Fibonacci hashing 只收同键等待者——`wake_matching` 扫描面 O(全体睡眠者) → O(同键)。
- 否决「每对象 waitq + 超时队列全双挂」（经典形）：Task 链接字段翻倍、双挂一致性易错；本方案只在同键扫描面复用双挂收益。否决「哨兵专桶」：v1 哨兵个位数，全局兜底冷路径不值得多一套纪律。
- 桶号复用 P1 `wait_bucket`，`KEY_BUCKETS == WAIT_BUCKETS` const 断言钉死「计数桶非零 ⟺ 链非空」对应律；维护收口 `insert_sorted`/`unlink` 两机械面，无第三写点。
- 代价与影响：pick 22→26cy（键链初始化 +4cy，微基准不测唤醒面，换 O(K) 值得）；Task +16B；见 [SLEEPQ per-key 笔记](./2026-11-12-sleepq-per-key-wake-chain.md)。

### D2 PI 形 B 修复的共享表护栏 proc_tag != 0（42a794e0，七十八轮）

- 受捐面扩为「SLEEPQ 在队者 ∪ 本核 RQ 在环者」关掉 Mars Pathfinder 形无界反转；但 RQ 面受捐要求 `proc_tag != 0`——共享表（bootstrap 表「视作持有一切」）runnable 不受捐。
- 为什么：无护栏首发 smoke 即炸——fa 对端点在共享表，pong(32) 每轮捐满全环 → 优先级塌平 → G1.5 超时 + `try_preempt` 直切契约塌。护栏是「受捐方须真实可识别」的执法，不是性能取舍。
- 否决「修消费侧」（fa 迁私有表等三处改动只救一个 bench，矩阵喷洒仍在——治标不治本）与「只清不捐」（半修无意义）。代价：共享表 runnable 维持 4-3b 盲区形态，v1 粒度债与「常驻服务勿用共享表」指引共存；SMP 对核环收口留 v2。见 [PI 形 B 笔记](./2026-11-12-pi-form-b-runnable-donee.md)。

### D3 eager 止血带的可退场设计（703eb7c1，九十轮）

- 两处 `force_eager` 翻转（hexyl walk arm + `uu_run` U1 批）只动 svcmgr spawn 点：boot→BSH ok 271s→51.7s（5.2×，系统级最大提升），瞬态足迹 <1MiB（128MiB 预算论证在案），X-5/X-6 仪器面零扰动（dpg 自测族保持 demand）。
- 否决「等 0x24 批供一并解决」（裁决时程不可控且楔死诊断未收口）与「全部程序 eager」（OOM 面 + 仪器面失真）。
- 退场条件钉死：0x24 批供裁决落地且吞吐墙实证解除后，两处回 false（或按裁决形整体退役分流）——止血带与根治正交共存，不把止血做成永久债。判读 X6 观测行须先扣止血带面（hexyl pg≈29 vs 旧 84+）。

### D4 per-parent 配额用 count_type 表扫描，sidecar 计数三重否决（cd202dfa，九十二轮）

- sidecar 被否三次：①Process 无 parent 字段可挂（加字段 = 改热结构，见坑 1）②EL1 侧无宿主进程承载计数 ③退出时递减走 UAF 面。`count_type` 挂 handle 表单一真值，扫描只发生在创建面（冷路径）——正确性优先于常数因子。
- 帽值 24 定标有据：svcmgr 稳态 5-7、dpgf 满配波峰 20-22，放行全合法面。同款先例：安全修复 #5 三帽（六十九轮）同口径。
- 影响面：`process.rs` 帽常量 + `syscall.rs` fail-fast 先于分配；smoke 三跑 86/86 零误伤。

### D5 fastpath P0 扩展：一行去掉 handle 排除（e52b1fdb，五十八轮）

- fork 条件去掉 `nhandles == 0`——论证：handle 传输（take + 内联）在 fork 前共享前缀已完成且全链非阻塞；IPC 设计 §7「迁移面留 slowpath」是历史保守切分，非实测障碍。
- 直切覆盖从「无 handle 窄窗」扩到「对端空 + 恰一等待者」全形态（echo/talker/launcher RPC 全走直切），FAST direct=128/128 达 100% 直切率。
- 教训：注释里的保守边界要重新论证而不是沿用——但扩面后检查链语义每次演进都有随迁义务（IPC 设计 §7 同步）。

### D6 PI 全图重推门控：双计数哨兵而非增量簿记（daa5ef7e，六十一轮）

- 保留全图重推「零簿记」的正确性优势，用 `PI_DERIVED_WAITERS` + `PI_BOOSTED_SLEEPERS` 双计数挂 insert/unlink 机械面做可证必要条件对——常态零捐赠时 O(n²) 重推坍缩为 2 原子读。
- 否决增量簿记：与零簿记取舍相悖，v1 睡眠者个位数规模压力不显性，「规模压力显性再换」。
- 连带方法论样本：pick 19→22cy 回归归因被证伪——PI 不在 pick 微基准执行路径（孤立环无 block/wake），A/B 实测定谳 TCG 噪声/代码布局效应。

### D7 tick→ns 倒数乘-移：快路 + 域外回落精确式（06d9511e，六十轮）

- `ns_reciprocal_for` 预计算 (mul, shift)，快路 1 mul + 1 shift 消 2×udiv（每 IPC 往返省 54-138cy）；溢出证明 tick<2^52 ∧ mul<2^12 联立（@62.5MHz ≈ 2.35 年 uptime），域外/初始化前/异常频率回落旧两除精确式逐位同语义。
- 精度构造成立：round-half-up ⇒ 每 tick 误差 ≤0.5ns（QEMU 62.5MHz 二进精准误差恰 0）；编译期 const 钉板 + 15 频率对拍。顺带修 billing >288s u64 溢出。
- 设计要点：快优化永远带慢路保底——「域外回落」是这类数值替换的安全网形态。

### D8 REAPQ 容量 32→256 扩容而非排水策略改造（e13ca7f2，一百九十轮）

- 根因：SMP 参与态下 push 来自两核退出链而 drain 只有 CPU0 idle/yield 尾——rootfs churn 退出突发 33-60 个不排水堆积，32 槽溢出丢 Task Box → 标记缺（竞态型 4 绿 2 红）。
- 扩容 = 峰值 4-8× 余量 + 2KiB BSS 零成本；排水策略改造（新内核任务/新锁）属过度工程。「泄漏可接受」的旧前提在 SMP 破缺，如实更新而非辩护。
- 此刀清掉 SMP 三虫终账第三只（①vruntime 账本 RMW §A148 ②dpg_press 降软 ③REAPQ）——多虫并发时先各自单独验证定谳，再等净窗合验。

### D9 ZFOD 学习式直供 mark_zfod()（0faec353，七十四轮）

- 零臂供给成功才 `mark_zfod()`，此后该 VMO 缺页 `probe_el0` 直接 `supply_page`——内核零页直供，免 pager 往返。A/B 实证 pager 供页减 23%（1277→975）。
- 否决全量 ZFOD/预映：足迹税与 dpg 自测族 demand 语义冲突。「学习式」让策略面零配置：用过一页零的 VMO 自然收敛，无需新 ABI 位。
- 与 eager 止血带正交共存（一个消零页往返、一个消整幅装载）。

### D10 退出异步化：宁慢勿错的溢出回退（9791a99c，九十四轮）

- 退出钩子只做 mark_exited + wake joiners + push（O(1) 簿记），drain 由 `reap_drain` 尾部钩子在 idle/yield 排水点异步完成——零新内核任务、零新锁（KL 窗互斥）。
- HT_DRAINQ 256 槽溢出时回退同步路径：宁慢勿错；pinned 引用防 UAF。这是「异步化改造必须带同步保底」的定型样本（与 D7 域外回落同构）。
- 验证面：ktest done + rootfs 全标记（TERM/EXIT/APP/SIG/FAT 5000）——退出路径改动必须打满 churn 面。

### Pitfalls——五个最大的坑

- **热结构体加字段破坏 smoke**：`Cell<bool>` 新字段改 Task 布局 → rootfs PS/BC 标记缺（c58b5293，S3-a）；改用 `held_locks` 位图 bit6 零布局变化即修复，bit5 与 HELD_TABLE 冲突又实踩一次。教训：热路径结构体先想位图/复用既有位域；smoke 标记序列是最敏感的布局哨兵，缺标记先查结构变化再查逻辑。
- **agent 沙箱影子写与阵亡 null——产物先验存在性再信**：变异标定曾把「ELF 哈希未变」误读为构建没换血，实为沙箱影子写 + 同源构建字节巧合（沿革 S-5f 判例素材）；~20 波 agent 中 4 次 null/失败，主会话补位成常态预案（沿革一百八十六）；W3 外迁族 17 crate 落库无一笔回写布局树，靠巡检自审抓回重放（沿革二百二十八/二百四十四 Q52）。教训：agent 交付的关键刀，主会话先 `git log`/`ls-tree` 验存在性；关键路径留主会话直做预案。
- **环境型阻塞如实记、不阻塞结论**（Spotlight 实录，无仓内锚）：macOS Spotlight 索引在大规模构建/target 扫描期间抢盘导致文件 I/O 显著变慢，关闭 BerryOS 目录索引后恢复；同类：QEMU 字段被并行会话长跑占（SMP 终验 0/8 相执行，沿革一百九十一至一百九十四）、repo 根 `rootfs*.cpio` 全体 builder 共写（多会话之夜结构性易脆，沿革二百二十四）。教训：环境 blocked 时判定依据维持各自单独验证，终验挂「待净窗」而非宣称失败。
- **跨面语义收紧的真实成本按十文件计**：RIGHT_TRANSFER 全生态执法 10 文件 32+ 处（内核创建面 4 + boot 授能 14 + ktest 2 + spawner 1 + 用户态 dup 掩码 26，六十六轮）；fastpath 检查链每次扩面同样牵 IPC 设计 §7 与全生态调用点随迁。教训：动「随 handle/消息迁移」的语义 = 全仓搜索面，立项时预算就按生态联动计，不是单点刀。
- **测量红号先分三源：真信号 / 环境噪声 / 缓存陈旧**：VER mix 假红曾有两源——真混合构建（哨兵实战战果，属真信号）与 `build.rs` rerun-if-changed 只挂链接脚本致 `BERRY_GIT_IDENT` 冻结首跑缓存（增量机假红，修 = `.git/HEAD` + 分支 ref 双触发，沿革一百八十六）；pick 19→22 曾疑似性能回归，实为 TCG 噪声（D6）。教训：红号先归因三源再动手修。

### Methodology——性能优化方法论

- **端到端分解先行，找单一杠杆**：89 轮 boot 271s 分解 = U1 uu 批 120-131s + M3DEMO 54-60s + BSH 18-29s，三段 2/3 同一根因（dpg 单页供给吞吐）→ 一根杠杆（eager）拿 5.2×。不分解就会去修 M3DEMO——澄清后它是 fs-RPC/TCG 成本，止血带无可施力也无需施力。
- **A/B 对照与回归归因纪律**：eager 271→51.7s、ZFOD 1277→975、copy_from_user 256 锁→1 锁（16 走查）都留对照数；回归两例对照——pick 22→26 归因键链初始化 +4cy（接受，换 O(K)），pick 19→22 归因证伪（PI 不在路径，TCG 噪声）。归因不了不接受，归因得了才取舍。
- **结构性论证 > 表面修复，负空间也是产出**：P2-4 SMP 双窗穷举论证（5 读窗全在 PT 锁内 + 5 写点同锁 + last_page 窗局部 + 无锁外写者）裁定「不适用关闭」，省掉一次假修复（见 [P2-4 笔记](./2026-11-13-p24-smp-dual-window-not-applicable.md)）；tick→ns 溢出联立证明、PI 形 B 集合不相交不变式（block 先摘环后发钩 / wake 先入环后发钩）同族。
- **微基准与端到端两面互证**：pick/freeze/billing cy 级验收（`tools/sched_baseline.json` git tracked）+ boot 秒级分解 + FAST 直切率 + dpg pg/s 四面——微基准看不见 wake O(N)（「微基准不测唤醒面」），端到端看不见 +4cy；91 轮基线刷新（pick 26→22、policy delta 66.6→50.0%）与 95 轮终验各跑一遍才算闭环。
- **「零成本」宣称先测，静态审计过现役面复核闸**：AM「零额外成本」靠 fetch_add ≈0.2cyc 实测（<0.01% 量级）才敢落字（沿革一百九十一）；审计 agent 的 F2 发现经现役面复核定谳为旧面快照误判（沿革一百九十三）——静态审计产出一律过「现役面复核」闸再裁。
- **止血带与根治分刀、带退场条件**：eager（止血）与 0x24 批供（根治）正交共存；止血带写明退场条件并钉在笔记（D3），防止血固化为永久债。

### Handover——遗留事项（下一代会话指南）

- **P0-2 X-6 案② 锚窗泊车：已落树待裁**（25959c7，沿革一百三十九）——仪器刀 0 定标 1.9×，低于 kill 线 2× 与案②门 3×（锚窗命中率实测近五五开）；②① 绑定去留待 P1-2 修复后 R'≥18 再判。
- **P1-2 批供 0x24 表批：已落树待裁**（e774406e，沿革一百五十七）——soak 楔死 fix-or-withdraw 已呈仓主；裁决直接决定 eager 止血带退场（D3）与「批供 0x24 楔死（单页 0x23 往返 ~33ms/RTT）」这盏 dpg 吞吐墙的存废。
- **P2-3 pager 挂死看门狗：兄弟 lane 在席实施中**——动手前 `git status` 看 `kernel/src/dpg.rs` 是否仍是他人改动面（本会话收尾时即 M 态）；P3-2 demand 挂死设防挂其后，超时阈值同源标定。
- **P3-4~8 远期项**：P3-4 PI 形 C 设防（Event/WAIT_ANY 无 PI，形 B 框架已备）/ P3-5 FPSIMD lazy save（context_switch 135cy 大头）/ P3-6 SLEEPQ per-key 规模化（睡眠者百级+深水区）/ P3-7 1.0.0 三同步门 / P3-8 tools/berry 版本收编。
- **基线漂移复测**：`tools/sched_baseline.json` pick=22（91 轮刷新）vs 95 轮终验实测 23cy——1cy 漂移未定谳；下一代会话开门先跑 `tools/sched_ci.py` 全量对拍再下性能结论。R3 延迟界数据（中断→任务 2000cy worst ≈1.3µs / IPC fastpath 6.6k worst / 形 A ≤800ms+ΣCS / 形 C 无界）是 v2 工业 RT 的前置基准，勿重复测量直接引用。
- **idle 满核修窗五件**（沿革二百五十六调度轮三）：主根因 = guest 侧 boot 演示常驻面永不退出（task_a/task_b 内核忙循环），首刀 = 演示面 fw_cfg 门控（owner 域）；owner 调度彻底计划 12 死链留随迁窗。

### Toolbox——六件分析工具使用指南

- **[tools/sched_ci.py](../../../tools/sched_ci.py)**——CI 一管到底：build → QEMU boot → 遥测分析 → 基线对拍一次跑完；阈值与口径单源在 dashboard，本件只管编排。开门复测与回归判定的首选入口。
- **[tools/sched_dashboard.py](../../../tools/sched_dashboard.py)**——串口调度遥测一次收割 + 单屏摘要 + 基线回归检测：PERF 三指标（验收门）、公平性 FAIR-K、策略表三态开销、shadow divergence 全收一张表。手跑 QEMU 后看全景用它。
- **[tools/sched_perf_analyze.py](../../../tools/sched_perf_analyze.py)**——从 QEMU 串口 PERF 行对照验收标准：`qemu ... | python3 tools/sched_perf_analyze.py`。单指标快速判定用。
- **[tools/starvation_check.py](../../../tools/starvation_check.py)**——饥饿界离线检查器（调点调度 1.1 §6 公式），配置期工具不跑 QEMU；改权重/带宽配置后先过它再上机。
- **[tools/policy_ab_eval.py](../../../tools/policy_ab_eval.py)**——策略 A/B 评估：shadow divergence 率对照 PROMOTE（<0.1）/ DEMOTE（>0.5）/ HOLD 阈值，A-B 闭环（对账 → 评估 → 0x83 执行）的判定单源。
- **[tools/advisor.py](../../../tools/advisor.py)**——启发式策略服务：telemetry 进、PolicyEntry 出（五规则 baseline），首个真实策略生成器；确定性、非交互、exit code 契约与 ab_eval 同风格。

## Alternatives considered

- **只在 `docs/阶段沿革.md` 追记区留总结**：落选——6000 行详史不可低成本检索，且沿革已有「九十五轮终局」与「七十八轮交接总结」两篇会话级总结，再追加只会稀释；传承要点需要独立可发现的家，沿革保史实锚。
- **写入 `docs/系统优化路线图-v1.0.md` 或调度彻底计划**：落选——那些是 plans/docs 家讲「做什么与做到哪」，本篇的主载荷是「为什么与教训」（决策因果），按四句口诀归 `notes/implemented/`；路线图只承载遗留项状态，两边互链不双写。

## Consequences

- 本篇是 95 轮时点的知识快照：性能数字（pick/freeze/直切率/boot 秒数）会随后续刀漂移，权威永远是 `tools/sched_baseline.json` 与路线图状态列；本篇不新增随迁义务，引用数字时注明时点。
- 十个决策的完整 alternatives 与锁序论证在各自笔记（D1/D2/D4/D10 等）与沿革条目，本篇是索引层不是替代——深挖先读链接笔记再读沿革锚。
- 坑 3 的 Spotlight 条目为会话环境实录，仓内无档（如实标注）；其余各坑均有提交或沿革锚可追。

## Related

- [系统优化路线图-v1.0](../../../docs/系统优化路线图-v1.0.md)——遗留项状态权威（P0-2/P1-2/P2-3/P3-4~8 逐项）。
- [阶段沿革](../../../docs/阶段沿革.md)——95 轮详史（5132 行起为本文提取面）；R3 实时延迟界章。
- [eager 止血带笔记](./2026-11-13-eager-tourniquet-boot-critical.md) · [SLEEPQ per-key 笔记](./2026-11-12-sleepq-per-key-wake-chain.md) · [PI 形 B 笔记](./2026-11-12-pi-form-b-runnable-donee.md) · [P2-4 不适用裁定笔记](./2026-11-13-p24-smp-dual-window-not-applicable.md) · [P2-2 退出异步化笔记](./2026-11-13-p22-exit-async-ht-drain.md) · [P3-3 配额笔记](./2026-11-13-p33-process-create-per-parent-quota.md)
