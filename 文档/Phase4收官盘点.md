# Phase 4（IPC 与服务框架）收官盘点

> 产出方式：2026-10-16 会话 workflow 多智能体审计（5 研究 + 2 对抗验证 → 合成），基线 e95c6a0；行号已对工作树落点核对。
> 结构：第一部分 = 收官报告（出口对表 + 实证修正）；第二部分 = P5 入口建议；附录 = 挂账修复方案。**执行结果见文末**——P5-0 清场两提交（d4dcf6a runqueue 两元环修 / a27eb9c waiter 位拆分）已落地并过全 gate。

---

## 第一部分：收官盘点报告

- 日期：2026-10-16 ｜ 分支：p4-svc ｜ 基线：e95c6a0（工作树 `.claude/worktrees/34c`）
- 输入：五份研究（出口对表 / 遗留扫描 / P5 调研 / 代码分析 / 实证）+ 两份对抗验证（语义 / 时序）；本报告为合成结论，未新开调查（仅对修复方案引用的代码行号做落点核对：`进程调度/src/runqueue.rs:70`、`内核/src/user.rs:557-671`、`内核/src/main.rs:886-917`、`工具链/smoke.sh:108` 均实证在位）。
- 姊妹篇（同批产出）：《P5 入口建议》《waiter/pinger 前存挂账修复方案》（见本文第二部分 / 附录）。

## 0. 结论（先行）

**Phase 4 出口标准达成，可宣称收官（含 G-eco 生态关）。** 14 项出口逐条对表：landed 9 / partial 3 / deferred 2；partial 均为「机制已落 + 复测/扩展留位」而非缺功能，deferred 均为阶段化目标明文推迟（非遗漏）。无单项阻塞收官宣称。

收官话术须带三条限定 + 一条时效性提示：

1. **v1 ABI 实现数 25/27**：0x33 process_exit / 0x35 thread_start / 0x36 thread_exit 未实现（退出三号随后续批）——v1 口径下最大的一笔在册欠账，P5 前宜落；话术勿称「全量」。berry-libc 现以 `park()`（补发 EOF 后 10s 周期永睡）充当用户程序收尾。
2. **稳态矩阵带死者基线** rogue_ptr + waiter + pinger：waiter/pinger 是全系统唯一在册「bug 类」挂账（其余留项均为设计性），已实证定位 waiter 根因并修正 pinger 归因（见 §4），修复方案见姊妹篇，宜 P5 前修；修后基线收缩为 rogue_ptr 唯一。
3. **intent = 传播五件套已落、声明入口与消费者留 v2**（ADR #42 两段式既定形状：查表键先于消费者）。在册错位：ABI options 低 8 位域已冻结（0x01–0x05 合法）而内核对非 0 一律 -EINVAL——实现入口时须对齐（含 Job 级授权与 pledge 治理）。
4. 时效性：ABI §8.0 对外层冻结触发条件「G-eco 第三方样例或公开发布」很可能已随 hexyl（crates.io 上游逐字未动，d-3c）满足——**下一次任何 ABI 变更前应先补 IDL 单源生成 + CI 结构体布局断言**（当前内核侧与 berry-sys 手写两份靠人工同步）。

## 1. 出口对表（14 项：landed 9 / partial 3 / deferred 2）

| # | 出口条目 | 判定 | 证据（commit + smoke 标记 + 代码/测试） | 收窄与备注 |
|---|---|---|---|---|
| 1 | 同步消息传递（seL4 风格）——Phase 4 首条 | **landed** | 830e7fb channel 三号（有界队列深 8 / inline ≤256B / 随行 handle ≤4 + handle 随消息迁移）；d1b58b3 CHAIN ok=16；59c6a98 wait_many MANY ok=17；ba5a0c0 fastpath FAST ok=20；`内核/src/channel.rs`；ktest H/I 段盖全错误面 | v1 形态 = 非阻塞有界队列原语 + 阻塞收敛到 object_wait 信号面（非 seL4 rendezvous 严格同步）——拆步表「前置设计对齐 #1」明载此收窄，ABI 签名冻结无 flags 位。另注：路线图列的 `进程间通信/` 目录仍空（仅 .gitkeep），实现收敛在 内核 crate——目录布局与路线图字面不符但功能全落 |
| 2 | 异步通知（Event / EventPair / IRQ 投递） | **partial** | c8148c9 event 三号 0x54/0x06/0x04；5fb4ffd 4-1 阻塞 wait + 遍历唤醒（WAIT ok=15）；`内核/src/event.rs` | 缺 EventPair（仅 `内核/src/handle.rs:133` KernelObj 变体占位、无 create 号）与 Interrupt 对象 / IRQ→Event 投递（IPC设计 §3 未做，GIC 属「启动关键暂留内核」）。v1 基线只列「Event 异步通知」，已足；EventPair/IRQ 面无排期，需求随用户态驱动（P5/P6） |
| 3 | 经能力的共享内存授权（grant） | **partial** | 5c02272 VMO/Vmar 九号全量 rights 门控（map 需 MAP 且映射 rights 恰 READ\|WRITE）；VMO 跨进程交付活证（3-4b 礼物 VMO 随消息迁移、4-5b svcmgr vmar_map 映子进程） | 「两进程同映一枚 VMO 零拷贝交换」的显式 IPC 路径未演示（礼物 VMO 只 vmo_read 未 map、rootfs VMO 只读消费）——IPC设计 阶段化目标明排 v2；机制在、消费面留 v2，不硬凑 |
| 4 | 服务管理器 / init 骨架 | **landed** | 1044e68 4-5a console 服务循环（首个 wait_many→drain 服务循环 + 调试槽 0xFE/0xFF 退役 + 用户打印全量真 IPC）；29cac7a 4-5b user_svcmgr EL0 经真 syscall 全链拉起 echo（SVC ok=21）；G-eco 后 svcmgr 亦编译平像并 rootfs 拉起 hello/stdhello/hexyl | v1 形态：svcmgr 为 boot 手搭 EL0 进程（非自举 init 链）；console 因「用户态无 MMIO 映射」留内核任务（ADR 启动关键暂留内核），用户态同构服务循环由 echo/talker 兑现 |
| 5 | 服务注册与发现（路径式命名空间） | **deferred** | 无实现（零 namespace/mount/badge 符号）；v1 以纯能力互联替代：bootstrap 迁移（3-4a）+ handle 随消息（3-4b）+ 一次 send 迁移双 handle（4ae38e0，DEMO ok=23） | IPC设计 §5/§8 明文「v1 先用纯能力传递跑通服务互联；namespace 作为可用性层后加」——v3 项（namespace 服务 + mount + badge 同批），诚实缺口、按计划推迟 |
| 6 | 监督/重启（失败隔离的服务面） | **deferred** | v1 明确不做（4-6 草案 §6 Q3 已答）；进程级失败隔离已落（2b-4 起只死自己 + watchdog 错杀断言） | 缺的是服务级监督重启，随 v2 svcmgr + Job；路线图 Phase 4 字面未单列此项 |
| 7 | 出口增项①：AI 最小演示（遥测→离线策略 + 快路径开销与策略无关） | **landed** | 57964e1 4-6B：tele dump 双行 schema → `工具链/advisor.py`（确定性规则 + 越界探针）→ bind 注入（+8 合法 / +200 被 clamp 64，probe `for_each_sleeper` 机器断言 `prio_effective==64`）→ rt2≤2×rt1；AIDEMO ok=22 | 滑移条款未触发（草案 §7）。留位两处：开销不变量为 TCG 容差版（-icount 精确双轮/真机 p99 留 G 卡）；演示主体 = 包络内权重微调，EPP/放置字面上 v1 无消费点（管线全链是真活证） |
| 8 | 出口增项②：intent 传播五件套（ADR #42） | **landed** | 2939eae 4-3c：msg_intent 消息头（fastpath 一次快照零记账、结算全 recv slowpath）+ 每消息重导 + 三道帽（hop 8 / 每跳 50ms / 链 400ms，超顶降级不 panic）+ channel opt-out + 通胀三计数器；INTENT ok=19 + ktest K 段十用例；`进程调度/src/intent.rs` | 五件套之 1「reply 回退」以「每消息重导自然回落」等价兑现（v1 无 reply 原语）；共享内存 VMO 不自动传播 = 机制上免费。无调度消费点（第一消费者随 EEVDF/v2）为 ADR #42 既定形状 |
| 9 | 出口增项②附：fastpath 验收线 <1200/<500 cycle 实测复核 | **partial** | ba5a0c0 4-4：fa_ping/fa_pong 128 轮专用 bench，RT≈28.4k 参考 cycle，命中/直切 128/128；FAST ok=20 + ktest L 段 | QEMU TCG 含翻译开销出不了真 cycle——只定基线量级；真机 <1200(x86_64)/<500(A57)（seL4 档）线明文留 G 卡复测（IPC设计 §7） |
| 10 | IPC设计 §2.1 PI 捐赠（QNX 发送者语义）+ 拆步 4-3 | **landed** | b1a2907 4-3a pick 按 prio_effective 出选（跨层严格优先）+ 808e94b 4-3b PI 捐赠（SLEEPQ 全图重推 ≤8 轮 + 冻结随行 + handle 表反查受捐）；PI ok=18 + ktest J 段八用例；`内核/src/pi.rs` | 拆步表 4-3 验收字面「client 唤醒延迟上界（G 卡用例）」留正式关卡——本步验收 = 字段断言 + 功能闭环（IPC设计 4-3b 明载）。Event 捐赠边为诚实缺口（v2 再议） |
| 11 | G-eco 生态关：未修改第三方 Rust 程序跑通 | **landed** | 4d2234b→07b6ed0 五步全落：hexyl 0.17.0 上游逐字未动经 musl target + berry-libc shim + berry-loader 流式装载全链，256B 输入 2156B 输出与宿主基准逐字节全等（HEXYL ok）；路线图已标「已过（2026-10）」 | 硬验收字面「ripgrep/hexyl 级别」——hexyl 已满足量级，ripgrep 未验不构成缺口。连带交付：rootfs 通道（fw_cfg）+ berry-sys 编译链 + std 面（D4 路线① 收官） |
| 12 | 里程碑：两个用户态服务交换消息 | **landed** | 4ae38e0 4-6A：svcmgr 拉起 talker（第 9 个 EL0）+ echo2（第 10 个），talker 凭消息收到的 console 别名与 t2 端点 round-trip 验回显打 DEMO ok=23 + 稳态 'd'；能力编排 = duplicate + 一次 send 迁移双 handle | 更早活证：pinger/ponger 两 EL0 阻塞链（CHAIN ok=16）；talker/echo2 是「svcmgr 拉起的两个服务」最纯形态。连带修复 try_preempt 幻影出选饿死（c013ca3，出选:切入 29:1→1:1） |
| 13 | 里程碑：服务管理器拉起一个服务 | **landed** | 29cac7a 4-5b：process_create（双出参）→vmo 三步→vmar_map→channel_create→thread_create→process_start（bootstrap 迁移自证）→send/wait/recv 验回显打 SVC ok=21 + 稳态 'v'，零内核代劳 | 超额兑现：G-eco-c/d 同链又拉起 hello/stdhello/hexyl（svcmgr 现拉 5 个程序） |
| 14 | 拆步表 4-0/4-1/4-2：阻塞 object_wait 面（Phase 3 尾注 P4 首务） | **landed** | 5c090fb + 429758c（4-0 SLEEPQ + block/wake + thread_sleep + 唤醒遥测环，BLOCK ok=14）；5fb4ffd（4-1 阻塞 wait_one：poll-first + 醒来重验 + 钉住纪律，WAIT ok=15）；d1b58b3/59c6a98（4-2 信号位空间分半 + wait_one 扩 Channel + wait_many，CHAIN ok=16 / MANY ok=17） | 阶段化目标 v1 行主体全落 |

## 2. 里程碑活证

- **smoke 双相全绿为常态**：裸相 23 标记 + rootfs 相 7 标记（ROOTFS ok src / ROOTFS vmo / rootfs hello / ECO ok / STD ok / STD err ok / HEXYL ok）+ switches 金丝雀严格递增。P4 新增标记 14–24：BLOCK / WAIT / CHAIN / MANY / PI / INTENT / FAST / SVC / AIDEMO / DEMO / ECO。
- **双里程碑超额**：svcmgr 同链现拉 5 个程序（#13）；两服务互联走「一次 send 迁移双 handle」的纯能力编排（#12）。
- **生态终验**：hexyl 0.17.0 上游逐字未动，输出与宿主基准逐字节全等——第三方 musl std ELF 全链（shim + 流式装载）的机器断言级证明。
- **质量面**：ktest 新增 J（PI）/K（intent）/L（fastpath）/DTB 四段；host 11+29 测；run-el2 前缀回归；两轮 100s/75s QEMU 实证 boot 确定性（两次 boot 稳态字符数逐位相同）。

## 3. 遗留缺口两级分类

### 3.1 A 级：阻塞宣称收官

**无。** 拆步表 4-0~4-6 与 G-eco 五步范围全部落地，无单项阻塞。

### 3.2 B 级：诚实留项（逐项 29 条；原扫描自计 28，逐条清点实为 29——计数出入如实保留）

**G 卡复测类（3，随真机/精确计时同批）**

| 留项 | 排期 | 现状一句话 |
|---|---|---|
| fastpath 验收线 <1200/<500 cycle 真机复测 | G 卡 | TCG 仅 ≈28.4k 参考 cycle（含翻译开销）；TCG 侧已收检查链/语义恒等 + 128/128 计数 + FAST ok |
| PI 三任务链 client 唤醒延迟上界 | G 卡 | 4-3b 已收字段断言 + 功能闭环（PI ok + ktest J），定量面依赖真机/精确计时 |
| AI 开销不变量精确版（-icount 双轮 / 真机 p99） | G 卡 | 容差版 rt2≤2×rt1 已收 AIDEMO ok；clamp 单调收窄已是机器断言 |

**bug 类挂账（1，唯一在册）**

| 留项 | 排期 | 现状 |
|---|---|---|
| waiter/pinger 前存超时自陷 + s/C/P 早停 | 挂账待查（宜 P5 前修） | 已实证定位与修正归因（见 §4），修复方案见姊妹篇 fix_plan；不阻塞收官（24 标记在死亡窗口前完成、watchdog 错杀断言按基线执法），但 P5 shell 起交互进程后该家族风险放大 |

**v2 既定（12）**：共享 VMO 零拷贝大消息通道 + futex（0x55/0x56，映射原语已落缺约定面）；Job 树 + 撤销 + 创建权门控（process_create job=0 豁口——结构性前置缺失，v1 无真实威胁面）；内核对象 Job/Interrupt/Timer/Resource（EventPair 无排期，单点通知面 Event 覆盖）；rights 位全量执法（RIGHT_DUPLICATE 已定义不门控，duplicate 只紧不松兜底）；用户页 UserRo(AP=11) + NX/W^X；object_signal 类型面扩展（随退出批）；console 用户态化（需设备 syscall 0x71–0x73 + Resource；真 init 亦随 P5/v2）；服务监督/重启（svcmgr+Job）；intent 声明入口 + IntentProfile 消费者（ABI 域已冻结 vs 内核 -EINVAL 错位在册；第一消费者随 EEVDF M4/timer 号段）；公平回退 fair class（4-3a 明示接受，EEVDF 底座 M4）；Event PI 捐赠边（再议；wait_many+哑 Event 纪律主动退出 donor 面）；SMP 睡眠/唤醒重做（G2 关卡前不缴多核税是成文纪律）。

**v3 既定（4）**：channel 对端 DEAD 检测 + v1 泄漏政策三件套收口（泄漏方向安全；stdin EOF 已用零长消息约定在消息层补等价语义）；telemetry_open(0x84) 消费路径（环写入 + ABI 形状 + watchdog 断言已落，只写不映）；路径式命名空间 + mount + badge（0x81/0x82）；AI 策略注入运行时 ABI policy_bind(0x83)（2026-10-16 ADR #21 修订：机制预留 + 实验轨道，不立运行时 ABI 正是修订内容）。

**未定排期（8）**：**退出三号 0x33/0x35/0x36（v1 口径最大欠账，P5 前宜落；连带 ktest B 段样本换号 0x35/0x36）**；睡眠超时 tick 粗粒度（tickless 精化，量差非质差、有 idle 兜底）；fastpath 随行 handle 面（纯性能，slowpath 语义全覆盖）；IDL 单源生成 + CI 布局断言（时效性提示见 §0.4）；PI 冻结 boost 长跑永挂（QNX 同语义，v1 接受）；SLEEPQ 单队列 O(n)/哨兵误醒规模化（显式取舍，SMP 窗口再收）；asm 模板换轨收尾（good/spawner/waiter/pinger/ponger 仍双产源并存，P5 前宜清完）；berry-libc 桩面按需扩（D4 路线②③封存非欠债）。

## 4. 实证修正（对 CLAUDE.md 4-6 A 半注记的改写建议）

原文「waiter/pinger boot 后 ~10s 各自超时自陷死 + s/C/P 早停（29cac7a 基线同现，归 user 模板侧）」经两轮 QEMU 实证 + 双对抗验证，修正为：

1. **waiter 死因不是 10s 超时**：waker2 一次性无确认 signal(U3=1<<35) 与 waiter 路 1 的 100ms 窗口竞速——今日 boot 确定性死于 ~1s 的「意外命中」相位（dump x00=0、x03==x05+100ms 恰路 1 deadline 算术、x02=U3）；文档记载的 ~10s -ETIMEDOUT 是同一竞速的另一相位（signal 早到被 UMASK 清位抹掉 → 路 3 十秒空等）。两相位 dump 可判（x0=0 vs x0=-110）；waker2 是共享 BOOTSTRAP_HT 上的裸 spawn 内核任务，被 PI v1 共享表粒度错捐 eff 32 → 冻结随行 + 严格优先使其确定性抢在 waiter 前面（放大器，非本因）。根因 = 模板把「waker2 最早 3 slice 后才置位」的调度时序当契约。对抗验证双双 confirmed。
2. **pinger 死是下游症状**：上游是 abuser/spawner/child/ponger **四进程** ~5-6s（批量退出级联窗口：ai_demo/ai_probe/it_*/sleeper/waker exit + CHAIN/SVC/DEMO 标记簇）同时静默——零 dump 零超时、两次 boot 字符数逐位相同（非竞速），唯一自洽解释为「已就绪却不再被调度」（runqueue 成员丢失/结构性饿死，**机制未定位**——调度器枚举路径全审计无缺陷）。pinger 队列 10s 不腾位 → -110 自陷（16.16s 实测）；s/C/P 早停同根。「console 饱和自旋饿死」假设（H2）被双验证 refuted。
3. **新发现真 bug（非本案元凶）**：`进程调度/src/runqueue.rs:70` unlink 的 `prev == next` 单元素判定在两元环 {X,Y} 上同样为真 → 误走空环分支 head=None，幸存者带 stale 链静默失联（症状同型）；现矩阵常驻环 ≥6 打不中该分支，未来矩阵收缩即踩中——宜立即一行修（`prev==task && next==task`）。
4. **金丝雀口径两说（如实并陈）**：死后切换率 6.1/s→39.7/s 跳变——验证A 指金丝雀只计 tick 抢占（sched.rs:896），39.7/s 超出 TCG ~5Hz 节奏上限、任何假设都无法解释，重推导前不能裁决任何一方；验证B 解读为 ≈幸存者 block/wake 往返、佐证环变小。取证明须先核实口径。
5. **观察基建注记**：`工具链/smoke.sh:108` rootfs 相前 `: > "$LOG"` 截断裸相日志——裸相现场须自起 QEMU 独立日志补捕（实证已示范）；可顺手改分相留档。

## 5. 收官判定

- **判定：Phase 4 出口标准达成，可宣称收官**——以 §0 三条限定 + 时效性提示为话术边界；「挂账：waiter/pinger」保留为收官后第一笔清偿项（P5-0 前置）。
- 建议落盘动作：①本报告入 `文档/`；②CLAUDE.md 4-6 A 半注记按 §4 改写、阶段沿革「挂账」行链接 fix_plan；③执行 fix_plan（P5-0）；④P5 入口按姊妹篇启动（A-1 内核 RX）；⑤下次 ABI 变更前先补 IDL/CI 布局断言（§0.4）。

---

## 第二部分：P5 入口建议（shell 线：bsh）

- 日期：2026-10-16 ｜ 前提：Phase 4 收官判定成立（见收官报告）
- **结论先行：选 A（文档正路两刀：内核 RX 中断 → bsh 作为 rootfs 程序），B（退出路径）作 P5 v0.5 紧随其后、不并行开（单线主线纪律）；C（svcmgr 内嵌命令循环）只配当 A-1 的过渡演示形态，不作正式入口；轮询 RX 只配 bring-up 脚手架（常驻定时唤醒违反 tickless/省电纪律，不成为形态）。**
- 前置小步（P5-0）：waiter/pinger 挂账修复 + runqueue 两元环一行修（另见 fix_plan）——实证显示死点在 boot 后 1~16s 即现，交互进程会放大 10s 帽家族与调度失联面，宜先清场再起 shell。
- 注记：路线图 P5 里程碑是双命题「UART 上交互式 shell；PM 服务据负载改频」——本建议只覆盖 shell 线，PM 服务（调度器 M3）是并行另一线，入口对表时勿混。

## 1. Blocker 对表

| 编号 | 级别 | 内容 | 现状/地基 |
|---|---|---|---|
| B1 | **硬 blocker（第一刀本体）** | UART RX 物理输入路径不存在：HAL 纯 TX（无 FR 检查/无 getc）；GIC 只使能 IRQ27、无 SPI 面（ISENABLER1/ICFGR2/ITARGETSR 要新开）；exception_irq 单源假设（trap.rs 3b3 审查 L8 注释）须按 id 分流——PL011 RX 在 QEMU virt 为 SPI 33（以实测为准）；console 内核任务只有 TX 排水面 | 用户态 stdin 消费面**全就位**（berry-libc read：fd0→env 块 stdin 端 recv + 零长 EOF + PEND 残段缓冲）——缺的只是「物理字节→channel」的 feeder；console 的 WAIT_ANY+哑对照 PI 纪律要求 RX 唤醒走 wait_many 第三项而非改回 wait_one |
| B2 | 真用前必须（v0 可绕） | 进程无退出路径（0x33/0x35/0x36 未注册）——shell 每跑一条命令泄漏一个进程（~300KB 级镜像 VMO+页表+栈+TCB）；berry-libc exit/_exit/abort 全部 park() 永驻 | v0 用 EOF 约定绕过（B3）；v0.5 必补；连带 ktest B 段样本 0x33→0x35/0x36 换号 |
| B3 | v0 可绕 | 子进程完成通知无内核对象（signal_state 只收 Event/Channel，Process 不可 wait、无 Job） | v0 用 d-3c EOF 约定替代：子 stdout 交 shell 私有通道对端、零长消息=完成（verify_hexyl 现成模式，berry-libc park 前自动补发 EOF）；真 wait（Process EXITED 位）随 0x33 一并落 |
| B4 | 软 | argv/env 面窄（env 块 64B / libc argv 名区 256B·16 槽） | `run bin/hello` 够用；管道/多参数/长命令行时参数化放宽（工程小） |
| B5 | 体验面 | 交互输出与稳态矩阵混流（全系统共享单 console TX；>192B 分片插花） | v0 接受混流或 shell 私有 relay（hexyl 同款）；smoke 无碍（rootfs 相独立标记集） |
| B6 | 纯软件 | mini-readline 不存在 | 回显/退格起步，历史/补全随后（shell设计 §2.4 P5 第一刀范围），无外部依赖 |
| B7 | 测试基建 | smoke 交互相不存在（QEMU `< /dev/null`） | 脚本化 stdin 注入（输入文件经 -nographic 进串口）+ 无竞速标记设计（「标记不得建立在竞速上」纪律：确定性收到预发命令、验完子输出才打单消息原子标记——MANY ok/HEXYL ok 加固先例） |

## 2. 入口选项

| 选项 | 内容 | 依赖 | 工作量 | 首个 QEMU 里程碑 | 评语 |
|---|---|---|---|---|---|
| **A（推荐）** | 两刀：①内核 RX 中断（P5 第一刀）；②bsh 按 stdhello/hexyl 同链进 rootfs（musl+berry-libc+berry-ld），prompt+行读+内建+`run <name>`，子 stdout 私有对端+EOF=完成 | 无硬依赖：d-3a env 块/d-3b stdin+EOF/G-eco-c·d rootfs 流式装载/4-5a console 服务循环四块地基全就位，RX 是唯一缺的物理层；EOF 当完成信号 v0 够用 | 第一刀 ~2-4 工作日；第二刀 ~2-4 工作日 | 第一刀：交互窗口打字回显 + smoke 交互相断言 `RX ok`；第二刀：`berry> run bin/hello` 出 `rootfs hello` + rootfs 相新标记 `BSH ok` | 与三份文档对表零冲突——开发体验 §3「串口双向打通是 P5 第一刀」、shell设计 §5 阶段表同序，A 是既定裁决的执行而非新决策 |
| B（v0.5 紧随） | 退出路径先行：0x33/0x36 任务真退出 + Process EXITED 可 wait 位 + ktest B 段换号 | 撞「v1 任务不退出」既定假设网：watchdog 错杀断言基线、boot 手搭进程 Box::leak 政策、process_start 永久 retain、Drop 链不回表——都要重对 | ~1-2 周 | shell spawn 子 → 子真 exit → shell wait 收 EXITED → `EXIT ok` 标记 | 解决 B2+B3 根因；是「真用」前必须但不是「能玩」的前提——排 A 后不阻塞交互正反馈，排 A 前则多等 1-2 周且交互仍不可用；单独提前也不解决输入问题 |
| C（不作正式入口） | 最快演示：svcmgr 内嵌命令循环（bsh-in-svcmgr）——RX 内核侧照 A 第一刀；svcmgr 已持 rootfs VMO+console 端，直接 recv 命令→复用 spawn_rootfs_elf | 与 A 共用第一刀（RX 绕不开） | A-1 基础上 ~1 工作日 | 同 A 第二刀形态（`BSH ok`），但 shell 住在 svcmgr 里 | 只在想要「最小 PR 先验 RX 数据面」时作 A-1 过渡形态：svcmgr 职责膨胀 + shell 后续演进要搬家 = 弯路；shell 不上 rootfs，shell设计 §5 v0 验收形态打折 |

## 3. 推荐理由（A 两刀走、B 紧随）

1. **文档正路**：开发体验.md §3 与 shell设计.md §5 阶段表同序（第一刀 RX+readline → v0 bsh+run）——A 是执行既定裁决。
2. **地基就位度极高**：四块地基（env 块 / stdin+EOF / rootfs 流式装载——hexyl 实证任意 musl std ELF / console 服务循环）全收官，RX 是唯一缺的物理层；「EOF 当完成」有 verify_hexyl 现成模式，不必等退出路径。
3. **正反馈最快**：第一刀落串口双向即成（开发体验 §3 场景前半），第二刀落 `berry> run` 成真——小步提交、每刀独立 QEMU 里程碑的项目纪律正合。
4. **B 的定位**：退出链是「真用」前必须但不是「能玩」的前提；B 单独提前不解决输入问题，故排 A 后作 v0.5，不并行开（单线主线纪律）。

## 4. 首步拆解草案（每步可 QEMU 验证）

### P5-0 前置小步（清场）
1. fix_plan 两轨落地（waiter 位拆分 + runqueue 两元环一行修）→ `just smoke` 双相零漂移 + 150s 裸跑死者仅 rogue_ptr。

### A-1 内核 RX（第一刀，~2-4 工作日）
1. **A-1a HAL RX 读面（bring-up 脚手架）**：uart_getc（FR RX FIFO 非空检查 + 读 DR）；临时轮询 trace 打点（仅脚手架，不进正式形态）。验证：`-nographic` 手敲字节见 `RX raw` trace 行。
2. **A-1b GIC SPI 面**：SPI 33（PL011 RX，实施以实测为准）使能原语（ISENABLER1/ICFGR2/ITARGETSR）+ exception_irq 按 id 分流（撤 trap.rs 单源假设注释）。验证：开中断后注入一字节 → 分流路径打 `RX irq` 行。
3. **A-1c IRQ 侧静态环形缓冲 + Event 置位**（零分配零切换纪律，对齐遥测环先例）。验证：连注多字节、缓冲计数断言不丢。
4. **A-1d console 任务扩 RX**：wait_many 扩第三项 RX Event（保持 WAIT_ANY 哨兵 + 哑 Event 对照的 PI 纪律，**勿改回 wait_one**）→ 任务语境排水 ring → channel_send 喂 stdin 端（boot 建 stdin 对、读端随 env 块交付消费者）。验证：注入 `abc\n` 经 channel 到达（临时 stdhello 变体读 fd0 回显）。
5. **A-1e smoke 交互相**：stdin 注入文件支持 + 新标记 `RX ok`（确定性收到预发字节才打——无竞速纪律）。验证：`just smoke` 裸相+rootfs 相+交互相全绿。

### A-2 bsh 进 rootfs（第二刀，~2-4 工作日）
1. **A-2a 骨架**：`用户态/bins/bsh`（stdhello/hexyl 同链进 rootfs.cpio）prompt 循环。验证：run-rootfs 后 bsh 被拉起打 banner+prompt。
2. **A-2b 行读**：mini-readline 起步（回显/退格；历史/补全随后）。验证：交互窗口打字回显 + 注入脚本断言回显字节。
3. **A-2c argv 切分 + 内建**：echo/version/exit（exit v0=park 收尾，真退出随 B）。验证：注入 `echo hi` → 输出 `hi`。
4. **A-2d `run <name>`**：berry-loader cpio 步进找 `bin/<name>` + spawn 编排（从 svcmgr 抽共享小 crate berry-spawn 或首版内联）+ 子 stdout=shell 私有对端 + 零长 EOF=完成（verify_hexyl 模式）+ 中继 console；argv/env 上限参数化（B4）。验证：`run bin/hello` → `rootfs hello`。
5. **A-2e 标记 `BSH ok`**：rootfs 相 +1——shell 收到预发命令、验完子输出后单消息原子打印（MANY ok/HEXYL ok 加固先例）。验证：`just smoke` rootfs 相 8 标记。

### B v0.5 退出路径（~1-2 周，A 收后启动）
1. 0x33/0x36 任务真退出（出 runqueue、Process/Thread Drop 链、per-process 表回收、VMO/Vmar 解除、ASID 回收）——重对四约定（watchdog 错杀基线 / boot Box::leak 政策 / process_start retain / Drop 不回表）。验证：ktest + 临时用户程序真退出无 panic、环尺寸收缩断言。
2. signal_state 扩 Process EXITED 内核派生位 → `wait_one(child)` 真等待（替 EOF 约定的正式面）。验证：shell spawn 子 → 子真 exit → shell 收 EXITED → `EXIT ok` 标记。
3. ktest B 段「未实现真号」样本 0x33 → 0x35/0x36（CLAUDE.md gotcha 在册）。
4. berry-libc exit 族接 0x33（替 park；park 保留兼容或迁移后退役）。

## 5. 不建议的路径

- **C 正式化**：svcmgr 职责膨胀 + shell 演进要搬家；shell 不上 rootfs 使 shell设计 §5 v0 验收打折。
- **常驻轮询 RX**：不动 GIC、console 短超时轮询 FR——常驻定时唤醒正面违反 tickless/省电纪律，只配 A-1a 的 bring-up 脚手架。
- **先 B 后 A**：多等 1-2 周且交互仍不可用（RX 绕不开）。

---

## 附录：waiter/pinger 前存挂账修复方案

- 日期：2026-10-16 ｜ 分支：p4-svc ｜ 基线：e95c6a0
- 输入：代码分析（模板/内核事实）+ 实证（smoke/150s 裸跑两份日志）+ 验证A（语义）/ 验证B（时序）对抗验证；行号已对工作树落点核对（user.rs / main.rs / runqueue.rs / smoke.sh）
- **结论先行**：waiter 根因已双验证闭合（H1 竞速），修复方案成熟——首选「位拆分 U4=1<<38」（含验证A 扒出的三个必补洞），备选「条件驱动握手」（零模板面）；pinger 及 s/C/P 早停根因**未定位**（H2 双双被驳，静默四进程指向未定位的 runqueue 失联），本方案给出「先落两元环一行修 + 遥测取证复跑」的调查轨，**不宣称已修**。两轨独立提交、独立验证。

## 1. 根因结论（按对抗验证存活度排序）

| 排序 | 假设 | 验证A | 验证B | 存活结论 |
|---|---|---|---|---|
| 1 | **H1 waiter×waker2 握手纯时序竞速**：waker2 一次性无确认 signal U3(1<<35)，可能落在 waiter 三段前置任何窗口；路 2/3 的 UMASK 全清把早到信号抹掉；PI 共享表错捐（endS 入共享表 + waker2 裸 spawn）把 waker2 抬 eff 32 使相位确定性提前 | confirmed | confirmed | **唯一幸存，链条闭合**。dump 三重定位：x03==x05+100ms（路 1 deadline 算术，路 2 是 poll、路 3 ≥10s，唯路 1 成立）＋x02=U3＋x00=0（wait_one 命中返回而非 -110）→ 路 1 断言 `cmn x0,#110; b.ne 3f` 自陷。模板路 3 注释（user.rs:615-616「waker2 最早 3 slice 后才置」）自证把调度时序当契约；waker2 fire-and-forget（main.rs:886-917，signal 后无条件打 WAIT ok 退出）；「W WAIT ok」在日志 113 行先于 waiter 死亡（157 行）旁证 signal 先落 |
| 2 | **H2 pinger 死于 ponger 'P' console EBUSY 自旋饥饿**（深 8 持续饱和 + 环位赢家通吃） | refuted | refuted | 淘汰。四路反驳：good 同通道成功 897 次；三个静默者（abuser/spawner/child）零阻塞调用，被调度必打印或自陷出 dump，两者皆无＝未被调度；ponger 130s 连一次 10s 超时 dump 都没有；两 boot 字符数逐位相同（S=8/s=16/C=8/P=7）非竞速可复现。**可 salvage 子命题**：pinger 死在 ponger 停止 recv 的下游为真（dump x02=WRITABLE、x03==x05+1e10ns 恰 10s 帽）；console 深 8 + 无界自旋的饱和基础设施为真（CHAIN ok 实测 2s→6.4s 漂移＝现存 flake 面） |
| 3 | **H3 内核侧调度/唤醒缺陷**（丢唤醒/SLEEPQ 序/PI 语义） | uncertain | uncertain | 窄面排除成立（block/wake/expire/fast_wake/try_preempt/yield/exit 全在 IRQ 屏蔽窗内单核串行，全部环写点清单化核对，无枚举内缺陷）；宽面排除不成立——四进程「已就绪却永不再被调度」要求存在**未定位**的环成员丢失/结构性饿死机制，时点与 ai_demo/ai_probe/it_*/sleeper/waker 批量 exit 及 CHAIN/SVC/DEMO 标记簇重合。两验证各自附带新发现（见下） |

**如实标注两说（未决，不作裁决依据）**：
1. **化妆打印有界重试**：验证A——值得做，定位为 flake 防卫（CHAIN ok 漂移是真实威胁）；验证B——不建议为此事件动机采纳（与本案无因果，且丢弃打印会掩蔽未来 console 饥饿的诊断信号）。若采纳：仅限非标记打印（'P'/'s'/'C'），**标记打印（CHAIN/MANY/WAIT/DEMO/SVC ok）的重试形态是契约、绝不动**。
2. **金丝雀切换率 6.1/s→39.7/s 跳变**：验证A——金丝雀只在 tick 抢占分支递增（sched.rs:896），不计 yield/block/exit/直切，39.7/s 超出 TCG ~5Hz tick 节奏上限、任何假设都无法解释，重推导前不能裁决任何一方；验证B——40/s ≈ 幸存 d/v/u 生产者 block/wake 往返，佐证「环确实变小了」。实施轨 2 时先核实金丝雀口径。

**双验证一致的新发现（真 bug、非本案元凶）**：`进程调度/src/runqueue.rs:70` unlink 的单元素判定 `if prev == next` 在**两元环 {X,Y}** 上同样为真（X.rq_prev==X.rq_next==Y）→ 误走空环分支 head=None，幸存者 Y 带 stale 链静默失联——症状形状与本案同型（无 dump、无 panic、选择性失联）；但现矩阵常驻环 ≥6，该分支打不中。未来矩阵收缩常驻面即踩中，宜立即一行修。

## 2. 最小修改点（文件/行级，行号已核对）

### 轨 1：waiter 位拆分（首选，H1 confirmed 的对应修）

原理：握手信号改用**专用位 U4=1<<38**（全仓普查空闲：现役 1<<32 哑 Event / 1<<33 SLEEP_OBJ / 1<<34 ktest·AI_EV_DONE / 1<<35 waiter U3·pi E / 1<<36 E_rel / 1<<37 waiter U2——实施时按「新增用户位前先查碰撞」纪律复查），waiter 三处清位掩码豁免 U4——信号无论早/中/晚到都穿越 waiter 自己的清位：早到则路 3 poll-first 即中、晚到则睡中被唤；路 1 的 -110 断言不再可能被握手信号误触（U4 ∉ U3 请求，未命中 observed 不回写）。验证A 扒出的三个必补洞全部纳入：

| # | 位置 | 现状 | 改法 |
|---|---|---|---|
| 1 | 内核/src/user.rs:564（路 1 防御清位） | `movz x2, #0xFFFF, lsl #32`（+565 movk #0xFFFF lsl#48 = UMASK） | `#0xFFFF` → `#0xFFBF`（UMASK^U4；movk 行不动）——防早到 U4 被路 1 开局清位抹掉致路 3 十秒空等 |
| 2 | user.rs:598（路 2 清位） | 同上 | 同 → `#0xFFBF` |
| 3 | user.rs:617（路 3 清位） | 同上 | 同 → `#0xFFBF` |
| 4 | user.rs:610-612（路 2 observed 断言） | `ldr x8,[x4]` / `movz x9,#0x20,lsl#32` / `cmp x8,x9` | **变体二（推荐）**：在 610 后插一条 `bfc x8, #38, #1`（掩掉 U4 再比，期望常量保持 #0x20）——observed 回写的是全满足集 current，早到 U4 会让 observed=U2\|U4≠U2 炸断言；变体二对信号任意到达相位稳健。**变体一（最小 diff）**：期望常量 `#0x20`→`#0x60`（U2\|U4），零长度变化，但隐含「信号早于路 2 读 observed」的残余时序假设——waker2 若被延迟（如摘除 PI 错捐后相位后移）即翻车，且**与第 7 条去放大项组合时主动危险**（相位后移直接触发路 2 断言）。本 bug 教训即「模板不烤时序假设」，故推荐变体二 |
| 5 | user.rs:634（路 3 请求位） | `movz x2, #0x8, lsl #32`（U3） | → `#0x40, lsl #32`（U4=1<<38） |
| 6 | user.rs:639（路 3 期望常量） | `movz x9, #0x8, lsl #32` | → `#0x40, lsl #32` |
| 7 | 内核/src/main.rs:886-917（waker2，signal 断言 ~907） | signal 置位 1<<35（U3） | → 1<<38（U4），断言文案 U3→U4；waker2 的 3-slice 前置睡眠可保留（位拆分后任何相位都过） |
| 8 | user.rs:615-616 注释 | 「waker2 最早 3 slice 后才置——此刻必无残留位」（错误时序假设自证） | 改写为位拆分契约说明（握手专用位 + 清位豁免 + 任意调度序成立） |

约束：全部 movz/bfc 立即数级——变体二模板 +4B（`.balign 16` 起始对齐与 4 字节倍数断言仍过；waiter 无 `adr` 内嵌数据、数值标号分支自动重定位；waiter 无外部硬编长度依赖——spawner 子模板长度 gotcha 不适用）；boot handle 定值断言（waiter Event 0x0001_0001 / console 0x0001_0002，表序）零变化；路 1 请求位（580 行 U3）与路 2 请求超集（605 行 0x220<<32）**不动**；ktest I 段用独立对象上的 1<<33-35，零纠缠；WAIT ok 链（waker2 signal 成功即打）不变；PI 面——Event 用户位等待无 donor_edge（pi.rs 对 Event donor 返 None），零影响。

**备选方案（轨 1 不动模板）**：条件驱动握手（ai_probe 模式）——waker2 在 IRQ 屏蔽窗内轮询 `for_each_sleeper`（契约：屏蔽窗内调用），直到 waiter 真睡在 E 上（wait_obj 匹配 Event ObjRef + wait_signals 含 U3）才 signal，带 ≤64 步重试上限、超限 loud（panic=标记缺失兜底）。零模板面、回归面更小；代价是 main.rs 引入 sched 内部依赖、复杂度中等。两案都比维持现状严格占优，实施会话任选其一即可。

**可选去放大项（非修复，勿单独当修复）**：waker2 换 `spawn_with_tag` 挂 leak 私有表（Event 改插私表，对齐 4-5/4-6「长命睡眠内核任务不睡共享表」纪律）——只改相位分布、不除根因；且**与变体一（#0x60）组合时主动危险**（见上）。

### 轨 2：pinger / s·C·P 早停（根因未定位——调查轨，非修复宣称）

1. **立即落地（独立提交）**：`进程调度/src/runqueue.rs:70` 一行修——`if prev == next` → `if prev == task && next == task`（两元环 unlink 误走空环分支的潜伏 bug；现矩阵 ≥6 常驻成员零行为扰动）＋ ktest 新用例：两元环 unlink X 后 Y 自环且 head 正确、再 unlink Y 归空。
2. **取证遥测**：unlink/exit/link_back 只读计数 + 环尺寸快照原语（零分配零切换纪律，对齐遥测环先例）→ 150s 复跑比对环尺寸轨迹，定位四进程（abuser/spawner/child/ponger）失联时刻的环操作序列。
3. **候选排查方向（按验证建议序）**：批量退出级联窗口（ai_demo/ai_probe/it_*/sleeper/waker exit + CHAIN/SVC/DEMO 簇，~5-6s）的 tick/SVC 交错重放；EL0 异常入口 SP_EL1 不变量（「EL0 运行期间 SP_EL1 留本任务内核栈位」被破坏则该任务帧丢、永不可切回且无 dump——症状形状吻合）；金丝雀口径核实（见 §1 两说 2）。退出路径批（P5-B v0.5 落 0x33 时）天然重对 exit 环操作，可与本轨合流。
4. **勿入的弯路**：全局改 channel 深度 8 / 调度器结构重构——H2 方向已被双验证否定，回归面牵动 ktest H 段深度断言与 PI/INTENT/FAST 全套 boot 验收时序，远大于本案。

### 争议项（两说，若采纳须单独提交便于回退）

化妆打印（'P'/'s'/'C'）有界重试（如 64 次 -EBUSY 后放弃本圈、直接回服务循环——保住「ponger 必须持续推进 recv」的负载关键不变量；同族先例：spawner 稳态 send 本就容 -EBUSY 跳过）。采纳与否见 §1 两说 1；无论采纳与否，**smoke 标记打印的重试形态（自旋/阻塞）是契约、绝不可一并改**。

## 3. 验证计划

提交顺序（小步提交纪律）：
1. 提交 1：runqueue 两元环修 + ktest 用例（零扰动，smoke 全绿即合）。
2. 提交 2：waiter 位拆分（变体二）+ waker2 常量/断言/注释。
3. Gate：`just smoke` 双相零漂移（裸相 23 + rootfs 相 7 + 金丝雀严格递增）＋ `just run-el2` 前缀 21 ＋ host 测试（模板为内核侧，host 面不受影响）。
4. 长跑观察（≥150s ×2，自起 QEMU 独立日志——smoke.sh:108 rootfs 相前 `: > $LOG` 截断裸相日志，留存日志只有 rootfs 相；实证已示范自起补捕）：
   - waiter 进入稳态 'W' 持续流动（历史 run 全程 0 个稳态 'W'）；
   - **死者仅 rogue_ptr**（waiter 不再死；pinger 死否随轨 2 进展另计——轨 2 未收前 pinger 预期仍死）；
   - s/C/P 不早停属轨 2 范畴：修 waiter 后复跑观察静默窗口是否复现，为取证遥测提供对照；
   - 金丝雀严格递增；'u'/'S'/'v'/'d' 稳态流动不回归。
5. 提交 3（若采）：化妆打印有界重试，单独提交；复跑确认 CHAIN ok/MANY ok/WAIT ok 时序无漂移恶化。
6. 轨 2 取证循环：遥测 → 复跑 → 定位 → 另立修复提案（不在本方案内宣称）。
7. 文档同步：CLAUDE.md 4-6 A 半注记改写（~10s 超时 → 竞速双相位描述，见收官报告 §4）＋「挂账」行销账／收缩死者基线（rogue_ptr+waiter+pinger → 视轨 2 进度分步收缩）＋ 陷阱节补一条「模板握手信号用专用位、清位豁免——模板不烤调度时序假设」；waiter 后续编译化换轨（G-eco 收尾）时 U4 契约随迁。

## 4. 回归风险清单

| # | 风险 | 缓解 |
|---|---|---|
| 1 | 模板长度/对齐纪律破坏 | 变体二仅 +4B（一条 bfc），4 字节倍数断言仍过；不动 `.balign`/banner/adr 结构；无外部硬编 waiter 长度 |
| 2 | 常量联动不完整（8 处漏一 = 新竞速/新断言炸） | 按§2 表逐项核对；尤其三处清位掩码与路 2 断言、路 3 请求/期望、waker2 置位须同提交同改 |
| 3 | 变体一（#0x60）残余时序假设与去放大项组合危险 | 推荐变体二（bfc 全相位稳健）；若选变体一则禁止同时采纳 waker2 私有表 |
| 4 | 用户位碰撞 | U4=1<<38 全仓普查空闲；实施时复查现役位表（CLAUDE.md 纪律） |
| 5 | boot handle 定值断言漂移 | 不动插表序；waiter Event 0x0001_0001 / console 0x0001_0002 断言保持 |
| 6 | ktest I 段/WAIT ok 链回归 | ktest 用独立对象零纠缠；WAIT ok 打点条件（signal 成功）不变；smoke 第 15 标记须仍恰一次 |
| 7 | waker2 私有表（若采）spawn_with_tag 契约 | tag 断言非 0；Event 改插私表后共享表 handle 路径同步清理；定位为去放大、非修复 |
| 8 | 两元环修改变行为 | 现矩阵常驻环 ≥6 不触发该分支＝零扰动；ktest 用例锁语义 |
| 9 | 化妆打印有界重试的两说风险 | 单独提交可回退；不掩蔽标记；接受「打印变稀」不影响活性；采纳前记录契约修订理由（asm 原形为无限自旋） |
| 10 | 超范围改动（channel 深度/调度器结构） | 令禁止（§2 轨 2.4）；H2 已否定该方向 |
| 11 | 死者基线/文档失同步 | 修后 watchdog 早退不致 smoke FAIL（实证：watchdog 早退、晚死无人断言），但 CLAUDE.md/smoke 记载的基线须同步收缩 |
| 12 | pinger 侧过早宣称已修 | 轨 2 是取证计划；长跑观察点承担验收；修复提案另立 |

---

## 执行结果（P5-0 清场，2026-10-16 落地）

按本盘点 §5 建议落盘动作 ①②③ 执行（④⑤ = P5 启动与 ABI 时效性，另行推进）：

- **提交 1 `d4dcf6a`**：runqueue 两元环一行修（`进程调度/src/runqueue.rs` unlink 判据锚定 task 自身）+ ktest `RING2` 段（两元环摘除 X 后幸存 Y 自环可出选、三元素中摘、归空——锁语义）。现矩阵常驻环 ≥6，零行为扰动。
- **提交 2 `a27eb9c`**：waiter 位拆分（变体二 `bfc`）8 处（三清位掩码 #0xFFFF→#0xFFBF 豁免 U4 + 路 2 observed 断言前掩位 + 路 3 请求/期望改 U4 + 头注重写为位拆分契约）+ waker2 signal 置位 U4/断言文案。模板 +1 指令（+4B），boot handle 定值断言零变化。
- **Gate 全过**：`just smoke` 双相零漂移（裸相 23/23 + rootfs 相 7/7 + switches 金丝雀严格递增）＋ `just run-el2` 前缀 21 ＋ host 11+51 passed ＋ 155s 裸跑 ×2。
- **waiter 销账**：稳态 'W'×17（历史 run 全程 0）；`WAIT ok` 第 15 标记仍恰一次。
- **死者基线收缩**：rogue_ptr + pinger（waiter 不再死）；**spawner 存活**（历史 run 亦死）——轨 2 静默失联家族现观察为 abuser/child/ponger（pinger 为下游）。
- **确定性复现基线成立**：两次 155s boot 稳态字符计数逐位相同（u:1234 S:34 s:78 C:24 p:19 P:26 W:17 v:96，唯 'd' 105/104 差 1——稳态化妆打印自然抖动）——非竞速、可复现，**轨 2 取证遥测就绪**（对照基线即此组数字）。
- **争议项「化妆打印有界重试」未采纳**（验证B 立场：与本案无因果、掩蔽 console 饥饿诊断信号）；若日后采纳须独立提交，且 smoke 标记打印重试形态是契约绝不动。
- **文档同步（提交 3，本提交）**：CLAUDE.md 4-6 A 半注记按第一部分 §4 改写 + 下一步行销账改 P5 入口 + 陷阱节补「模板握手信号用专用位、清位豁免」条；AGENTS.md 陷阱节同步；路线图 Phase 4 块补收官状态行；本文件落盘。
- **遗留转入**：轨 2 取证（pinger/abuser/child/ponger 静默失联——根因未定位，不宣称已修）挂账 P5 期间择机；P5 入口按第二部分 Plan A 启动（A-1 内核 RX）。
