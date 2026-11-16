# A 线遗漏审计：调度——承诺 vs 实现逐项对拍

> 版本：v1.0（2026-11-16；文件名按任务书指派不带版本号，版本记于本行）｜性质：全面遗漏审计 A 线——调度线「设计文档（调度器设计 v1.2）＋计划文档（调度彻底计划 v1.2）＋研究底稿（调度深度调研 01-08 号）的承诺/裁决/建议 ↔ 代码实现态」逐项对拍；只查不改，全仓只读
> 锚（必读件全读）：[调度彻底计划 v1.2](../../docs/调度彻底计划-v1.2.md)（CP-01..19 刀表 §4＋三刀快照 §4.6＋五维终判 §8）·[06 号综合定谳](../调度深度调研/06-综合定谳-v1.0.md)（§1.7 逐刀影响＋§2 差距矩阵＋§3 升版建议＋§4 最紧迫三刀＋§5 五维终判）·[调度器设计 v1.2](../../docs/调度器设计-v1.2.md)（§3.8 timer slack＋policy 三元组等承诺）·[调度器 API 参考 v1.7](../../docs/调度器API参考-v1.7.md)；辅证：07 号屏蔽窗表·08 号三刀复核·SMP-01·01-03 号研究件·10/11 号 PM 复核·X6 批供与 FPSIMD 裁决书·系统优化路线图 v1.0·阶段沿革·ADR #42/#68
> 采集窗：2026-11-16；方法 = 三层文档承诺面全量提取 → 代码逐项 grep/直读核对（本稿所有「零命中」均本轮独立复验）→ 两源交叉（文档谱系 × 现役源码/工具/标记面）；行号 = 本轮直读值（「开刀时现场重数」纪律沿用）
> 边界：零 git 操作（禁 git——HEAD 时点归因处引文档自记 git 锚并标「推断」）；只写本件；QEMU 零起（纯静态审计，所有运行态结论引 08 号/沿革在册实录）；本目录为他线并行审计共用，只精确落本件
> 推断声明：§1.5 三处状态判读、§2 两格判词、§6 两处定性为「推断」，均随文标注归 owner；其余结论均有两源

---

## §0 总判（一屏版）

| # | 审计项 | 判 | 一句话 |
|---|---|---|---|
| 1 | CP-01..19 逐刀 | 已落 3（含三刀子件）·骨架在 3·未开 11·阻塞近解除 1·半开 1 | 波一数据面刀（CP-05/06 机制）＋三刀全部落码；验收相（SMPBENCH/POLICY-S/BAND-S）grep 仍零命中；CP-02 阻塞面已被 X-6 条件 GO 裁决实质消化（计划状态列滞后）；CP-08 出现「计划未动 vs 沿革已闭合」两源冲突（§1.5） |
| 2 | 06 号差距矩阵残留 | 落后项 6 维全部未动、零格恶化 | 硬 RT／AI 学习型／能效 EAS／多核拓扑／抢占模型／IPC 预算捐赠六维落后判词全部维持；增量仅三刀（实时/观测两格）与 M4 骨架（AI 格半格松动——声明链闭合、查表消费仍零）（§2） |
| 3 | 设计 v1.2 §3.8 承诺 | 纯面全在、机制面三缺 | timer slack 判定律纯面（slackface 12 测）＋policy 三元组立法在册；机制面三子件全未接——内核定时器消费零命中、bootparams 第三键 `timer_slack` 未落、syscall ABI 面未落；IntentProfile 表仅 host 常量表在、内核运行时加载/更新通道零（§3） |
| 4 | API 参考 v1.7 | 3/4 在册、1 漏登 | class_fair_intent/FairBand::new/band_of 三件 v1.7 在册；oneshot.rs（第 13 文件、tighten_deadline 1 fn，lib.rs:53 已 `pub use`）未随迁——「sched/ 12 文件/148 pub fn」口径对现树 13 文件/149 fn 漂移（§4） |
| 5 | KC-P1..P5 证据状态 | 立法在册·底座半在·零样本未开庭 | 五条判据 ADR #68 在册；测量底座 WAKE_RING/per-domain CPU 账/policy gen 世代/遥测双环在位，AI 票本体与 berry-power 记账闭环（DVFS 档位×时长）未建——五条全部零样本，与 CP-15 未动自洽（§5） |
| 6 | 审计新发现 | 漂移 3 件＋超预期 2 件 | ①CP-02 阻塞态滞后于 X-6 裁决书（条件 GO 2026-11-14）；②CP-08 两源状态冲突（术语半边归属未划清）；③API 参考漏登 oneshot；超预期＝svcmgr intent 透传（声明链闭合）＋M4 带参数骨架全三件（工厂/表/读出口）（§1.5/§6） |

---

## §1 任务 1：CP-01..CP-19 逐刀对拍

状态口径：已落＝机制进树且有独立验证；骨架在＝数据结构/工厂在而执法或消费未接；未开＝代码零命中；阻塞＝外部裁决门。对照基线＝计划 v1.2 状态列（2026-11-15，[计划](../../docs/调度彻底计划-v1.2.md):87-121）。

### 1.1 波一（CP-01..CP-08）

| 刀 | 计划 v1.2 状态 | 本轮实测 | 判 | 证据（两源） |
|---|---|---|---|---|
| CP-01 文档重锚（七处漂移） | 未动 | 七处逐项复验全部仍在：①[指南](../../docs/策略服务用户指南-v1.0.md):41「v1 无 handle 门控」原文在；②:111「placement 暂无消费」原文在（与[设计 v1.2](../../docs/调度器设计-v1.2.md):342-348「placement 第一消费者已落地」直接矛盾）；③[AI 调度设计](../../docs/AI调度设计-v1.0.md):42 仍写共享环形缓冲（裁十债二直传终裁未改口）；⑤[sched/src/util.rs](../../sched/src/util.rs):121 `em_max_capacity(0)` 域 0 硬编码在码；⑦[README.md](../../README.md):5/:23「AI-native by construction」在；④SLEEPQ 口径差与⑥R3 随迁未见消费文档改指 | **未开**（且清单应扩容——§1.5 新发现 3 件） | 计划:93 漂移清单 × 本轮逐条 grep 直读 |
| CP-02 SMPD-0 基线钉板＋X-6 收口 | 未动；「唯一硬阻塞 = P0-2/P1-2 fix-or-withdraw 仓主裁决」 | 阻塞面实质消化：[X6 裁决书](../../docs/X6批供与FPSIMD裁决书-v1.0.md):4（2026-11-14）裁 P0-2+P1-2 **条件 GO**（d0105a060）；[系统优化路线图](../../docs/系统优化路线图-v1.0.md):74-83/:116-124 两项均「已落树·条件 GO 已裁」，soak 死锁已修复（51s/14000 切换零挂死，[沿革](../../docs/阶段沿革.md):6071-6078 一百零一轮），余 DPGF n=15＋DPGF2 n=3 完整确认待 QEMU 空窗（:6080-6089 一百零二轮）；smoke-smp 八相 PASS 复现无在树记录 | **阻塞近解除**（推断：条件 GO＋死锁修复实证是否构成 §6.1 触发器 1 的「终裁出果」归 owner 拍板；DPGF 完整确认是尾款） | 路线图 §1/§2 × 沿革一百零一/一百零二轮（文档两源；未动 git） |
| CP-03 SMPD-1 SMPBENCH | 未动；host 面可预研 | 代码 grep `SMPBENCH` 全仓零命中（本轮复验）；[tools/smoke-smp.sh](../../tools/smoke-smp.sh):2-8 八相清单无该相；host 预研面 hdrface 数学在（[API 参考](../../docs/调度器API参考-v1.7.md) §五 re-export 族） | **未开** | 计划:95 × markers/smoke-smp 直读 |
| CP-04 SMPD-2 POLICY-S | 铁门后；shadow 验证环前置子项已落 | 前置子项确认在树：[sched/src/policy.rs](../../sched/src/policy.rs):32/:515-528 shadow 侧 `SHD_GATE` 验证环＋三槽，判定律单源 [polface/src/lib.rs](../../polface/src/lib.rs):365-447 `HazardGate`（reader_enter 验证环/pick_write 避让扫描），host 18 测绿（[08 号](../调度深度调研/08-三刀复核.md):76）；`POLICY-S` 相本体 grep 零命中 | **前置已落·相本体未开** | 计划:96 × 08 号复核实录 |
| CP-05 SMPD-3 band 原子账 | 已落码；余 BAND-S＋lazy pick | 机制在树（[sched/src/band.rs](../../sched/src/band.rs) `CONSUMED`/`DEADLINES` 全局原子账）；余项注仍在码：:62「SMP lazy-pick 执法刀（SMP §3）再升 AtomicBool」、:199「超卖窗断言归 BAND-S 相，本刀不执法」；`BAND-S` grep 零命中 | **已落码·余验收未开**（维持） | 计划:97 × band.rs 直读 |
| CP-06 SMPD-4 divergence 分账＋A/B 裁决 | 已落码；余 A/B 形态裁决＋首份报告 | per-CPU 分账在（policy.rs `DIVERGENCE_PER_CPU`，[设计 v1.2](../../docs/调度器设计-v1.2.md):301 节注）；首份 SMP A/B 对照报告全仓无件（research/docs 零命中 A/B 报告文件）；判官工具 [tools/policy_ab_eval.py](../../tools/policy_ab_eval.py) 在（阈值代理面） | **已落码·裁决未开**（维持） | 计划:98 × 工具/文档检索 |
| CP-07 SMPD-5 公平性验收＝MS-1 | 未动 | soak-smp 通道在（[tools/soak-smp.sh](../../tools/soak-smp.sh)）；EEVDF lag 跨核有界断言/吞吐对称比/CI 议程零代码零文档记录 | **未开** | 计划:99 × 检索 |
| CP-08 util→PM 共享环 ABI | 未动（PM 线刀 2 外部依赖） | **两源冲突**：[沿革](../../docs/阶段沿革.md):6270 三百三十七轮记「PM 预拉（CP-08 闭合）」——svcmgr 预拉段 QEMU「PM 三证齐」（双 eco up＋dry-run），[11 号复核](../调度彻底计划/11-深水区复核.md):3/:9 只验此半边、[10 号](../调度彻底计划/10-大窗轮复核.md):10 验装配半边；但真通道未接——[berry-power main.rs](../../user/bins/berry-power/src/main.rs):172-175 自注「自检 dry-run 后 park 常驻、不写任何帽/票/发布位」，内核→berry-power util 信号通道零代码 | **半开**（推断：沿革/复核件所用「CP-08」指装配＋预拉半边，计划 CP-08 验收线「berry-power 消费 util 信号＋零频率写点断言＋PM 崩溃保频」未动——术语半边归属需 CP-01 重锚时划清；C 线审计同判「闭 4.5/开 8.5」，[C 线报告](./C-定制化与PM线遗漏审计.md) §0-4） | 沿革 337 × berry-power 源码自注 |

### 1.2 波二（CP-09..CP-14）

| 刀 | 计划 v1.2 状态 | 本轮实测 | 判 | 证据 |
|---|---|---|---|---|
| CP-09 EAS 选核 compute_energy/FEuC | 未动（形状在算法留） | [sched/src/em.rs](../../sched/src/em.rs):8 自注「算法（compute_energy/FEuC 选核公式）不落仓库——留 M2/真机」；overutilized 域 0 硬编码未去（util.rs:121，06 号 §1.5-1 修订项未落） | **未开**（维持） | 计划:106 × em.rs/util.rs 直读 |
| CP-10 PM governor M3 | 未动（berry-power 本体未建） | berry-power 仍骨架 park 形态（main.rs:172-175）；「PM 服务据负载改频」零兑现 | **未开**（维持） | 计划:107 × 源码自注 |
| CP-11 timer slack＋IntentProfile 表 | 未动；slackface 纯面 12 测已备 | 机制面三子件全未接：kernel/sched `slack` grep 零命中（本轮复验）；[kernel/src/bootparams.rs](../../kernel/src/bootparams.rs):237/:243 词表仅 `sched_slice_hz`/`intent_default` 两键、第三键 `timer_slack` 未落；syscall ABI 面零 | **未开**（纯面在——详 §3） | 设计 v1.2:156 × bootparams/内核直读 |
| CP-12 band 执法并线 M4 ×4 带 | 未动；入场券判定刀前置 | 执法未并线（band.rs 默认全关立法不变）；**骨架超预期**：FairBand 带实例＋`class_fair_intent` 工厂＋IntentBand 表＋`band_of` 读出口四件全在（[sched/src/class.rs](../../sched/src/class.rs):468/:499、[iprofile/src/lib.rs](../../iprofile/src/lib.rs):351/:390/:490、policy.rs band_of），但 spawn 侧接线点预留未接（API 参考 v1.7:270 自注「M4 集成面预留窗」）；入场券判定刀零动作 | **骨架在·执法未开**（计划状态列「未动」指并线本体，准确但未记骨架增量） | 计划:109 × class.rs/iprofile 直读 |
| CP-13 (B) 分类器 advisor v3 | 未动 | 五件套零落：telemetry_open 0x84 全仓零命中（本轮复验）；per-thread 键/band 写路立法/Job 门控/传输表述改口（AI 调度设计:42 仍共享环）均未动；[tools/advisor.py](../../tools/advisor.py) 为 v1 整表出工具 | **未开**（维持） | 06 号 §1.7-CP13 × 检索 |
| CP-14 admission 补写＋SET_POLICY＋stats | 未动 | admission ΣU 在码（[kernel/src/syscall.rs](../../kernel/src/syscall.rs):2409 Q12 定点）但**不含 band 预算项**（仅 deadline 任务利用率）；SET_POLICY Job 门控未收紧（指南:41 仍「v1 无门控」即证）；stats 查询面/berryctl --json 未做（[berryctl main.rs](../../user/bins/berryctl/src/main.rs):2 自注「--json 留 v2」） | **未开**（维持） | 计划:111 × syscall.rs/berryctl 直读 |

### 1.3 波三（CP-15..CP-19）

| 刀 | 计划 v1.2 状态 | 本轮实测 | 判 | 证据 |
|---|---|---|---|---|
| CP-15 AI kill criteria 复核 | 未动（挂 CP-06 首份 A/B＋M4 窗） | 判据/口径预立在册（计划:117 能耗半句「未测不记无收益」）；首份 A/B 报告零样本（CP-06 裁决未开）；KC-P1..P5 证据态详 §5 | **未开**（判据在册·零样本） | 计划:117 × §5 本轮盘点 |
| CP-16 学习策略 v4+ | 未动（CP-15 门后） | user/bins 无 AI 服务件（本轮 ls）；影子轨道入场四步机制在（policy.rs PROMOTE/DEMOTE） | **未开**（维持） | 计划:118 × 目录检索 |
| CP-17 v2 RT 谱 | 未动；三证据子件已兑现 | 三子件确认在树：FPSIMD lazy（[arch/aarch64/src/task.rs](../../arch/aarch64/src/task.rs):79 `fp_needed` 出生定档）；忙核 oneshot（[sched/src/oneshot.rs](../../sched/src/oneshot.rs):32 收紧律＋sched.rs:2504/:3421 两消费臂，R-1 δ 塌缩 16.5-33.1× 与 `RT ok n=96` 断言进树 [kernel/src/main.rs](../../kernel/src/main.rs):4334）；屏蔽窗全表（[07 号](../调度深度调研/07-syscall屏蔽窗表.md) 42 号 icount 双跑 exact＋R3 双态）；尾刀本体（cyclictest 级验收/静态池/PI 形 C）未立项 | **子件三落·尾刀未开**（维持计划口径） | 计划:119/:151-155 × 07 号/08 号/源码 |
| CP-18 真机证据面 MS-3 | 未动（SoC 待拍板） | 零动作；真机 EM/OPP/功耗计/分位门禁全部未触 | **未开**（维持） | 计划:120 × 检索 |
| CP-19 4 核＋与拓扑演进 | 未动 | 拓扑零字段增量：[sched/src/runqueue.rs](../../sched/src/runqueue.rs):456-462 `Topo` 仍仅 `nr_cpus`＋`capacity[2]`（定长 2 硬编码维持）；llc_mask/reserved 核位无；[sched/src/spin.rs](../../sched/src/spin.rs) 无 64B 对齐（SpinLock 伪共享审计低垂子件未动，06 号 §1.7 新增件） | **未开**（维持） | 计划:121 × runqueue.rs/spin.rs 直读 |

### 1.4 06 号 §1.7 新增刀/子件四件对拍

| 件 | 挂点 | 状态 | 证据 |
|---|---|---|---|
| shadow 验证环 | CP-04 子项升格 | **已落**（3cf462，08 号复核三连跑 50 万+读零撕裂、负对照撕裂实证载荷灵敏度） | 计划:153 × 08 号:76 |
| 忙核 arm-to-nearest-deadline | CP-17 前置 | **已落**（oneshot.rs＋两臂＋R-1 塌缩；sched 域文件在途归 owner 收笔——08 号:76 归属清单，本轮见 lib.rs:25/:53 已并） | 计划:154 × oneshot.rs/sched.rs |
| syscall 屏蔽窗全表 | CP-17 子件 | **已落**（07 号 42 号表双跑 exact；kernel 探针测毕还原零残留——08 号:76 实核） | 计划:155 × 07 号 |
| SpinLock 伪共享对齐审计 | CP-19 低垂子件 | **未动**（spin.rs 无对齐面） | 06 号:79 × spin.rs 直读 |

### 1.5 与计划 v1.2 状态列的偏差——新发现漂移三件（CP-01 清单应 7→10）

1. **CP-02 阻塞态滞后**：计划:94 写「唯一硬阻塞 = P0-2/P1-2 fix-or-withdraw 仓主裁决（R'≥18 才保 ②① 绑定）」，但本轮全仓 grep `R'≥18` 零命中且路线图:74-83/:116-124 已是「已落树·条件 GO 已裁 2026-11-14（裁决书 d0105a060）」——裁决已出、死锁修复已实证（沿革一百零一轮），阻塞语义从「等仓主裁决」变为「等 DPGF n=15＋DPGF2 n=3 完整确认（QEMU 空窗）＋smoke-smp 八相复现」。计划 §6.1 触发器 1（X-6 终裁出果→CP-02 解锁）已处于半触发态（推断：条件 GO 是否即「终裁出果」归 owner；若算，波一重排应启动）。
2. **CP-08 状态两源冲突**：计划:100「未动」vs 沿革 337/10 号/11 号「CP-08 闭合（装配半边＋预拉半边）」。实质 = 服务常驻化落地而 util 通道本体（计划验收线三件）零代码。两文件各说半边，属典型「文档落后于代码＋术语半边未划清」型漂移（推断：以计划验收线为准 CP-08 应记「装配+预拉已落·通道未接」，归 CP-01 重锚）。
3. **API 参考 v1.7 漏登 oneshot**：详见 §4——计划:93 CP-01 验收线含「API 参考与 sched/src 对拍零漂」，本轮实测该对拍已破（12 文件/148 fn 口径 vs 现树 13 文件/149 fn），CP-01 清单应增第八处。

---

## §2 任务 2：06 号差距矩阵残留项（落后格逐项复核）

基线＝[06 号 §2.1](../调度深度调研/06-综合定谳-v1.0.md):89-101（计划 §8.1:237-249 为其收编版）。本轮逐格复核：**无一格恶化、无一落后格转追平；增量集中在实时/观测两格（三刀）与 AI 格半格松动**。

| 落后格（06 号判词） | 本轮实测 | 残留判 |
|---|---|---|
| 硬 RT 落后 1 代（vs Linux PREEMPT_RT；CP-17） | 三子件落（FPSIMD/oneshot/屏蔽窗表——§1.3），四缺件去二维持；余 PI 形 C 无界＋多核跨核上界未测，两件零动作 | **维持落后 1 代**（证据面前进、代差不变） |
| AI/学习型落后 1 代（vs LAVD/ADPF；CP-12/13） | 查表硬消费仍零：`latency_nice`/`uclamp`/`timer_slack_ns` 列 kernel/sched 消费 grep 零命中，[sched/src/intent.rs](../../sched/src/intent.rs):5 自注「无调度消费点」；增量＝声明链闭合（svcmgr manifest intent 透传，沿革 6422-6425 二百六十八轮——计划 §8.1 该格未记） | **维持落后 1 代**（声明链≠查表消费，半格松动为推断判读） |
| 能效 EAS/EM 落后 1 代（CP-09） | compute_energy 零（em.rs:8）；overutilized 域 0 硬编码维持（util.rs:121）；EM cost 派生列/artificial EM（06 号 §1.7-CP09 修订项）未落 | **维持落后 1 代** |
| 多核拓扑感知落后 2 项内（CP-19） | Topo 两字段维持（runqueue.rs:456-462）；llc_mask/reserved 核位零；「落后但时机未到」（真机未至）判语维持 | **维持** |
| 抢占模型落后 1 代（P-C 归 v2） | 检查点模型内部有增量（g3-1d 真探形 `preempt_checkpoint`/命中计数，[API 参考](../../docs/调度器API参考-v1.7.md):29-33 在册）但任意点抢占（P-C）零动作 | **维持落后 1 代**（增量不越代） |
| IPC×调度集成落后 1 代（vs seL4 SC 捐赠） | MsgBilling/per-domain CPU 账已落（[设计 v1.2](../../docs/调度器设计-v1.2.md):257 M1.1-4 行）；对象化预算捐赠/消费量查询零 | **维持落后 1 代** |
| EEVDF 缝一（deferred dequeue/睡眠 lag 衰减） | 设计文档无条款（grep 零命中）；裁十「立法占位随 CP-09/CP-12 前裁」未动 | **维持未裁**（06 号 §3.2-1 措辞降级建议也在「未随 v1.2 收编」清单，计划:317） |
| 06 号 §3 升版余项（挂下版） | 逐项零动作：CP-05/12 bypass 继承条款、CP-13 五件、CP-14 PI G 卡收编、CP-17 pyRTA 远期候选、裁二/裁九依据增补、K13 单列、OQ-8 增补（计划:317 自记） | **全部未动**（与计划自记一致，非新遗漏） |

---

## §3 任务 3：设计 v1.2 §3.8 承诺 vs 代码

§3.8 承诺拆成第一段（v1/v2 查表键）六件承诺逐项对拍（[设计 v1.2](../../docs/调度器设计-v1.2.md):97-156）：

| 承诺 | 实现态 | 证据 |
|---|---|---|
| timer 原语 slack＋policy 三元组「v1 day-one」（:156） | **纯面在、机制面三缺**：判定律四件（六档 slack 域/聚合分桶/三律/policy 三元组）＝[berry-slackface](../../user/berry-slackface/src/lib.rs) host 纯函数 12 测（本轮 `#[test]` 计数复验）；机制面——内核定时器路径消费（kernel/sched `slack` 零命中）、bootparams 第三键（bootparams.rs 仅两键）、syscall ABI 面（零）全未接；slackface 头注 :13 自认「全部归后续接线刀」 | 设计:156 × slackface 直读 |
| policy 三元组第三键 | **立法在册、键未落地**：三元组形 = `(slice, intent_default, timer_slack)`（slackface:58/:304 `PolicyTriple`＋:319 `policy_triple_valid` 值域执法＋:332 `resolve_slack`）；前两键已在 bootparams 词表（:237/:243），**第三键 `timer_slack` 未进词表**（bootparams parse 零分支）——即三元组三缺一 | slackface:291-332 × bootparams.rs:237-243 |
| TCB intent 五字段（:106-116） | **已落**（task.rs 字段面＋4-3c 传播；[API 参考](../../docs/调度器API参考-v1.7.md):509 五字段在册） | API 参考 §六 × 设计 |
| IntentProfile 表「启动配置加载＋运行时更新＋保守回退」（:118） | **半落**：表本体＝iprofile host 常量表（手表表 [iprofile/src/lib.rs](../../iprofile/src/lib.rs):105 起＋PC 表注释位）；**内核运行时加载/更新/回退通道零**（kernel/sched 对 IntentProfile 的调度消费零命中，仅 class.rs 经 `band_of_intent` 消费 M4 带表——不同表） | 设计:118 × iprofile/内核 grep |
| deadline/RT 任务也查表（EPP/uclamp 列）（:132） | **未落**（uclamp 全仓调度消费零命中） | 设计:132 × grep |
| 入口治理（pledge 只降不升，:154）＋遥测落盘（:155） | 遥测半落（intent 档位直方/通胀计数 `intent_settle_current_ext`＋berry_itally 单源，W3-Q6 在册）；pledge 单调收敛执法零（intent.rs 无 pledge 面——设计自注「入口治理随声明入口 syscall 落地」，该 syscall 未建） | 设计:154-155 × intent.rs/API 参考:126 |
| 第二段 M4 骨架（:134-140） | **超预期**：设计只写「凭入场券」，实际 FairBand/class_fair_intent 工厂/IntentBand 六行缺省表/band_of 读出口四件已进树（§1.2-CP12）——文档没写的骨架增量；执法/入场券/带间选择器如册未动 | class.rs:465-508 × iprofile:329-490 |

§3.8 之外顺带核到两条相关承诺：设计 §4.1 绝对截止期重编程纪律在码（idle/park `arm_at`＋oneshot 两臂，sched.rs:2504/:3421）；§5.4 PM 双服务骨架现状注记与代码自洽（berry-power park 常驻，本轮复验一致）。

---

## §4 任务 4：调度器 API 参考 v1.7 在册对拍

口径：API 参考对象 = `sched/` crate（[v1.7 头](../../docs/调度器API参考-v1.7.md):3 自记「12 文件/148 pub fn」＋:12 维护合同「改任何 pub fn 签名或增删 → 同步本文并升版本」）。

| 新增 API | 在册否 | 证据 |
|---|---|---|
| `class_fair_intent` 工厂 | **在册**（v1.7 升版条目：class.rs 2→4 fn，:269-270 条目全文） | API 参考:269 |
| `FairBand::new`（IntentBand 形参） | **在册**（:266；`IntentBand` 类型以形参出现，其本体表 `band_of_intent` 归 iprofile crate 不在本文对象域） | API 参考:266 × class.rs:481 |
| `policy::band_of` | **在册**（:315-316，v1.7 同批） | API 参考:315 |
| shadow `HazardGate` | **不在册——域外**：polface crate（判定律单源 [polface/src/lib.rs](../../polface/src/lib.rs):365-447），本文对象仅 sched/；在册家 = 计划 §4.6 刀①（:153）与 polface 模块头注——非漂移，但 sched 侧消费者（policy.rs:60 import）若视为 sched API 面则无条目（判：合规缺位，无成文 API 参考覆盖 face 族） | polface:365 × API 参考:3 对象域 |
| oneshot `tighten_deadline` | **漏登**：sched/src/oneshot.rs:32 为 pub fn 且 [lib.rs](../../sched/src/lib.rs):53 `pub use` 直达 crate 根（正是本文:9 所列收录形态），v1.7（2026-11-15 升于 01cd881b）早于该文件并树（沿革 6415 二百六十七轮物理在其后）——现树 13 文件/149 pub fn vs 头注 12/148，**维护合同已破**（CP-01 验收线「对拍零漂」同破） | oneshot.rs:32 × lib.rs:53 × API 参考:3/:624 |

结论：四问三在册一漏登；漏登件（oneshot）应随 v1.8 收录（sched.rs 80 不变、oneshot +1、合计 148→149、文件 12→13）。

---

## §5 任务 5：KC-P1..P5 证据状态更新

KC-P1..P5 = ADR #68 电源 AI 票 kill criteria 五条（[架构决策](../../docs/架构决策.md):76；单源判据表 [PM-09](../PM-09-AI与省电-v1.0.md):153-157），CP-15 以 KC-P2 的 95% CI 口径为对齐锚（计划:117）。本轮逐条盘点证据底座：

| # | 判据（ADR #68） | 判据载体（PM-09） | 本轮实测 | 证据态 |
|---|---|---|---|---|
| KC-P1 | 预测命中率打不过 persistence 基线→票下线 | WAKE_RING | WAKE_RING 在码（[kernel/src/telemetry.rs](../../kernel/src/telemetry.rs):283-351 per-CPU 环）；预测器/票本体零 | 底座在·零样本 |
| KC-P2 | 固定基线 A/B 能耗收益 95% CI 下界<3%→永久 hint-only | berry-power 记账面（DVFS 档位×时长） | berry-power 骨架 dry-run 后 park（main.rs:172-175），档位×时长记账闭环零；功耗计归 CP-18 未至——与 CP-15「能耗半句真机前原理性不可测」预立口径（计划:117）一致 | 底座缺·原理性不可测 |
| KC-P3 | 开票组 vs 关票组 p99 恶化超阈→下线回滚 | 遥测环＋smoke 断言面 | 遥测双环在（R-0 64 桶＋HDR 897 桶）；smoke SKT 断言面在（[markers.sh](../../tools/markers.sh):24 `SKT ok n=3`）；开/关票对照组零 | 底座在·零样本 |
| KC-P4 | 推理自耗+自身唤醒增量>毛收益 1/3→平台分层收缩 | 记账面＋WAKE_RING（AI 服务自身进程账） | per-domain/per-process CPU 账在（M1.1-4 MsgBilling，设计:257）；AI 服务进程零（无票） | 底座在·零样本 |
| KC-P5 | 票龄>2 慢循环即弃＋命中率衰减→自动回退 | 策略表世代＋命中率统计落盘 | policy gen 世代在码（policy.rs gen/shadow_gen＋陈旧拒收）；**命中率统计落盘零** | 半底座·零样本 |

总判：**立法在册、底座半在、五条全部零样本未开庭**——与 06 号 §3.4「kill criteria 判官工具齐、判据数字零样本」判定零变化；本轮唯一增量是 berry-power 从「rootfs 未拉起」变「svcmgr 预拉＋dry-run＋park」（沿革 337），使 KC-P2 载体从「无实体」变「有壳无账」。执法时点不变（CP-15 开庭前置 = CP-06 首份 A/B＋CP-13 前，开放问题 3 仍归 owner）。

---

## §6 超预期与遗漏总账

**超预期（实现了文档没写/写少的）两件**：
1. **Intent M4 骨架三件全进树**：设计 §3.8 第二段只承诺「凭入场券」，实际 FairBand＋class_fair_intent 工厂＋IntentBand 六行缺省表（含值域执法 band_row_valid/band_table_valid）＋band_of 读出口已落并注册 API v1.7（class.rs:465-508/iprofile:329-490）；行为零漂移设计（trait 全方法委托现役 Fair）守住「骨架不实装」纪律——计划 CP-12 状态列「未动」未记此增量。
2. **svcmgr intent 透传（声明链闭合）**：manifest 声明→svcmgr 读取→透传→内核 spawn 链路闭合（沿革二百六十八轮），intent 从「设计位」到第一个活消费者；但带选择面（class_fair_intent 接线）仍留 owner 窗——差距矩阵 AI 格的「消费零」口径需随 CP-01 重锚改「查表消费零·声明链已闭合」（推断：声明链是否计入「消费面」归 owner）。

**遗漏（文档承诺悬空且无刀收口的）三件**：
1. 设计 §3.8「deadline/RT 任务也查此表（EPP/uclamp 列）」——uclamp 零消费且 CP-11 刀面未含此子件（CP-11 只覆盖 slack＋三元组＋手表表落表）；
2. 设计 §3.8「IntentProfile 表运行时更新＋保守回退通道」——CP-11 验收线只写「表运行时更新与保守回退通道」有覆盖，但机制面现状（内核零加载）未在任何状态列出现，属「承诺-刀单-现状」三点断链；
3. `Task.obligations` 位图＋CMP-1..4 host property（裁十债三）——调点 v1.1:601 自记「挂 1.2」后无下文，sched 源码零命中，开放问题 10 的「清偿或降级 ADR 追记」未动。

**对五维终判的增量判读（推断）**：本轮无机制面新刀（三刀已在计划 v1.2 计分），机制 9.0/策略 5.0/观测 7.5/测试 8.0/文档 6.5 五分维持；文档维有恶化压力——CP-01 七处漂移全在且新增三处（§1.5），若下轮复评应压至 6.0 档（归 owner 校准）。

---

## 来源索引

文档谱系（本轮全读/逐行抽核）：
- [调度彻底计划 v1.2](../../docs/调度彻底计划-v1.2.md)：:87-121（刀表）/:93（CP-01 七处）/:117（CP-15/KC-P2 对齐）/:147-157（§4.6 三刀）/:229-265（§8 差距矩阵与五维）/:317（未收编余项）
- [06 号综合定谳](../调度深度调研/06-综合定谳-v1.0.md)：:57-79（§1.7 逐刀影响）/:89-111（§2 矩阵＋领先项）/:115-136（§3 升版）/:140-153（§4 三刀）/:157-169（§5 五维）
- [调度器设计 v1.2](../../docs/调度器设计-v1.2.md)：:97-156（§3.8）/:156（timer slack 注记）/:257（M1.1-4 记账）/:301（SMPD-4 节注）/:342-348（placement 第一消费者）
- [调度器 API 参考 v1.7](../../docs/调度器API参考-v1.7.md)：:3（148/12 口径）/:12（维护合同）/:266-270/:315-316/:624
- [07 号 syscall 屏蔽窗表](../调度深度调研/07-syscall屏蔽窗表.md)（42 号表＋R3 双态）；[08 号三刀复核](../调度深度调研/08-三刀复核.md):76（门禁与归属实录）；[SMP-01](../调度深度调研/SMP-01-多核调度面全面审计-2026-11.md)；[01 号收编](../调度彻底计划/01-调度现状全景与在途线收编.md)
- [系统优化路线图 v1.0](../../docs/系统优化路线图-v1.0.md):74-83/:116-124（X-6 条件 GO）；[X6 批供与 FPSIMD 裁决书 v1.0](../../docs/X6批供与FPSIMD裁决书-v1.0.md):4；[阶段沿革](../../docs/阶段沿革.md):6043-6089（99/101/102 轮）/:6270-6282（337/338 轮）/:6415-6425（267/268 轮）
- [10 号大窗轮复核](../调度彻底计划/10-大窗轮复核.md):10/:58（CP-08 装配半边）；[11 号深水区复核](../调度彻底计划/11-深水区复核.md):3/:9（预拉半边）；[架构决策](../../docs/架构决策.md):76（ADR #68/KC-P1..P5）；[PM-09](../PM-09-AI与省电-v1.0.md):153-157（KC-P 单源表）；[调点调度 v1.1](../../docs/调点调度-v1.1.md):601（obligations 挂账）
- 漂移件直读：[策略服务用户指南](../../docs/策略服务用户指南-v1.0.md):41/:111；[AI 调度设计](../../docs/AI调度设计-v1.0.md):42；[README.md](../../README.md):5/:23

代码与工具（本轮直读/grep）：
- sched：[oneshot.rs](../../sched/src/oneshot.rs):32、[lib.rs](../../sched/src/lib.rs):25/:53、[policy.rs](../../sched/src/policy.rs):32/:515-528、[class.rs](../../sched/src/class.rs):465-508、[band.rs](../../sched/src/band.rs):62/:199、[util.rs](../../sched/src/util.rs):121、[em.rs](../../sched/src/em.rs):8、[intent.rs](../../sched/src/intent.rs):5、[runqueue.rs](../../sched/src/runqueue.rs):456-462、[spin.rs](../../sched/src/spin.rs)
- face 族：[polface/src/lib.rs](../../polface/src/lib.rs):365-447（HazardGate）；[iprofile/src/lib.rs](../../iprofile/src/lib.rs):71/:105/:351/:390/:490；[berry-slackface/src/lib.rs](../../user/berry-slackface/src/lib.rs):58/:291-332（12 测）
- kernel/user/tools：[bootparams.rs](../../kernel/src/bootparams.rs):237/:243；[telemetry.rs](../../kernel/src/telemetry.rs):283-351（WAKE_RING）；[main.rs](../../kernel/src/main.rs):4334（RT ok）；[syscall.rs](../../kernel/src/syscall.rs):2409（admission）；[task.rs](../../arch/aarch64/src/task.rs):79（fp_needed）；[berry-power main.rs](../../user/bins/berry-power/src/main.rs):172-175；[berryctl main.rs](../../user/bins/berryctl/src/main.rs):2；[tools/sched_baseline.json](../../tools/sched_baseline.json)（pick=22 复读）；[tools/markers.sh](../../tools/markers.sh):16/:24；[tools/smoke-smp.sh](../../tools/smoke-smp.sh):2-8；[tools/policy_ab_eval.py](../../tools/policy_ab_eval.py)；[tools/advisor.py](../../tools/advisor.py)

本件推断声明：§1.5 三处（CP-02 半触发/CP-08 半开/API 漏登归 CP-01）、§2 AI 格半格松动、§6 超预期定性/五维 6.0 压力档——均标「推断」归 owner；其余结论文档×代码两源。
