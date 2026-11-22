# AB-01:CP-06 首份 A/B 数据——ktest 负载生成器与 timer_slack × band 四组对比

Type: survey
Status: current
Updated: 2026-11-16
版本:1.0

> 定位:CP-06(A/B 裁决)窗口的首份数据产出——06 号综合定谳 §1.7-③ 判词「首份真实 A/B 报告(CP-06)未动」的证据面清偿第一刀。载体 = kernel ktest 形态负载生成器(`test_loadgen`,kernel/src/ktest.rs:8861)+ 真机械手驱驱动器,产出 timer_slack(0/10ms)× band(off/on)四组 A/B 的唤醒→首切延迟(p50/p99)与 burst 吞吐。本件只收数据与判读,机制面细节引用给代码行号,不双写。
> 方法:ktest 串口行(`KTEST LOADGEN *`)两跑全量收割(QEMU virt 单核 TCG,cortex-a72,CNTFRQ=62.5MHz 实测);负载 = 三类工作负载各 4 任务(interactive:burst 100µs+睡 10ms/utility:burst 10ms+睡 1ms/burst:CPU 100% 恒跑),每任务 6 轮,每轮唤醒→首切延迟记一笔(lat = 首切 CNTVCT − 请求唤醒时刻〔未顺延 deadline〕,每型每组 20 样本,nearest-rank 分位);驱动器把 spawn/SLEEPQ/EEVDF pick/tick 记账/band 记账/oneshot 编程全走现役真 API,ktest 手驱消费(见 §1)。
> 证据分级:【证】= 本轮 QEMU 实跑串口输出(out/logs/loadgen-ab-*.log 两跑)与一手行号;【界】= ktest 忙等口径的工具边界(见 §4);推断标「推断」。数字全部两跑在册(跑 1/跑 2),单跑现象显式标注。
> 纪律:本刀只动 kernel/src/ktest.rs(测试件);研究只读。boot 基线 `wip/sched-1.1` 工作树(与 06 号同线);dpg_press 串口红 = 并行会话 dpg.rs 在途件(其自身日志 dmesg-e2e2-1794771471.log 同红且零 LOADGEN 行——与本刀无关,实证归属)。

---

## 0. 一句话结论

**四组 A/B 首次拿到可复现数据,两条机制主线都兑现了设计语义:CP-11 timer_slack 对 utility 的 +10ms 顺延精确落地(A3/A4 utility p50 ≈ 10.6-10.8ms vs A1/A2 ≈ 0.75ms)且交互零牺牲(interactive p50 四组全在 0.35-1.06ms);band 限流把 utility 尾延迟放大 3.2×(p99 ≈ 36-38ms vs 基线 1.3-11.5ms)同时把让出的 CPU 全部喂给 burst(吞吐 +75%)——「差异化调度」从机制面第一次变成了可 grep 的数字。**

### 0.1 与真系统面的分工(双数据面,勿混读)

CP-06 首份产出是**双数据面**:本件 = ktest 手驱机械面(单 boot 四组、忙等 µs 口径);同窗另一刀 = 真系统 boot 探针(fw_cfg `opt/berry/loadgen`,真 IRQ/真调度/真睡眠,四臂 ×3 boot,30ms slack 档)——见 [13-首份AB报告-loadgen](../调度彻底计划/13-首份AB报告-loadgen.md)(其 §2.3 记录了本件的在途调试窗,本件 §2 即其预告的「四组数据补录」)。两面恰构成两极对照:真系统面 **band 轴零显著差**(g2 vs g1 全指标在离散带内——FairBand 五方法全委托 unit Fair、执法不并线是入场券纪律的立法态,13号 §0-2);本手驱面 band 差异化 3.2× 尾延迟 + burst +75% 是**手驱消费点**(consume/throttle 由 harness 点火)对「若执法在环」的机械面正控制——两面同真:现役真系统无 band 行为差,ktest 面证明机制一旦消费即兑现差异化。slack 轴两面同向(真系统 +25.9ms/30ms 档,本件 +10.0ms/10ms 档,均精确落在顺延律上)。

## 1. 载体与口径(ktest 负载生成器)

### 1.1 形态(代码锚)

- **注册**:kernel/src/ktest.rs:125(`run()` 末位新号纪律——ktest done 前);定义体 :8861-9417。
- **受控 spawn**:`loadgen_spawn(pattern, count, burst_ns)`(:8931)——真 `spawn_intent`(sched/src/sched.rs:1227)×12 入真 PER_CPU_RQ,M4F 式走查验证在席后 `rq_unlink_kt` 全摘;现役带任务打 `[band]` 串口行(boot 实测 8 行:lgIA0-3 q_ns=10000000 w=256..1024 / lgUT0-3 q_ns=20000000 w=64..512)。
- **运行环**:本地 `Runqueue`(test_band/FAIR-K 先例——真 RQ 无 pick 出口);enqueue/pick/tick/dequeue 走真 class 机械——`class_of_task`(sched/src/class.rs:631)per-task 分派、`Fair::pick_next_filtered`(:378,eligible 过滤→band 限流真生效)、`Fair::tick`(:219,LAST_VCT 回落位 ktest 窗合法)。
- **睡眠面**:真 SLEEPQ(`sleepq_insert`/`sleepq_unlink`,test_fast/RING3 先例)+ 真 CP-11 顺延律 `timer_slack_defer`(sched/src/intent.rs:89,block_current 入队点同款 sched/src/sched.rs:2567)+ 真 oneshot 面(`tighten_deadline` sched/src/oneshot.rs:32 + `arm_at` arch/aarch64/src/timer.rs:110)。
- **band 面**:真 `configure`/`consume`/`period_check`/`throttle_all`/`unthrottle_all`(sched/src/band.rs:109/165/200/241/273;test_band 先例——事件驱动记账,ktest 手驱消费点)。
- **驱动主环**(:9027 `run_group`):到期唤醒(批扫睡者)→ band 翻期 → pick → 首切记账 → 跑一个 sub-unit(250µs)→ tick 记账 → band 记账 → 轮次结算(睡眠腿走真顺延律)。
- **负载形状**(:8876-8902):interactive burst 100µs+睡 10ms(intent 0x02)/utility burst 10ms+睡 1ms(intent 0x03)/burst 恒跑(intent 0x04);每类 N=4、ROUNDS=6;sub-unit 250µs;slack A3/A4=10ms;band 包络 interactive 20ms/60ms(组总额 2.4ms 恒不越)、utility 20ms/60ms(期内占有率 ≈0.49〔首跑 ranUT/wall=240/485 实测〕→ 期中必越)。

### 1.2 测量口径

每任务每轮 **lat = 首切时刻 − 请求唤醒时刻**(入睡前未顺延的 deadline)。分量:slack 顺延(确定性,仅 utility ≥UTILITY 档)+ 到期检出粒度(≤1 sub-unit,忙等口径)+ 入环排队(EEVDF 出选序)+ band 限流等待(仅 A2/A4)。样本 = 每任务 ROUNDS−1 笔(首轮 spawn 即在环无前置睡眠),每型每组 4×5=20 笔,nearest-rank 分位(p50=第 10 位,p99=第 20 位=最大)。burst 任务无睡眠面只计吞吐(sub-unit 完成数)。断言面(全绿两跑):样本数恰满/分位单调/lat ≥1 tick/A3-A4 utility p50 ≥ 顺延量(数学下界)/interactive p50 ≤5ms/A2-A4 utility 必越线 thr≥1 且 interactive 恒不越 thr=0/关态零执法/收口环空+SLEEPQ 空+真 RQ 零残留。

## 2. 数据(两跑全量,µs)

### 2.1 组行(跑 1 = out/logs/loadgen-ab-1794772056.log;跑 2 = 跑 1 后复跑)

| 组 | slack | band | ia p50 | ia p99 | ut p50 | ut p99 | burst 单元 | wall ms | thr | 跑 |
|---|---|---|---|---|---|---|---|---|---|---|
| A1 | 0 | off | 820 | 2470 | 758 | 11530 | 963 | 492 | 0 | 1 |
| A1 | 0 | off | 1064 | 2078 | 763 | 1263 | 960 | 484 | 0 | 2 |
| A2 | 0 | on | 345 | 1268 | 766 | 37538 | 1688 | 672 | 11 | 1 |
| A2 | 0 | on | 346 | 394 | 748 | 37758 | 1682 | 667 | 11 | 2 |
| A3 | 10ms | off | 350 | 1636 | 10776 | 11341 | 1153 | 538 | 0 | 1 |
| A3 | 10ms | off | 405 | 1599 | 10835 | 11286 | 1156 | 534 | 0 | 2 |
| A4 | 10ms | on | 412 | 1434 | 10804 | 36480 | 1687 | 668 | 11 | 1 |
| A4 | 10ms | on | 347 | 410 | 10621 | 36314 | 1687 | 668 | 11 | 2 |

配套实跑 CPU 对账(ranIA/ranUT/ranBK,跑 1):A1=2/244/243ms、A2=2/240/424ms、A3=2/244/288ms、A4=2/240/421ms——负载形状自证(ia 24×100µs=2.4ms、ut 24×10ms=240ms 恰吻合)。oneshot 探针(真 arm_at+tighten_deadline 收紧+忙等检出):两跑 0-6µs。

### 2.2 汇总行(串口 `KTEST LOADGEN cmp *`,跑 1 原样)

```text
KTEST LOADGEN cmp interactive_p50_us A1=820 A2=345 A3=350 A4=412
KTEST LOADGEN cmp interactive_p99_us A1=2470 A2=1268 A3=1636 A4=1434
KTEST LOADGEN cmp utility_p50_us   A1=758 A2=766 A3=10776 A4=10804
KTEST LOADGEN cmp utility_p99_us   A1=11530 A2=37538 A3=11341 A4=36480
KTEST LOADGEN cmp burst_units      A1=963 A2=1688 A3=1153 A4=1687
```

## 3. 判读(五条发现)

1. **CP-11 顺延律精确落地【证】**:utility p50 在 A3/A4(10.6-10.8ms)vs A1/A2(0.75ms)——+10ms 顺延(625000 tick 截尾换算)分毫不差;超出顺延量的 0.6-0.8ms = 检出粒度+排队分量。断言面钉了数学下界(p50 ≥ 顺延量)两跑全绿。这是 slackface L1/L2(顺延只加不减/交互零牺牲)在真 CNTVCT 域的首次端到端实证。
2. **交互零牺牲【证】**:interactive p50 四组 0.35-1.06ms、thr_ia 恒 0(A2/A4 断言钉死)——slack 不顺延 interactive(档 <UTILITY)、band 配额不越(组总额 2.4ms ≪ 20ms/期),两道差异化机制都不碰交互任务,与设计合同逐字一致。
3. **band 限流的尾延迟代价【证】**:utility p99 A2=37.5/37.8ms、A4=36.3/36.5ms vs A1=11.5/1.3ms、A3=11.3/11.3ms——限流等待(越线→翻期,上界=period 60ms)把尾延迟放大 3.2× 以上且两跑高度稳定(36.3-37.8ms 窗内);p50 不动(A2/A4 ≈ A1)——限流是尾事件不是中位事件,正是「包络执法」的预期形状。
4. **让出的 CPU 喂给 burst【证】**:burst 单元 A1→A2 = 963→1688(+75%)、A3→A4 = 1153→1687(+46%),ranBK 243→424/288→421ms——utility 被限流腾出的 CPU 时间几乎全额被 burst 任务收割(burst 恒跑恒在环,是天然的黑洞)。band 差异化的「再分配」侧面首次拿到数字:差异化不是免费的,是「从 utility 挪给 burst」。
5. **次要发现(推断,单源两跑同向)**:interactive p50 在 A1(820/1064µs)系统性高于 A2/A3/A4(345-412µs)——【推断】utility 被顺延或限流压制后,唤醒/burst 洪峰减弱,interactive 醒者更快出选(A1 内 utility 每 1ms 醒一次制造队列竞争)。A1 两跑间 ia p50 漂移(820 vs 1064)与 ut p99 漂移(11530 vs 1263)是 TCG 噪声面;A2-A4 各值两跑紧贴(≤7% 漂移),主结论不依赖漂移域。

## 4. 工具边界(【界】,报告消费须知的口径限制)

1. **忙等口径**:ktest 在 timer::init/IRQ 之前跑,睡眠唤醒为轮询检出(检测粒度 ≤1 sub-unit=250µs),oneshot「精度」数字(0-6µs)是忙等检出下界不是真 IRQ 唤醒延迟——真 IRQ 口径归 rwake 族 boot spawn 面(kernel/src/main.rs:914 同形,后续刀)。A/B 组间同口径,差分结论不受此影响,绝对值消费须换算。
2. **手驱消费点**:band 记账/翻期在驱动器循环里手驱消费(band.rs 执法点未并线热路径——M4 入场券纪律,class.rs:437 注),等于「执法在环但由 harness 点火」;并线后(入场券过)数字应复跑。
3. **单核 TCG**:绝对延迟含 TCG 译码噪声;分位口径 n=20(p99=最大样本),尾事件分辨粒度粗——首份证据够判方向,不够定标。
4. **sub-unit 250µs**:到期检出/再 pick 粒度近似(真系统 slice 粒度粗得多),对 wake 类延迟引入 ≤250µs 一致性偏置。
5. **dpg_press 串口红归属**:两跑 boot 后段有 `dpg_press` panic 行——并行会话 dpg.rs 在途件的自证断言(其日志 dmesg-e2e2-1794771471.log 同红且无 LOADGEN 行),与本刀无关;本刀收口断言(环空/SLEEPQ 空/真 RQ 零残留/util 清零/timer 掩蔽)两跑全绿,boot 后续标记(DEMO ok/THR ok 等)正常。

## 5. 后续刀口(消费面建议)

1. **M4 入场券数据面**:本报告 = 入场券三件(「v1 遥测实证 + loadgen + 尾延迟断言」)中的 loadgen+尾延迟两件首次齐——band 并线(带参数进热路径 quantum/权重执法)前,建议用本 loadgen 复跑并线后四组,直接对拍本件 §2 基线。
2. **真 IRQ 口径**:把 loadgen 形态移植到 boot spawn 面(rwake 同族,fw_cfg 门控),消 §4-1 忙等边界。
3. **p99 分辨率**:ROUNDS 抬到 ≥50(样本 ≥200)或接 HDR 环(record_wake_lat 现成,ktest.rs:8007 先例)拿桶化 p99——首份 n=20 只够「尾事件存在且 3.2×」的判词。
4. **A2 与 A4 几乎同值(ut p99 37.5 vs 36.5ms)**:slack 与 band 两机制在 utility 尾上近似叠加无关(p50 由 slack 抬,p99 由 band 抬)——两因子正交性的更强证据需因子设计扩组(如 slack 1/5/10ms 梯度),归下一刀。

## 6. 引用锚(一手行号,2026-11-16 工作树)

- 载体:kernel/src/ktest.rs:125(注册)、:8833(LgGroup)、:8840(LgSlot)、:8861(test_loadgen)、:8931(loadgen_spawn)、:9027(run_group)、:9378(cmp_row)。
- 机制:sched/src/sched.rs:1227(spawn_intent)、:2549(block_current 顺延消费点 :2567);sched/src/intent.rs:64(TIMER_SLACK_NS)、:89(timer_slack_defer);sched/src/band.rs:109/165/200/241/273(configure/consume/period_check/throttle_all/unthrottle_all);sched/src/class.rs:631(class_of_task)、:378(pick_next_filtered)、:219(Fair::tick)、:437(M4 入场券纪律注);sched/src/oneshot.rs:32(tighten_deadline);arch/aarch64/src/timer.rs:110(arm_at)、:202(cntvct)。
- 先例:kernel/src/ktest.rs:7346(test_band_m4/M4F)、:8007(test_hdr_wake)、:7130(test_band)。
- 数据:out/logs/loadgen-ab-1794772056.log(跑 1 全量串口)及同目录次新 loadgen-ab-*.log(跑 2)。
- 背景判词:research/调度深度调研/06-综合定谳-v1.0.md §1.7-③(「首份真实 A/B 报告(CP-06)未动」)与 §0 终判(「瓶颈不在机制在证据与消费」)。
