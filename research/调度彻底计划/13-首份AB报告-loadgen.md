# 13 号：首份 A/B 数据报告——负载生成器 loadgen（CP-06 首份产出）

> 实测日期：2026-11-16（凌晨窗）。环境：Apple M3 Pro / QEMU 11.0.3 Homebrew TCG / 单核 `virt`（与 06/07/09/12 号同机同版本）；宿主 load ≈13（并行会话多 QEMU 在飞，rep 离散带按此语境读）。任务：CP-06 首份 SMP A/B 报告——「首份 A/B 报告未动」是 06 号综合定谳策略维 5.0 的扣分锚之一（[06 号综合定谳](../调度深度调研/06-综合定谳-v1.0.md):164-165），本件即该缺口的首份产出。载体**双数据面**：①真系统 boot 探针（本刀落码，[kernel/src/main.rs](../../kernel/src/main.rs):4479 探针族——fw_cfg 门控 spawn 三类受控工作负载，真 IRQ/真调度/真睡眠，四臂各独立 boot × 3 reps）；②ktest 机械面（[kernel/src/ktest.rs](../../kernel/src/ktest.rs):8859 `test_loadgen`——单 boot 四组、手驱消费点形态；落码在途件，见 §2.3）。口径纪律：**QEMU/TCG 只报功能、顺序与相对趋势，绝对分位与功耗属真机阶段**（[调度彻底计划](../../docs/调度彻底计划-v1.2.md):20 §1 原则⑦）；本件全部数字为 TCG 趋势口径，不作绝对门禁。配置轴：timer_slack off/on（CP-11 第三键，[调度彻底计划](../../docs/调度彻底计划-v1.2.md):108）× band off/on（intent→band 分派面，HEAD 10297ac22「水通管道」任务①）。方法：四臂同内核同 QEMU 拓扑、唯一变量 = fw_cfg `opt/berry/loadgen` 值（`s=<ns>;b=<0|1>`）；每臂 3 次独立 boot 取中位数；延迟单位 = CNTVCT 周期 @62.5MHz（[tools/bench.py](../../tools/bench.py) FREQ 同源，1 cycle = 16ns）。推断一律显式标注。

---

## §0 结论速览（五条）

1. **slack 轴成立（真系统面，主判据）**：声明 slack 30ms 时 utility 唤醒→运行延迟 p50 从 18.6ms 升至 44.5ms（**+25.9ms**，g4 vs g2 中位数；g4 三 rep 带宽仅 [43.0, 45.5]ms——信号量级 ≈26× 臂内离散），方向与量级兑现 CP-11 LATE-only 顺延律（[sched/src/intent.rs](../../sched/src/intent.rs):89 判定律 → [sched/src/sched.rs](../../sched/src/sched.rs):2567 block_current 入队点消费）；同臂 interactive p50 恒在 13.6-17.2ms 噪声带内**零迁移**（「交互零牺牲」律的数据面兑现）。**无 intent 时 slack 惰性实证**：g3（slack on × band off，sleeper 为 UNSPECIFIED 档）utility p50 16.3ms 与基线 18.5ms 无差——顺延资格经 intent 档（≥UTILITY）门控的结构耦合从经验面坐实（timer_slack_defer 的 UNSPECIFIED 恒等臂，[sched/src/intent.rs](../../sched/src/intent.rs):93-95）。
2. **band 轴真系统面 = 零显著差（诚实 null，非失败）**：g2 vs g1 全指标（ia/ut p50/p99、吞吐）均在 rep 离散带内——这正是现役机制的预期行为：FairBand 五方法全委托 unit Fair（[sched/src/class.rs](../../sched/src/class.rs):465-469「行为逐位一致，M4 并线时成为消费源」），带参数（quantum 10/20ms、权重包络 256..1024/64..512、latency_target——g2 boot 的 `[band] lg_ia0 intent=2 q_ns=10000000 w=256..1024` 行为证）**只落分派面不落行为面**，band 执法不并线是入场券纪律的立法态（[sched/src/band.rs](../../sched/src/band.rs):12-15；[调度器设计](../../docs/调度器设计-v1.2.md):138 入场券四件套）。差异化行为的正控制在 ktest 机械面（手驱 throttle/consume 消费点，§2.3）。
3. **吞吐四臂不变**：burst 过载源 121/124/122/123 units/s（±2%）——单核饱和工况总供给受限，slack（延迟换合并窗）与 band 分派均不动总量；「slack 用 utility 延迟换什么」在单核饱和形态下答案是「什么都不换」（唤醒次数与总工作量恒定）——省电收益属聚合唤醒减少面，本载荷形态测不到，归 §5 局限。
4. **判据分列（CP-06 报告模板纪律执行，[调度彻底计划](../../docs/调度彻底计划-v1.2.md):98）**：本报告性能差列如上；「决策一致率（divergence 率）」列**本载体缺席**——无换表载荷（POLICY-S 相未建，铁门 CP-02），divergence 是决策一致率代理不是收益本体，本件不冒用；能耗列**未测**（真机功耗计到位前原理性不可测，开庭记「未测」不记「无收益」，CP-15 口径，[调度彻底计划](../../docs/调度彻底计划-v1.2.md):117）。
5. **策略维对照（推断，归 owner 拍板）**：5.0 → **6.5**——三条扣分锚中「intent 硬消费零」已破（slack 真消费 + intent→band 端到端分派 + 本件首份消费生效数据）、「kill criteria 零样本开庭」已破（本件 = 首样本，QEMU 口径）、「band 执法全关」与「PM 服务未建」仍真；详见 §6 逐锚对账。

## §1 方法与载体

### 1.1 真系统 boot 探针（本刀落码——数据主面）

- **门控与配置通道**：fw_cfg 具名项 `opt/berry/loadgen`（rt_probe R-1 同款门控形态，[kernel/src/main.rs](../../kernel/src/main.rs):4486；缺席 = run/smoke 零扰动，EFI 路天然缺席）。四臂内核基座 = HEAD `f45b7985b` + 本刀探针增量（隔离工作树构建——主树同期有并行刀在途件，隔离保四臂同源；主树已含同款探针落码随下次整树 smoke 收口）。值语法 `s=<ns>;b=<0|1>`（分号分隔——QEMU `-fw_cfg` 的 string 值以逗号切参）；boot 早窗解析（timer::init 前——`set_slice_hz` 契约，[kernel/src/main.rs](../../kernel/src/main.rs):782 早读块）。slack 经 [`set_timer_slack_ns`](../../sched/src/intent.rs):68 落 sched 全局（与 bootparams 第三键同 sink，四臂同源注入）；slice 档经 G5 官方 setter `set_slice_hz(100)`（与 bootparams `sched_slice_hz` 同一函数，[arch/aarch64/src/timer.rs](../../arch/aarch64/src/timer.rs):335）抬到 10ms 片——缺省 10Hz 的 100ms 唤醒量化窗会吃掉 30ms 级 slack 信号（v1 唤醒粒度恒 1 slice，rt_probe 首跑实录 [kernel/src/main.rs](../../kernel/src/main.rs):4288-4295），四臂同值内部可比。
- **三类受控工作负载（任务书口径「每类 N 任务 × M ms 级 CPU burst」）**：interactive N=2 × 48 轮，40ms 周期 + 3ms burst；utility N=2 × 32 轮，60ms 周期 + 8ms burst；burst N=2 连续 5ms 自旋单元至收工旗（过载源，单核饱和）。b=1 臂 sleeper 经 [`spawn_intent`](../../sched/src/sched.rs):1227 落 intent（现役带 interactive/utility，boot `[band]` 行自证分派与带参数）；b=0 臂 plain spawn（缺省 UNSPECIFIED：无带、无 slack 资格）——两臂同一任务体代码路径（[kernel/src/main.rs](../../kernel/src/main.rs):4568 `task_lg_slp`），差只在 spawn 面，A/B 可归因。
- **测量口径**：每轮 lat = 醒后首读 CNTVCT − 入睡前绝对周期锚 deadline（含 tick 粒度 + 排队 + slack 顺延；rt_probe oversleep 同口径，漏周期再锚同律）；分位 = 最近秩 ceil(p·n/100)（rt_probe 同形）；样本量 ia 96 / ut 64（n<100 时 p99 与 max 重合是样本量诚实上界，勿为观感改次大值）。master prio 8 轮询收割（20ms 块），全部载荷 prio 0（「被延迟本身就是待测的调度延迟」）。
- **串口输出**：`LOADGEN gate on s=.. b=.. slice_hz=..`（门控自证）→ `LG ia/ut min/p50/p99/max/n` + `LG bu units/span_ms/units_per_s` → `LOADGEN ok`（G43 拼行包络）。

### 1.2 判据分列与口径纪律（CP-06 模板）

CP-06 验收线明文「报告模板分列两类判据——决策一致率（divergence 率）与性能差（HDR A/B 尾延迟）各自成列、不得互替」（[调度彻底计划](../../docs/调度彻底计划-v1.2.md):98）。本件三列处置：性能差列 = §2/§3 主体；决策一致率列 = 缺席（无换表载荷；载体归 POLICY-S 相，铁门 CP-02）；能耗列 = 未测（CP-15 预立口径）。本件分位为原始样本直取（非 HDR 桶保守上界口径）——见 §5。

## §2 数据

### 2.1 真系统四组矩阵（3 reps 中位数；µs @62.5MHz；boot 探针）

| 指标 | g1 slack=off band=off | g2 slack=off band=on | g3 slack=30ms band=off | g4 slack=30ms band=on |
|---|---|---|---|---|
| interactive p50 | 17248 | 16676 | 16950 | 13594 |
| interactive p99 | 60362 | 66628 | 56030 | 81205 |
| utility p50 | 18490 | 18596 | 16256 | **44524** |
| utility p99 | 77013 | 66452 | 86541 | **116561** |
| burst units/s | 121 | 124 | 122 | 123 |
| 窗口 span（ms，master 视角） | 2214 | 2168 | 2119 | 2305 |

臂内 3-rep 离散带（中位数语境的噪声底，宿主 load ≈13 下实测）：ut p50 带宽 g1 [16.3,19.6]ms / g2 [17.6,19.1] / g3 [16.1,16.9] / g4 [43.0,45.5]ms；ia p50 各臂带宽 2.5-4.7ms；p99 带宽普遍 2×（ia_p99 全臂 [42.3,103.7]ms）——p99 量级可比、个值不作判据（§5）。

**关键差分**：utility p50 g4−g2 = **+25.9ms**（30ms 声明顺延，净差 25.9——差值 −4.1ms 的解释见 §3.1 推断）；utility p50 g3−g1 = −2.2ms（零迁移）；interactive p50 四臂 13.6-17.2ms（零迁移）；吞吐四臂 ±2%。

### 2.2 证据行摘录（原始串口，全文归 /tmp/lg-logs/×12 + 门控行）

```text
g2 boot: [band] lg_ia0 intent=2 q_ns=10000000 w=256..1024   ← b=1 分派面自证
         [band] lg_ut0 intent=3 q_ns=20000000 w=64..512
g1: LG ia min=9188 p50=980250 p99=4195688 max=4195688 n=96
    LG ut min=98875 p50=1054187 p99=3207312 max=3207312 n=64
    LG bu units=258 span_ms=2200 units_per_s=117
g4: LG ut min=1954563 p50=2884000 p99=9113625 max=9113625 n=64   ← +30ms 顺延在样本里
    LG bu units=317 span_ms=2666 units_per_s=118
```

（`LOADGEN gate on s=.. b=.. slice_hz=100` 行每臂在席；cycles 原值，µs = 周期 × 16/1000。摘录 = 各臂首跑单 rep——§2.1 表为 3-rep 中位数，两者个值差即 rep 离散。）

### 2.3 ktest 机械面（上刀件，落码在途——A1 绿、A2 起红）

上刀/并行刀在 `ktest.rs:8859 test_loadgen` 落 ktest 形态负载生成器：单 boot 四组（A1 基线 / A2 band / A3 slack / A4 双开，slack 10ms 档），真机械面手驱（spawn_intent 真分派 + 本地 Runqueue 真队列机械 + SLEEPQ + timer_slack_defer + oneshot tighten/arm_at + band configure/consume/period_check/throttle_all 手驱消费点——「M4 并线前热路径本无执法点，手驱 = 入场券证据面的可用形态」，[kernel/src/ktest.rs](../../kernel/src/ktest.rs):8815-8817），含 utility 配额越线断言（thr_ut≥1）与 interactive 零牺牲断言（thr_ia==0）。**本报告实测窗内该件仍在调试**（03:25 实跑：A1 组 interactive 样本 20<24 panic 于 :9261〔丢轮 bug，已修〕；03:33 实跑：A1 组绿出数；03:41 实跑：A1 组绿出全字段——`KTEST LOADGEN A1 slack=0ms band=off interactive p50=786 p99=1538 utility p50=522 p99=782 burst=190 wall=88 oneshot=5 thr=0 ranIA=0ms ranUT=40ms ranBK=47ms freq=62500000`（µs 口径：忙等检出粒度，无真 IRQ 延迟分量——与 §2.1 真系统 ms 口径不同类，恰构成两极对照）——但 A2 组仍 panic 于 :9307「utility 配额须越线」（手驱 consume 记账未达越线，调试中）。机械面四组数据补录归 CP-06 窗口续刀；本件数据主面 = §2.1 真系统探针，机械面价值定位见 §3.2。

## §3 分析

### 3.1 slack on/off 对 utility 延迟：成立，机理链全通

机理链：声明 slack（bootparams 第三键或探针通道同 sink）→ `TIMER_SLACK_NS` 全局 → [block_current](../../sched/src/sched.rs):2567 入队点按 `intent_effective` 顺延（UTILITY/BACKGROUND/MAINTENANCE 三档 +slack，ns→tick 截尾——30ms@62.5MHz = 1,875,000 周期恰整除无截尾损失）→ SLEEPQ 队头 `wait_deadline` 继承顺延值（下游 arm_at/tighten/expire 三臂零改动）→ 醒在顺延后 deadline 的下一 tick。数据兑现：g4 utility p50 +25.9ms ≈ +30ms 顺延 − 4.1ms。**−4.1ms 归因（推断）**：顺延使 utility 唤醒整体后移 30ms，与 burst 过载源及 interactive 的竞争重叠相位改变，排队等待分量略降（p50 里含排队，非纯定时器粒度）；量级 4ms 与排队分量尺度相符，真机相可用逐样本分解闭合。
「slack 换什么」：本载荷形态（固定唤醒次数 + 饱和单核）下吞吐零变化 = **延迟买了零吞吐**——slack 的设计收益在聚合唤醒减少（合并窗内到期者一 pass 摘尽，[sched/src/sched.rs](../../sched/src/sched.rs):2561-2566 注）与省电（定时器事件减少），前者需「同窗多到期者」载荷形态（多 utility 任务同相位睡眠）、后者属能耗半句（未测，CP-15 口径）——均不在本载荷形态内，如实记（§5）。

### 3.2 band on/off 对 interactive 延迟：真系统 null 是入场券纪律的实证

g2 vs g1 全指标在噪声带内 = 现役机制的正确行为：intent→band 分派面已通水（[band] 行 + spawn_intent 端到端，10297ac22），但 FairBand trait 方法全委托 unit Fair（[sched/src/class.rs](../../sched/src/class.rs):516-519 注「行为零漂移（入场券未过，带差异化行为不实装）」）——带参数此刻只落字段。**这个 null 恰是 CP-12 入场券纪律要求的基线证据形态**：band 并线的 kill 判据 =「带差异化必须打败扁平 EEVDF 零假设」，本 null 把零假设的基线钉在案（同载荷同吞吐下扁平形态 interactive p50 ≈16-17ms——未来 M4 并线后同载荷复跑即得对照）。ktest 机械面的手驱执法（utility 配额越线必限流、interactive 恒不越线）是差异化正控制：证明**机制有能力**差异化、缺的只是热路径接线（入场券）——与本 null 合成「机制就绪、执法未并线」的完整证据对。

### 3.3 吞吐变化：四臂不变（单核饱和供给受限）

burst units/s 121/124/122/123（±2% 在 rep 离散带内）。机理：单核饱和 + 固定唤醒次数 + 固定 burst 总量 → CPU 供给全部被消耗，slack 只移相位、band 分派不动物权；吞吐不变是供给面的必然，也是「延迟差异非以吞吐为代价」的正面证据（slack 的 utility p50 +26ms 无吞吐税）。

### 3.4 观察者效应注记

master prio 8 每 20ms 轮询块唤醒（≈50 次/s 抢占窗）+ 每臂独立 boot 的 ktest/bench 前序——四臂同值同构，A/B 内部可比；绝对值语境（如 ia p50 ≈16ms 高于纯 tick 粒度 ~5ms）含过载竞争与观察者分量，真机相再分解。

## §4 结论：CP-06 首份产出达成 + CP-15 载体就绪度

1. **首份可复现 A/B 数据在册**（06 号 §5 策略维扣分锚「首份 A/B 报告未动」闭合一格；观测维「首份 A/B 零样本」同步闭合，[06 号综合定谳](../调度深度调研/06-综合定谳-v1.0.md):165）。复现配方单源：同内核 + `-fw_cfg name=opt/berry/loadgen,string=s=<ns>;b=<0|1>` + `-fw_cfg name=opt/berry/demo-face,string=off`，3 reps 中位数。
2. **CP-15 AI kill criteria 复核载体就绪**：固定基线、双轴可控、机器可读输出、判据分列纪律全部在位——kill 执法的尾延迟判据从此「有秤可称」；能耗半句仍原理性不可测（记未测，CP-15 预立口径）。
3. **CP-12 入场券证据面基建落位**：零假设基线（本 null）+ 手驱正控制（ktest 面）+ 过载尾延迟差量具（本探针）三件中两件实测在案；「饿死事件计数/空闲核金丝雀」两件随真机/SMP 相补（[调度器设计](../../docs/调度器设计-v1.2.md):138 入场券四件套）。

## §5 局限（如实）

1. **TCG/单核/单机**：绝对分位不进门禁（计划 §1 原则⑦）；宿主 load ≈13 抬高 p99 离散（个别臂 p99 range 2×）；p50 中位数语境信号量级 26× 带宽，判据稳。
2. **样本量**：ia 96 / ut 64——p99 = 最近秩 ≈ max（诚实上界）；尾延迟判别力归后续加轮（探针常量即可抬）或真机 HDR 相。
3. **非 HDR 桶口径**：原始样本直取分位，非 HDR 897 桶保守上界口径（观测面判官工具族吃 HDR 行）；探针样本进 record_wake_lat 遥测环的收割面未接（telemetry_open 0x84 消费链在并行刀在途）。
4. **slack 收益半句缺测**：聚合唤醒减少（合并窗收益）需同相位多 sleeper 载荷形态；省电属能耗半句（未测，CP-15）。
5. **ktest 机械面数据未入**（§2.3 在途红）；band 差异化的正控制数据暂缺，本件 band 轴只有真系统 null 一面。
6. **单核形态**：SMP 面（按核分 c0/c1、divergence 决策一致率列）未开——CP-06 模板的 SMP A/B 形态裁决仍留窗（-smp 2 相、偷取/SGI 干扰面）。

## §6 策略维对照：5.0 → 6.5（推断，归 owner 拍板）

06 号 §5 策略维 5.0 的判词锚逐条对账（[06 号综合定谳](../调度深度调研/06-综合定谳-v1.0.md):164）：

| 06 号扣分锚 | 当时态 | 本刀后态 | 分数向 |
|---|---|---|---|
| 「intent 硬消费零」 | 九环四通三骨架两未接（03 号 §1.1） | slack→定时器真消费（CP-11 落 + 本件消费生效数据）；intent→band 分派端到端（10297ac22）；环 6 timer slack 通、环 7 带实例升「分派面通」；telemetry_open 0x84 在并行刀在途 | **+1.0** |
| 「kill criteria 零样本开庭」 | 零样本（03 号 §3.4） | 首样本在册（本件，QEMU 趋势口径四组×3 reps 双轴）——开庭有证，唯真机分位与能耗半句仍空 | **+0.5** |
| 「band 执法全关」 | 入场券未过立法态 | 仍真（真系统 null 实证行为零差异——纪律的代价面非失败态，06 号 §5 第三句原判词维持） | 0 |
| 「PM 服务未建/berry-power 空转」 | 未建 | 仍真（CP-08/CP-10 未动） | 0 |

合计 5.0 → **6.5（推断）**。口径注：若 CP-15 复核后永久 hint-only 关账，「策略维会以查表键即终态的口径重新及格」（06 号 §5 第三句）——6.5 是「管道有水、承重未开」的中途态读数，两种终局（PROMOTE 或关账）均不改本件数据的证据地位。

## §7 引用锚（路径:行号，2026-11-16 实核）

- 本刀落码：[kernel/src/main.rs](../../kernel/src/main.rs):782（早读块）/1872（门控 spawn 块）/4011（用户位登记 1<<52）/4479-4745（探针族：常量、配置、task_lg_slp/task_lg_bu/task_lg_master）
- 机制链：[sched/src/intent.rs](../../sched/src/intent.rs):64-103（TIMER_SLACK_NS + timer_slack_defer 判定律）；[sched/src/sched.rs](../../sched/src/sched.rs):2549-2572（block_current 入队点顺延）；[sched/src/sched.rs](../../sched/src/sched.rs):1227-1254（spawn_intent + [band] 行）；[sched/src/class.rs](../../sched/src/class.rs):465-519（FairBand 全委托立法）；[sched/src/band.rs](../../sched/src/band.rs):12-15（默认全关/入场券注）
- 计划与模板：[docs/调度彻底计划-v1.2.md](../../docs/调度彻底计划-v1.2.md):98（CP-06 验收线与两类判据分列）/108（CP-11）/109（CP-12 入场券）/117（CP-15 能耗半句）/20（§1 原则⑦ QEMU 口径）
- 评分基座：[research/调度深度调研/06-综合定谳-v1.0.md](../调度深度调研/06-综合定谳-v1.0.md):157-169（§5 五维与终局判词）；[research/调度深度调研/03-AI原生成熟度-v1.0.md](../调度深度调研/03-AI原生成熟度-v1.0.md):29-41（§1.1 九环总表）
- 在途件：[kernel/src/ktest.rs](../../kernel/src/ktest.rs):8795-9381（test_loadgen，03:34 实跑 panic 于 :9261——调试中）
- 原始数据：/tmp/lg-logs/g{1..4}-r{1..3}.log（12 boot 串口全文）+ /tmp/lg-logs/ktest-loadgen.log
