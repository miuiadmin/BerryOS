# icount 基线两项：switch_pair 门控 block/wake 对与 pick_scan 纯函数面

Status: implemented

## Problem

[04 号调度热路径审计](../../../research/内核深度进化/04-调度热路径-Top5优化机会-v1.0.md)「基线缺口与建议」节钦定行动项（「随任一刀补」）——遗漏检查轮（wf_5a6238b8-a36）CONFIRMED MAJOR：五刀皆未偿、零记录缓办，调度刀收益判定全程停留在结构论证（笔记自述），审计许诺的确定性收益通道未建立。两路勘察（wf_3781dc11-8e6）：

- **前提红线（设计路头号发现）**：审计字面「两任务 yield 往返」结构性不可行——yield_now 先 pick 后抬值 + ringface lag-优先出选律（vr 落后让出者重选自己=no-op）+ 成切后抬全环 vruntime max（裸 boot 环含矩阵任务 icount 量级追赶不可行）。改形定案=门控 block/wake 往返对（真正任务对切换先例=lat IPC 路）。
- 基建面：BENCH 行唯一发射源=bench.rs `bench<F>` 骨架；icount 跑法=`-icount shift=1,sleep=off`（0.125 cyc/instr 量化）；确定性纪律=`--prove` 五跑逐值相同 + verdict exact 支撑 ICOUNT_TOL=0.0。

## Decision

- **pick_scan**（bench.rs 前置干净窗，与原六项同窗）：本地 Runqueue+20 枚合成 unit 带任务（ktest POLICY setup 先例：Box::leak+enqueue 正路；vr=i/vd=i+1000 定值铺展；一枚 Running 作 my_cur 吃 is_cur 分支；resync_vruntime_sum 洗账），1024 次直调 `Fair::pick_next_filtered`（生产面，含 note_pick 计数与游标推进），出选指针累计+尾断言防死存储消除。环长 20=审计附记二常态值（128 是 RING_WALK_CAP 帽非生产形；混合带变体挂账）。纯函数面=相位无关 icount 下布局不敏感，最干净 L2 行。
- **switch_pair**（新 `bench::run_sched_epoch()`，插窗 berry_sched::init↔telemetry::init——唯一「调度器在+TRACE_CAP=0.0 结构性零（钩子未注册）+boot DAIF 屏蔽窗」槽位）：fw_cfg 键 `opt/berry/sbench` 门控（pilat 同款值面不解析；noktest 整窗跳）；spawn A(prio 8)+B(prio 0)，boot 手递首切用 `yield_now_keep_irq`（plain yield 出口解蔽破窗；boot 自 block 则 IDLE 幽灵在队不可收场——两备选均被 impl 轮否证的结构性理由），A 体 `bench(b"switch_pair",128,||{wake(B);block(A)})`、B 对称（每次迭代=一完整任务对切换环；block/wake 直调无 klock 包络——klock_exit 解蔽破窗，全纪元结构性 PSTATE.I=1）；裸键=零大小静态地址（dpg FAULT_KEY 先例），位 1<<53 入 USER_SIGNAL_BITS 登记；测毕 A/B exit_current 自灭环空回 boot 续行。N=128 非 100 倍数+纪元总切换 ≈259≪SOAK_PRINT_EVERY=10000（金丝雀打印不落窗）。口径陷阱记名=切换热路径**下界形**（无 SVC 往返〔归 B-1〕/无 TTBR0-ASID churn〔同 AS 跳过〕/FPSIMD 全量照付）。
- **trailer 迁移**：`bench::run()` 不再打 BENCH ok；`run_sched_epoch()` 纪元尾恒打 `BENCH ok n=8`（门缺席仅缺 switch_pair 行、trailer 照发——noktest 时整窗跳=生产 boot 与旧形逐位一致）。N_METRICS 6→8。
- **G1.5 配对修**（impl 轮识别、主线落）：`note_switch` 的 SWITCHES_TT 计数移入钩子在位分支——sbench 纪元使「hook 缺席期也计」从防御支变可达（≈259 次入分母环零记录，watchdog 断言在 --icount 形态确定性红）；两计数同窗语义不变，hook 缺席期分母零计=断言面单调更强。
- **bench.py**：`_iter_table` 增两行（1024/128）；`run_icount` 默认恒递 sbench 键（基线面恒八行）；**trace-dual 两臂 sbench=False 显式禁键**（TRACE_CAP 证书面保持前调度器纪元七项——互锁红线记名）；run_once/run_icount 的 len==done_n 对账改交集语义（缺席项恰为门控集才放行——B-0/trace-dual 面 7 行+n=8 过、错名缺席照红，八案例仿真）；六项/六行注释族滚八。
- **注册面**：markers.sh MARKERS 与 smoke-image.sh KERNEL_MARKERS 两表字面 n=6→n=8（逐字节一致机器验证）+smoke.sh 注释；docs/基准与压力测试设计 v1.0→v1.1（八项表+调度器纪元窗段+yield 改形定谳记名+L0/L1/L2 口径）；规则/07 与测试策略两处域外字面随滚（遗漏检查处置纪律）。
- **BCTS 冻结件**：bcts@0..3 的 `BENCH ok n=6` 失配——按判例滚 bcts@4（提交期立件）。

## Alternatives considered

- **审计字面「yield 往返」**：结构性不可行（见 Problem 前提红线）——改形并定谳记名入设计标准。
- **BENCHFA 式独立前缀**：零扰动六行契约但不入 L2 门（违审计原意）——弃。
- **boot 手递备选**（plain yield/boot 自 block）：出口解蔽破窗/IDLE 幽灵在队——keep_irq 形定案。
- **pick_scan 测 trait 面**：审计标尺对象是生产面——pick_next_filtered 定案。
- **回溯 A/B 对拍已落五刀**（checkout 父提交跑两探针）：约一小时工作量换台账数字——归 owner 裁（挂账）。

## Consequences

- 调度刀收益判定通道建立：机会一②（探峰圈消减）/机会三（per-CPU 分槽）/机会五①（快照锁对消减）的后续刀有了确定性标尺；switch_pair/pick_scan 基线值由主线 prove 轮定档（见下）。
- 旧六项基线值因镜像布局骰重掷预期漂移（buddy 族三项轨道对族）——归因条目入 bench_baseline_icount.notes.md（史档纪律）、`--icount --update`+`--trace-dual --update` 双份重烘。
- 验证：cargo build 绿；bench.py --selftest 63/63（含 bench.rs↔_iter_table 八项活树镜像）；手动 icount 诊断单靴证全链健康（ktest done+BENCH 行 8/8 含两新项+trailer n=8+用户态正常推进）；prove 五跑受并行 lane QEMU 争用拉长宿主墙钟超 BOOT_TIMEOUT_ICOUNT=120 帽（工具立法帽不放宽——静场重跑定档）；boot smoke 待 prove 后收口（n=8 标记）。
- 对抗审查：见审查节（待补）。
- 沿革五百五十五；BCTS@4 立件。
