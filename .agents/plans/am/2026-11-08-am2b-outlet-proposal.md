# AM-2b 压力事件带内出口提案
Type: proposal
Status: active
Updated: 2026-11-08

## 问题与约束

AM-2a 已落等页停顿台账：DPG_STALL_CYC/N/MAX 三计数器 + `dpg_stall_note`（kernel/src/telemetry.rs:925-943），dpg 两臂三点计值（kernel/src/dpg.rs:172-178、282-288、299-305）；「阈值事件带内出口挂 AM-2b（am/appd 消费链成形时）」（kernel/src/telemetry.rs:920-921）。原始裁决（.agents/plans/am/design.md:37-39）：PSI 等价、partial 70ms / complete 700ms 窗口初值【推，需实测定标】、不设固定 minfree；梯度与阈值全在 am（design.md:35）——内核只出机制。
硬约束：零新 syscall（ADR #68 号预算零消耗，docs/架构决策.md:76）。号预算现状 nr_total=37 / nr_defined=41 / nr_registered=40 / nr_ceiling=45（tools/abi.toml:23,34；kernel/src/abi_gen.rs:6-8）——余量 4 号，本刀不动。am/appd 均未成形（user/bins/ 无对应产物）——出口裁决先于消费链。

## 一、三候选出口机制

### 候选①：系统压力 Event + boot 接线 gift 链（推荐）

机制：boot 建系统级 Event、内核静态 retain ObjRef（RX_EV 同款，kernel/src/main.rs:2441-2444），handle 经 gift 链交 am——boot 定值插进 svcmgr 表（现末槽 0x0001_0007 = H_FBPIPE，user/bins/svcmgr/src/main.rs:622；新槽 = 第 8 插 0x0001_0008），svcmgr spawn am 时经 env 块 extras 槽迁交（processargs v1 协议，user/berry-libc/src/io.rs:62-69；extras 同槽多产面惯例 :29-45）。am 以 `object_wait_one` 等内核占用的用户位（避开 RX_EV 已占 1<<39，main.rs:2437；取 1<<40 = partial 跨阈、1<<41 = complete 跨阈）；内核在窗口翻卷点跨阈置位——写位与 SLEEPQ 扫描同 SQ 窗（`rx_irq_notify` 形，main.rs:2653-2658）；am 持 SIGNAL 右自清位（`object_signal` 只收高 32 位用户位，kernel/src/syscall.rs:3346-3348；boot 侧 insert 授 SIGNAL+DUPLICATE——`event_create` 出生授 SIGNAL|DUPLICATE 同口径，tools/abi.toml:554-558）。恢复检测 = 边沿协议：am 消费置位后清位、以窗口长为 deadline 再等——超时且无再置位即恢复，内核不报恢复沿。
成本：内核 boot 接线（Event + retain + dup 进表 + 定值断言）十行级；env 块 extras 槽 + libc accessor 一个；am 侧 wait 协议。零新号。
ABI 面：NR 零变更；占用用户位 2 个——高 32 位面的内核占用位现无登记处（abi.toml [[signal]] 表只登记低 32 内核派生位，:200-204），归属开放问题⑤。
局限：位面只承载边沿事件；am 若需窗口占比数值作策略输入，须消息面或 telemetry_open 前向（不随本刀）。内核置位写者为新增非 SVC 写者，需修约 event.rs 纪律（kernel/src/event.rs:16-18）——或翻卷点保持在内核任务语境免修约（§二推荐后者）。
先例依赖：RX_EV 全链（main.rs:2437、2636-2658、2678-2687）= 内核 IRQ 语境写 Event 用户位 + SQ 窗唤醒 + 清位，成熟；gift 链 = svcmgr boot 手搭定值槽（main.rs:1198-1248）+ env 块（io.rs:62-69），成熟。

### 候选②：内核派生信号位（AM-1.5 WAKE_PENDING 推广）

机制：低 32 位新增 PRESSURE 派生位（1<<4），`signal_state` 的 Process 臂派生自全局压力状态（kernel/src/syscall.rs:3301-3303 现只派生 TERMINATED）；am 等自身 Process handle 的该位——libc 进程经 env gift[2] 已自持 self_proc（user/berry-libc/src/io.rs:47-49），零 gift 成本。
成本：abi.toml [[signal]] 常量层 add-only 一行（WAKE_PENDING 同款先例，:191-198）+ signal_state 扩臂 + 跨阈广播唤醒。零新号。
ABI 面：零新号；新增内核派生位一个。
局限（对 AM-1.5 先例可推广性的诚实评估）：WAKE_PENDING 成立的两支柱 = 事件有天然属主对象（目标 Process）+ 按属主钉键单点广播一跳全收（syscall.rs:3054-3072）；压力事件无天然属主——挂 am 自身 Process 承载系统级量，语义锚弱；跨阈唤醒无现成广播原语（`wake_matching` 按对象键匹配），需遍历进程表或新锚点机制。位语义 = 电平派生（am 不可清、随内核态翻转）——恢复沿通知要求内核侧翻卷持续跑，与 §二惰性翻卷冲突。任一进程可等该位，契约面全进程扩大。
先例依赖：AM-1.5 派生位路线（kernel/src/suspend.rs:11-15；syscall.rs:3314-3321）证明可行，但上述两支柱不直接复刻。

### 候选③：查询口（如实记：本刀约束下不可行）

现状核查：v1 无任何内核遥测查询面——telemetry_open 0x84 在册 stage v3 未实现（tools/abi.toml:627）、object_get_info 0x07 stage v2；已实现查询族（vmo_get_size 0x1A / process_get_exit_code 0x3A / sched_getattr 0x3D）无一可载 dpg 台账。DPG/WAKESRC/freeze 族现行消费者全在内核内：watchdog G1.5 块（kernel/src/user.rs:1977-2041）、ktest 断言（kernel/src/ktest.rs:748-752）、boot 打印（kernel/src/main.rs:3659）。
落实路径 = 前向激活 0x84（nr_defined 41→42、耗余量）或全新号（硬违 #68 与帽）——均破本刀零新 syscall 约束；且纯轮询无事件语义，am 常驻轮询烧唤醒，与省电身份及 wake-flood 遥测纪律方向相反。
保留价值：0x84 前向激活是候选①未来数值面的补全路径，独立裁决（开放问题④）。

## 二、窗口聚合机制

热路径增量：`dpg_stall_note` 现为三原子（fetch_add×2 + CAS 峰值，telemetry.rs:925-935）；窗口聚合最小增量 = +1 窗口累计原子（同窗 fetch_add）。partial/complete 双窗初值 70ms/700ms【推】——共享单累计、双观察窗双阈值，比双窗各自累计省一原子（两窗判据同源 Σ，仅窗长与阈不同）。
翻卷位置（推荐 note 点惰性）：dpg 三点计值处已有 cntvct（dpg.rs:172-178 等）——note 时比对窗口起点，跨窗即 fold 判阈置位。写者保持内核任务语境（dpg pager 探针路径），规避 event.rs:18 非 SVC 写者禁令，候选①免修约。迟滞合理性：停顿早段后静默的窗推迟到下一 note 才 fold——恰好只在压力持续（note 不断流）时通知及时，与 am 梯度动作的需求同形。tick 钩子（`irq_tick_acct` 包装面同款先例，main.rs:2609）不取：tickless idle 无 tick（sched/src/sched.rs:2511-2523）而 idle 期本无台账增量，惰性已覆盖；且 tick 语境 = IRQ 写者需修约。查询侧算无查询面，仅候选③成立（已否）。
阈值判据（PSI 语义映射）：partial ≈ PSI some = 窗口内等页停顿占比 Σcyc/W ≥ θ_p。complete ≈ PSI full 存在诚实缺口：全局 Σ 分不清并发 waiter 数（多 waiter 同窗 Σ 可 > W），full 判据需 waiter 深度跟踪（睡前后深度 ±1 原子 + 非零时长累计，热路径 +2 原子）——v1 先落 partial 单窗、complete 判据挂实测定标后再裁（开放问题⑥）。
定标路径：AM-2 验收同源 memhog 注入 → dpg 台账读点扩 ktest 标记行/打印面（WAKESRC 收割同款）→ smoke 收集 → 真机/KVM 列定标。TCG 下 CNTV tick ≈ SLICE_HZ/2 频偏（AGENTS.md）⇒ 阈值不得落编译期常量，boot 期按 CNTFRQ 换算周期数。

## 三、推荐与开放问题

推荐候选①：全链先例在场（RX_EV + boot 定值槽 + env 块 gift）、零新号、ABI 面最小、边沿协议与惰性翻卷语境自洽。候选②留作派生位族再扩展时重评（am 若 boot 手搭则其零 gift 成本有竞争力——随开放问题②一并裁）；候选③的 0x84 前向激活独立刀。
开放问题（交主会话拍板）：
1. 出口选型：①为主，或②在特定 spawn 形态下混用。
2. gift 槽位归属：内核→svcmgr boot 定值第 8 插（0x0001_0008）再经 env 块 extras 交 am，还是 am boot 手搭（svcmgr 同款）直接定值插——am 的拉起者与规则/05 监督分级先决（am 未成形，本提案不预设）。
3. 阈值初值定标路径：θ_p 初值与 70ms/700ms 窗的关系式（占比阈 or 绝对停顿时长阈）；memhog 注入矩阵、ktest 标记行与 smoke 收割脚本归属。
4. 0x84 telemetry_open 是否前向激活（余量 4→3）——本提案建议不随本刀。
5. 内核占用用户位的登记面：abi.toml [[signal]] 注记 or AM 设计文档登记 1<<40/1<<41。
6. PSI full 判据缺口：v1 partial 单窗先行，或热路径 +2 原子上 waiter 深度跟踪。

## 四、证据锚（正文已带，此处集中纪律锚与一处锚点漂移）

- 「不建第二数据库」纪律：kernel/src/telemetry.rs:879,915 与 docs/调点调度-v1.1.md:183,417 均引「#45」，但 docs/架构决策.md:54 的 #45 = 存储布局与应用沙箱；原则正身 = 调点调度 v1.1 §5.11（经 ADR #64 追认收口，docs/架构决策.md:3 增补行）。既有引用为滞后前指错锚，本提案不沿用。
- 号预算冻结常量双源一致：AGENTS.md「syscall ~30–45（v1 37 在册）」= tools/abi.toml:23,34。
- AM-1.5 设计原文写「Event 通知 token 持有者」（design.md:49），落地形 = 派生位 + 按进程钉键广播（syscall.rs:3054-3072），未建 Event 对象——候选①/②对照时以落地形为准。

2026-11-08 拍板实施：候选①，六裁决见 [am design v0.4.0](design.md)。
