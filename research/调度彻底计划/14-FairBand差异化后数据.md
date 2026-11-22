# 14 号：FairBand 差异化后数据——R5 全面复验（boot+smoke+loadgen 双面）

> 实测日期：2026-11-16（上午窗）。环境：Apple M3 Pro / QEMU 11.0.3 Homebrew TCG / 单核 `virt`（与 13 号/AB-01 同机同版本）；宿主 load ≈7-10（四并行 QEMU 在飞，rep 离散带按此语境读）。构建态：内核 = HEAD `e648147a7`（第五轮终验：FairBand 四 API 行为差异化）+ 在飞注释勘正（kernel/src/main.rs 文档注释两面，零行为——boot banner `e648147a7-dirty` 自证）；隔离 `CARGO_TARGET_DIR=target-r5v`。任务：R5 落地后的全面 QEMU 验证——①SMOKE_PHASES=boot 冒烟（改动不破 boot）；②loadgen 四组复跑对比 R4 基线（ktest 机械面基线 = [AB-01](../调度深度调研/AB-01-CP06首份AB数据-ktest负载生成器-v1.0.md) 两跑；真系统面基线 = [13 号](13-首份AB报告-loadgen.md) §2.1 四臂×3 reps 中位数）；③band on/off 差值（delta）表。口径纪律同 13 号：QEMU/TCG 只报功能、顺序与相对趋势，绝对分位不进门禁（[调度彻底计划](../../docs/调度彻底计划-v1.2.md):20 §1 原则⑦）；推断一律显式标注。原始日志：/tmp/lg-logs-r5/（12 探针 boot + 冒烟副本）+ /tmp/lg-r5-driver.out。

---

## §0 结论速览（六条）

1. **boot+smoke 绿**：`SMOKE_PHASES=boot SMOKE_KEEP_LOG=1 timeout 150 just smoke` rc=0、boot 标记 32/32 到齐、帽内收（150s 未触）；ktest 段全绿含 `ktest done`——FairBand 行为差异化改动不破 boot。
2. **ktest LOADGEN 四组零回归（含 A2/A4）**：四组全指标与 AB-01 R4 基线吻合——ut_p50 逐位级（A1 763 vs 758/763、A3 10803 vs 10776/10835、A2 768 vs 766/748、A4 10810 vs 10804/10621）、burst 逐位级（960/1689/1156/1693 vs 963/1688/1153/1687）、thr 11↔11、wall 483/667/533/668 vs 484-492/667-672/534-538/668——ia 族在 R4 自身两跑漂移带内（AB-01 §3.5 已记 A1 ia p50 820↔1064 为 TCG 噪声面）。
3. **A2/A4「数据同 R4」的正确判读 = 零回归，非「行为差异未生效」**：该面在 R4 基线期即由 harness 手驱 raw 四 API 执法（AB-01 的 thr=11 / ut_p99 3.2× 正是它——[kernel/src/ktest.rs](../../kernel/src/ktest.rs):9429-9439 consume/throttle_all 手驱点）；R5 对该面零代码路径改动（`git show e648147a7 -- kernel/src/ktest.rs` 仅两个 hunk 均在 test_band_m4；`-- sched/src/band.rs` 仅注释；驱动记账仍 unit `Fair.tick`〔ktest.rs:9427〕）——复现即正确。R5 行为差异化在同一 boot 的证据面 = **BANDM4G 新断言**（§2.3）。
4. **真系统面 band 轴 R4 null → R5 翻转（主判据）**：utility p99 的 g2−g1 差从 R4 的 −10.6ms（null，13 号 §0-2）翻为 **+75.1ms**（151852 vs 76734µs 中位数，2.0×）；g4−g3 同向 +46.4ms。ut_p50 不动（g2−g1 −0.9ms）=「包络执法是尾事件不是中位事件」——与 ktest A2 面同形状（AB-01 §3.3），两面对应互证。
5. **slack 轴保真 + 交互零牺牲保真**：g4−g2 ut_p50 = +26.9ms（R4 +25.9ms——30ms 声明顺延在 R5 内核上分毫重建）；interactive p50 四臂 15.3-15.7ms 平坦带（R4 13.6-17.2ms 同判）。
6. **「interactive p50/p99 应有变化」在探针形态下不成立且机理清楚**：探针 `set_slice_hz(100)`（[kernel/src/main.rs](../../kernel/src/main.rs):4497/:803）→ 全局 slice 10ms == interactive 带 quantum 10ms（[berry-iprofile 表行 2](../../iprofile/src/lib.rs):412-419）——interactive 授片无差异可表达；quantum 差异的直接证据在 BANDM4G（exh_ia=10 vs exh_ut=0——interactive 短片燃尽、utility 长片不燃）。ia_p99 g2−g1 方向向下（−20.3ms）但 3-rep 带宽交叠（48.7-98.1 vs 62.9-161.6ms），只记方向不作判据（13 号 §2.1 同口径：p99 个值不作判据）。

## §1 方法与复现配方

- **冒烟**：`CARGO_TARGET_DIR=target-r5v just build`（5.1s 增量）→ `SMOKE_PHASES=boot SMOKE_KEEP_LOG=1 timeout 150 just smoke`——boot 相单 QEMU（smoke.sh :476 形态），32 标记到齐即收（smoke.sh :518 分相门）。
- **ktest 面**：同一冒烟 boot 的 ktest 段（`berry run`/smoke 路不带 noktest——tools/berry/src/qemu.rs:324-325 口径），`KTEST LOADGEN A1..A4` + `cmp` 五行原样收割（ktest.rs:9601-9658）。
- **真系统探针**：13 号同配方单源——QEMU boot 相形态 + `-fw_cfg name=opt/berry/loadgen,string=s=<ns>;b=<0|1>` + `-fw_cfg name=opt/berry/demo-face,string=off`；四臂（g1 s=0 b=0 / g2 s=0 b=1 / g3 s=30ms b=0 / g4 s=30ms b=1）×3 reps，**rep-major 交错**（g1..g4 × r1..r3——均衡宿主负载漂移，与 R4 的臂主序异）；早收锚 = 探针专属 `LG bu units=` 行（ktest 面的 `LOADGEN ok` 在 ktest 段早出，不可作锚——v1 实踩）；SIGTERM 收（R4 同款）。µs = 周期 × 16/1000（62.5MHz）。
- **基线**：ktest 面 = AB-01 §2.1 两跑（out/logs/loadgen-ab-1794772056.log + 复跑）；真系统面 = 13 号 §2.1 三 rep 中位数（原始 /tmp/lg-logs/g*-r*.log 12 份在席本轮复验）。

## §2 数据

### 2.1 boot+smoke（门面）

`smoke: boot 标记 32/32 到齐`、rc=0、150s 帽内；串口日志存 /tmp/lg-logs-r5/r5-smoke-boot.log（原件 berryos-smoke.QUVXrJ）。ktest 段收 `BANDM4G diff`、`BANDM4 ok n=2`、`KTEST LOADGEN` 四组、`LOADGEN ok`、`ktest done`；spawn 面 `[band]` 八行在席（lgIA0-3 q_ns=10000000 w=256..1024 / lgUT0-3 q_ns=20000000 w=64..512——分派面与 R4 逐字同形）。

### 2.2 ktest LOADGEN 四组 vs AB-01 基线（µs；R4 = 跑1/跑2，R5 = 本次单跑）

| 组 | 轴 | ia p50 | ia p99 | ut p50 | ut p99 | burst | thr | wall ms |
|---|---|---|---|---|---|---|---|---|
| A1 R4 | band=off | 820/1064 | 2470/2078 | 758/763 | 11530/1263 | 963/960 | 0 | 492/484 |
| A1 R5 | band=off | 462 | 1604 | 763 | 1049 | 960 | 0 | 483 |
| A2 R4 | band=on | 345/346 | 1268/394 | 766/748 | 37538/37758 | 1688/1682 | 11 | 672/667 |
| A2 R5 | band=on | 378 | 1108 | 768 | 37164 | 1689 | 11 | 667 |
| A3 R4 | slack band=off | 350/405 | 1636/1599 | 10776/10835 | 11341/11286 | 1153/1156 | 0 | 538/534 |
| A3 R5 | slack band=off | 344 | 370 | 10803 | 10811 | 1156 | 0 | 533 |
| A4 R4 | 双开 | 412/347 | 1434/410 | 10804/10621 | 36480/36314 | 1687/1687 | 11 | 668/668 |
| A4 R5 | 双开 | 346 | 352 | 10810 | 36418 | 1693 | 11 | 668 |

对账行（ranIA/ranUT/ranBK ms，R5）：A1=2/240/240、A2=2/240/422、A3=2/240/289、A4=2/240/423（R4 跑1：2/244/243、2/240/424、2/244/288、2/240/421）——负载形状与再分配（utility 被限流让出的 CPU 喂给 burst）同形复现。判读：**四组零回归**——ut_p50/burst/wall 逐位级，ia 族与 ut_p99（A1）在 R4 自身漂移带内；band=on 组（A2/A4）与 R4 同值是§0-3 的预期态（该面代码路径 R5 零改动）。

### 2.3 BANDM4G：R5 行为差异化的 ktest 证据面（同 boot 新断言，全绿）

`BANDM4G diff exh=10/0 flips=29/4 cross=0/5`（[kernel/src/ktest.rs](../../kernel/src/ktest.rs):7830；段体 :7588-7845）：G1 双带同环 20 轮 ×12ms 跑段（介于两带 quantum 10/20ms 之间）——**exh_ia=10/exh_ut=0**（interactive 短片每轮燃尽、utility 长片每派发重授恒有余量 = quantum 差异真消费，断言 exh_ia≥3/exh_ut==0）；**flips_ia=29/flips_ut=4**（翻期频次比 8ms:50ms 期长 ≥3×——latency_target 差异真消费）；**cross_ia=0/cross_ut=5**（interactive 配额 10ms/8ms 期 >100% 不可越线、utility 双带交替烧穿 20ms/50ms 配额——quota 差异真消费）；G2 utility 单飞限流→`HELD_THROTTLE` 置位 + VD 重臂权重整数精确等于 low_weight（64）/翻期解限等于 high_weight（512）——**权重包络差异真消费**；G3 unit 授片 = 全局 slice（零漂移钉板）。此面 = R4 语义审计链 1「行为差异=0」的对位正控制，本轮 boot 一次过。

### 2.4 真系统四臂 vs 13 号 §2.1（µs，3-rep 中位数；Δ = R5−R4）

| 指标 | g1 off/off R4→R5 (Δ) | g2 off/on R4→R5 (Δ) | g3 30ms/off R4→R5 (Δ) | g4 30ms/on R4→R5 (Δ) |
|---|---|---|---|---|
| interactive p50 | 17248→15326 (−1922) | 16676→15713 (−963) | 16950→15689 (−1261) | 13594→15344 (+1750) |
| interactive p99 | 60362→77530 (+17168) | 66628→57273 (−9355) | 56030→50842 (−5188) | 81205→76882 (−4323) |
| utility p50 | 18490→18181 (−309) | 18596→17314 (−1282) | 16256→18209 (+1953) | 44524→44236 (−288) |
| utility p99 | 77013→76734 (−279) | 66452→**151852 (+85400)** | 86541→78710 (−7831) | 116561→125158 (+8597) |
| burst units/s | 121→126 (+5) | 124→116 (−8) | 122→119 (−3) | 123→127 (+4) |
| span ms | 2214→2218 (+4) | 2168→2142 (−26) | 2119→2194 (+75) | 2305→2377 (+72) |

**band on/off 差值（delta，组内同 run 对比——判别主面）**：

| 差值 | R4 | R5 | 判 |
|---|---|---|---|
| ut_p99 g2−g1 | −10561（null） | **+75118**（151852 vs 76734） | band 轴翻转（2.0×） |
| ut_p99 g4−g3 | +30020 | **+46448**（125158 vs 78710） | 同向放大 |
| ut_p50 g2−g1 | +106 | −867 | 不动（包络=尾事件） |
| ia_p50 g2−g1 | −572 | +387 | 不动（四臂平坦带） |
| ia_p99 g2−g1 | +6266 | −20257 | 方向向下、带宽交叠不作判据 |
| burst g2−g1 | +3 | −10 | 方向不一致不判（噪声） |
| **slack 对照** ut_p50 g4−g2 | **+25928** | **+26922** | 30ms 顺延保真（±1ms） |

R5 rep 带（µs）：g1 ut_p99 [69553,181917]、g2 ut_p99 [65068,166958]、g3 ut_p99 [59716,309164]、g4 ut_p50 [42830,45124]（紧带——slack+band 复合信号最稳）、ia_p50 全臂 [8988,21481]。p99 带 2× 同 R4 口径（13 号 §5-1），判据立在中位数与形状（p50 平/p99 抬），不立个值。

## §3 分析

1. **band 轴翻转的机理链（【推】，真机相可分解）**：R5 起生产热路径真消费带参数——八授片点 `class_of_task().grant_cycles()`（[sched/src/sched.rs](../../sched/src/sched.rs):1410/:1593/:2632/:2984/:3767/:3859/:4078/:4116——utility 10→20ms 长片）+ 生产 tick class 分派（sched.rs:3630-3631 → [FairBand::tick](../../sched/src/class.rs):737-759：片耗尽 VD 重臂走带 quantum×带权重、自翻期、consume→throttle_all）+ tick 入口带翻期扫（sched.rs:3577 → class.rs:891）。utility 尾延迟抬升三候选：20ms 长片使唤醒 utility 等待窗变长 / 权重包络 64..512 的 VD 重臂改变 EEVDF 出选序 / 偶发带限流（20ms/50ms 配额 vs 组占有 26.7% 理论不越，但记账相位可偶越——p50 不动与偶发不矛盾）。探针无 per-band 限流计数遥测，三候选未分解——真机相加计数即分。
2. **interactive p50 不动的两层机理**：①探针形态下 interactive quantum（10ms）== 全局 slice（10ms@slice_hz 100）——授片零差异；②interactive 权重重臂只改竞争序，探针里 interactive 恒最高档出选——p50 平坦是语义正确非失效；quantum 差异在 BANDM4G 的受控形状（12ms 跑段介于 10/20ms 之间）直接断言（exh 10/0）。
3. **两面对应（互证）**：ktest A2 面（配额 20ms/60ms 手驱）与真系统 g2 面（表行 20ms/50ms 懒武装）同现「ut_p50 平 + ut_p99 抬」形状——包络执法的尾事件语义跨面稳定；R4 时真系统面 null 而手驱面有差（执法未并线），R5 后两面皆差（执法并线）——「机制就绪、执法未并线」的证据对（13 号 §3.2）完成合拢。
4. **slack/交互零牺牲穿过 R5 无损**：g4−g2 +26.9ms（R4 +25.9ms）、四臂 ia_p50 平坦——FairBand 行为差异化不吞噬 CP-11 已兑现的语义（回归正面证据）。

## §4 预期对账（任务书判据逐条）

| 任务书预期 | 实测 | 判 |
|---|---|---|
| boot+smoke 不破（timeout 150） | 32/32、rc=0、帽内 | ✓ |
| A1/A3 与 R4 一致（零回归） | ktest 面 ut_p50/burst/wall 逐位级；真系统 g1/g3 p50/burst/span 带内（ia/ut p99 属 2× 噪声类） | ✓ |
| A2/A4 interactive p50/p99 应有变化；「数据同 R4=行为差异未生效」 | ktest 面 A2/A4 ≈R4——该面 R5 零代码路径改动（diff 实证），复现=零回归；行为差异化证据在同 boot BANDM4G（全绿）+ 真系统面 band 轴翻转（ut_p99 2×） | **判读修正**：「应有变化」的对象应为 utility 尾延迟与受控 quantum 计数——interactive p50 在探针形态无差异可表达（quantum==全局 slice），非失效 |
| 数据对比表（R4 vs R5 delta） | §2.2/§2.4 双面全表 | ✓ |

## §5 局限与边界（如实）

1. **TCG/单核/宿主 load 7-10**：绝对分位不进门禁；p99 3-rep 带宽 2×（g3-r1 ut_p99 309ms 离群、g1-r3 161ms 离群在册）——判据立中位数+形状。
2. **burst 吞吐 R5 组内差方向不一致**（g2−g1=−10/g4−g3=+8）——供给面噪声，不判；R4 的「吞吐四臂不变」在 R5 复跑为 ±8% 带内摆动，同归噪声类。
3. **dpg_press panic（预存红，非本刀）**：本轮 12 探针 boot 每份 1 次（g1-r3 2 次）、冒烟 boot 亦 1 次、**R4 全部 12 份探针日志同在席**（同文案 dpg.rs:1009「θ_p 误报判据前提破」；AB-01 §4-5 已归 dpg.rs 在途面）——两代同形+有无载荷均在席 = 与 FairBand/本刀无关；不阻冒烟 32 标记。
4. **ktest 面 A2/A4 复现不携带 R5 信息**：该面手驱 raw API 与 R4 基线期同码路径——不能以它判「行为差异生效/未生效」，生效证据面 = BANDM4G + 真系统探针（本件 §2.3/§2.4）。
5. **未分解项**：g2 ut_p99 抬升三候选（长片/权重/偶发限流）无遥测分解（§3-1【推】）；SMP 面（跨核限流位、divergence 列）仍留窗（13 号 §5-6 同）。
6. **登记欠账**：本目录 README 登记行未补 14 号（任务书单文件限定）；原始日志在 /tmp（会话级），归档随 owner 批次。

## §6 引用锚（路径:行号，2026-11-16 实核）

- R5 落地面：[sched/src/class.rs](../../sched/src/class.rs):590/:609/:619/:630（FairBand 四 API 带参包装 consume/period_check/throttle_all/unthrottle_all）、:737-759（FairBand::tick 三轴执法）、:844-849（TaskClass::grant_cycles）、:891（band_period_sweep）；[sched/src/sched.rs](../../sched/src/sched.rs):1410 等 8 授片点、:3577（tick 入口扫）、:3630-3631（生产 tick class 分派）；[iprofile/src/lib.rs](../../iprofile/src/lib.rs):392（INTENT_BAND_TABLE）、:412-419（[2] interactive 10ms/8ms/256..1024）、:421-428（[3] utility 20ms/50ms/64..512）。
- 载体：[kernel/src/ktest.rs](../../kernel/src/ktest.rs):9124（test_loadgen）、:9302-9321（组配置 explicit configure——ktest 面配额 20ms/60ms 与表行参数解耦）、:9427（unit Fair.tick 手驱）、:9429-9439（raw consume/throttle 手驱点）、:9576-9583（差分断言）、:9595-9600（四组定义）、:7588-7845（BANDM4G 段）、:7830（diff 观测行）；[kernel/src/main.rs](../../kernel/src/main.rs):4497/:803（LG_SLICE_HZ=100）、:1902-1911（探针 spawn_intent vs plain spawn）。
- 基线：[research/调度深度调研/AB-01](../调度深度调研/AB-01-CP06首份AB数据-ktest负载生成器-v1.0.md):43-52（ktest 四组两跑）、:58-64（cmp 行）；[13 号](13-首份AB报告-loadgen.md):32-41（真系统四臂+离散带）、§0-2（band 轴 null 判词）。
- 原始数据：/tmp/lg-logs-r5/（g{1..4}-r{1..3}.log + r5-smoke-boot.log）、/tmp/lg-r5-driver.out（驱动实录含宿主 load 首尾）；R4 原始 /tmp/lg-logs/（12 份复验在席）。
