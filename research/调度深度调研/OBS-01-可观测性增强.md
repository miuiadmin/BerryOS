# OBS-01:调度可观测性增强——per-band 延迟统计面(出选/runtime 计数)+ EAS 决策日志 + per-intent 吞吐

Type: survey
Status: current
Updated: 2026-11-16
版本:1.0

> 定位:06 号综合定谳 §5 观测维 7.5 扣分项(「用户态闭环未接/跨核唤醒上界不可测/真机相零」)的最小补面第一刀——不追大而全的 telemetry ABI,先把**调度器自身的决策与账面变成可 grep 的串口行**:per-band 出选/runtime 计数 + per-intent 吞吐计数(`SCHED` 周期行)+ EAS 选核决策日志(`EAS place` 行)。载体 = sched crate 新模块 `stats.rs`(纯计数面)+ 三处最小接线,零 trait 改动、零 Task 字段、零新锁。
> 方法:代码直读(新鲜视角,不依赖 01-07 号审计结论)+ QEMU virt TCG 单核/双核实跑验收(cortex-a72,timeout 带帽,串口行逐行核对)+ host cargo test 全量(212 测组)。证据分级:【证】= 本轮实跑串口/构建输出;【界】= 设计取舍的工具边界;推断标「推断」。
> 纪律:本刀只动 sched/(stats.rs 新增 + class.rs/sched.rs/lib.rs 三处接线)+ 本件;kernel/justfile/tools 只读。boot 基线 `wip/sched-1.1` 工作树——**同窗多并行会话在途**(SMP 验证会话改 band.rs/util.rs/ktest.rs、TCP 会话改 vnet/user、GUI 会话改 berry-appshell/justfile/tools),归属拆解见 §3。

---

## 0. 一句话结论

**三件观测面全部落地并可 grep:单核 boot 40s 实测 7 条 `SCHED picks i:...| band2 picks=.. rt=..cy` 周期行(5s 节流、计数单调),SMP+LB 门 boot 实测 `EAS place cpu=-(K=2) hint=0 u=0/0 spare=1024/1024` 决策行(旧一次性门在默认 boot 结构性冷——只记 Some 决策而默认无 policy hint,本刀改首调即打含回退行);ktest 零新断言(观测面非合同面)、host 212 测组全绿、unit Fair 路径逐位零触碰。**

## 1. 实施面(代码锚)

### 1.1 计数面载体:stats.rs(新模块)

- **静态账**(sched/src/stats.rs:38-52):`BAND_PICKS`/`BAND_RUNTIME`(`[AtomicU64; BAND_SLOTS]` 镜像 band.rs 表形)+ `INTENT_PICKS`(`[AtomicU64; 8]`,键 = intent_effective 编码)。跨核一份全局账(band.rs `CONSUMED` 同裁决——带级观测是系统级信号非 per-CPU 决策输入);Relaxed 纯计数(IRQ_TICKS/LB_TICKS 同口径),原子 static 免入册 writable_globals(band.rs:78-81 A12 纪律)。
- **读出口**(stats.rs:88-107):`band_picks`/`band_runtime`/`intent_picks`,lib.rs:58-60 re-export——telemetry/DUMP 消费面预留。
- **记账点**(消费面单源,别处不重复记账):
  - pick 记账:`Fair::pick_next_filtered`(sched/src/class.rs:433-436)——**生产出选唯一路径**。任务书原文「FairBand 每次 pick 时记录」,但新鲜视角直读发现:生产 pick 是队列级扫描(`FairBand::pick_next` 是 ktest 直调委托面,生产不经它——class.rs:714-717 自注「生产 pick 是队列级 Fair::pick_next_filtered 不经本面」),挂 FairBand::pick_next 会结构性零命中。八个生产调用点(sched.rs yield:1376/exit:1567/block 重验:2616/try_preempt:2994/park 派发:3319/tick 抢占:3771/SGI:3868/park 重查:3936)全经 pick_next_filtered,故挂此单点即全覆盖。
  - runtime 记账:`FairBand::tick`(class.rs:753-755)——delta 取 `peek_tick_delta()`(class.rs:127,与 `Fair::tick` 记账同基准的只读孪生);unit Fair 路径零触碰(unit 任务零漂移纪律)。
- **口径注记**:pick 记「调度器出选决策」而非「上下文切换」——picked==prev 的续选也计(出选决策确已发生);runtime 粒度 = tick 采样(片边界),非全量计时,作带间相对对比信号【界】。

### 1.2 SCHED 周期串口行

- **挂点**:`on_irq_tick` 的 slice-燃尽 + preempt_count==0 区(sched/src/sched.rs:3750-3756,stuck 扫描块后、S-6 拉取块前)——hb 行同语境:G43 防 PRINT_LOCK 持窗自死锁的既有门内(被 PRINT_LOCK 窗任务打断的 tick 早退于 :3662-3664,不达本区),行原子走 `with_print_lock`(hb 先例 sched.rs:3674)。
- **节流**(stats.rs:109-127):cntvct 5s 窗,load+CAS 单赢家——**不能用 swap 形**(HB_LAST 的 swap 形靠「值/500」比较成立;delta 节流下 swap 被窗内调用持续前推锚 = 滑窗永不到期,首版实抓此 bug 后改 CAS)。
- **行形**(实测样例,单核 40s 七行):

```text
SCHED picks i:UNSPEC=1399 LATCRIT=0 INTERACTIVE=2 UTILITY=3 BACKGROUND=2 MAINT=0 | band2 picks=2 rt=0cy band3 picks=3 rt=0cy
```

intent 六档全印(固定序稳定列,便于跨 boot diff);band 只印非零槽。单核照打(par 门外——intent/band 账面单核同样成立)。

### 1.3 EAS 决策日志

- **旧面的结构性冷(新鲜视角发现②)**:旧一次性门(sched.rs 旧 :795-798/858-866)`EAS_LOG_ONCE` 只在 `Some(cpu)` 真决策时打,而 `eas_place` 返 Some 需要 policy placement hint ∈ {1,2}——默认 boot 无任何 hint 注入(ai_demo 表 placement=0,kernel/src/main.rs:4251/4257;ktest 的 PL/EB 系直调面被 par 门挡),即**默认 run-smp 该行永不出现**,「SMP 选核决策日志」证据面空转。
- **本刀改形**(sched/src/sched.rs:795-806, 864-885):首调即打 + 每 256 次一行(EAS_LOG_EVERY),**含 None 回退行**;快照 = 决策结果 + hint + 双核 util/spare(spare = topo capacity − util 饱和减,与选核能量判据同源同尺度)。raw UART 直写不吃 PRINT_LOCK(spin_bark 先例:调用点在 spawn 屏蔽窗内或持 KL,不新增 KL→PRINT 序边;跨核并写最坏形 = 字节交错,观测面容忍【界】)。par 门保持(ktest 直调/单核零输出漂移)。
- **决策语义零变化**:`eas_pick_place` 纯决策面未动,`eas_place` 只重排日志与返回(旧 `?` 早退改 match,行为逐位等价)。

## 2. 验证证据(全部实跑)

- 【证】单核 boot(`just run` 形态 + `opt/berry/noktest` 跳 ktest,40s 帽):7 条 SCHED 行,计数单调(UNSPEC 1399→4169,INTERACTIVE=2/UTILITY=3 恒定——boot 期 intent 任务睡后不再出选,合理),**零 EAS 行**(par 门单核零漂移)。
- 【证】SMP+LB 门 boot(`-smp 2` + `opt/berry/noktest` + `opt/berry/lb`,90s 帽):`EAS place cpu=-(K=2) hint=0 u=0/0 spare=1024/1024` 于 `PAR gate on`(:19)/`LB gate on`(:20)后首现(:25)——首个 spread 线程(thread_start spread=true,kernel/src/syscall.rs:1729)spawn 即触发;SCHED 行双核正常;`LB ok c0=0 c1=104 st1=0 n=4` 既有门禁不受扰。
- 【证】host cargo test(just test-host 全量):**212 个 test-result: ok、零 crate 失败**;recipe 尾 selftest_guard 红 = GUI 会话新工具 svg2icons.py 未接线(tools/selftest_guard.py 直跑复现,与本刀无关,§3)。
- 【证】ktest 零漂移:本刀零新断言(观测面非合同面);带本改动 boot 两跑 ktest 前段全绿(`LOADGEN ok`/`EASPROD ok`),止于并行会话在途红面 SMPLZ(§3 归属)。ktest 期(ktest 在 timer::init 前,main.rs:768→828)SCHED 行结构性零输出;EAS 行 par 门零输出——ktest golden 行为零漂移 by construction。
- 【证】band runtime 接线实证(ktest 整程 boot,240s 帽):`SCHED picks i:UNSPEC=1003 LATCRIT=0 INTERACTIVE=108 UTILITY=3855 BACKGROUND=5597 MAINT=0 | band2 picks=108 rt=16087625cy band3 picks=3855 rt=10036688cy`——BANDM4(kernel/src/ktest.rs:7689/7779 走 `class_of_task(...).tick`,note_runtime 真可达)与 LOADGEN 带任务真烧 CPU 后账面落数;且 ktest 负载构成本身被观测面直接数字化(utility 3855 picks vs interactive 108——band 限流重臂下的反复出选形态可见)。同跑 ktest 全绿:BANDM4 ok/LOADGEN ok/SMPLZ ok(SMP 会话同窗已修复其红面)/ktest done。

## 3. 归属与干扰记录(多会话热树,逐件拆解)

- **LOADGEN 时敏 panic 二分(本刀最大验证弯路)**:本刀完整版首两跑在 LOADGEN 断言炸(A4 interactive p50 ≤5ms / A1 utility p50 上界,组别漂移);二分定位:X2(仅保留 note_pick,撤 note_runtime/sched_line_poll/EAS 日志)全绿;随后同代码完整版两连跑 `LOADGEN ok`(止于 SMPLZ)。可达性分析收口:LOADGEN 驱动环(kernel/src/ktest.rs:9364-9440)pick 走 `Fair.pick_next_filtered`(note_pick 在场)、tick 走 `Fair.tick` 直调(note_runtime 不可达)、ktest 期无 IRQ tick(sched_line_poll 不可达)、par 关(EAS 不可达)——三处在 LOADGEN 均不可达 + X2 绿 + 同代码后跑绿,判 LOADGEN 两炸为**时敏 flake 或并行会话在途态伪影**(推断;TCG guest 钟=墙钟的 §A110 时敏 flake 族先例,ai_demo 断言曾因宿主争用同形挤穿,main.rs:4283-4287 注)。
- **SMPLZ panic(非本刀)**:断言「L3 40 折后须入量化死区(parked 冻结喂 overutil 门的收口)」——SMP util/park/lazy-pick 域,恰为并行 SMP 会话同窗新增测试(其 ktest.rs:9945 imports `any_armed/lazy_enforce/util_idle_fold` 即其同窗 band.rs/util.rs 新 API)。路径限定 stash 归因法在热树不可用(stash 会连带走走并行会话的 sched/ 改动致其 kernel 悬空,实踩后立即恢复)。
- **selftest_guard 红(非本刀)**:tools/svg2icons.py(GUI 会话新增)未接线 justfile。
- **树卫生**:本刀文件 = sched/src/stats.rs(新)+ class.rs/lib.rs/sched.rs 三处 OBS-01 标注接线;band.rs/util.rs/ktest.rs/justfile 的 modified 态全归并行会话,零触碰。git 零 add/零 commit(纪律)。
- **尾注(已闭环)**:SMPLZ 被并行 SMP 会话自行修复后,本刀整程 ktest 补跑全绿且 rt>0 实证入手(§2 末条)。

## 4. 边界与后续(诚实清单)

- **【界】runtime 粒度 = tick 采样**(片边界 + oneshot 提前醒),非 per-task 精确计时;绝对时长以 slice_left 口径为准,本面作带间相对对比。
- **【界】raw UART 跨核交错**:EAS 行不吃 PRINT_LOCK(KL 序边不新增),双核同时打行的最坏形 = 字节交错;频率门(首行+每 256 次)把概率压到观测容忍。
- **【界】全局账不分核**:per-band/per-intent 计数跨核聚合;per-CPU 维度(哪核跑了哪带)未分——SMP 专属归因留后续(与 SMP-01 §3 lazy-pick 执法刀同窗)。
- **未接的 06 号扣分项(本刀不盖)**:用户态闭环(telemetry_open 0x84)未动——本面是内核侧串口最小面,用户态消费 ABI 是独立刀;跨核唤醒上界(02 号 §⑤-4)部分缓解(EAS 行可见放置决策,wake 延迟上界仍无直接行);真机相仍零(QEMU TCG 实证)。
- **行预算**:SCHED 5s 一行(smoke 数分钟窗数十行内,与 hb/stuck 同噪声带);EAS 首行+每 256 次(boot 全程个位数)。若后续 smoke 对行数敏感,节流常量单点可调(stats.rs:118 / sched.rs:806)。

## 5. 来源索引

- 触发与扣分项:[06-综合定谳-v1.0](06-综合定谳-v1.0.md) §5 观测维行(:165)。
- 代码锚(2026-11-16 共享树快照;并行会话在途,行号可能漂移):sched/src/stats.rs(全文件)/class.rs:433-436,753-755/sched.rs:795-806,864-885,3750-3756/lib.rs:58-60。
- 先例引用:hb/stuck 行(sched.rs:3665-3745)/spin_bark raw UART(sched.rs:701-718)/band.rs 原子账纪律(:69-81)/LB_TICKS 全局账(:2055-2074)。
- 验证工件:/tmp/obs1-noktest.log(单核 7 行 SCHED)/tmp/obs1-smp-lb.log(EAS 行 :25)/tmp/obs1-testhost.log(212 ok)/tmp/obs1-full-r1/r2.log(LOADGEN ok 双跑)/tmp/obs1-ktest-full.log(ktest 整程绿 + band rt>0)。
- 并行归属:SMP 会话(band.rs/util.rs/ktest.rs SMPLZ)/GUI 会话(svg2icons)/TCP 会话(vnet/user)——见 §3。
