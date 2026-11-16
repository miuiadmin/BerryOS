# K——语义完整性（全面遗漏审计第四轮·语义级）

> 版本：1.0 ｜ 日期：2026-11-16 ｜ 定位：第四轮语义级审计——全量回归 100% 绿之后不查「测试过没」而查「端到端真的在工作」：五条链（intent→band→调度差异化／EAS place→eas_pick_cpu／A/B loadgen／PM util 通道／timer slack 机制）逐条三问（代码链通？测试覆盖？QEMU 真跑过？）。方法：精读 kernel/src/ktest.rs（BANDM4／EB-0／LOADGEN／INTENT 四标记段）＋sched/src/class.rs＋policy.rs＋sched.rs＋band.rs＋intent.rs＋user/berry-em/src/lib.rs＋user/bins/berry-power 全件＋svcmgr/berry-exec 透传面，每条结论给路径＋行号；QEMU 证据取自 out/logs/ 留档日志与构建产物逐行比对；推断标「推断」；全仓只读，本文件为唯一产出件（README 登记行按本轮「只写产出件」纪律未补）。

## 〇、结论速览

| 链 | 代码链 | 测试覆盖 | QEMU 实测 | 语义总判 |
|---|---|---|---|---|
| 1 intent→band→调度差异化 | assignment 面**全通**（manifest 必填 intent→svcmgr→exec→setattr→class_of_task→FairBand 实例）；**行为差异化零接线**（FairBand trait 全委托 unit Fair；band 执法四 API 热路径零调用） | BANDM4 六段全（M4C 参数差异／M4F spawn 面断言俱在）；**EL0 setattr→带实例无断言** | ✓（[band] 行＋BANDM4 ok＋INTENT ok＋eco: app it 03/04） | **通到「带实例分派」，差「参数消费＋执法」**——注释自认 M4 并线归后续，非隐瞒但语义上差异化现为 0 |
| 2 EAS place→eas_pick_cpu | place():762→eas_place→eas_pick_place→berry_em::eas_pick_cpu **全通**；但生产 hint 源结构性为零 → 生产 SMP 上 eas_place 恒 None | EB-0/1/2＋EA 族＋berry-em host pick_cpu_laws（含 4 核 little/big 表）全 | ktest 单核直调 ✓（EAS ok）；**SMP 真选核零证据**（EAS pick 行从未出现） | **测试可达、生产未激活**——管道②闭合但无生产 hint 注入面（v3 policy_bind 0x83 是预期通道） |
| 3 A/B loadgen | 四组（slack 0/10ms × band off/on）真机械手驱 | 断言按组锚定（顺延下界／限流越线／零牺牲），结构上排除四组全同 | ✓✓ 两份日志双 run 复现：ut_p50 758→10776（slack）、thr 0→11＋ut_p99 1507→37916＋burst 963→1686（band） | **真差异、真复现**——「A1=A2=A3=A4 全同」不成立；A1≈A2 的 p50 是语义正确（band 作用于 p99/吞吐） |
| 4 PM util 通道 | 前三环**真通**（Fair::tick EWMA→syscall 0x40→berry-power 真读→ingest→max_est→schedutil req→cap aggregate eff）；末端两断点（无常驻轮询〔当前源码〕、无内核写回——eff_khz 只进 println） | SYSCALL 面＋lib host 31 测全 | ✓ 单读实证（util=127@k）；**重大发现：≤07:53 日志曾有 pmfreq 常驻循环版（util 爬坡 238→559、四帽源 src3/pmseat），当前源码（08:01）与 08:58 重建 rootfs 均已不含** | **通道通、执行面空转、常驻面现缺**——owner 的频率实现「曾到位后被撤」（推断：08:01 批量编辑回退，需对账） |
| 5 timer slack | 全通（bootparams 键→apply→sched 全局→block_current 入队点 LATE-only 顺延→下游 arm_at/tighten/expire 零改动继承） | TS 十一条断言＋LOADGEN LG-0b 锚＋slackface 单源 host 钉板 | ✓ A3/A4 实测 p50≈due+10ms；**产品键路径无 QEMU 留档**（eco: bootparams timer_slack 行零命中） | **机制链全通＋实测**；产品键注入面无留档（轻症） |

## 一、链 1：intent→band→调度差异化

### 1.1 代码链逐环（assignment 面全通）

manifest `[app]` 五键**必填**含 intent（user/berry-manifest/src/lib.rs:12、:538、:1138——缺键 parse 拒收）→ svcmgr app_launch 以 `intent: man.intent as u8` 填 SpawnSpec（user/bins/svcmgr/src/main.rs:6309；设计注 :6130-6134「Intent M4 透传」）→ berry-exec `intent != 0` 才发 `berry_sched_setattr`（0x3C）（user/berry-exec/src/lib.rs:705-715，spec.intent 字段 :370-379）→ 内核 `apply_sched_attr_to_task` intent 双落 `intent_declared/effective`（kernel/src/syscall.rs:1625-1630；thread_start 面 :1741、process_start 面 :1870 均走此）→ `class_of_task` 按 `intent_effective` 查工厂分派（sched/src/class.rs:631-636→:512-514→`berry_iprofile::band_of_intent` 表行）→ `enqueue_with_kick` 等调度原语六点消费（sched/src/sched.rs:1222-1225 注、:1252）。内核侧 spawn 面 `spawn_intent` 同链且打 `[band]` 观测行（sched/src/sched.rs:1227-1254，串行打印 :1238-1250）。

### 1.2 行为差异化：零接线（注释自认，非漂移）

FairBand 的 trait 面**全方法委托现役 unit Fair**（sched/src/class.rs:521-537——enqueue/dequeue/pick_next/tick/priority 五委托；priority() 返 0 同 Fair）：带参数（quantum_ns/low_weight/high_weight/latency_target）**只落字段不进热路径**（class.rs:469「带参数此刻只落字段，M4 并线时成为消费源」；:549-551「本环闭合的是 assignment 面，热路径 per-band quantum/权重执法仍归 M4 并线面」）。实际 quantum 全局单值 `hal::timer::slice_cycles()`（class.rs:97）、权重走 `prio_effective`（policy/PI 合成，policy.rs:421）——均与带无关。band 预算执法四 API（configure/consume/period_check/throttle_all/unthrottle_all，sched/src/band.rs:109-293）**全仓热路径零调用**（kernel/＋sched/ 除 ktest 外 grep 零命中；ktest.rs:8820-8822 自注「M4 并线前热路径本无执法点，手驱 = 入场券证据面的可用形态」）。结论：**「调度差异化」当前语义 = 带实例分派＋[band] 打印，调度行为逐位零漂移**——这是入场券纪律的明示设计（class.rs:453-458），但对「端到端真的在工作」的回答是：分派环真工作，差异化环未开始。

### 1.3 测试覆盖与 QEMU 实测

BANDM4 六段（kernel/src/ktest.rs:7396-7583，run() 注册 :110）：M4A 现役集恰两带（:7421-7430）／M4B 工厂门（:7434-7447）／M4C **参数差异生效**（:7452-7471——quantum/权重包络/latency_target 三轴不同＋方向钉板）／M4D 表→工厂单一事实链（:7476-7495）／M4E 同环共存互不吞并（:7501-7520）／M4F **spawn 面端到端有断言**（:7529-7580——真 spawn_intent 入真 PER_CPU_RQ 在席 `seen=[1,1,1]`、三任务 TaskClass 分派逐一对拍（Band/Band/Unit）、spawn 面两带 quantum 不同＋interactive 权重包络整体更高）。**覆盖缺口**：EL0 sched_setattr→class_of_task==Band 无任何断言（ktest SchedAttr 测只测 merge 纯面 :6502-6525；sa_parse 负向 :6230），用户态 spawn 也无 [band] 观测行（打印只在内核 spawn_intent）。QEMU 实证：run-1794773101.log 有 `[band] m4spIA intent=2 q_ns=10000000 w=256..1024`／`m4spUT intent=3 q_ns=20000000 w=64..512`＋`BANDM4 ok n=2`；六任务 intent 结算链 `it_driver/it_sinkA/it_sinkB` [band] 行＋`INTENT ok`（main.rs:1352-1357 spawn）；rootfs 真应用透传 `eco: app CAMERA it 03`／`APPQUIET it 04`／`BROWSER it 03` 等（run-rootfs-1794786581.log）——manifest→svcmgr 环 QEMU 实证。

## 二、链 2：EAS place()→eas_pick_cpu

### 2.1 代码链通

`place()` SMP 门（sched/src/sched.rs:758——ttbr0/pinned/spread/par_enabled 四门，内核任务结构性 CPU0）→ EAS 首接 :762 `eas_place` → 三源收集（EM 域表 boot 注册 main.rs:857、per-CPU util EWMA、policy placement_hint，sched.rs:844-855）→ `eas_pick_place` 判序（:811-835——①hint 词汇执法 0/越域→None；②overutil 门 ：825-832；③hint→prev 映射 1→CPU0/2→CPU1；④`berry_em::eas_pick_cpu` 两段选核）→ user/berry-em/src/lib.rs:273-326（每域最高 spare capacity 候选 ∪ prev、最小能量边际胜出、平局 prev 胜、放不下/无据除名→None）。overutil 门在 policy 侧再判一次（policy.rs:474-476）。util EWMA 生产侧在 Fair::tick 真记账（class.rs:263-267——每 tick `util_accumulate`＋`util_tick_window`，即 QEMU 运行期数据面真在滚）。

### 2.2 EB-0 覆盖与 SMP 真跑判定

EB-0 **有且钉死回退语义**：ktest.rs:8766-8768 `eas_place(7)==None`（未绑 policy 表 = 无意见不进 EAS 面——place() 回退 K=2 的机器断言）；EB-1 绑 hint 表三源端到端 `Some(1)`（:8771-8776）；EB-2 util 泵越阈 overutil 让位 `None`（:8777-8786）；EA 族纯决策面七断言（:8718-8753）；收口 EC（:8788-8794）。berry-em host 测 pick_cpu_laws 含 Linux Example 2 四核 little/big 全景（lib.rs:521-551）＋空域防御（:556-568）。**SMP 真跑判定：零证据**——选核自证行 `EAS pick cpu=`（sched.rs:858-866，par 门内首决策打一次）在 out/logs 全部 19 份留档日志零命中（串只存在于 out/berryos.img 二进制）；smoke-smp 日志默认写 /tmp 不留档（tools/smoke-smp.sh:109-117）。**且生产 hint 源结构性为零**：placement 非零的 Entry 全仓只在 ktest（ktest.rs:3487/:4461/:7869/:8625/:8772）；生产唯一 bind 点 ai_demo 两 entry 均 `placement: 0` 且用后 unbind（main.rs:4237-4270）→ 生产 SMP 上 placement_hint 恒 0 → eas_place 恒 None → place() 恒走 K=2。**判定：链路代码与测试全通（ktest QEMU 实证 EAS ok），但 EAS 选核在生产 SMP 上不可达——「测试可达、生产未激活」**（激活通道 = v3 policy_bind 0x83 写路，D 线已记 0x83 门控）。

## 三、链 3：A/B loadgen 四组数据真差异

### 3.1 断言结构排除「四组全同」

四组 A1(slack=0,band=off)/A2(0,on)/A3(10ms,off)/A4(10ms,on)（ktest.rs:9332-9337）；差分断言**按组锚定**：slack>0 组 `ut_p50 >= defer_cyc`（顺延下界，:9302-9304）vs slack=0 组 `ut_p50 <= 5ms`（:9308-9309）——两组上界/下界互斥，A1 与 A3 的 utility p50 数值上**不可能同值**；band on 组 `thr_ut>=1 ∧ thr_ia==0 ∧ unthr>=1`（:9314-9316）vs off 组 `thr==0`（:9318-9319）；interactive p50 恒 ≤5ms（:9311-9312）。组内结构面（样本数恰满/分位序单调/lat≥1 tick，:9284-9295）与收口对账（环空/SLEEPQ 空/slack 未被扰动/RQ 零残留，:9230-9236、:9399-9414）全在。

### 3.2 QEMU 实测数据（两份日志双 run 复现）

out/logs/loadgen-ab-1794772056.log：`utility_p50_us A1=758 A2=766 A3=10776 A4=10804`（slack +10ms 实测命中）；`utility_p99_us A1=11530 A2=37538`（band 限流尾延迟 3.3×）；`burst_units A1=963 A2=1688`（utility 被限流让出的 CPU 真给了 burst）；thr A1/A3=0、A2/A4=11。run-1794773101.log 独立复跑同形（ut_p50 768/765/10780/10806、thr 0/11/0/11）。**注意 A1≈A2 的 p50（758 vs 766）不是链路没通而是语义正确**：band 配额 20ms/60ms 期不截 p50（utility 期内占有率≈0.49 不越线），差异化体现在 p99（限流等待）与 burst 吞吐——与组常量注释的设计计算一致（ktest.rs:8895-8902）。结论：**数据真有差异、机制真在执法、双 run 稳定复现**。

## 四、链 4：PM util 通道（syscall 0x40→EWMA→频率帽→聚合）

### 4.1 前三环真通＋QEMU 单读实证

生产侧 EWMA：Fair::tick 每拍记账（class.rs:263-267）。syscall 0x40 `sched_util_snapshot`（kernel/src/abi_gen.rs:97；handler syscall.rs:2632-2643 读 `berry_sched::cpu_util` 逐核写回；注册 :4095；ktest 拒面/返回槽数断言 ktest.rs:2363-2401）。berry-power 起机**真调 syscall**（user/bins/berry-power/src/main.rs:161-166——`berry_sched_util_snapshot` 真读，rc>0 走 `ingest_util_snapshot` 逐核折叠 util.rs:76-90→EWMA）→ `max_est`（util.rs:103-106）→ `decide()`（lib.rs:198-208——`caps.aggregate()` 取 min 帽＋`schedutil_freq_khz(util,fmax,capacity)` 频率请求＋eff 钳帽）→ 诊断行打印。QEMU 实证：run-rootfs-1794786581.log `berry-power: skeleton dry-run ... util=127@k`（**@k = 内核快照真值**，非合成回落）＋`eco: berry-power up`（svcmgr 预拉成功）。

### 4.2 末端两断点＋代际断裂（本轮最大发现）

**断点①执行面**：内核无 DVFS/聚合点写面（kernel/ 全仓无 set_freq/dvfs 消费点；main.rs:43-46 自注「真实写通道随内核聚合点 ABI 立法挂 CP-08/CP-10 刀」）——eff_khz 只进 println，频率裁决零落地（QEMU TCG 亦无变频面，判定为机制空转而非环境限制）。**断点②常驻面（当前源码）**：main.rs:39-40 自注「剩余留窗：常驻循环的周期轮询面（当前仅自检单读）」＋:199 `park()` 常驻——当前 PowerCore 方法面无任何 step/tick 入口（lib.rs:148-219 仅 new/push_byte/ingest_*/decide）。**代际断裂**：≤07:53 的五份日志（run-rootfs-1794767417/1794786581、dmesg-e2e×3）均含 `berry-power: pmfreq util=238→335→420→494→559 cap=…@src3 eff=…khz` 常驻循环行（util 单调爬坡 = 真周期读内核 EWMA 追负载；cap src2/src3 切换 = 多源聚合真执法；dry-run 行含 `pmseat=` 段）——而 **pmfreq/pmseat 两串在当前源码全仓零命中**，且 cap 源枚举现为三源 0..2（cap.rs:22-29，日志的 src3 指向四源版）；构建产物 out/berryos.img（02:31）仍含 pmfreq 串，rootfs.cpio 已于 08:58 重建且不含。时间线：源码批量编辑 08:01（cap.rs/lib.rs/main.rs 同刻 mtime）→ 08:58 重建。**判定（推断）**：owner 的 PM 频率实现（pmfreq 常驻轮询＋pmseat 四帽源）曾落盘并 QEMU 实证到位，08:01 编辑将其回退/撤除为「单读＋park 三源」版——若非有意回退需立即对账；若有意，main.rs 窗 3「首环已闭合」注与日志实证矛盾应更新。前三环＋裁决纯函数（host 32 测〔justfile:850/:855 口径〕＋cap exhaustive 聚合序 cap.rs:113）判定为真；**「频率实现到位」按当前树判：未到位（无常驻、无写回）**。

## 五、链 5：timer slack 机制

### 5.1 链路全通

bootparams 词表第三键 `timer_slack`（kernel/src/bootparams.rs:65 值域 [0,999_999_999]、:258-259 解析）→ apply 注入 sched 全局（:338-339 `set_timer_slack_ns`；fw_cfg 探针通道同 sink main.rs:806）→ 全局单源（sched/src/intent.rs:64-75 TIMER_SLACK_NS）→ **块级应用在 block_current 入队点一次落定**（sched/src/sched.rs:2567-2572——`timer_slack_defer(intent_effective, deadline, slack, cntfrq)`；LATE-only 判定律 intent.rs:89-103：仅 UTILITY..=MAINTENANCE 三档顺延、交互/LC/未声明恒等、ns→tick 截尾、不回绕）→ 下游三臂（idle park arm_at／忙核授片 tighten_deadline :2646-2649／expire 扫描）全读 SLEEPQ 队头 `wait_deadline` 零改动继承顺延值（:2560-2566 注）。

### 5.2 测试与 QEMU 实测

ktest 有断言：test_timer_slack 差分钉板十一断言（ktest.rs:6079-6131——交互精确/utility+slack/未声明零漂移/零 slack 恒等/截尾/饱和帽/单调性）；LOADGEN LG-0b A/B 锚（:8973-8987）＋组内 `timer_slack_ns()==g.slack_ns` 未扰动断言（:9236）＋收口归零（:9414）。QEMU 实测：A3/A4 组 utility p50=10.78/10.81ms ≈ due(1ms 睡)+10ms 顺延＋检出开销——**真睡眠顺延在 QEMU 量到**。产品键路径：`eco: bootparams … timer_slack=NS` 诊断行（bootparams.rs:342-361）在全部留档日志零命中——键从未在任何留档 boot 注入过，产品键注入面无 QEMU 留档（实测证据均走 LG 探针通道＝同一 setter 同一全局同一消费点，机制面等价；「产品键→全局」这一小段仅代码＋bootparams 解析面覆盖）。判定：**机制链端到端通且实测**；留档缺口为轻症。

## 六、附带发现与建议

1. **P1｜berry-power 源/二进制代际断裂**（§4.2）：08:01 编辑撤 pmfreq 常驻环＋pmseat 四帽源；若 latest 全量回归（E 线 100% 绿）跑在 08:58 重建后，则 PM 频率面回归覆盖的是「单读＋park」版——pmfreq 版从未有对应源码在册的回归记录。建议 owner 对账回退意图：误撤则恢复（日志已证可行），有意则更新 main.rs 窗 3/窗 4 注释与「首环已闭合」口径。
2. **P1｜EAS 生产不可达**（§2.2）：hint 源结构性为零使管道②在生产恒 None——「水通管道」闭合的是机制不是激活；建议随 v3 policy_bind（0x83）写路落地补一条生产 hint 注入面，或在调度器设计 v1.3 增量注中明示「生产激活挂 v3」。
3. **P2｜EL0 setattr→带实例零断言零观测**（§1.3）：[band] 行只在内核 spawn_intent 打；建议 ktest 补一条 svc sched_setattr 后 class_of_task 对拍，或 apply_sched_attr_to_task 尾随打行（成本面权衡）。
4. **P2｜bootparams eco 行全键无留档**（§5.2）：不止 timer_slack，slice_hz/intent 键也无任何留档 boot 注入证据；建议 smoke 族加一相带 bootparams 注入。
5. **P3｜smoke-smp 日志默认 /tmp 丢弃**（tools/smoke-smp.sh:109-117）：SMP 相证据依赖 SMOKE_KEEP_LOG=1 手动保留；「SMP 真跑过」类审计每次都撞留档缺口，建议失败外也留末相日志于 out/logs/。
