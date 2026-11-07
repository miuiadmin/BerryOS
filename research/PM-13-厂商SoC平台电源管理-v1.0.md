# PM-13 厂商 SoC 平台电源管理（v1.0）

> 调研日期：2026-11-07 · 方法：WebSearch/WebFetch/web_reader 取证 Linux 主线（torvalds/linux master）源码 + Qualcomm 官方文档 + LWN + AOSP 官方文档，中英文并用；仓内锚点实查行号。
> 确定度图例：**高** = 一手源码/官方文档（含函数名级证据）；**中** = 二手转述（LWN/社区文/厂商开发者故事）；**低** = 推断。
> 分工：一期 PM-00 定协议位（P3/P3a）、PM-03 管 Linux 主线机制与 uclamp 主线侧；二期 PM-08 给 Fuchsia 对照（板级 FIDL + latency 无人消费反面实证）、PM-10 判定 QEMU 无 PSCI SYSTEM_SUSPEND。**本报告补第三块证据：真实厂商 SoC（高通/MTK/三星）的电源域硬件与「睡眠决策下放另一处理器」的行业实态**，并给 P3/P3a 校准 + PM-16 sim 设备建议。

## §0 一页结论

1. 「驱动申报电源档 + 唤醒延迟，OS 只投票、远程执行者裁决」**不是 BerryOS 的发明，是手机 SoC 十年验证的行业共识**：高通每驱动投 corner/level 票（rpmhpd 源码 `rpmhpd_aggregate_corner()`），SDM845 起聚合执行全下放硬件加速器（ARC/VRM/BCM）与常驻 AOP 固件（Qualcomm 官方文档）。P3a 方向正确，缺的是行业已验证的三件事：**min-residency 成对字段、active/sleep 双票语义、执行者类型位**（确定度：高）。
2. 真实量级：簇级电源域唤醒 2.35–4.4 ms、CPU 轨断电 0.75–1.35 ms、收益门槛（min-residency）7–10 ms（sm8550.dtsi 实数）。`wake_latency_ns` 用 ns 表达 ms 级延迟成立，但**只报 wake_latency 不报 min_residency 会让仲裁器在短窗里做亏电决策**——DT 绑定把两者定为同等必填（确定度：高）。
3. 厂商私有 perf 接口（perflock/mpctl/perfservice）与主线 EAS/uclamp 拉锯十年的终局：schedtune→uclamp（5.3）、GKI vendor hooks、ADPF（Android 12 起）——**收敛点全部是「应用只报目标+实际时延，OS 决定频与核」**，与 BerryOS「上层只投票」纪律同构（确定度：高）。
4. MTK/Exynos 佐证同构：MTK SVS 硬件引擎算电压、Linux 只写回 OPP 表且只降不升；Exynos ACPM 固件在 APM 上裁决 PMIC/DVFS/TMU，2024 年底起协议驱动进主线。**「执行在远端、声明在 OS 侧」是普遍形态**（确定度：高）。
5. QEMU virt 无任何上述硬件；PM-16 需要 sim-pd（配置化延迟+min-residency+计数器）+ sim 远程执行者（票缓存→suspend 入口 flush）两个模拟件才能验 P3a 全链（确定度：高，设计属本报告建议）。

## §1 主线全景

### 1.1 高通电源中枢三代演进

| 代 | 代表 SoC | 投票通道 | 聚合执行者 | 主线状态 |
|---|---|---|---|---|
| RPM (SMD) | MSM8974～SDM660 | SMD 消息（`corn`/`swen`/`vfc`/`vlvl` 键） | RPM 处理器固件聚合 | `drivers/pmdomain/qcom/rpmpd.c` + `smd-rpm.c`（高） |
| RPMh | SDM845～SM8750 | RSC/TCS 硬件槽位 `<header,addr,data>` 三元组 | **硬件加速器 ARC/VRM/BCM 直接聚合**，AOP 固件只兜底复杂依赖与 AOSS 睡眠 | `rpmhpd.c`/`rpmh.c`/`rpmh-rsc.c`/`cmd-db.c`（高） |
| 后 RPMh 收敛 | SM8450+ | QMP 消息 RAM / QMI | AOSS(AOP) 常驻微控 + QMI TMD 远程服务 | `qcom_aoss.c`、`qmi_tmd.c`(2025)（高） |

演进驱动力：SDM845「用硬化 IP 块聚合并施加请求」「远端完全无需处理器干预」（[LWN 748981](https://lwn.net/Articles/748981/) 定性转述），比 RPM 快 3–10 倍（官方原文 "at least 3 to 10 times faster"）、续航 +2.5%（"two-and-a-half percent"）——**两个数字仅出 [Qualcomm 45_RPMh](https://docs.qualcomm.com/doc/80-88500-4/topic/45_RPMh.html) 官方页，LWN 原文无此两数**（确定度：高；【勘误 2026-11-07：原注两源混挂，数字单归 Qualcomm 页】）。Qualcomm 官方同页还有一句关键表述：**「SoC 资源睡眠态彼此独立，RPMh 里没有（统一的）睡眠模式」**——睡眠是每资源独立塌缩的结果，不是全局开关（确定度：高）。

高通域分类（rpmhpd.c 实查，全 SoC 复用同一组静态定义）：

| 域 | 语义 | 特殊结构 |
|---|---|---|
| cx / cx_ao | 数字核电压轨 / 其 active-only 镜像 | peer 成对；部分 SoC 下 cx 挂 mx 为父域 |
| mx / mx_ao | 主轨 | cx/gfx 的父域（SDM845/SM7150 等） |
| mxc | mx 补轨 | `skip_retention_level`（通用 per-domain 机制，DT/compatible 配置——mxc 为当前在用实例，非域专属；效果=禁用 retention 档） |
| lcx / lmx | 低速轨 | 常与 mss 联动 |
| gfx / mmcx | GPU / 多媒体轨 | mmcx 部分 SoC 挂 cx 父域 |
| mss | modem 子系统 | — |
| ebi | DDR 总线轨 | — |
| nsp/nsp0/nsp1/nsp2 | 神经处理单元 | 新 SoC 拆多域 |
| qphy / gmxc / dcx / gbx | USB PHY / GPU 主轨等 | 按代增补 |

`_ao` 后缀域即 `active_only = true` 的 peer：投票只在唤醒态生效，`to_active_sleep()` 把它的 sleep 票强制置 0（确定度：高，源码实查）。

### 1.2 厂商对照

| 厂商 | 远程执行者 | OS 侧声明物 | 遥测 | 主线化程度 |
|---|---|---|---|---|
| 高通 | RPM 固件→RPMh 硬件 + AOP | corner/level 票、ACTIVE/WAKE/SLEEP 三票 | debugfs qcom_stats + rpm_master_stats | 全量进主线（高） |
| MTK | SPM/SCPSYS（域开关）+ SVS 硬件引擎（电压）+ SCP | genpd 域 + OPP 表（SVS 只改 OPP 电压且只降不升） | 主线无统一遥测 | 域驱动进主线；SPM 深睡固件留在 ATF（中） |
| 三星 | ACPM 固件（跑在 APM 模块） | ACPM 协议消息（PMIC/DVFS/TMU 分协议） | 主线暂无 | 2024-12 协议驱动进主线，DVFS 2025、TMU 2026（高） |

### 1.3 电源域通用建模（genpd + DT）

- Provider/consumer：设备 `power-domains = <phandle>` 消费；provider 可挂 `operating-points-v2`——「能缩性能的电源域」自己带 OPP 表，设备 OPP 再用 `required-opps` 引用域档（[power-domain.yaml](https://github.com/torvalds/linux/blob/master/Documentation/devicetree/bindings/power/power-domain.yaml)，高；required-opps 正文在 opp-v2-base 绑定）。
- 域空闲态：`domain-idle-state.yaml` 必填三件 `entry-latency-us` / `exit-latency-us` / `min-residency-us`——**min-residency 定义为「进入后开始产生省电收益的最短驻留」**（[同文件](https://github.com/torvalds/linux/blob/master/Documentation/devicetree/bindings/power/domain-idle-state.yaml)，高）。
- 运行时 API：genpd `set_performance_state`——rpmhpd/rpmpd 均实现此回调收 corner 请求（高）。
- 高通域档位表（vlvl→hlvl 映射）**来自固件 cmd-db 而非 DT**，资源地址同源查找（rpmhpd.c `cmd_db_read_aux_data()`，高）——OS 侧声明物要么自报（BerryOS manifest 路线）要么运行时问固件要，写死在 DT 里的是「性能档」而非轨档。

### 1.4 vendor perf 十年拉锯（接 PM-03 主线侧）

| 时段 | 事件 | 载体 |
|---|---|---|
| ~2009–2016 | perflock：HAL/中间件 dlopen 闭源 libqcopt 调 `perf_lock_acq`，mpctl 守护进程管票 | 高通私有（#21） |
| 2016 | EAS 进主线 4.7；schedtune 以树外补丁随 Android common 内核分发 | 主线+树外并存 |
| 2019 | uclamp 进 5.3（Arm，Patrick Bellasi），官方定位「Android 树外 SchedTune 的主线替代」 | [LWN 793079](https://lwn.net/Articles/793079/) |
| 2020 | GKI + vendor hooks（`android_vh_*`，tracepoint 式回调点），厂商不改核心内核挂私有逻辑 | Android 11 |
| 2021–2024 | ADPF：PerformanceHintManager（Android 12/API 31）、NDK（13）、reportActualWorkDuration（15） | [ADPF](https://source.android.com/docs/core/perf/performance-hint-api) |

终局形态：应用报 target/actual duration，OS 决定频与核；官方理由之一「原生 governor 爬频约 200ms」（高）。MTK 同路：perfservice/perfctl 场景 boost → MAGT+ADPF（[开发者故事](https://developer.android.com/stories/games/mediatek-adpf)，中）。

## §2 关键事实与证据

### 2.1 高通 RPMh：三票制与仲裁（必答①）

| # | 事实 | 证据 | 确定度 |
|---|---|---|---|
| 1 | 每个消费者投「corner」票，rpmhpd 汇总本域+active-only peer 取 max 后下发 | [rpmhpd.c](https://github.com/torvalds/linux/blob/master/drivers/pmdomain/qcom/rpmhpd.c) `rpmhpd_aggregate_corner()`：`active_corner = max(this_active, peer_active)` | 高 |
| 2 | **三票制**：ACTIVE_ONLY 立即生效；WAKE_ONLY「由 RPMh 在系统唤醒时施加」；SLEEP「在系统睡眠时施加」——驱动源码注释原文 | 同上函数注释："as WAKE_ONLY votes (applied by RPMh on system wakeup) and as SLEEP votes (applied by RPMh on system sleep)" | 高 |
| 3 | sleep/wake 票不即时发：驱动缓存，suspend 入口 `rpmh_flush()` 批量写 TCS；注释「只在确认自己运行在最后一个处理器上时调用」，锁忙即放弃（说明还有兄弟处理器活着） | [rpmh.c](https://github.com/torvalds/linux/blob/master/drivers/soc/qcom/rpmh.c) `rpmh_flush()`/`__rpmh_write()` 注释 | 高 |
| 4 | 「wake set 恒完整、sleep set 不完整」——flush 时 wake 写要等完成、sleep 不等 | 同上 `send_single()` 注释 | 高 |
| 5 | 升档同步等 ack、降档异步（省延迟） | rpmhpd.c `rpmhpd_send_corner()`："Wait for an ack only when we are increasing the perf state" | 高 |
| 6 | 域档位表与资源地址来自固件 cmd-db；`RPMH_ARC_MAX_LEVELS=32`；retention 档跳过 = `skip_retention_level` **通用 per-domain 机制**（mxc 为在用实例，非 mxc 硬编码） | rpmhpd.c `rpmhpd_update_level_mapping()` / `cmd_db_read_aux_data` | 高 |
| 7 | sync_state 未触达前所有票**钳到最高档**防早塌（所有消费者 probe 完成才允许真聚合） | rpmhpd.c `state_synced` 分支 "Clamp to highest corner if sync_state hasn't happened" | 高 |
| 8 | legacy RPM 只有 ACTIVE+SLEEP 两票（无 WAKE 票），票经 SMD 发 RPM 处理器聚合执行；旧 SoC 用 corner（上限 6）、新 SoC 用 level（TURBO 等） | [rpmpd.c](https://github.com/torvalds/linux/blob/master/drivers/pmdomain/qcom/rpmpd.c) `QCOM_SMD_RPM_ACTIVE_STATE`/`SLEEP_STATE`、`KEY_CORNER`/`KEY_LEVEL` | 高 |
| 9 | legacy 域的 floor 票（vfc/vfl）域关着也照发——地板请求不随域开关撤 | rpmpd.c `rpmpd_send_enable()` 旁注释 | 高 |
| 10 | AOSS 域（cx/ddr 塌缩开关）经 QMP 消息 RAM 请求 AOP 固件；消息为 ≤64B ASCII 如 `{class: clock, res: qdss, val: 1}`，门铃信箱通知、1s 等 ack；另注册 2 个 cooling device | [qcom_aoss.c](https://github.com/torvalds/linux/blob/master/drivers/soc/qcom/qcom_aoss.c) | 高 |

### 2.2 高通遥测（必答①续）

| # | 事实 | 证据 | 确定度 |
|---|---|---|---|
| 11 | 每子系统睡眠四元组 Count / Last Entered At / Last Exited At / Accumulated Duration（睡眠中现场补正累计值）；debugfs `/sys/kernel/debug/qcom_stats/<sub>`；覆盖 modem/wpss/adsp/cdsp/cdsp1/gpdsp0/gpdsp1/soccp/dcp/slpi/gpu/display/孤岛域/apss | [qcom_stats.c](https://github.com/torvalds/linux/blob/master/drivers/soc/qcom/qcom_stats.c) `qcom_print_stats()`/`subsystem_data` | 高 |
| 12 | 数据源：子系统走 SMEM item 605–631；SoC/DDR 低功耗模式（aosd/cxsd/vmin/vlow）从 ioremap 消息 RAM 直读；SM8450+ DDR 累计时长要发 QMP `{class: ddr, action: freqsync}` 让固件刷新 | 同上 | 高 |
| 13 | 主处理器视角统计（"Master Stats v2"，2013–2023 全系沿用，仅 debugfs）：`wakeup_reason`（**0 = rude wakeup 粗暴唤醒 / 1 = scheduled 预定唤醒**）、`active_cores`、`num_shutdowns`、`last_sleep_trans_dur`、`last_wake_trans_dur`、XO 断电计数/时长 | [rpm_master_stats.c](https://github.com/torvalds/linux/blob/master/drivers/soc/qcom/rpm_master_stats.c) struct `rpm_master_stats` | 高 |
| 14 | 远程热缓解：QMI TMD 服务（2025 进主线）按固件自报清单动态发现远端 cooling device 并设档（max 级别运行时获知）；**服务重启后对既有 cdev 自动重发当前状态**（重同步），不重复注册 | [qmi_tmd.c](https://github.com/torvalds/linux/blob/master/drivers/soc/qcom/qmi_tmd.c) | 高 |

### 2.3 DT 建模与真实延迟量级（必答②）

| # | 事实 | 证据 | 确定度 |
|---|---|---|---|
| 15 | sm8550 实数：CPU 轨断电（银/金/金加核）exit-latency 750/1300/1350 us，min-residency 6700/8136/7480 us；簇态 exit 2350/4400 us，min-residency 9144/10150 us；簇态挂 PSCI `0x41000044`/`0x4100c344` 经 OSI 下放 | [sm8550.dtsi](https://github.com/torvalds/linux/blob/master/arch/arm64/boot/dts/qcom/sm8550.dtsi) idle-states 各节点 | 高 |
| 16 | exit-latency-us 与 min-residency-us 在通用绑定里**同为必填**；语义「最坏退出延迟」「进入后开始有省电收益的最短驻留」 | [domain-idle-state.yaml](https://github.com/torvalds/linux/blob/master/Documentation/devicetree/bindings/power/domain-idle-state.yaml) | 高 |
| 17 | 电源域可缩性能：provider 节点挂 `operating-points-v2`；性能域节点名允许 `performance-domain`；设备侧 `required-opps` 绑设备 OPP 到域档 | [power-domain.yaml](https://github.com/torvalds/linux/blob/master/Documentation/devicetree/bindings/power/power-domain.yaml) | 高 |
| 18 | 量级推论：轨级/簇级电源域唤醒在毫秒段（非微秒段），比 CPU 单核 idle（百微秒段）深一个数量级——**wake_latency 分层分布** | #15 数据归纳 | 高 |

### 2.4 MTK（必答③）

| # | 事实 | 证据 | 确定度 |
|---|---|---|---|
| 19 | SVS 专用硬件引擎按 bank（CPU 小/大核、CCI、GPU）算电压：`opp_volt[i] = volt[i]*volt_step + volt_base`；驱动经 `dev_pm_opp_adjust_voltage()` **改写 OPP 表**让 cpufreq/GPU devfreq 消费，**钳制永不高于 DT 默认电压**（只降不升） | [mtk-svs.c](https://github.com/torvalds/linux/blob/master/drivers/soc/mediatek/mtk-svs.c) `svs_adjust_pm_opp_volts()` | 高 |
| 20 | SVS bank 分阶段：INIT01（上 vboot 校准）→ INIT02 → MON（运行时监视）；2-line bank 高/低温分工按 turn point 各管一段；热区耦合：温度出 [ltemp, htemp] 窗口加电压偏置；任何错误相位→回默认电压 | 同上 SVSB_PHASE_* 与 `tzone_htemp_voffset` | 高 |
| 21 | 主线 genpd 域驱动直写 SPM/SCPSYS 寄存器：SRAM 隔离/ISO、RTFF（retention 触发器）保存恢复（UFS/PCIE PHY 特例）、infracfg/SMI 总线保护（带 ACK 轮询）、modem 域走 SMC 安全调用 | [mtk-pm-domains.c](https://github.com/torvalds/linux/blob/master/drivers/pmdomain/mediatek/mtk-pm-domains.c) | 高 |
| 22 | 新代「HW voter」(HWV) MTCMOS 域：原子/IRQ-safe 的 genpd 操作，走 `MTK_SIP_KERNEL_HWCCF_CONTROL` 安全通道（mt8196-hwv-hfrp/hwv-scp）——**硬件投票器开始替代软件写序列** | 同上 compatibles 与 HWV 分支 | 高 |
| 23 | 主线域驱动无 SPM 深睡固件接口——MTK 深睡（suspend 进 SPM 固件序列）留在 ATF/PSCI 侧，未随域驱动进主线 | 上一文件无 suspend/固件调用（实查）；ATF 侧归属为推断 | 中 |
| 24 | MTK 场景 boost 走厂商 perfservice/perfctl 私有面（简单场景识别+boost）；新一代收敛到 MAGT+ADPF | [tinylab 厂商优化综述](https://tinylab.org/android-perf-optimize-solutions-of-mobile-manufacturers)、[MediaTek ADPF 故事](https://developer.android.com/stories/games/mediatek-adpf) | 中 |

### 2.5 Exynos 略写（必答③续）

| # | 事实 | 证据 | 确定度 |
|---|---|---|---|
| 25 | ACPM（Alive Clock and Power Manager）固件跑在 APM 模块统一裁决；2024-12 核心协议驱动进主线（`CONFIG_EXYNOS_ACPM_PROTOCOL`，v6.19 已见 `drivers/firmware/samsung/exynos-acpm-pmic.c`），2025-08 DVFS 协议+时钟驱动（固件暴露变步长索引时钟，驱动只做翻译），2026 TMU 协议跟进；首批服务 GS101/Tensor | Lore 邮件（"firmware: add exynos ACPM protocol driver" / "exynos-acpm: add DVFS protocol and clock driver"）+ [主线文件](https://github.com/torvalds/linux/blob/master/drivers/firmware/samsung/exynos-acpm-pmic.h) | 高 |
| 26 | 分协议设计：PMIC / DVFS / TMU 各自协议跑在同一 mailbox 通道之上——**能力分域而非大一统电源接口** | 同上 Lore 系列 | 高 |

### 2.6 vendor perf 生态收敛（必答④）

| # | 事实 | 证据 | 确定度 |
|---|---|---|---|
| 27 | Pixel 2 时代 power HAL `dlsym(libqcopt, "perf_lock_acq")` 拿闭源库发 perflock（后移除）；mpctl 守护进程管票（logcat: PERF-MPCTL，iobusy pfd、非法请求拒绝） | [AOSP wahoo 提交](https://android.googlesource.com/device/google/wahoo/+/c6a673fef9c90d8bc57fc487640f0642aca74167%5E2..c6a673fef9c90d8bc57fc487640f0642aca74167) | 高 |
| 28 | uclamp 5.3 落主线，Arm 驱动，定位「Android 树外 SchedTune 的主线替代」；每任务/cgroup 上下限钳制影响选频与放置 | [LWN 793079](https://lwn.net/Articles/793079/) | 高 |
| 29 | ADPF：应用报 target+actual 工作时长（ns，单调钟），OS 决定频/核；「原生 governor 爬频约 200ms」是官方给的理由；明确不鼓励应用自选核（设备常忽略亲和请求） | [ADPF 文档](https://source.android.com/docs/core/perf/performance-hint-api) | 高 |
| 30 | GKI vendor hooks：`android_vh_*` 命名的 tracepoint 式回调点让厂商不改核心内核挂私有逻辑；社区批评其为「Android 内核的最后避难所」 | [GKI kernel-code](https://source.android.com/docs/core/architecture/kernel/kernel-code)、[linux.org.hk 转载](https://www.linux.org.hk/archive/20261015-7687-vendor-hooks-became-the-last-refuge-of-a.html) | 高/中 |

## §3 教训与坑

1. **只报唤醒延迟不报收益门槛必亏电**：DT 把 min-residency 与 exit-latency 定为同等必填（#15/#16）。短睡眠窗进深档 = 白付 2–4 ms 退出延迟还多耗电。簇级 min-residency（~9–10 ms）比 exit（~2.4–4.4 ms）大 2–4 倍，两者没有固定比例，**必须各自申报**。
2. **票分三套不是一套**：active 立即、wake/sleep 预编程进硬件、suspend 入口才 flush（#2/#3）。单票模型（BerryOS P3 现状）表达不了「睡眠时允许塌、醒来立刻顶上」这类真实需求。
3. **wake set 必须完整、sleep set 可以缺**（#4）：唤醒路径出错的代价远高于睡眠路径——恢复集宁可冗余。
4. **驱动离场脏票**：高通用 sync_state 钳最高档兜底（#7）=「宁费电不塌域」；Fuchsia 用 remove_with_required_element（PM-08:285）。两边都证明**默认安全侧是高功耗而非低功耗**，显式撤票机制必须一等公民。legacy rpmpd 更进一步：floor 票域关着也照发（#9）。
5. **升档同步、降档异步**（#5）：延迟预算花在「变快」的方向，「变省」允许滞后——仲裁器不必对称等待。
6. **遥测被丢在 debugfs**（#11/#13）：四元组+唤醒原因分类是十年沉淀的最小集，但产品线拿不到（需 root）。BerryOS 把记账放一等接口（四件套 attribution loop）方向正确。
7. **私有 perf 面十年不收敛的根因**：perflock/mpctl 无声明式契约，全靠 dlopen 闭源库+socket 暗语（#27）；主线最后用「带内票 + 标准化 hint」（uclamp/ADPF）收编。教训：**接口没有公开契约就永远收不进生态**——vendor hooks 是这个失败的补丁而非解。
8. **MTK 深睡留在 ATF**（#23）：域开关进主线、深睡序列不进——「公开建模域、私下执行深睡」是厂商常态；OS 设计不能假设全部电源行为可声明，manifest 必须留「执行在远端」的表达位。
9. **SVS 只降不升**（#19）+ rpmhpd 钳最高档（#7）+ 错误相位回默认电压（#20）：三家同一哲学——**自动调压调档的安全侧永远是保性能/保供电**，省电优化失败回退到保守态。
10. **域成对建模**（cx/cx_ao，§1.1）：「active 态需求」与「睡眠可塌性」是两个正交维度，行业用 peer 域对表达——P3 用单 lease 表达时必须显式区分票型，否则丢失这个正交性。

## §4 BerryOS 启示

对 PM-00:238-239 P3/P3a 的逐位校准：

1. **`wake_latency_ns` 必填成立**（真实量级 0.75–4.4 ms 轨/簇级，u64 ns 表达无压力），但**必须成对加 `min_residency_ns`**（同必填）。仲裁器消费规则落到断言：`预期睡眠窗 < min_residency_ns 的域，不得裁决进入低档`。这是对 PM-08「latency 死在配置层没人吃」的正面补全——berry-power 裁决降档时消费 `(wake_latency_ns, min_residency_ns)` 二元组，缺一不可判。两值无固定比例，禁用推导默认。
2. **manifest 加执行者位**：`domains[].executor: LOCAL | REMOTE_PROC | REMOTE_HW`（2 bit）。
   - LOCAL=驱动服务直写寄存器（对应 MTK pm-domains/HW voter，#21/#22）；
   - REMOTE_PROC=票转发另一处理器执行（高通 RPM/Exynos ACPM/MTK SCP，#8/#25）；
   - REMOTE_HW=硬件聚合器直接吃票（RPMh ARC/VRM/BCM，#1/#10）。
   该位决定票的生效时机语义（立即 vs suspend 入口）与故障面：REMOTE_PROC/REMOTE_HW 域必须声明「执行者失联」策略，对齐 qmi_tmd 服务重启重同步（#14）。
3. **lease 扩展双票语义**：P3 四元组 hold/intent 之外加 `vote: ACTIVE | SLEEP`（默认 ACTIVE）。sleep 票由 berry-power 缓存、**suspend 入口统一 flush**——微内核形态下天然成立：flush 权单一授予 berry-power（对应「最后一个处理器才 flush」，#3；rpmh_flush 锁忙即弃同理）。驱动服务绝不逐票跨 IPC 打扰 suspend 路径，与「高频路径不跨特权界」（PM-00:316）一致。v1 可先只实现 ACTIVE 票 + SLEEP 票位预留。
4. **wake 集完整性断言**：仲裁器生成的恢复集必须完整（宁可高配），对齐 #3 教训；`auto_fallback_ms` 超时触发时恢复到完整 wake 集再降级。
5. **脏票检测入 G0–G5 断言**：域引用计数归零但域仍开 → 记账告警（对应 #4/#7 两家兜底手段）；默认安全侧=保持高功耗档，显式撤票才降。
6. **遥测字段集直接抄行业最小集**：每域 `{count, last_entered_at, last_exited_at, accumulated}` + 唤醒原因粗二值 `{rude, scheduled}`（#11/#13）。rude 唤醒率正好喂 P3a `false_wake_suppress` 的评估与落地刀 1 的唤醒记账；`last_wake_trans_dur` 类字段可用于校验 manifest 申报的 wake_latency 是否失真（申报 vs 实测偏差超阈值 → 告警）。
7. **升档快、降档慢**：berry-power 下发升档票可阻塞等确认、降档票异步记账（#5）。
8. **manifest 校验器加结构断言**：`deps[]` 里 assertive 依赖的父域 wake_latency 必须大于等于子域（高通用 mx→cx 父子结构为原型，§1.1）；`skip_retention_level` 类例外位（mxc）说明**档位表要支持 per-domain 剔除位**，P3a `levels[]` 建议留 mask。
9. **给 PM-16 的 sim 设计**（QEMU virt 全无上述硬件，须模拟）：
   - `sim-pd` MMIO 设备：配置区（wake_latency_ns、min_residency_ns、levels[]+mask、executor 位、min 窗拒绝开关）+ 计数器区（四元组 + rude/scheduled），由 berry-power 在域进出时读写；
   - sim 远程执行者：独立用户态「假 RPM」服务吃投票缓存，仅在 suspend 入口钩子统一应用 wake/sleep 集——QEMU 上即可验「票缓存→flush→应用→遥测回读」全链（绕开 PM-10 判定的无 SYSTEM_SUSPEND 限制：flush 语义在 berry-power 层模拟）；
   - 注入故障面：假 RPM 可配置失联，验 REMOTE_PROC 域的重同步路径（对齐 #14）；
   - 九断言挂点建议：A 系加「min_residency 短窗拒降」「脏票检测」「REMOTE_PROC 失联重同步」「suspend flush 单持有者」四条，直接映射 #3/#7/#14/#16。

## §5 引用源列表

主线源码（torvalds/linux master，2026-11 检视）：

- rpmhpd.c：https://github.com/torvalds/linux/blob/master/drivers/pmdomain/qcom/rpmhpd.c
- rpmh.c：https://github.com/torvalds/linux/blob/master/drivers/soc/qcom/rpmh.c
- rpmpd.c：https://github.com/torvalds/linux/blob/master/drivers/pmdomain/qcom/rpmpd.c
- qcom_aoss.c：https://github.com/torvalds/linux/blob/master/drivers/soc/qcom/qcom_aoss.c
- qmi_tmd.c：https://github.com/torvalds/linux/blob/master/drivers/soc/qcom/qmi_tmd.c
- qcom_stats.c：https://github.com/torvalds/linux/blob/master/drivers/soc/qcom/qcom_stats.c
- rpm_master_stats.c：https://github.com/torvalds/linux/blob/master/drivers/soc/qcom/rpm_master_stats.c
- mtk-svs.c：https://github.com/torvalds/linux/blob/master/drivers/soc/mediatek/mtk-svs.c
- mtk-pm-domains.c：https://github.com/torvalds/linux/blob/master/drivers/pmdomain/mediatek/mtk-pm-domains.c
- mtk-scpsys.c：https://github.com/torvalds/linux/blob/master/drivers/pmdomain/mediatek/mtk-scpsys.c
- exynos-acpm-pmic.h：https://github.com/torvalds/linux/blob/master/drivers/firmware/samsung/exynos-acpm-pmic.h
- sm8550.dtsi：https://github.com/torvalds/linux/blob/master/arch/arm64/boot/dts/qcom/sm8550.dtsi
- domain-idle-state.yaml：https://github.com/torvalds/linux/blob/master/Documentation/devicetree/bindings/power/domain-idle-state.yaml
- power-domain.yaml：https://github.com/torvalds/linux/blob/master/Documentation/devicetree/bindings/power/power-domain.yaml

官方/社区文档：

- LWN：RPMh communication support（2018）：https://lwn.net/Articles/748981/
- Qualcomm 官方 RPMh 文档：https://docs.qualcomm.com/doc/80-88500-4/topic/45_RPMh.html
- AOSP wahoo perf_lock_acq 移除提交：https://android.googlesource.com/device/google/wahoo/+/c6a673fef9c90d8bc57fc487640f0642aca74167%5E2..c6a673fef9c90d8bc57fc487640f0642aca74167
- LWN uclamp（793079）：https://lwn.net/Articles/793079/
- Android ADPF：https://source.android.com/docs/core/perf/performance-hint-api
- Android GKI kernel-code（vendor hooks）：https://source.android.com/docs/core/architecture/kernel/kernel-code
- tinylab 厂商性能优化综述：https://tinylab.org/android-perf-optimize-solutions-of-mobile-manufacturers
- MediaTek × ADPF 开发者故事：https://developer.android.com/stories/games/mediatek-adpf
- linux.org.hk 转载（vendor hooks 批评）：https://www.linux.org.hk/archive/20261015-7687-vendor-hooks-became-the-last-refuge-of-a.html

仓内锚点：

- `research/PM-00-全系统省电架构综合裁决-v1.1.md:238-239`（P3/P3a 定义）、`:316`（高频路径不跨特权界）
- `research/PM-08-Fuchsia电源栈源码级-v1.0.md:218`（板级电源配置 FIDL）、`:285`（抄/改表，latency 无人消费实证）
- `research/PM-10-无真机电源验证-v1.0.md`（QEMU 无 PSCI SYSTEM_SUSPEND）
- `research/PM-12-二期深研综合与PM00回填表-v1.0.md`（确定度图例与报告格式基线）
