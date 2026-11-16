# 06 号：idle 满核归因——三面证据链与修窗清单

> 实测日期：2026-11-14。服务对象：轮三 14 号令（12 号性能定标遗留开放题：guest 全静〔交互零重绘〕QEMU 仍 99% 单核，vgpu 档不消失〔99.5% vs 99.2%〕）。任务：三面归因（guest 谁在转 / QEMU wfi 行为 / 宿主壳轮询）＋修窗清单（可修面归档不代修）＋副刀（调度器设计 v1.1→v1.2 漂移回填归属先查）。环境：Apple M3 Pro / QEMU 11.0.3 Homebrew（与 07/09/10/12 号同机同版本）；镜像 `out/berryos.img`（sha256 `d477bb68…`，与 12 号 `berryos-b1bc.img` 逐字节同源）；探针全落 `/tmp/b14probe/`（8 组轮次 + 最小内核四变体 × 两加速器 + QMP/HMP 探针 + QEMU v11.0.3 源码三件），零仓改（本文与 README 索引行外）。方法：12 号 S1 桌面档形态复现（berry CLI `device start --profile desktop --accel tcg` + 手动 QEMU 同参复刻）＋ QMP/HMP 活体采样（`info registers` PC 采样 → 镜像内 KERNEL.ELF 符号化；`log int` 窗口记账 IRQ/SVC 率）＋最小裸内核 wfi 四变体 × TCG/HVF 定标 ＋ QEMU v11.0.3 源码一手直抓。确定度：**[高·实测]** = 本机实测/两源；**[高·源码]** = 源码一手；推断一律显式标注。

---

## §0 结论速览（五条，全部两源）

1. **主根因定谳（a 面，权重≈全部）：默认镜像的 boot 演示常驻任务永不退出，guest 结构性无 idle——「idle 满核税」的测量窗实为满载窗**。`task_a`/`task_b` 是无条件 spawn 的内核忙循环任务（kernel/src/main.rs :795-808 spawn；:2238-2254/:2257-2271 任务体 `-> !` 纯忙转零 yield；:749-752 头注「v1 任务不退出」`Box::leak`），加上 boot 期 EL0 演示舰队（good 'u'/abuser 'S'/waiter 'W'/pinger 'P'/ponger 'p'/spawner-pong 'C''s'——main.rs :837 起 + kernel/src/user.rs 模板）与 ktest 矩阵金丝雀（ktest::run，仅 `opt/berry/noktest` 门跳过——main.rs :726-733）。12 号自己的原始数据复核即坐实：其「idle 60s」窗内串口 **1141/1142 行**（s1A1/s1B1，其中 105/116 行 `switches=` 金丝雀 + 其余全为矩阵字母流）——「guest 全静」是**交互零重绘**（launcher 饿死无帧），不是 CPU 静 [高·实测]。
2. **PC 归因直抓（现场定谳）：稳态 vCPU 时间落在 task_a/task_b 忙循环体 + EL0 演示进程 syscall 环 + 异常入口**。HMP `info registers` × 14 采样（manual QEMU 同镜像，GFXF 后稳态）：`task_a+0x5c` × 6、`task_b+0x5c`、`exc_sync_el0+0x84`、`exception_irq+0x28`、EL0 用户文本 `0x10c4`/`0x1148`（镜像内 KERNEL.ELF 符号化）；`log int` 6s 记账窗：**SVC 59,614 次 ≈ 9.9k/s + IRQ 57 次 ≈ 9.5/s**。vCPU 线程 CPU 99.7%（t+12min 仍满，串口 1.1 行/s 持续——**系统永不静默**）[高·实测]。
3. **tickless 现态定谳：oneshot 机制已实现但默认镜像不可达，现役 timer 实为周期 10Hz tick**。runqueue 恒非空（忙环常驻）→ 单核 idle_loop 走「有活」支 `arm_slice + yield`（sched/src/sched.rs :3155-3162），timer IRQ handler 每次入口 re-arm 整片（arch/aarch64/src/timer.rs :184-194 `handle` → `arm_slice`；SLICE_HZ=10——:24-29）→ 10Hz 周期 tick 永续（实测 IRQ 9.5/s 吻合）。tickless park 机构本体（`arm_at` 到 SLEEPQ 最近 deadline——timer.rs :107-121；`mask` 深 park——:155-167；masked-wfi 三步闭环——kernel/src/main.rs :4990-5060〔CPU1〕/ sched.rs :3072-3138〔CPU0〕，Linux arm64 cpu_do_idle 同款）**在位且形态正确，只是被常驻忙环结构性饿死**——「周期 tick→oneshot 改造」不需要做，需要做的是把忙环请出去 [高·实测+源码]。
4. **QEMU 侧面（b 面）定谳：干净 park 两加速器都让出（0.0%）——idle 满核与 QEMU wfi 无关；但存在一条结构性自旋险径（pending-but-masked 中断 + wfi = 忙旋）**。最小裸内核四变体 × TCG/HVF 实测（/tmp/b14probe/minwfi）：v1 masked-wfi 纯 park = TCG 0.0% / HVF 0.0%；v4 CNTV IMASK 深 park（BerryOS 同形）= 0.0% / 0.0%；v2 纯忙循环 = 100.0% / 99.9%；**v3 挂起 GIC 线 + masked-wfi = 99.8% / 99.9%（`-d int` 零已取异常 = 纯 QEMU 侧自旋，guest 零前进）**。源码机制：TCG `HELPER(wfi)` 早退臂（target/arm/tcg/op_helper.c :371-401 `if (cpu_has_work(cs)) return;`）+ `arm_cpu_has_work` 只看 `CPU_INTERRUPT_HARD` **不查 PSTATE.I**（target/arm/cpu.c :143-173）；HVF `hvf_wfi` 同形早退 + vtimer 已过期早退（target/arm/hvf/hvf.c :2094-2129/:2072-2092）。07 号 HVF 档 idle 100.1% 与 TCG 同形——guest 忙（v2 形）即足解释，**无「HVF wfi 缺陷」** [高·实测+源码]。
5. **修窗清单（§4）首刀：boot 演示常驻面 fw_cfg 门控（owner 域 kernel）——预期 idle CPU → ~0%（P2 v1/v4 两锚）**；noktest 单独不够（p8 实证：task_a/b + EL0 舰队照转）。副刀判定：调度器设计 v1.2 漂移回填 **owner 当日已完成本体**（docs/调度器设计-v1.2.md 未跟踪新件 mtime 2026-11-14 01:06，头块 v1.2 增量逐项覆盖 01 号对账漂移面）——如实记 no-op，零碰（§5）。

## §1 方法与载体

### 1.1 轮矩阵（八轮 + 四变体定标）

| 轮 | 形态 | 加速器 | 窗 | 采集面 |
| --- | --- | --- | --- | --- |
| p1 | berry CLI desktop（12 号 S1 同形：1080x1920@60，gui=1，data disk） | TCG | boot→GFXF ok + idle 91s | `sample_cpu.py`（12 号同款 proc_pid_rusage）+ `stampser.py` 串口时戳 |
| p5 | 同 p1（长窗观察） | TCG | idle 300s（QEMU 早逝于 t+3s，见 §6） | 同上 |
| p6 | 手动 QEMU 同参复刻（+自有 qmp/gdbstub） | TCG | boot→GFXF + 稳态 ~12min | QMP `query-cpus-fast` / HMP `info registers` PC 采样 ×14 / HMP `log int` 6s 记账窗 / vCPU 线程分实体采样 |
| p7 | 手动 QEMU + `noktest` | HVF | boot（早 fatal） | 串口 |
| p8 | 手动 QEMU + `noktest` | TCG | boot→稳态 ~2min | 同 p6（PC 采样 ×10 + 线程采样） |
| m1-m4 | 最小裸内核四变体（`/tmp/b14probe/minwfi`，rustc aarch64-unknown-none 直编，-kernel ELF 直载） | TCG + HVF 各一遍 | 10s 定标窗 | proc_pid_rusage 分线程 |
| 复算 | 12 号原始数据（/tmp/b12probe/s1A1/s1B1 cpu.tsv + serial-stamped.tsv） | — | idle 尾窗 61s | 两源纪律对拍 |

**最小内核四变体**（~/tmp/b14probe/minwfi/src/main.rs，~60 行）：v1 = DAIF 全屏蔽 + `wfi` 死环（无定时器武装、无 GIC 使能——干净深 park 形）；v2 = 纯 `spin_loop` 死环（零 MMIO 零打印——哑旋者形）；v3 = GICv2 dist/cpuif 使能 + PPI27（CNTV）使能 + CVAL=0（恒过期）+ CTL=0b01（线拉高）+ DAIF 屏蔽 + `wfi` 死环（**挂起线 + masked-wfi 形**）；v4 = 同 v3 但 CTL=0b11（IMASK 源头屏蔽——**BerryOS 深 park 同形**，kernel timer.rs `mask()` 同值 0b11）。

### 1.2 机制取证（源码一手，当日实取）

QEMU v11.0.3：`target/arm/tcg/op_helper.c`（HELPER(wfi) 全文）、`target/arm/cpu.c`（arm_cpu_has_work 全文）、`target/arm/hvf/hvf.c`（hvf_wfi / hvf_arm_wfi_timer / hvf_wfi_timer_cb）。仓内 HEAD `17e3dd27` 工作树（路径:行号面见 §3/§7）。

### 1.3 采样口径两则（本轮新钉）

①**macOS 采坑（新，方法论件）**：`proc_pid_rusage(进程pid)` **不含 vCPU 线程**——线程是独立调度实体有独立 pid（p6：主 pid 0.0% 而 vCPU 线程 pid 99.7%；p8 同形 0.0% vs 78%）。QEMU CPU 必须采 `query-cpus-fast` 的 `thread-id`（或全 pid 枚举求和）。12 号当时「进程 pid + /usr/bin/time 对拍命中」的口径在其形态成立（本轮 p1 同法复得 94.7%），但分线程口径才是硬口径——后续 idle 测量纪律件（§4-5）。②「交互静 ≠ CPU 静」：idle 断言必须以**串口静默 + vCPU halted** 双条件，不能以零重绘代（12 号判例）。

## §2 数据

### 2.1 主税复现与两源对拍（p1 + 12 号原始数据）

| 量 | p1（本轮复现） | 12 号 s1A1/s1B1（原始数据复算） | 判读 |
| --- | --- | --- | --- |
| idle 窗 QEMU user | **94.7%**（91s 窗，进程口径） | **99.6% / 99.2%**（idle 尾窗 61s 复算；报告值 99.5%/99.2% 两源吻合） | 满核复现，两轮两法一致 |
| idle 窗串口流量 | **1738 行/91s ≈ 19 行/s**（1681 矩阵字母行 + 55 `switches=` 行 + 2 其它） | **1141/1142 行**（含 105/116 `switches=` 行 + 矩阵字母） | **「idle」窗 = 矩阵满载窗**——12 号测量时点串口在刷，税非 idle 税 |
| switches 速率 | **166 次/s** 持续（2800→17000） | 同量级（60s 窗 10.5k-11.6k 次 ≈ 175-193/s） | 切换churn常驻——EL0 舰队 syscall 环 + 帧片轮转 |
| LAUNCH ok | 未到（饿死，09 号 §3-3 同判） | 未到（两档同） | launcher 首帧饿死 = 「交互零重绘」之来源——桌面静是饿出来的静 |

### 2.2 PC 归因与中断记账（p6，manual QEMU 同镜像，稳态）

- **PC 采样 ×14**（HMP `info registers`，0.5s 间隔）：`task_a+0x5c` × 6、`task_b+0x5c` × 1、`exc_sync_el0+0x84` × 2、`exception_irq+0x28` × 1、EL0 用户文本 `0x10c4`/`0x1148` × 4——**忙环体 + EL0 syscall 环 + 异常入口三件套**，零样本落在 wfi/park/idle_loop [高·实测]。
- **`log int` 6s 记账窗**：SVC 59,614 次 ≈ **9.9k/s**（EL0 舰队 syscall churn——与 switches 166/s 并存：多数 SVC 快路径零切换）；IRQ 57 次 ≈ **9.5/s**（CNTV tick，10Hz 周期形态吻合 §3-3 机制链）[高·实测]。
- **vCPU 线程**：99.7%（t+12min）；主循环线程 0.0%；串口 1.1 行/s（boot 后期字母节流态）——**12 分钟不静默，无终态化** [高·实测]。

### 2.3 最小内核 wfi 定标（m1-m4 × TCG/HVF）——b 面结构性定谳

| 变体 | 形态 | TCG user | HVF user | 判 |
| --- | --- | --- | --- | --- |
| v1 | masked-wfi 纯 park（无线无源） | **0.0%** | **0.0%** | 干净 park 两加速器都让出——QEMU 无结构性不让 |
| v2 | 纯忙循环（哑旋者） | 100.0% | 99.9% | 静默旋者即满核——**主税形态**（串口非必要条件） |
| v3 | 挂起 GIC 线 + masked-wfi | **99.8%** | **99.9%** | **结构性自旋险径**：pending-but-masked → wfi 早退环（TCG `-d int` 窗 417,696 行中**零已取 IRQ**——纯 QEMU 侧转，guest 零前进） |
| v4 | CNTV IMASK + masked-wfi（BerryOS 深 park 同形） | **0.0%** | **0.0%** | 现役 park 形在两加速器下都真睡 |

**机制链（源码直抓，v11.0.3）**：TCG `HELPER(wfi)`——`if (cpu_has_work(cs)) { return; }`（op_helper.c :386-390，注释自认「Don't bother to go into our low power state if we would just wake up immediately」）→ 早退后翻译块续行、guest 回到 wfi 再翻译执行 = 忙旋；`arm_cpu_has_work` = `CPU_INTERRUPT_HARD` 置位即真、**不查 PSTATE.I**（cpu.c :166-172）——GIC 线拉高即触发，与 guest 是否屏蔽取中断无关。HVF `hvf_wfi` 同形（hvf.c :2096-2100 `if (cpu_has_work(cpu)) return 0;`）另加 vtimer 已过期早退臂（:2118-2121，`hvf_arm_wfi_timer` 返回 -1 时「vtimer already expired, don't halt」）。真机语义：WFI 挂起后由 pending 线唤醒（PSTATE.I 只挡取异常不挡醒）——QEMU 用「早退续行」模拟同一前进语义，代价是**醒条件已成立期间 vCPU 满速空转**；v3 把该窗口做成永久即 100% [高·源码+实测]。

### 2.4 noktest 反证（p8）——修窗范围钉板

TCG + `noktest`（跳 ktest::run，out/qemu-noktest.sh 同款键）：矩阵字母照流（0.7 行/s），PC 采样同落 `task_a+0x5c`/EL0 文本/异常入口，vCPU 线程 78%（背景负载窗，主循环线程 0.0%）——**noktest 不解 idle 税：task_a/task_b spawn 无条件（main.rs :795-808 在 ktest 块之外）+ boot EL0 舰队无条件（:837 起）**。77-99% 波动为宿主背景窗（owner 会话在场，§6），结构性判「永不 idle」由源码钉死（任务体 `-> !` 零退出零阻塞）[高·实测+源码]。

### 2.5 HVF 真镜像（p7）——既有缺口如实

HVF + noktest boot 早 fatal（x14=0x21038 族寄存器 dump → `fatal, halt`）——07 号 §4-2/09 号 §3-4 既有缺口族，当前树 HVF 真镜像 idle **不可测**。HVF 面结论落在两锚上：07 号 HVF headless idle 100.1%（其世代镜像，[高·实测]引用）+ 本轮 v2 形 HVF 99.9%（哑旋者满核不挑加速器）——07 号的 HVF 满核无需「HVF wfi 缺陷」假设即解释成立 [高·两源]。

## §3 三面归因（证据链 → 权重）

### 3.1 a 面（guest 谁在转）——主根因，权重≈100%

**证据链**：①常驻忙环源码钉死（task_a/task_b `-> !` 忙循环 + EL0 演示舰队节流字母环 + ktest 矩阵金丝雀——§0-1 引）；②12 号自己 idle 窗原始数据复算坐实满载（§2.1）；③本轮 p1 复现 94.7% + 19 行/s 串口 + 166 switches/s（§2.1）；④PC 归因直落忙环体/EL0 环/异常入口，零样本落 park（§2.2）；⑤9.9k SVC/s + 9.5 IRQ/s 记账（§2.2）；⑥noktest 反证排除 ktest 单因（§2.4）；⑦t+12min 无终态化（§2.2）。**定谳：idle 满核税 = guest 常驻演示面的真满载，非调度/定时器/QEMU 缺陷**。vgpu 与 ramfb 不可分辨（12 号 §2.1）由此平凡解释：税与显示链路无涉。

**服务面排除（任务书点名三处的现态）**：svcmgr 主 serve = `wait_many` 并等（user/bins/svcmgr/src/main.rs :483-567，帽 10s :94/:794），pager 空闲 50ms 轮询（:125 `PAGER_IDLE_POLL_NS`，20Hz 唤醒源——真 idle 时的主醒源，量级无害但见 §4-4）；guifbd serve = 三路确定性轮询 + `wait_many` 并等（user/bins/guifbd/src/guest.rs :1393-1431，帽 10s :116，软 vsync 16.67ms 仅 vblank 有人订时排点 :113/:327-366——零唤醒不变量在册）；simbattery = 纯阻塞收（user/bins/simbattery/src/main.rs :40-110，1h 长帽）；launcher frameloop = `wait_many` + `next_wakeup` 帽 10s（user/bins/berry-launcher/src/frameloop.rs :617-640，:95/:99）。**服务 wait 面全体阻塞形，无轮询税源**；`object_wait_many` 是阻塞并等非轮询。

### 3.2 b 面（QEMU wfi 行为）——清白 + 一条险径

干净 park（v1/v4）两加速器 0.0%——**wfi 让出无结构性障碍**；唯一险径 = v3 形（pending-but-masked + wfi = 早退忙旋）。对 BerryOS 现役 masked-wfi 语的含义：正常 tick 路径无险（CNTV 到期 → guest 醒 → enable_irq 取走 → handler re-arm CVAL 线落——实测 IRQ 9.5/s 干净无 storm）；险径触发条件 = 某 level IRQ 源 pending 后 handler 不排干且 PSTATE.I=1 期间 park——病理态才达，当前不触发。**b 面不是现税来源**；v3 记为结构性在册险径（修窗 §4-3）。

### 3.3 c 面（宿主壳轮询）——已退役项，如实引

12 号 §2.1：headless 形态 berry host 3.0%（壳轮询不跑）；16ms 盲轮询只在窗口形态（07 号 §0-4 的 7-9%）—— berry 进程自己的税，与 QEMU 进程 99% 分属两实体。**c 面自始不是 QEMU 满核的候选**，12 号已判，本轮无新证不动。

### 3.4 tickless 现态定谳（任务书点名题）

现役 `on_irq_tick` 是**周期 tick 形态**（10Hz，handler 每入口 re-arm 整片——timer.rs :184-194；实测 9.5 IRQ/s 吻合），但这是**忙环常驻的后果不是设计缺件**：oneshot 机构全在位（park `arm_at` 最近 deadline = oneshot 武装面——timer.rs :107-121 + sched.rs :3122-3129/:5036-5043；授片夹取防跑步机——timer.rs :144-153）。忙环请出后 idle_loop 走 `mask`+masked-wfi 或 `arm_at`+wfi，tick 自然停（v1/v4 锚 0.0%）。**「周期 tick→oneshot 改造」无改造件，只有可达性件**——修窗归并 §4-1。

## §4 修窗清单（每件：域归属 + 预期收益；归档不代修）

| # | 修件 | 域 | 内容 | 预期收益 | 证据锚 |
| --- | --- | --- | --- | --- | --- |
| 1 | **boot 演示常驻面 fw_cfg 门控** | kernel（owner） | task_a/task_b spawn（main.rs :795-808）+ boot EL0 演示舰队（:837 起）挂 `opt/berry/nodemo` 类具名键（noktest 先例同形 :726-733）；产品/桌面档默认缺席 | **idle CPU → ~0%**（两加速器）；tickless park 机构即刻生效（§3.4）；launcher 饿死面大概率同愈（09 号 §3-3 的 boot 相拥挤源同批） | v1/v4 0.0% 两锚 + p8 反证 |
| 2 | ktest 金丝雀窗收敛 | kernel（owner） | 矩阵金丝雀轮数上限或产品档默认 noktest（boot 相 TCG 拥挤——09 号 §3-3 判「现行树 TCG 饿死 launcher 首帧」） | boot 相墙钟下降；LAUNCH ok 复达；**不治稳态 idle**（p8 证） | p1/p8 对照 |
| 3 | masked-wfi 险径防御 | kernel（owner，低优） | 病理态兜底：park 环 wfi 立返计数（如 N 次立返且 pending 线不变）→ 诊断行 + 主动 unmask 取走 | 关闭唯一 QEMU 侧结构性自旋面（v3）；正常态零触发 | v3 99.8%/99.9% + 源码三件 |
| 4 | 服务唤醒率面（真 idle 后的主醒源） | svcmgr/guifbd（可做域，owner 之外） | pager 50ms 轮询（svcmgr main.rs :125）事件化；guifbd vblank 有订时 60Hz 排点（guest.rs :113）按需化 | 真 idle 后唤醒率 20Hz→更低（续航面二阶）；CPU 面已 0% 后此为余量件 | §3.1 服务面枚举 |
| 5 | idle 测量纪律件 | 测量（可做域） | ①idle 断言 = 串口静默 + vCPU halted 双条件（「交互静≠CPU 静」——12 号判例）；②QEMU CPU 必采 `query-cpus-fast` thread-id（macOS 进程 pid 不含 vCPU 线程——本轮 p6/p8 坑）；③背景负载窗如实标 | 后续 idle/续航测量不复踩 | §1.3 两则 |

**收益排序**：件 1 单刀即解主税（≈100% → ~0%）；件 2 治 boot 相；件 3-5 病理/余量/纪律件。**修件 1 落地前，一切「idle 档」性能测量（含 12 号 §5 表 idle 行）实为满载档**——后续显示/调度定标引用须带此注。

## §5 副刀：调度器设计 v1.1→v1.2 漂移回填——判定 no-op（owner 当日已完成，如实记）

- **归属先查结果**：`docs/调度器设计-v1.2.md` = 未跟踪新件（git `??`），mtime **2026-11-14 01:06**（本会话进行中）——owner 正在写。
- **内容核对**：v1.2 头块增量逐项覆盖 01 号对账漂移面——SMP 现实回填（SMPD-3 band 全局原子账/SMPD-4 per-CPU 分账/双核终验注 ：78）；性能口径 pick=22cy 入册（基线链注记 21→26→22 带时点）；timer slack 判定律纯面注记 + 机制面接线窗如实（:156，挂 CP-11/裁十债一）；PI 面在途演化指针（:89，一事实一归属指 IPC设计 §4-3b）；PM 双服务 §5.4。
- **判定**：副刀任务面（漂移回填）**owner 在动且本体已毕**——按任务书纪律「owner 在动则 blocked 如实」记 no-op，零碰。其文末自记残余（`docs/调度彻底计划-v1.0.md` 12 处链接待并行会话解占后随迁）在册，不重复立账。

## §6 环境限制如实

- **背景负载**：owner 会话全程在场（其 berry shell 会话 TCG+noktest 跑 /tmp/berryos-hvf-debug.img + smoke-image 会话两枚 QEMU 实测可见）——绝对值有抬底/挤压（p8 的 78% vs p1 的 94.7% 同因），A/B 与结构性判读不受扰（v1/v4 的 0.0% 为硬零）。
- **p5 早逝**：berry CLI 长窗轮 QEMU 于 GFXF 后 ~3s 无声退出（host.log `QMP: connection closed`，qemu.log 0 字节无 stderr，串口止于矩阵行中段）——未定谳（单发，不复现于 p6/p8 手动同参轮）；长窗结论由 p6（12min 手动轮）承接。
- **HVF 真镜像不可测**：当前树 HVF boot 早 fatal（p7，0x21038 族——07 号 §4-2 既有缺口族），HVF 面以 07 号引用 + 最小内核 HVF 定标双锚闭合。
- **PC 采样粒度**：HMP `info registers` 为即时快照非追踪——14 样本 × 3 轮一致落点（忙环体/EL0/异常入口）足以定谳归属，但不作各任务精确 CPU 占比（那需要 icount/B-2 通道，02 号 §6 在册）。
- **switches 166/s 的精确分解**（tick 轮转 vs 直切臂 vs SVC 链）未逐臂分账——归因不依赖此分解（主判已由 PC 采样直落），如实标。
- **12 号窗内矩阵构成与其轮的差异**：其 s1A1 idle 窗 switches 行 105-116 vs 本轮 p1 55/91s——同形不同率（boot 相深浅/任务混合差异），满载判读不受影响。

## §7 来源索引

- **本轮实测产物**：`/tmp/b14probe/`——`run1.sh`/`run5.sh`（berry CLI 轮）/`probe_qmp.py`/`probe2.py`（QMP/HMP 探针）/`measure.sh`（最小内核定标）；`minwfi/`（最小内核源 + link.x，rustc aarch64-unknown-none 直编）/`minwfi-v1..v4.elf`；数据 `p1/{cpu.tsv,serial-stamped.tsv}`、`p6/{pcs.json,irqlog.txt,qemu.log,serial.log}`、`p8/serial.log`、`p2/q-tcg-v*.err`；`kernel-image.elf`（镜像内 KERNEL.ELF FAT16 链提取，符号化基准）。12 号遗留复用：`/tmp/b12probe/{sample_cpu.py,stampser.py,s1A1,s1B1}`。
- **QEMU v11.0.3 源码（当日实取，存 /tmp/b14probe/qemu-src/）**：[op_helper.c](https://gitlab.com/qemu-project/qemu/-/blob/v11.0.3/target/arm/tcg/op_helper.c)（HELPER(wfi) :371-401——cpu_has_work 早退臂）；[cpu.c](https://gitlab.com/qemu-project/qemu/-/blob/v11.0.3/target/arm/cpu.c)（arm_cpu_has_work :143-173——不查 PSTATE.I）；[hvf.c](https://gitlab.com/qemu-project/qemu/-/blob/v11.0.3/target/arm/hvf/hvf.c)（hvf_wfi :2094-2129/hvf_arm_wfi_timer :2072-2092/hvf_wfi_timer_cb :2038-2052）。
- **仓内（HEAD `17e3dd27` 工作树，路径:行号）**：kernel/src/main.rs :726-733（noktest 门）/ :749-752（忙环任务头注「v1 任务不退出」）/ :795-808（task_a/b 无条件 spawn）/ :837-844（boot EL0 舰队）/ :2228-2230（main idle 路径）/ :2238-2254（task_a 体）/ :2277（A_PRINT_EVERY=2M）/ :4990-5060（CPU1 masked-wfi park 环）/ :5231（CPU1 park 10Hz 注）；sched/src/sched.rs :56-77（switches 金丝雀）/ :3072-3164（idle_loop 单核两支）/ :3318-3340（on_irq_tick）；arch/aarch64/src/timer.rs :24-29（SLICE_HZ=10）/ :107-121（arm_at oneshot）/ :144-153（arm_slice 夹取）/ :155-167（mask 0b11）/ :184-194（handle rearm）；kernel/src/user.rs :1616-1700（EL0 演示模板族）；user/bins/svcmgr/src/main.rs :94/:125/:265-348/:483-567；user/bins/guifbd/src/guest.rs :113/:116/:1393-1431；user/bins/simbattery/src/main.rs :40-110；user/bins/berry-launcher/src/frameloop.rs :95/:99/:617-640；tools/berry/src/qemu.rs :135-160/:219-222（spawn_image——无 `-smp` = 单 vCPU）；docs/调度器设计-v1.2.md（副刀证据件）。
- **谱系**：12 号（§2.1 idle 满核原始数据 + §5 ④ 开放题本体——本轮主证源）；07 号（§0-1 idle 三源互证/HVF 100.1% 引/§5-③ ramfb 排除面）；09 号（§3-3 TCG 饥饿判——与 §0-1 同源印证）；01 号（调度彻底计划对账——副刀基准）；03 号（刀序里程碑——本文落其研究位）；README.md（目录索引）。
