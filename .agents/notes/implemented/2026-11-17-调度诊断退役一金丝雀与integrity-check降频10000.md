# 调度诊断退役一刀:switches 金丝雀与 integrity_check 降频 100→10000

Status: implemented

## Problem

切换热路径挂着两件 bring-up 诊断,以 SOAK_PRINT_EVERY=100 为共同节流:①`bump_switches`(sched/src/sched.rs)每 100 次切换跨点即 PRINT_LOCK+五次 UART 调用拼 `switches=N kl=M` 行——切换点可能在 IRQ 上下文(tick 抢占支/SGI),打印窗口整段撑大 IRQ 屏蔽窗;②`integrity_check`(同文件)随同跨点在 tick 抢占支与 F5 强制让点支执行,本体 = 双核 RQ 锁+SQ 锁三段取放+双环全走查+SLEEPQ 全走查,全在 IRQ 窗内,SMP 参与态下还与对核 pick/steal 争 RQ 锁。自注「bring-up 诊断(G1)」且 04 号调度审计判为 Top-2 机会(见 [04 调度热路径 Top-5 审计](../../../research/内核深度进化/04-调度热路径-Top5优化机会-v1.0.md) 机会二):G1(分时金丝雀)与 S-6(直切臂漏计收口)里程碑均已定谳,bring-up 语境已过,该删未删。

## Decision

- **单常量降频 100→10000**:`SOAK_PRINT_EVERY` 是打印与 `integrity_check` 的共同节流源(两处调用点都吃 `bump_switches` 的 crossed 返回值),改一个常量即把「每 100 切换的 PRINT_LOCK+UART+三锁三全环走查」同时降百倍——最小侵入,金丝雀语义(计数持续涨=分时健康)完整保留。
- **计数面零变化**:SWITCHES 每 switch 一次 fetch_add 不动;watchdog 经 `switches()` 只读访问器、soakmon 经 `berry_sched::switches` 直读计数器,与本打印频率天然解耦(审计方案①「打印与断言解耦」的既有事实面)。
- **降频而非删**:smoke-smp(相 1/相 2 canary_check)与 smoke-image(阶段 4/6)仍把 `switches=` 行当限期硬门,ktest RING 族另走本地 `ring_check` 全量对账——线上降频+测试侧全量即审计「两全」裁定的落地形态;全删/counter-only 会当场打断这些门,须先走解耦刀(见 Consequences)。
- 注释三处同步:bump_switches 降频定谳注(日期+审计指针+解耦面)、helper 头注 `%100`→`%SOAK_PRINT_EVERY`、integrity_check 头注消费点频率更新。

## Alternatives considered

- **编译期 feature 门或 counter-only 全删**(任务书与审计方案①的原选项):金丝雀行归零,smoke-smp 相 1(DEADLINE_CANARY=400s 内需 ≥3 行)与 smoke-image 阶段 4 当场必红——打印消费面未先解耦前不可全删,故选任务书第一选项(降频 10000)。
- **integrity_check 移 idle_loop 低频段**(审计方案②另一半):把走查挪出抢占 IRQ 窗,语义更好但要新接线与频率口径,超出「最小侵入·零行为变化」的快赢判分;且 10000 档下 IRQ 窗内走查已是 200s+ 级稀疏事件,边际收益小,留后续。
- **hb 心跳/stuck 扫描/spin_bark 同批退役**(审计方案③「探针族」):频率低(每 500/2500 tick)且属 S-6 定谳结论一次性退役面,不同里程碑不同批,不并入本刀。

## Consequences

- 切换热路径每 100 次的 PRINT_LOCK+UART+三锁全环走查降为每 10000 次一次;IRQ 屏蔽窗周期性 µs 级尖峰与 SMP RQ 锁争用放大同比例缩小。计数/断言/ABI/调度语义面零变化。
- **消费面冲击(需后续刀)**:smoke-smp 相 1 实测全程仅 ~1000 次切换(out/smoke-smp-fail.20261117-093442.p1.log 10 行金丝雀),10000 档下限期硬门必红;smoke-image 阶段 4 同理;smoke.sh 主路金丝雀是软门(不足只警告)不受影响;bcts-parkprobe.py 的 churn 判据(尾部 ≥5 条严格递增)采样粒度变粗、no_churn_evidence 判定增多。解耦路径 = 审计方案①:门改读 `switches()` 计数器(watchdog 访问器已在)或 soakmon `sw=` 字段,再议全删。
- 同文件并行刀:preempt_checkpoint len 门控(见 [preempt_checkpoint len 门控笔记](2026-11-17-preempt-checkpoint-len门控-SVC尾检查点免锁前置过滤.md))与本刀同窗落同文件,验证为合体树实跑。
- 验证实跑:berry-sched aarch64 构建绿(隔离 target-qw1);berry_verify quick 三拍绿(fwcfg 26 键/docs 406 篇 0 警/ABI 五面);`SMOKE_PHASES=boot just smoke` boot 相 32/32 标记到齐 exit 0(含 ktest done = 全套 ktest 零回归、docs_guard PASS 0 警随 build 链收尾);手动 QEMU rootfs 生态长窗跑金丝雀行实证(见实施会话记录)。
