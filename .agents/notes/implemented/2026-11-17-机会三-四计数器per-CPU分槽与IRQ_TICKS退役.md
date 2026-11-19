# 机会三：四计数器 per-CPU 分槽与 IRQ_TICKS 退役

Status: implemented

## Problem

[04 号调度热路径审计](../../../research/内核深度进化/04-调度热路径-Top5优化机会-v1.0.md) 机会三：四个最高频计数器（IRQ_TICKS/SWITCHES/SWITCHES_TT/PREEMPT_CKPT_HITS）全是全局单 `AtomicU64`——SMP 参与态下双核对同一 cache line 做 Relaxed RMW，忙对稳态每 tick+每 switch×2+每 syscall 至少四次跨核行乒乓。三路勘察（wf_b56c58f5-adc）定谳：读者面全数 ≥/增长型零等值比较（watchdog 严格增长/G1.5 tel>=tt〔tel 本身已是两环求和——两侧对称〕/fa ckpt>=1/SOAK 打印），快照求和语义安全；**头号暗礁=LB_TICKS 裸 `[AtomicU64;2]` 判例两槽 8B 同行共存**——照抄则跨核 RMW 照旧、整刀收益归零；SmpCpu 尾插形态被 126/128 帽满+跨 crate 面驳回。

## Decision

- **槽形**：sched.rs 新类型 `Counter`（`#[repr(C, align(64))]` 单字段 AtomicU64——WaitBucket 判例形），三计数器迁 `[Counter; 2]`（SWITCHES/PREEMPT_CKPT_HITS/SWITCHES_TT），写侧一律本核槽 `fetch_add(Relaxed)`（LB_TICKS :3853 同款路由；`cpu_id()` TPIDR==0 防御窗回落 0 槽=单核正确，S-2b 判例）。硬约束入注：禁退回裸数组形态。
- **IRQ_TICKS 退役**（删定义+删写点）：全树零读者、每 tick 每核一发跨核共享 RMW 纯白付；tick 诊断面已由分核影子 `SmpCpu.ticks_pre`（note_tick_entry）/`SmpCpu.ticks`（note_tick）承担，金丝雀语义在分核面原样存活。
- **bump_switches 跨点新语义**：判定走**本核槽取模**（禁每 switch 求全局和——重新引入跨核 load 自 defeats 分槽本意）；打印值改 PRINT_LOCK 内两槽求和快照（宿主 smoke-smp awk 严格递增门强制打印面=全局和）。旧「整除判定与打印同值」性质显式放弃。返回语义=「本核槽跨界」布尔（integrity_check 两消费点：SMP 态走查总数 `floor(T0/N)+floor(T1/N)` 与旧 `floor(T/N)` 差 ≤1——审查轮计数论证核证「总量级不变」注释正确）。
- **三访问器求和快照**：`switches()`/`switches_tt()`/`preempt_checkpoint_hits()` 返回两槽 Relaxed load 和（hb 行 `tot=hb0+hb1` 与 telemetry::total 两环求和先例），签名不变消费者零改动；快照语义注记（两槽和非原子 ns 窗——读者全 ≥/增长型安全，禁新增等值比较）。
- **SOAK_PRINT_EVERY 保 10000/槽**：smoke-smp 相 1/2 canary ≥3 行硬门的既有债（0ea3d11ad 降频后全程 ~1000 切换必红）不本刀顺修——属消费面解耦刀（门改读计数器）；本刀判定本核槽化使双核各 ~500/槽再分薄，债加深一档已预挂账。
- **13 位勘正**：bump_switches 头注「十一位」实数 13 位/8 函数（旧注漏 on_irq_tick F5 限流让点副核 park/idle 两臂），位谱全枚举；同批滚四处陈账副本（SAFETY「六位皆然」/SWITCHES_TT「11 位」/tick 抢占支「六位」等）。
- **双轨不合并**：`SmpCpu.sw_cnt`（volatile 读面、sw_park_readout 消费）与全局 SWITCHES 槽并存——读面语义不同，合并不值。

## Alternatives considered

- **SmpCpu 尾插槽字段**：第三判例全套访问器现成，但逻辑尺寸 126/128 帽满、尺寸/偏移钉板全动、跨 crate 归属错位——弃。
- **SWITCHES 由 sw_park_readout 求和派生（退役全局计数器）**：bump_switches 需本核槽判定+13 位点与 note_switch_cpu 序耦合——不值，双轨保留。
- **SOAK_PRINT_EVERY 降 5000/槽保全局行率**：扰动已定谳的 10000 档语义——不动；行率债归消费面解耦刀。
- **打印值打本核槽值**（免求和）：smoke-smp awk 严格递增断言在双核交错打印下必红（本核槽值序列会回落）——强制全局和。

## Consequences

- SMP 忙对稳态：每 tick+每 switch×2+每 syscall 的跨核同 cache line RMW 归零（三计数器各占独占行）；IRQ_TICKS 白付 RMW 直接消失。单核路径逐位同旧（slot1 恒 0、和=旧全局值、跨点序列同旧）。
- **等值残边（审查轮勘正定性）**：打印序列由旧「构造性严格递增」降为「非降、严格递增为主态」——对核同窗跨点+PRINT_LOCK 争用下相邻两行可逐位相等（非纯理论：对核持锁读和时本核跨界已落地、获锁后重读可同和）；实践量级远小于 TCG 抖动面且门控相 canary 常缺（已录债），仅 -smp 量产窗暴露。smoke-smp 头部验收条目 3 与 sched.rs 注已同步降格措辞。
- canary 债加深一档（见 Decision 第五条）——消费面解耦刀（0ea3d11ad Consequences 既账）优先级上升。
- HB_LAST/STUCK_LAST 每 tick 双核 swap 留旧行=每 tick 行乒乓残留（诊断面、定谳后随探针退役）——显式挂账出范围。
- 验证实跑（隔离 target-s5c）：cargo build 绿×2；boot 相 smoke 32/32 ×2（单核零漂移）；手动 `-smp 2` 双核 boot：SMP 握手标记齐+ktest done（watchdog 增长断言+G1.5 配平随套过）+零 panic；switches 行 0=符合 canary 债预期形态。smoke-smp 全量验证 deferred 至消费面解耦刀（今日必红在既有债，非本刀罪）。
- 对抗审查（wf_e1f8dfa5-fa8，21 agent 四维+逐 finding）：17 发现（16 CONFIRMED/1 PLAUSIBLE/0 rejected）全处置——两处跨件悬垂先例引用（util.rs:22/syscall.rs:2689 改指 LB_TICKS+退役注）+telemetry.rs「11 位」+smoke-smp 同块 17 处残文（8 续行+8 echo+头部条目 3——值与口径双过时）+OBS-01 调研文档旧口径+API 参考三访问器行补求和快照行为注+main.rs soakmon 措辞；PLAUSIBLE 一条（严格递增降格非降）采纳改注；「总量级不变」注释经计数论证核证正确零改动。
- 沿革五百五十；API 参考 v1.13 三行行为注（计数 182 不变——签名未变零计数面）。

## 重落地附记（2026-11-19）

本刀提交（本笔记「验证」节所指）在合并 1e7872196 中被裁为僵尸态（代码面缺席、无冲突文件存活）。机会三 已按勘察落性表在 icount-baseline 分支重落地（见 2026-11-19 重落地刀笔记）——本笔记现状陈述自重落地起重新为真；旧提交号指向被裁形。