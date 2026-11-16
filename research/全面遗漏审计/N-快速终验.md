# N——快速终验（全面遗漏审计第六轮·R4/R5/v3 闭合链回看）

> 版本：1.0 ｜ 日期：2026-11-16 ｜ 定位：第六轮快速终验——全域空闲 + 基线全绿（343 篇 0 警/abi PASS）时点，回看 R4 语义审计（[K](K-语义完整性.md)）三大缺口的闭合链：缺口 1（intent→band 行为=0）经 R5 第五刀 M4 并线、缺口 2（EAS 生产未激活）经 v3 POLICY4/EAS-PA 刀、缺口 3（PM 频率 owner 后撤）现状归届。方法：五问全部实跑或直读源码（class.rs/band.rs/ktest.rs/berry-power 全件/sched.rs/policy.rs 逐行）+ 门禁四件（docs_guard/check-docs/abi-check/selftest 8 件）+ battery 全量实跑（本会话沙箱拒家目录 `~/.cargo-target` 构建锁——os error 1 环境限制，改 `CARGO_TARGET_DIR=workspace/target-cargo` 后跑通，判定不受影响）；QEMU 证据取 out/logs 留档（不再起 QEMU——任务书「快速轻量」）；每条结论给路径＋行号；推断标「推断」；全仓只读，本文件为唯一产出件（README 登记行按 K/L 判例留主会话）。

## 〇、结论速览

| 问 | 判 | 一句话 |
|---|---|---|
| 1 R4 缺口 1（intent→band 行为=0）→ R5 修复 | **PASS·闭合** | FairBand 四 API 全带参数（quantum/latency/low/high_weight 逐字段进热路径）＋M4G 行为断言＋QEMU 留档 BANDM4G 三面齐 |
| 2 R4 缺口 2（EAS 生产未激活）→ v3 修复 | **内核侧 PASS／EL0 侧代码在位、证据缺** | EASPROD 真链（0x83 COMMIT→placement_hint→eas_place）QEMU 实证；berry-power 提交链代码在位但 **rootfs 未重建**——v3 版 berry-power 从未进任何镜像，QEMU 零 `policy4` 行 |
| 3 R4 缺口 3（PM 频率 owner 后撤）→ 现状 | **归届在册·未闭合（设计留窗）** | 频率决策纯函数面真在（cap 三源 min 聚合＋FreqVerdict/schedutil）；常驻轮询（窗 3 剩余）与内核聚合点写面（窗 4）两断点仍留窗＝M 件 O1（P1·owner 对账）未动 |
| 4 全量回归 | **1 真实红·余全绿** | docs_guard 0 警/check-docs 343 篇 0 警/abi 39 号 PASS/selftest 8 件绿（bcts 34/34 腿）/battery 2138 测 0 败——唯一红：writable_globals live 对拍（`kernel/src/main.rs::ROOTFS_VOLS_LOADED` 未登记，12:15 编辑引入） |
| 5 v3 四动作（hint.rs）测试 | **PASS** | hint 三测（weight 三带/EPP 热档/四动作联合）＋battery 实跑 berry-power 36 测全绿（=31+CP-08 1+EAS-PA 1+POLICY4 3，与 justfile:853-855 在册口径逐位对齐）＋内核侧 EASPROD ktest |

**R4→R5→v3 闭合链总判**：三缺口中缺口 1 全闭合（代码×断言×QEMU 留档三面）；缺口 2 闭合到「内核消费面实证＋EL0 产面代码在位」，差一次 `just image` 重建＋QEMU 复跑收口；缺口 3 未进本轮闭合射程（M 件已归届 O1 留窗，非漂移复发）。另本轮实跑抓到 1 件新门禁红（可写全局登记漏，§四）。

## 一、缺口 1：intent→band 行为=0 → R5 第五刀 M4 并线（PASS·闭合）

### 1.1 四 API 全用 band 参数（grep band.quantum/band.high_weight 逐字段落实）

R4 判「行为差异化零接线」的病灶是 FairBand trait 全委托 unit Fair、带参数只落字段（K §1.2）。当前树修复面（`sched/src/class.rs` M4 并线段，模块注 :481-510 自述「2026-11-16 第五刀 M4 并线〔R4 语义审计链 1 补刀——『分派通、行为差异=0』的最后一毫米〕」）：

- **consume**（:590-603）：quota = 本带 `band.quantum_ns`（`quantum_cycles()` :558-560 换算源），带 CBS 期内律剪辑（:595-601——跨期运行不记入新期账，注释引 ktest M4G 实测抓到 6 次假越线）；判定律单源 `band::consume`（此处只携参）。
- **period_check**（:609-614）：期长 = 本带 `band.latency_target_ns`（`period_cycles()` :564-566）；翻期数学单源 `band::period_check`。
- **throttle_all**（:619-625）：环扫置 HELD_THROTTLE 后带成员 VD 按 **`band.low_weight`** 重臂（:622——被限流者垫后，注释明示「带权重替代全局 prio_effective 的消费点」）。
- **unthrottle_all**（:630-636）：解限流后 VD 按 **`band.high_weight`** 重臂（:633——高权带回场不排尾）。
- 配套：懒武装 `arm()` :571-576（未武装才按表行 configure，显式 configure 优先）；`vd_weight()` :640-648（throttled 镜像选 low/high）；单任务片耗尽重臂 `rearm_vd()` :654-658（grant=本带 quantum×带权重）。
- **trait 热路径差异化**：`impl SchedClass for FairBand` 的 `tick`（:737-759）三步——①片耗尽 VD 带参重臂（:745-747）②自翻期＋解限（:751-753）③`consume` 首越线→`throttle_all`（:756-758）；`enqueue` 保纯委托（:727-729——首选序零漂移，impl doc :719-725 给 ktest 编舞实证理由）；授片 `TaskClass::grant_cycles`（:844-849）Band 臂 = 本带 quantum 周期数；全带兜底翻期扫 `band_period_sweep`（:891-905）挂 `sched::tick` 入口（`sched/src/sched.rs:3596-3598`——防全员限流后 FairBand::tick 不可达的永久饿死）。

### 1.2 BANDM4 行为断言（M4G 段，R4 时不存在）

`kernel/src/ktest.rs:7596-7846`（注册 `test_band_m4()` :110、:7410 函数头）：LOADGEN 手驱机械真跑（真 pick/真授片/真 spin/真 tick，全部 `class_of_task` per-task 分派——:7608-7610 自述生产形状）。三段：

- **G1 双带同环**（:7641-7745）：前提断言两带槽默认关（:7623-7627 懒武装起测面）；授片断言 interactive=quantum_ns 周期数且 < utility（:7635-7637）；20 轮×12ms 跑段后——懒武装生效（:7719-7722）、interactive 配额 10ms/8ms 期>100% **不可越线**（cross_ia==0，:7723）、utility 双带交替 duty≈50% **须烧穿配额**（cross_ut>=1，:7724）、interactive 短片须频繁到期（exh_ia>=3，:7725-7729）且 utility 长片不到期（exh_ut==0，:7730-7733）、翻期频次比 8ms:50ms 须 ≥3x（:7734-7745）。
- **G2 utility 单飞**（:7747-7812）：配额越线→HELD_THROTTLE 置位（:7783-7786）＋**VD 重臂权重精确等式断言** `vd−vr == grant×1024/low_weight`（:7787-7793）；翻期→解限 n>=1＋THROTTLE 位清（:7799-7805）＋`vd−vr == grant×1024/high_weight`（:7806-7812）——四 API×带参数的行为级对拍，非仅返码。
- **G3 unit 零漂移**（:7814-7821）：未声明 intent 回落 Unit＋授片=全局 slice（回归钉板）。
- 收口（:7823-7835）：两带槽回 OFF；观测行 `BANDM4G diff exh=… flips=… cross=…`（:7838-7849）。

### 1.3 QEMU 留档实证

`out/logs/run-rootfs-1794801134.log:63`：`BANDM4G diff exh=10/0 flips=29/4 cross=0/5`（interactive 片耗尽 10 次 vs utility 0、翻期 29 vs 4 次〔比 7.25x ≥3x ✓〕、越线 0 vs 5 次——与 G1/G2 断言方向逐一相符）；:64 `BANDM4 ok n=2`；LOADGEN 四组（:75-84，band on 组 thr=11/off 组 thr=0，p99 尾延迟差异复现）。**新鲜度注**：该留档构建于 11:52:14（文件名时间戳换算），而 ktest.rs/class.rs mtime 12:06:46——构建后又编辑过；两标记段当前源直读与留档行为同形，「12:06 编辑未触及 M4G 断言语义」为推断（禁 git 无法 diff，内核 ELF 13:26 已含当前源，可随下次 boot 相复证）。

**判：缺口 1 闭合**——代码（四 API×带参数＋tick 三差异＋兜底扫）、测试（M4G 三段含 VD 权重精确等式）、QEMU 留档（BANDM4G 行）三面齐。

## 二、缺口 2：EAS 生产未激活 → v3 POLICY4/EAS-PA（内核侧 PASS／EL0 侧证据缺）

### 2.1 内核消费面：真链实证（ktest EASPROD，R4 时不存在）

R4 病灶＝「生产 hint 源结构性为零→eas_place 恒 None」（K §2.2）。当前树 `kernel/src/ktest.rs:9728-9915` 新测试 `test_eas_prod()`（注册 :133）：

- **EP-A**（:9777-9821）：真 SVC `process_create`（:9780-9790，0x83 授权 fixture 出厂带 MANAGE）→ 真 SVC 0x83 STATUS 持权过门 0（:9797-9808）→ 双半区断言（低半=active gen/高半=shadow gen，:9814-9817）＋self_key 与 current_tag 同源（:9818-9821）。
- **EP-B**（:9823-9860）：真 EWMA 折叠 CPU0 至 350 带（:9836-9843）→ 产面镜像律避忙核 hint=2（:9845）→ 提交前基线无意见（:9847）→ **真 SVC 0x83 COMMIT** placement=2（:9849-9855）→ gen 推进（:9856）→ `policy::placement_hint(7)==2` 透传（:9857-9858，注释「berry-power EL0 面将走的同一条链」）→ **`eas_place(7)==Some(1)` 落低核**（:9859-9860）。
- **EP-C 镜像翻转**（:9862-9887）：util 翻到 CPU1→hint=1→翻转 COMMIT→`eas_place==Some(0)`（:9886-9887）——决策随产面输入翻转，非常量巧合。
- **EP-D overutil 诚实面**（:9889-9902）：泵阈后 hint 表在绑也不透传（:9899-9900）→ `eas_place==None` 让位负载均衡（:9901-9902，Linux 同律）。
- 收口（:9904-9913）＋`EASPROD ok`（:9915）。

QEMU 留档：`run-rootfs-1794801134.log:85` `EASPROD ok`（:65 `EAS ok` 同帧）。内核侧消费面（`sched/src/sched.rs:762` place→eas_place→:844-845 `policy::placement_hint`；`sched/src/policy.rs:471` 读出口、:401 `apply` 写 TCB、:441 `weight_of`）R4 已通、本轮直读仍在位。

### 2.2 EL0 产面：代码在位、rootfs 未重建、QEMU 零证据

- **代码在位**：`user/bins/berry-power/src/main.rs:260-301`——`core.action_hints()`（:260）→ `self_proc_h()` 授权（:261，gift[2]=自身 Process handle，:246-254 注）→ STATUS gen 对账（:266-267）→ **`berry_policy_bind(auth, POLICY_OP_COMMIT, &entry, 1, gen, 0)`**（:278，entry 携 ①placement/③weight/④epp_flags 三字段 :271-277——②频率无 0x83 载体走 PM 域，:229-233 注）→ gen echo 回读对账（:281-284）→ 诊断行 `policy4 place=… freq=…khz(pm) wd=… epp=…`（:290-301）；svcmgr 预拉注释已更新到 policy4 口径（`user/bins/svcmgr/src/main.rs:1111-1118`）。纯函数面 `hint.rs:93-106 action_hints`（四动作一次读全）。
- **证据缺（本轮实测）**：①rootfs.cpio（mtime 11-16 12:01:15）strings 含旧版串 `berry-power: eas hint`、`policy4` 计 0——镜像内是 EAS-PA 刀版 berry-power；berry-power 源码 main.rs mtime 12:23:42/hint.rs 12:29:23 **晚于镜像 22-28 分钟**，rootfs1* 全系列（rootfs1/1.dpgb/1.dpge.dpgx/1.dpge.dpgx.x5/1.dpgf）policy4 计全 0——**v3 版从未进任何 rootfs**。②out/logs 全部留档 `policy4` 行零命中（本轮 grep）。③旧版 QEMU 实跑恰好自证授权面曾挂：`run-rootfs-1794801134.log:245` `berry-power: eas hint=2 key=0 st=-9 bd=-9 auth=0`（-9=-EBADF；:243 dry-run 行同帧）——POLICY4 刀的授权修复（gift/self_proc_h）后无 QEMU 复证。④SMP 真选核自证行 `EAS pick cpu=` 仍全留档零命中（结构性：单核 QEMU place() 的 SMP 门 :758 不进 EAS 面，K §2.2 已判）。

**判**：缺口 2 的「管道②无生产注入面」已闭合到——内核 0x83 写路＋EAS 消费有真链实证（EASPROD）；EL0 服务端提交链代码＋授权修复在位、host 测全绿；**但 v3 版 berry-power 从未重建进镜像、QEMU 端到端（policy4 行 st=0 bd=0 gn=gen）零证据**。收口动作＝一次 `just image`＋run-rootfs 相（轻）。

## 三、缺口 3：PM 频率 owner 后撤 → 现状（归届在册·未闭合＝设计留窗）

- **频率决策在**（当前树直读）：`user/bins/berry-power/src/cap.rs:22-29` 三源定席（thermal/调度 hint/低电）、:63-73 `set` 唯一帽写面（坏输入拒面 BadSource/ZeroCap）、:86-96 `aggregate` 在席 min＋平票源号小者胜（:40-45 `CapDecision` 带归因 src）；裁决 `lib.rs:107 FreqVerdict`/:220 `decide()`/:230——schedutil 频率请求 `req` + 聚合帽 min 后 `eff` 钳帽；dry-run 行 QEMU 实证 `freq req=124023khz eff=124023khz`（`run-rootfs-1794801134.log:243`，util=127@k 内核真值）。
- **聚合面测试在**：cap exhaustive 125 组合全序穷举（cap.rs:105-188 `aggregate_total_order_exhaustive` 等，battery 实跑绿）＋pm-integration 14 测（热降频全链 harness，justfile:864-868）。
- **两断点仍在窗（R4 判定的「无常驻、无写回」当前未接，但口径已明示为留窗而非悬空）**：常驻轮询＝main.rs:39-40 模块注「剩余留窗：常驻循环的周期轮询面（当前仅自检单读）」＋:256-259「窗 3 常驻循环留窗未开……一-shot 证据行后 park」＋:306 `park()`；内核写回＝:55-57 窗 4「内核聚合点写面」＋:233「内核聚合点写面留窗（窗 4）」——eff_khz 仍只进 println（:219-223）。M 件已归届：O1（P1·owner 对账，[M](M-最终判定.md) §二 78 行）「常驻轮询+pmseat 四帽源曾落盘 QEMU 实证后被 08:01 编辑撤」。
- **pmfreq/pmseat 复验**：当前源码（user/＋kernel/ 全部 .rs）零命中；仅在旧构建产物二进制残留（`user/cargo-target*/…/berry-power*`、`user/cargo-target-bp/…/berry-power`，推断=08:01 前构建未清）——「曾实现后撤」的代际断裂特征仍在，对账动作未见。

**判**：缺口 3 未闭合、也未恶化——决策/聚合纯函数面真在且有测，末端两断点按 M 件 O1 归届 owner（P1），当前树注释口径与实况一致（无 R4 时「首环已闭合注与日志矛盾」类漂移）。

## 四、全量回归（实跑）

### 4.1 门禁与 selftest（全绿）

| 件 | 命令 | rc | 结果 |
|---|---|---|---|
| docs_guard | `python3 tools/docs_guard.py` | 0 | PASS（0 警） |
| check-docs | `node .agents/tools/check-docs.mjs` | 0 | 检查 343 篇/告警 0（与任务书基线一致） |
| abi-check | `python3 tools/gen_abi.py --check` | 0 | PASS（NR v1 **39 号**在册，五面单源一致；[abi.toml](../../tools/abi.toml):33 `nr_total = 39` 含 0x85——M 件时点 38，+1 为新号非漂移） |
| berry_assemble/mkapp/mkimage/mkcpio(--selftest-split)/gensourceindex | 各 `--selftest*` | 0 | 五件 ok |
| bcts-run | `tools/bcts-run.sh --selftest` | 0 | **34/34** 腿绿（E 线时 33——+1 腿「--image 缺省名缺席拒」，扩测非漂移） |
| shvarcheck | `python3 tools/shvarcheck.py --selftest` | 0 | ok |

### 4.2 battery cargo test（`just test-host`，2138 测 0 败＋1 门禁红）

- **环境注**：首跑（默认 `~/.cargo-target`）在本会话沙箱下死于 `.cargo-build-lock` Operation not permitted（os error 1）——家目录写锁被拒，环境限制非代码失败；改 `CARGO_TARGET_DIR=<workspace>/target-cargo`（既有 workspace 内 target，517M 缓存）重跑通过。
- **绿账**：至 justfile:1089 中止点前 200 suite `test result: ok`、**1921 passed/0 failed**；中止点后尾段四件补跑（crypto 57/verify 10/guifbd 13/tools-berry 137）全绿——合计 **2138 测 0 败**。E 线七件口径逐位对齐：crypto 57/gpuproto 29/slackface 12/em 8/power **36**（=31+CP-08 1+EAS-PA 1+**POLICY4 3**，justfile:850-855 在册注释同值）/thermal 13/pm-integration 14。
- **唯一红（真实·新发现，P1）**：`python3 tools/writable_globals.py --selftest` 的 live 对拍面 FAIL——`only-scan=['kernel/src/main.rs::ROOTFS_VOLS_LOADED']`：`kernel/src/main.rs:165` `static mut ROOTFS_VOLS_LOADED`（:215 写/:658 读）未登记进 [writable_globals.txt](../../tools/writable_globals.txt)（该文件 main.rs 段 :44-47 仅 BSH_IN_H/FB_PIPE_H/INTENT_H/PRESSURE_EV_REF）。时间线：main.rs mtime 12:15:06 vs 清单 08:01:20——12:15 编辑引入新可写全局、清单未跟上；`just check`/CI 前置（justfile:462）自该编辑起挂。修复＝清单补一行（归 owner/主会话，本轮只读纪律不动）。

## 五、新功能测试：v3 四动作（hint.rs）

- **cargo test 有且绿**：`user/bins/berry-power/src/hint.rs:111-197` 三测——`weight_delta_three_bands`（:119-135，③三带+阈端含端+包络自律 ≤WEIGHT_MAX）、`epp_flags_thermal_bands`（:140-154，④热档联动 None/nominal 沉默、fair 起 Efficiency、flags 位自律）、`action_hints_end_to_end`（:159-197，四动作联合与分面逐位一致+忙轨高档+EM 缺席 ②归零 ①③④零污染）；battery 实跑 `hint::tests::*` 三行 ok（36 passed 之内）。
- **内核侧同刀测试**：EASPROD（§2.1）盖 0x83 真链面；产消两端各有机器咬合。
- **注**：②频率动作按注入形态定谳无 0x83 载体（hint.rs:10-16），其面由 cap/util 既有测盖（§三）——「四动作」测试口径＝①③④表字段＋②PM 域裁决，齐。

## 六、残留清单（本轮终态）

| 号 | 项 | 级 | 归届 |
|---|---|---|---|
| N-1 | **v3 berry-power 未进镜像**：rootfs(12:01) 含旧版（eas hint 串），源码 12:23/12:29 晚于镜像；`policy4` 行 QEMU 零证据、旧版实跑 st=-9（授权修复后未复证） | P1 | 一次 `just image`+run-rootfs 收口（内核 ELF 13:26 已新于全部源，boot 相无此障碍） |
| N-2 | **writable_globals 登记漏**：`ROOTFS_VOLS_LOADED`（kernel/src/main.rs:165）未入清单 → `just test-host`/`just check` 红 | P1 | 清单补行（一行修复） |
| N-3 | M4G/EASPROD QEMU 留档构建于 11:52，ktest/class 源 12:06 又编辑——留档对当前树新鲜度未证（推断：非断言语义级；ELF 13:26 新鲜可随 N-1 同批复证） | P3 | 随 N-1 boot 相一并自证 |
| N-4 | 缺口 3 两断点（常驻轮询/内核写回）＋O1 对账（pmfreq 旧二进制残留 `user/cargo-target*`）——M 件 O1 原样在册 | P1/owner | M §二 O1（未动） |

**终答**：R4 三缺口——缺口 1 **闭合**（三面证据齐）；缺口 2 **内核侧闭合、EL0 侧差一次镜像重建+QEMU 复跑**（N-1）；缺口 3 **归届留窗未闭合**（设计意图，非本轮射程）。本轮新抓 N-2 一件门禁红；其余全域基线（343 篇 0 警/abi 39 号/selftest 8 件/battery 2138 测）绿。
