# 21 号：icount 热路径审计——六项对拍与调度改动归因

> 实测日期：2026-11-17（上午窗 08:52-09:40）。环境：Apple M3 Pro / QEMU 11.0 Homebrew TCG；icount 通道单核 `virt`（`-icount shift=1,sleep=off` 定值，[tools/bench.py](../../tools/bench.py):179）；PERF 面单核 `virt` 同款（sched_ci 口径）。内核 = HEAD `35e64d41b` 隔离构建（`CARGO_TARGET_DIR=target-aud21`，08:52）。任务书：SMP 集成验证+性能审计 B 项——①icount 基线复跑六项；②与基线对拍（L2 界）；③调度热路径专项；④结论；审计轮只跑不改性、不刷基线。基线 = [tools/bench_baseline_icount.txt](../../tools/bench_baseline_icount.txt):14-19（2026-11-15 烘，kernel @ `96072f55`，[台账](../../tools/bench_baseline_icount.notes.md):88-99）。审计轮纪律：在场 QEMU 全程在场（gui8-smoke `96298`，icount 通道虚拟时钟免疫宿主竞争，[bench.py](../../tools/bench.py):361-363；PERF 面 TCG 墙钟有噪声面，用四次重复采样定方差，§3.2）。

---

## §0 结论速览（五条）

1. **六项四漂移、全部定态（prove exact）**：现役树单跑 + 五跑证明 verdict=exact max_dev=0.000%（[回执](../../out/bench-receipts/icount-prove-20261117-085731-73025.log)）——{cntvct_pair 0, alloc4k 37, alloc64k 82→**37**, handle_evt 274→**288**, commit1p 1683→**1699**, rw4k 6948→**6945**}。漂移四项按 ±5% 带判：handle_evt +5.11% 出带、alloc64k −54.9% 出带（方向变快）、commit1p +0.95% 与 rw4k −0.04% 带内；按现役 ICOUNT_TOL=0.0 逐值档（[bench.py](../../tools/bench.py):287）四项全红 = 「基线与树脱钩」形态，与 16 号批同族（四项漂移→prove exact→刷基线收口，[台账](../../tools/bench_baseline_icount.notes.md):88-99）。
2. **归因定谳 = 布局骰重掷，非子系统退化**（§2 四重证据）：①机制不可达——bench 六窗前置调度器纪元（[main.rs](../../kernel/src/main.rs):997 的 `bench::run()` 先于 :1104 的 `berry_sched::init()`，TRACE_CAP 结构性零同源 [bench.py](../../tools/bench.py):199-208），F1-F6+EAS wake+band steal+四带+try_lock 行为面在窗内结构性不可达；②路径零改动——`git diff 96072f55..HEAD` 的 handle.rs 仅 1 行注释改名、event.rs/vmo.rs/mm 零改动；③段布局大规模重掷——PT_LOAD 双 ELF 对比：代码段 +108724B / rodata +64376B / BSS +60248B（sched crate 区间 +2148/−297 行 + splash 1394 行 + ktest 扩面）；④方向混合（−54.9/−0.04/+0.95/+5.11）= 台账在册布局族签名（「加码只会变慢」排除 envelope 真开销假设）。
3. **调度热路径不在 bench 六项中（标注）**——专项证据链三面补强（§3）：①BENCHWAKE（icount 通道自带 R-0 观测面）五跑**逐字节相同**（wake 路径确定性无损）；②PERF 面（pick/freeze/billing/wake 对 [tools/sched_baseline.json](../../tools/sched_baseline.json) 对拍，四次采样）：pick 22→24-26cy（+2~+4，26 = [v1.2 计划](../../docs/调度彻底计划-v1.2.md):175 在册 11-13 时点历史值）、freeze 6→5（四次恒 5，改善）、billing 1→4（四次恒 4，代码级归因 = W3-Q76 `charge_step` 外迁后每次 charge 全表快照 16 次 atomic load 的真实增量，设计内代价）、wake p50 63→62-63（带内）；验收 3/3 PASS（416ns/80ns/64ns vs 5µs/5ms/1µs = 12x/62500x/15.6x 余量）；③ktest LOADGEN A1-A6 六档全绿：四带档（A5/A6）interactive p50 350-356µs / p99 356-360µs 与 band on/off 档一致——带执法不劣化 interactive 延迟。
4. **结论：轻微退化（可接受）**——icount 六项无子系统算法退化（路径零改动+机制不可达+布局签名三证）；调度热路径两处真实小增量（pick +2~4cy 特性面扩展、billing +3cy 记账重构）绝对值均在历史波动带内且离 kill 线 >12x；freeze/wake/fairness（spread=0）/divergence（50%）零劣化。无「需优化」级显著退化。
5. **挂账三件归 owner**（§4）：①icount 基线四项漂移待走 prove exact→`--icount --update` 刷基线收口（16 号批同款流程，审计轮不刷）；②billing +3cyc 若在意可做「命中槽短路快照」优化（改代码面，不在本轮）；③handle_evt 165→274（16 号）与本批 274→288 两跳的精确 bisect 仍挂账——本批「路径零改动+段布局重掷」证据强化布局归因，但 bisect 定谳未做（16 号台账 :94 已挂）。

## §1 icount 基线复跑（六项 + prove 五跑）

通道形态：`python3 tools/bench.py --icount`（B-2 定值通道，[bench.py](../../tools/bench.py):32-42——`-icount shift=1,sleep=off` 使 CNTVCT 变指令数代理 ≈0.125 cyc/instr，与 TCG 实时通道跨模式不可比、独立基线文件）。boot→done 标记 42.3-42.4s wall（帽 120s，五跑一致——在场 QEMU 并行下墙钟无劣化）。回执：单跑 [icount-20261117-085343-72503.log](../../out/bench-receipts/icount-20261117-085343-72503.log) + 五跑 [icount-prove-20261117-085731-73025.log](../../out/bench-receipts/icount-prove-20261117-085731-73025.log)。

| 项（iters） | 基线 11-15 | 现役单跑 | Δ | Δ% | ±5% 带 | exact 档 | prove 五跑 |
|---|---|---|---|---|---|---|---|
| cntvct_pair (4096) | 0 | 0 | 0 | 0 | 界内 | 逐值同 | 0,0,0,0,0 |
| frame_alloc4k (1024) | 37 | 37 | 0 | 0 | 界内 | 逐值同 | 37×5 |
| frame_alloc64k (256) | 82 | 37 | −45 | −54.9% | **出带（变快）** | 红 | 37×5 |
| handle_evt (1024) | 274 | 288 | +14 | +5.11% | **出带（变慢）** | 红 | 288×5 |
| vmo_commit1p (256) | 1683 | 1699 | +16 | +0.95% | 界内 | 红 | 1699×5 |
| vmo_rw4k (128) | 6948 | 6945 | −3 | −0.04% | 界内 | 红 | 6945×5 |

L2 界判读：任务口径「±5%」下超界两项 = handle_evt（+5.11%，恰出）与 alloc64k（−54.9%，变快向）；但现役执法形态为 ICOUNT_TOL=0.0 逐值精确相等档（[bench.py](../../tools/bench.py):211-223 注：exact 档在证明得 drift 时可降级 ±5% 档，本次 verdict=exact 属确定性无损的**定态位移**，与「漂移」相对——五跑逐值相同即树内确定）。四项漂移均为跨树布局项（§2），非噪声——prove max_dev=0.000% 封死噪声解释。

BENCHWAKE 观测行（R-0，不进门禁）：`n=516 min=1222 p50=8192 p90=32768 p99=33554432 max=86025788 sig=499 tmo=17`——五跑逐字节相同（[bench.py](../../tools/bench.py) prove 面打印）；对比 11-15 基线台账 n=582 min=1188 … max=107605622 sig=563 tmo=19（[notes.md](../../tools/bench_baseline_icount.notes.md):96）：n −66（boot 期间唤醒总数——boot 指令流/任务集随 splash 与调度改动变化）、tmo 19→17、分布统计整体同量级；观测面数值不进结论判。

## §2 漂移归因：四重证据链

### 2.1 机制不可达（调度改动行为面在 bench 窗内结构性不存在）

[main.rs](../../kernel/src/main.rs):997 `bench::run()` 先于 :1104 `berry_sched::init()`（与 timer init 同段）——bench 六窗全部运行在「ktest 后、timer/sched init 前的干净窗口（单线程 IRQ 屏蔽零调度噪声）」（:994-997 注）。调度器尚未初始化 ⇒ pick/switch/wake/EAS 选核/band steal/try_lock 的执行路径在窗内**结构性不可达**。这与 TRACE_CAP=0.0 的「结构性零」定谳同源（[bench.py](../../tools/bench.py):199-208：bench 六窗前置调度器纪元，遥测钩子窗口内结构性不可能发射）——调度改动对六项的影响只剩**静态足迹**（镜像布局）一条通道，恰是 §2.3 所证。

附带：icount 通道为单核形态（无 `-smp`）——SMP 双核行为面（跨核偷取/park 归因/try_lock 竞争）在六项中双重不可达（纪元前+单核）；SMP 行为面的验证归 A 部分八相 smoke，不在本件。

### 2.2 路径零改动（漂移项子系统算法面排除）

`git diff 96072f55..HEAD`（区间 194 提交，kernel/ 面）：

- [kernel/src/handle.rs](../../kernel/src/handle.rs)：仅 1 行——注释内文档链接改名（`5a83fb21f` 文档批），零代码效应；
- [kernel/src/event.rs](../../kernel/src/event.rs) / [kernel/src/vmo.rs](../../kernel/src/vmo.rs) / `kernel/src/mm/`：零改动；
- [kernel/src/telemetry.rs](../../kernel/src/telemetry.rs)：+106/−127——billing_charge 重构（§3.2，不在 bench 六项窗内）。

⇒ handle_evt / commit1p / rw4k 三个非 buddy 项的漂移不可能是各自子系统路径的真实算法增量；alloc4k/cntvct_pair 逐值不动佐证 buddy 扫描与观测锚无扰动。

### 2.3 段布局大规模重掷（漂移的机制来源）

PT_LOAD 双 ELF 对比（基 = `96072f55` 隔离构建；现役 = `35e64d41b` target-aud21；同链同 bin 数据）：

| LOAD 段 | 基 filesz | 现役 filesz | Δ | memsz Δ |
|---|---|---|---|---|
| 0（RX 头） | 1072 | 1088 | +16 | +16 |
| 2（RX 主代码） | 425112 | 533836 | **+108724** | 同 |
| 3（R rodata） | 395608 | 459984 | **+64376** | 同 |
| 4（RW data+bss） | 39706 | 40544 | +838 | **+60248**（958088→1018336） |

区间增量主体：sched crate +2148/−297 行（F1/F2 lazy 执法+util 冻结、F4 wake 重放+band 偷取、F5 限流让点、F6 SMP 测试相、CP-09 EAS M2+四带全开、CP-14 PI 三链、stats 观测面——`git diff --stat 96072f55..HEAD -- sched/`）+ kernel splash 三模块 1394 行 + main.rs +1541 行 + ktest 扩面。BSS +60248B 与 alloc64k 轨道回摆机制吻合（S-4 4-2 判例：镜像尾新增静态 BSS 挪帧分配器起点 → 64k 连扫边界位形变，[bench.py](../../tools/bench.py):270-277 台账在册；4-3 判例 82→37 恰回 4-2 前值 = 本次 −45 的同族回摆，单页 alloc4k 37 不动佐证纯拓扑项）。

### 2.4 方向混合签名（排除 envelope 真开销）

四项漂移方向 = −54.9 / −0.04 / +0.95 / +5.11（两项变快两项变慢）——加码只会变慢：若是调度改动（或任何算法）真实开销落入窗口，四项应同向变慢且与窗内可达面成比例。台账同族先例：G3-4b 批反向漂移判例（[notes.md](../../tools/bench_baseline_icount.notes.md):54-64）、16 号批 alloc64k 轨道振荡族（:94）。

handle_evt 专项归因：274→288（+14）。机制可达面为零（§2.1）+ 路径零改动（§2.2）⇒ 增量只能是布局项（代码段 +106KiB 引起的指令流位置/对齐变化在 0.125 cyc/instr 量化粒度下的微位移，§A95 族）。与 16 号 165→274（+109，同项挂账 bisect，[notes.md](../../tools/bench_baseline_icount.notes.md):94）构成两跳连续漂移——两跳的区间合量均含海量非调度面（splash/main/ktest），bisect 定谳仍挂账（§4）。

## §3 调度热路径专项

**标注：调度热路径（pick/switch/wake）不在 bench 六项中**——六项为 cntvct_pair/alloc4k/alloc64k/handle_evt/commit1p/rw4k（[bench.rs](../../kernel/src/bench.rs):83-89），无 pick/switch 项；且 bench 六窗前置于调度器纪元（§2.1），调度热路径**结构性**不进 icount 门禁面。以下三面间接证据补强（BENCHWAKE = icount 同通道自带；PERF 面 = ktest 遥测对拍独立基线；LOADGEN = 任务书指定间接数据）。

### 3.1 BENCHWAKE（icount 通道 R-0 观测面——wake→run 直方图）

五跑逐字节相同（§1）——wake 唯一路径确定性无损（同指令流同虚拟时钟下 wake/wake_matching/migrate 链零漂移）。分布统计（p50=8192 p99=33554432）与 11-15 台账同量级，观测面数值不进判（[bench.py](../../tools/bench.py):32-34「观测不动门禁」）。

### 3.2 PERF 面（ktest 遥测 vs tools/sched_baseline.json——四次独立 boot 采样）

遥测源：[kernel/src/ktest.rs](../../kernel/src/ktest.rs):6386-6416（本地 Runqueue 8 任务环 ×1000 pick、freeze/thaw ×100、billing_charge ×1000）+ HDRWAKE（test_hdr_wake 轮询口径）。单核 `virt`、TCG 实时通道（**非 icount**——宿主负载为噪声面，在场 QEMU 全程在场；用四次重复采样定方差）。对拍 [tools/sched_dashboard.py](../../tools/sched_dashboard.py)（判定单源）：

| 指标 | 基线 11-15 | r1/r2/r3/r4 | 判定 | 归因 |
|---|---|---|---|---|
| pick avg cyc | 22 | 26 / 25 / 24 / 26 | 稳定上移 +2~+4 | pick 路径真实特性增量（band 执法/EAS 决策/四带选择器入 pick 面——sched crate +2148 行）；**26 = [v1.2](../../docs/调度彻底计划-v1.2.md):175 在册 11-13 时点历史值**（21-26 历史带内） |
| freeze/thaw avg cyc | 6 | 5 / 5 / 5 / 5 | 恒定改善 −1 | 改善向；四次恒定 = 定态（P3-5 FPSIMD lazy 双路径已含在基线内，此 −1 为区间内增量，未逐行定位〔推断〕） |
| billing avg cyc | 1 | 4 / 4 / 4 / 4 | 恒定 +3 | **代码级定谳**：billing_charge 旧内联线性扫（命中即 fetch_add 返）→ W3-Q76 `charge_step` 纯函数外迁 + 每次 charge 先全表快照（BILLING_SLOTS=8 × 两数组 = 16 次 atomic load，[telemetry.rs](../../kernel/src/telemetry.rs):1041-1080）——并发正确性（CAS 败者收敛循环）+ host 钉板可测性的设计内代价 |
| wake p50/p99 ticks | 63 / 63 | 62/62、62/62、63/63、63/63 | 带内（±1） | 零漂移（噪声带内） |
| fairness spread | 0 [25,25,25,25] | 0 [25,25,25,25] | 稳 | 四带执法不破公平性 |
| divergence rate | 50% | 50% | 稳 | 零漂移 |

验收线（[v1.2](../../docs/调度彻底计划-v1.2.md):175 真机验收线；dashboard 判定）：pick 26cy=416ns < 5µs（**12.0x 余量**）/ freeze 5cy=80ns < 5ms（62500x）/ billing 4cy=64ns < 1µs（**15.6x**）——**3/3 PASS**。

### 3.3 ktest LOADGEN（任务书指定间接数据——六档全绿）

`KTEST LOADGEN A1-A6` 全档跑毕 + `LOADGEN ok`（严格断言内核侧执法过）：

| 档 | slack/band | ia p50 µs | ia p99 µs | ut p50 µs | burst units |
|---|---|---|---|---|---|
| A1 | 0ms / off | 442 | 1855 | 760 | 960 |
| A2 | 0ms / on | 348 | 383 | 39671 | 1739 |
| A3 | 10ms / off | 344 | 350 | 10797 | 1160 |
| A4 | 10ms / on | 346 | 351 | 39666 | 1740 |
| A5 | 0ms / on4 | 350 | 360 | 8 | 480 |
| A6 | 10ms / on4 | 350 | 356 | 39266 | 480 |

要点：四带档（A5/A6，×4 带实例化）interactive p50/p99（350-356/356-360µs）与 band on/off 档（A2-A4）完全一致——**四带执法不劣化 interactive 延迟**；A2/A4 vs A1/A3 显示 band 差异化效果在位（ia p99 1855→383µs）。TCG 分位不进门禁（13 号口径纪律），只取「档间相对趋势 + 全档过断言」。

## §4 结论与挂账

**系统性能结论：轻微退化（可接受）**。分面定谳：

- **icount 六项（boot 纪元子系统面）**：无退化——四项漂移全为布局骰重掷定态位移（§2 四重证据），子系统算法面零增量；按基线更新纪律（[v1.2](../../docs/调度彻底计划-v1.2.md):175「--update 随源不随人」）应刷基线收口。
- **调度热路径**：轻微退化（可接受）——pick +2~4cy（特性面扩展真实增量，历史带内 21-26，离 kill 线 12x）、billing +3cy（重构设计内代价，15.6x）；freeze 改善、wake/fairness/divergence 零劣化；LOADGEN 六档全绿、四带档不劣化 interactive。
- **无显著退化（需优化级）**——无任何指标接近 kill 线；两处小增量均有代码级归因且绝对值可接受。

挂账三件（审计轮不改不刷，归 owner）：

1. **icount 基线刷新**：四项漂移 prove exact 已备（本件回执链）——走 `--icount --update` 烘 {0,37,37,288,1699,6945} 并在 [notes.md](../../tools/bench_baseline_icount.notes.md) 追记条目（16 号批同款流程）；ICOUNT_TOL 维持 0.0（verdict=exact）。
2. **billing 快照短路优化**（若在意 +3cy）：命中槽可先单点探测短路全表快照——改代码面挂账，非本轮。
3. **handle_evt 两跳 bisect**（165→274→288）：16 号挂账未销；本批「路径零改动+段布局重掷」已强化布局归因，但精确 bisect 仍未做（维持挂账，[notes.md](../../tools/bench_baseline_icount.notes.md):94）。

## §5 环境注记与工件指针

- 在场 QEMU：gui8-smoke（pid 96298，EFI 形态）全程在场——icount 通道虚拟时钟免疫（[bench.py](../../tools/bench.py):361-363,388）；PERF 面 TCG 墙钟有噪声面，四次采样 pick ±1 / billing 0 / freeze 0 定方差；守卫走 `--qemu-wait 0` 逃生位（F-C6，[bench.py](../../tools/bench.py):368-420——icount 免疫明文）。
- 工件：icount 单跑 + prove 五跑回执七份（out/bench-receipts/icount-20261117-0853*.log 与 icount-20261117-085*-73025.log 族，含 serial_sha256）；PERF 面四次采样串口 /tmp/aud21-perf*.log（临时件，本件 §3.2 表为转录）；双 ELF 段表对比数据 §2.3（基 ELF 由 /tmp detached worktree `96072f55` + 现役 userbins 隔离构建——bin 与内核同时点耦合：现役 bin 配旧内核 boot 卡死〔串口 0 字节，推断 ABI 演进不兼容〕，故基 ELF 复现基线值的尝试中止，归因改走 §2 证据链）。
- 本件零改动面：只读审计 + 本报告落档；kernel/sched/tools/基线文件零碰；git 零提交。
