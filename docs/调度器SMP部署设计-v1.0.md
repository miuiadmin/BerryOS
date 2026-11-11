# 调度器 SMP 部署设计 1.0 预规格（调点调度 v1.4 · SMP 部署激活）· 骨架

> 版本：**1.0**（v1.0）｜2026-11-10｜文件名 `调度器SMP部署设计-v1.0.md`
> 【状态】**预规格骨架**——v1.4 周期开刀蓝图：SMP 安全基础设施（三件套）已
> 建、多核部署未激活，本文管「从基础设施到可验收部署」的刀序与技术口径。
> 【定位】调点调度 v1.4 = SMP 部署激活——承接 1.3 收官（SMP 三件套 + EAS
> 四件 + HDR 全落，皆单核等价验证）之后的多核兑现周期：per-CPU runqueue
> 稳定运行钉板、跨核策略一致性验证、SMP 基准与公平性验收、部署位激活。
> 【接口起点】`sched/src/policy.rs`（三槽 RCU + gen + hazard-lite +
> POLICY_LOCK 三件套 + shadow divergence 对账）+ `sched/src/sched.rs`
> （`PER_CPU_RQ:[Runqueue;2]` + place 路由 + 偷取）+ `sched/src/band.rs`
> （BandBudget 8 槽，S6-k 单核形态）+ `sched/src/hdr.rs`（per-CPU CpuHdr
> 双路 + 跨核合并）+ `tools/smoke-smp.sh`（八相线，`-smp 2` 全仓唯一入口）。
> 【证据】`调点调度-v1.3.md` §4–§5（三件套已落 + 未竟清单）；`SMP设计草案
> -v1.0.md` 及 S-2..S-5 子案（E0→E1 纪元 + 执行结果区）；`阶段沿革.md`
> S-6 定谳条（deadline 跑步机/arm_slice 单调夹取）+ 一百三十~一百三十二
> （三件套三刀）+ 一百七十（X-6 刀① SMP soak 楔死在飞）+ 一百七十二
> （smoke-smp 实跑钉板挂账）；ADR #28/#42/#58。
> 【读法】每节 3–5 bullet——现状=事实锚点、刀序=里程碑+出口门、设计=机制
> 口径、验收=硬门、风险=kill 线；已裁机制只做版本化引用，不重开裁决。

---

## 1. 现状分析

- **已有·SMP 三件套全件（策略面）**：policy.rs ①SHD_WHERE AtomicU8（写侧
  槽写→fence(Release)→store(Release) 发布序 / 读侧 Acquire——见新槽号必
  见新槽内容）②hazard-lite（HAZARD:[AtomicU32;2] 每核发布在读槽号 +
  active_snap() 验证环读者协议 + publish() 扫描避开 active∪HAZARD——单核
  HAZARD[1] 恒哨兵）③POLICY_LOCK CAS 写者锁（bind/bind_gen/bind_gen_
  shadow/promote/demote 五入口全串行，错误路径同解锁防泄漏）——沿革一百
  三十~一百三十二；单核等价三跑全绿（33/47/30 标记）在册。
- **已有·per-CPU runqueue 结构与 LB 底座（机制面）**：S-3b 落
  `PER_CPU_RQ:[Runqueue;2]` + place 直路路由（spread 切分：服务根恒家核、
  计算线程 spread=true 直路 rq1）+ 跨核偷取（try_steal_one 只偷 spread=
  true，判据「本核近空闲 len≤1 且对核显著更重」）；S-6 三修（arm_slice
  单调夹取防 deadline 跑步机 / 显式可迁移面 / spawn 先簿记后入环）后矩阵
  对称 3370:3375≈1:1（修前 22:1）+ smoke-smp 八相 PASS（2026-11-07 定谳
  条 + 一百一十六合入复验）。
- **已有·SMP 底座配套**：锁序立法（SQ>队列>REAPQ>表>分配器>ASID>打印，
  三镜确认无环）+ svc 分类表锁序（§A112 ABBA 预拦截）+ SGI 面（SGI_
  RESCHEDULE 收端使能在册；生产发送面归零——发送条件 = 目标核队列真有
  货，随路由化成立回归）+ HDR per-CPU 双路（CpuHdr 64B 对齐 + 跨核合并
  桶位直和）+ util per-CPU EWMA；G2（双核正确性）/G3（全抢占）关卡已过。
- **缺失·per-CPU runqueue 稳定运行未钉板**：八相 PASS 是 2026-11-07 时点
  快照；此后 v1.3 九刀 + X-5/X-6/AM-2 系列合入后 smoke-smp 未复验（沿革
  一百七十二挂账「smoke-smp 实跑钉板」），且 X-6 刀① 0x24 表批在 SMP
  soak 下楔死（rq 邻域锁自旋风暴，一百七十在飞——fix-or-withdraw 已裁）
  ——多核长跑稳定性现无在册绿灯。
- **缺失·三面空白**：①跨核策略一致性验证未动（换表等价断言金丝雀 + 各核
  见表 gen 漂移合法窗观测——调点 1.3 §4 未落件；SHD-S/BAND-S 相未扩，
  现行验收 = 单核 SHD-K 六段/DV-K 五段零回归）②SMP 基准缺席（跨核唤醒
  延迟未建——test_hdr_wake 是方案 A 忙等单核口径，方案 B 真 sleep/wake
  留 boot 刀；policy_ab_eval 阈值未随真 SMP 噪声底复测；shadow A/B
  harness SMP 形态未裁未建）③band 仍 S6-k 单核形态（BandBudget 8 槽跨核
  原子账 + lazy pick 未升，超卖界/翻期竞态检测法未定）——部署线隔离
  （`-smp 2` 只进 smoke-smp 线、主 smoke/CI 恒单核）是设计内状态，激活
  缺的是上述验收面而非入口本身。

## 2. 部署步骤（刀序）

- **SMPD-0 基线钉板 + 已知楔死收口（部署前置，零新功能）**：最新 main 树
  smoke-smp 八相实跑钉板（清偿一百七十二挂账）+ X-6 刀① rq 邻域锁楔死
  debug 收口（fix-or-withdraw 裁决兑现——疑凶 0x24 批面长 KL 窗 ×
  exit/reap 锁序）；出口 = 八相 PASS 复现 + ①形二进制 soak 楔死归零或撤
  刀 + 单核主 smoke 零漂移随跑——本刀不绿，后续刀不开。
- **SMPD-1 跨核唤醒延迟基准（测量先行，出噪声底）**：test_hdr_wake 方案
  B（boot 序真 sleep/wake）扩跨核形态——唤醒路由到 idle/park 副核（SGI
  戳 + park 环醒）全链 record_wake_lat，merged_percentile 全链出数；
  出口 = `SMPBENCH ok` 相进 smoke-smp + 首份跨核/单核 p50·p99 对拍（同机
  相对比值口径——板间/跨机不可比铁律）；policy_ab_eval 阈值复测以此为底。
- **SMPD-2 跨核 policy 一致性（gen 对齐验证）**：换表窗金丝雀相——LB 载荷
  下连续 bind_gen 换表，断言换表窗零丢片/零双跑（两核 pick 同 task 互斥
  由 hazard-lite publish 扫描 + RCU 读窗保证——验证而非新机制）+ 各核读
  gen 漂移合法窗观测行；出口 = `POLICY-S ok` 相（判据先 QEMU 趋势口径，
  真机分位口径随 RM 线）+ 既有 LB 相零回归。
- **SMPD-3 band 全局原子账**：BandBudget 跨核一份原子计数（翻期 CAS 单点
  + 消费 fetch_add）+ lazy pick 执法（选核时读账、不即时回冲——热路径零
  新锁）+ 超卖界/翻期竞态检测（超卖窗上界 = 一 slice 内每核各一份，界本
  身进断言）；出口 = `BAND-S ok` 相 + SHD-K/DV-K 既有段零回归 + deadline
  豁免/intent 即带键语义零漂移。
- **SMPD-4 shadow divergence 跨核合账 + A/B 形态裁决**：对账计数 per-CPU
  分账（写方限本核零争用——HDR CpuHdr 同款纪律）+ 读时合并（Σ恒立对拍、
  divergence_rate 分子分母同源合并）；A/B harness SMP 形态按 SMPD-1 噪声
  底裁（按核分 c0=基线 c1=策略 vs 时间片轮转——观察者效应小者优先）；
  出口 = divergence 合并零丢对拍 + 首份 SMP A/B 对照报告（v1.2 遗留第二
  项 SMP 形态清偿）。
- **SMPD-5 公平性验收 + 部署激活收口**：LB 相扩公平性口径（跨核吞吐对称
  比延续 S-6 矩阵 1:1 形态 + EEVDF lag 有界跨核成立）+ soak-smp ≥30min
  双核满载常驻绿（S-5d 纪律②形态）+ `-smp 2` 验收位进 CI 议程（挂 CI 随
  成本议——smoke-x86 先例）；出口 = 公平性报告 + v1.4 收官回填调点系列。

## 3. 技术设计

- **per-CPU runqueue 操作（steal/wake 路由）**：wake 路由优先「归属核直
  路」——Task 侧 spread/pinned 既有字段定形（服务根/svcmgr/pager 恒家核
  防协议断供，计算线程 spread=true 直路 rq1 + SGI 醒 park 环，发送条件 =
  目标核队列真有货）；steal 维持 S-6 收敛判据（本核 len≤1 且对核显著更重
  才 try_steal_one、只偷 spread=true）；SLEEPQ 归属核单点武装（插入核
  arm_at 单点武装 CVAL，迟醒界 ≤1 slice——规则③④）；不变量：偷取与直路
  同格合格（路由形态与断言解耦，子案 §9.6 裁决二）。
- **跨核 policy 表一致性（gen 对齐）**：全局单表 + ACTIVE AtomicU32 全局
  发布即现状形态（gen 全局单调天然成立——GEN_NEXT 锁内 fetch_add；per-CPU
  读者快照未做，2 核档远程读可受、4 核+ 复议）；一致性验证面 = 换表窗
  金丝雀（每核 pick 持快照指针、窗内不换 = RCU 读临界区；换表窗零丢片/
  零双跑由 hazard-lite 回收协议承保——本刀验证协议、不长第二套发布机制）
  + gen 漂移合法窗（各核 self_gen_now() 读值差 ≤1 发布代——起步做观测面
  非断言面，门禁化随真机线）。
- **SMP band 计账（全局原子 fetch_add）**：BandBudget 8 槽单核形态升跨核
  一份原子计数——翻期（周期翻卷+预算重充）CAS 单点、消费 fetch_add
  （Relaxed——纯计数不通信；翻期判定点 Acquire 配对见新代必见新账）；
  执法改 lazy pick（选核路径读账判超额，不即时回冲——唤醒/记账路径零新
  锁零新原子；超卖窗上界 = 一 slice 内每核各跑一份，进 BAND-S 断言）；
  band 预算 AI 不可触（全局原子账单点执法）与 deadline 豁免语义不变。
- **跨核 shadow divergence（per-CPU 计数合并）**：现状 DIVERGENCE_COUNT/
  SHADOW_EVAL_COUNT 已 AtomicU64 fetch_add——全局单 cache line 是两核
  apply 热路径的争用面；升 per-CPU 分账（[AtomicU64;2] 每核写本核槽，
  64B 对齐防伪共享——HDR CpuHdr 同款）+ 遥测读合并求和（divergence 率
  分子分母同源合并，口径零漂）；对账语义不动（apply 每调用 shadow 非空
  即对同一 task_key 双轨对拍、0 键丢弃同边界）；轨道切换清零（promote/
  demote 的 divergence_reset）改两槽同批置零——POLICY_LOCK 窗内原子，
  对账期语义（切换 = 新对账期）不变。

## 4. 验收标准

- **smoke-smp 全相通过**：既有八相零漂移（相 1 裸相 32 标记 + 相 2 rootfs
  51 + LB c0=0/c1≥24/n=4 + ASIDR n≥1 + TLBS n=10 + ALLOC n=320 + HT
  n=320 + PP a=b=320）+ 本周期新增相（POLICY-S/BAND-S/SMPBENCH）全绿；
  每刀单核主 smoke 八跑零漂移是硬门（SMP 刀不扰单核——调点 1.3 §12 出口
  门继承）。
- **SMP 基准（跨核唤醒延迟）**：SMPBENCH 相出 p50/p99 跨核 vs 单核对拍
  首份基线（HDR merged_percentile 全链；同机相对比值口径——绝对数板间
  不可比）；policy_ab_eval 判定阈值（<0.1/>0.5）随噪声底复测——定档、
  修订或维持三择一回填。
- **公平性验证（多核负载均衡）**：LB 相扩对称比断言（S-6 矩阵 1:1 形态
  延续，任核占比区间阈值随首跑定标——LB_TICK_TH 同法）+ EEVDF lag 有界
  跨核成立（per-thread lag 排序不因核数发散）+ soak-smp ≥30min 双核满载
  零断言失败 + 金丝雀跨核续爬。
- **回滚与退化底线**：任一刀红即回退该刀（每刀独立提交、独立可验证——
  小步纪律）；单核退化形态构造性成立（band/divergence 的 per-CPU 面 [0]
  槽单写 = 现状等价；三件套单核零行为变化先例同族）。

## 5. 风险与缓解

- **X-6 刀① rq 邻域锁楔死（在飞，SMPD-0 硬前置）**：多核部署的一切验收
  建立在 soak 不楔之上；缓解 = fix-or-withdraw 已裁（R'≥18 才保 ②① 绑
  定），SMPD-0 不收口不开后续刀；若 debug 长期不出果，v1.4 以「②锚窗
  泊车形」先行部署、①形退出 SMP 面（kill criteria 移植）。
- **TCG 双核非确定性**：跨核时序断言全状态谓词化（smoke-smp 既有纪律——
  TCG 双核线程交错比单核更差）；soak 失败须三连复现才立案（复现不了记
  frequency 账）；跨核基准只取相对比值，绝对数不进门禁。
- **全局表跨核瓶颈与锁序破坏**：POLICY_LOCK 锁序「表/KL 档禁嵌 RQ/
  SLEEPQ」立法在册——新增消费点（band 读账/divergence 合账）一律表侧
  只读、不得在 RQ/SLEEPQ 窗内做表写；写者锁 4 核+ 反噬 → kill criteria
  = SMP 线冻结 2 核档并文档化（调点 1.3 §14 继承，v2 复议 per-CPU）。
- **观察者效应多核放大**：shadow A/B 按核分形态有核间干扰新面（c0/c1
  载荷不对称污染对照）；缓解 = SMPD-4 形态裁决以 SMPD-1 噪声底为输入、
  时间片轮转备选；divergence 对账 per-CPU 化后 apply 热路径零新增争用
  （写本核槽——与 record_wake_lat 同族）。
- **band 打不过扁平（EAS 版同型风险）**：全局原子账 + lazy pick 若在
  多核载荷下无收益或负收益（超卖窗吃掉预算精度），降级回单核形态并文档
  化（接口保留、执法回即时）；判官 = SMP A/B 对照报告（SMPD-4 产出），
  与 EAS 线「placement 打不过零假设」kill criteria 同框架。

## 6. 参考

- 内部：`调点调度-v1.3.md` §4–§5/§12/§14（三件套 + 未竟 + 刀序 + kill）；
  `SMP设计草案-v1.0.md` + `SMP-S2/S3/S4/S5实施设计草案-v1.0.md`（E0→E1
  纪元与执行结果区）；`调度器设计-v1.1.md` §1/文末策略服务五节；`阶段
  沿革.md` S-6 定谳条 + 一百三十~一百三十二 + 一百七十/一百七十二；ADR
  #21/#28/#42/#58；`策略服务用户指南-v1.0.md`（六 op 与工具链）。
- 外部：Linux EAS/PELT（overutilized 与负载信号形状，只借形状）；cyclictest
  （唤醒延迟口径）；HDR histogram（Gil Tene 口径，hdr.rs 已落同源）。
