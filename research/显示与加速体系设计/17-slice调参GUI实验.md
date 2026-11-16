# 17 号：slice_hz 调参×GUI 性能活体实验——三档 boot 链实测与 sweet spot 侧写

> 实测日期：2026-11-14。服务对象：G5 bootparams 全链落地后的调度参数活体定标（调度+性能优化轮 B 刀）——`sched_slice_hz` 三档（default=编译缺省 10Hz / 250 / 1000）×GUI 桌面档 boot，调度参数对 GUI 帧链（FBW→FBSERVE→WM→GFXF→GFX→LAUNCH v=）的因果实证；产出供 bootparams 默认值与 IntentProfile 档位（`docs/调度器设计-v1.2.md` §3.8）参考。环境：Apple M3 Pro / 18GB / QEMU 11.0.3 Homebrew + AAVMF edk2-stable202408（与 07/09/12 号同机同版本）；TCG 单档（HVF ktest fatal 既有缺口族，12 号 §6 同判）。载体：HEAD `853ac85c` 工作树自建三盘（内核 sha256 `1886834b…` 三盘同源；rootfs 基座 60 条目 `cmp` 字节恒等、250/1000 盘 = +1 条目 `etc/boot-params.toml`〔`berry_assemble --emit` 产件，G5 产品链同形〕；唯一变量 = boot-params）。探针全落 `/tmp/slice-gui/`（harness 3 件 + 6 靴数据，零仓改，本文与 README 索引行外）。方法：三盘各自双跑（稳定性两源）+ 串口 marker 到达时戳（chunk 粒打点）+ `switches=` 金丝雀计率（每 100 切换一行）+ QEMU CPU 双源（proc_pid_rusage 24MHz timebase 1s 采样 × RUSAGE_CHILDREN 差分）。确定度：**[高]** = 本机实测/源码一手；**[中]** = 模型推演（锚已实测）；推断一律显式标注。

---

## §0 结论速览（六条，除标注外全部双跑两源）

1. **缺省 10Hz 在 GUI 桌面档全轴最差，且差得结构性**：marker 链 FBW→LAUNCH v= 9.28/9.65s（250 档 4.52/4.43s、1000 档 3.46/3.28s——快 2.1×/2.8×）；boot 段 QEMU CPU 12.2/13.2s → 6.45/6.47 → 5.49/5.47s；稳态吞吐（演示矩阵字符率）220/214 B/s → 269/259 → 417/421（+26%/+91%）；稳态 QEMU CPU 100.0/99.8% → 83.0/84.5% → 84.6/84.8%（缺省档多烧 ~15 个百分点还干更少的活）[高·实测]。
2. **因果分解定谳：等待相 ∝ 片长、计算相付切换税**——链内三段等待主导相随 slice 缩短塌缩：FBSERVE→WM 2.32s→0.30s→0.09s（26×）、GFX→LAUNCH 3.57s→1.00s→0.46s（7.8×）、GFXF→GFX 2.60s→1.45s→0.96s（2.7×）；唯一计算主导相 WM→GFXF 反向变慢 0.97s→1.72s→1.84s（+77%/+89%——tick+抢占在 CPU 密集相的净税）；slice 不敏感相如常（ROOTFS ok 5.85-5.89s——固件相在注入点之前；ktest done 6.06-6.10s 六靴恒定——注入后纯计算主导相，不受片长影响）[高·实测，机制分解标注推断见 §3]。
3. **switches 计率 = slice_hz 的活体直读**：稳态 124/123 /s（10Hz）→ 333/300（250Hz）→ 1076/1079（1000Hz）——1000 档恰好贴住 timer 硬底（`arm_slice` 夹取保证最早 1 个 SLICE 内必响 tick，`arch/aarch64/src/timer.rs:126-136`），即 1000Hz 档切换已被定时器驱动主导；10Hz 档 124/s ≫ 10 说明缺省档切换主体是块驱动（console-sender 环等）[高·实测；归因推断]。
4. **缺省档稳态独占+自旋饥饿实锤**（run-length 证据链）：10Hz 档稳态矩阵同类字母连续串均值 8.64/8.29（p50=8、最长 288/128 字符——任务一次占核倾泻整串），250/1000 档塌到 1.37/1.37 与 1.00/1.00（p90≤2/1——逐字符轮转）；10Hz 档还有 40s 级慢衰变（switches 斜率 20s 窗 ~190/s 滑落到 ~105/s 落定）而 250/1000 全程恒定。CPU 面（100% vs 84%）+吞吐面（低 47%）+run-length 三角互证：**缺省档有任务占核自旋等对方、对方够不着核**——时间片过长在无唤醒抢占路径上的等待者身上直接变现为饥饿（机制锚 `arch/aarch64/src/timer.rs:132` console-sender 重试环 spin→svc→block，标注推断）[高·实测三角；机制归因推断]。
5. **消息唤醒即时性不受 slice_hz 影响（机制面）**：唤醒路径 try_preempt 直切不等 tick（`sched/src/sched.rs:43` 六切换原语位；`docs/显示管线调度集成指南-v1.0.md:128` 明文）——受 slice 影响的是**超时唤醒粒度与无事件 CPU 让渡**（同文 :129 的预判在本实验全兑现：帧预算 16.67ms 内超时机会 0.17→4.2→16.7 次）。间接佐证 = 等待相塌缩恰发生在超时/节拍驱动的三段而计算相反向变慢（若链段是消息驱动则 10Hz 档也该快——见 §3.1 分解）。boot 期 HDRWAKE 探针六靴恒 62-63 cycles（≈1.0µs @62.5MHz，`kernel/src/ktest.rs:7616-7634`——busy-poll deadline 环、ktest 单核窗**零调度参与**，按构造与 slice 无关，如实记：它不是唤醒路径探针）。负载下 RT 半减后走平：AIDEMO rt1 12.68/12.49ms → 5.82/5.93 → 6.57/6.48（boot 期争用 RT 减半，250→1000 不再降——RT 地板由块驱动环节拍决定，推断）[高·实测；唤醒即时性=源码机制非本轮测量]。
6. **sweet spot 侧写：GUI/交互产品档取 [250,1000]，缺省 10Hz 不适合 GUI 桌面**。250 档拿到链收益的 82%（9.47→4.48s）、切换率 300/s 温和；1000 档链再快 1.1s、吞吐再 +58%、切换 1077/s 已贴 timer 底、计算相税 +0.87s 可见但被收益盖过——本负载上单调向好直到词表顶格，无实测轴在 1000 内转差。推荐：bootparams 默认/IntentProfile desktop-interactive 档落 **250 为均衡默认、1000 为响应优先档**；>1000（词表外）不建议线性外推（未测）。`docs/显示管线调度集成指南-v1.0.md:129` 的 128 建议在本数据带内方向一致（按单调趋势推断有效，128 点未测）[推荐=推断，数据锚实测]。

## §1 方法（三盘投放面 + 轮矩阵 + 工具口径）

### 1.1 三盘与 G5 同形投放（零仓改）

| 件 | 值 | 说明 |
| --- | --- | --- |
| 内核 | `target-cargo/…/debug/berry-kernel` sha256 `1886834b…` | 三盘同源（`just build` 产物——裸 `cargo build` 撞 G12 `~/.cargo-target` 沙箱拒，绕道 `CARGO_TARGET_DIR=target-cargo` 第五轮复现）；HEAD `853ac85c`+M 工作树（窗内并行 docs-only 提交 `34fc9a25` 零代码面，构建产物 ident 一致） |
| rootfs 基座 | 60 条目 34975744B，mkcpio 复现件与 `rootfs.cpio` **cmp 字节恒等** | 保真腿：不带额外条目重跑 `mkcpio --manifest tools/rootfs-manifest.toml` 后 cmp——三盘基座唯一变量确证 |
| boot-params 件 | `berry_assemble.py --emit` 产 32B/33B（`schema = 1\nsched_slice_hz = 250|1000\n`） | G5 产品链同形（43 号 §1 五段链的投放件口径；序列化单点 `tools/berry_assemble.py:1020` 起）；default 盘不投放（缺省零扰动合同，`kernel/src/bootparams.rs:20-29`） |
| 三盘 | mkimage 96MiB：default `781ab9d4…` / 250 `06f97034…` / 1000 `3d5e85a9…` | `just image` 同参复刻（`--efi/--hello/--kernel/--initrd/--size-mib`），三盘唯一差 = initrd 第 61 条目 |
| 投放验真 | 250 盘 eco `bootparams slice_hz=250 slice=250000`、1000 盘 `bootparams slice_hz=1000 slice=62500`、default 盘 bootparams 行数 0 | eco 行与 ROOTFS ok 相邻（+0.1ms）；slice= 值与算术 62.5MHz/hz 双源吻合（`kernel/src/bootparams.rs:317-323` 换算直证） |

### 1.2 GUI 桌面档 boot 形态（berry CLI spawn_image 复刻）

QEMU 11.0.3 `-machine virt -accel tcg -cpu cortex-a72 -m 256M` + AAVMF 双 pflash + 整盘 usb-storage（snapshot=on）+ FAT12 数据盘（`BERRYOSVBLK!` magic——launcher BLK_OK gift 门）+ ramfb + `fw_cfg opt/berry/display=1080x1920@60`（gui 位真值源）+ rtc 注入——命令形逐段取自 `tools/berry/src/qemu.rs` `machine_args`/`spawn_image`；裁面 = VNC/QMP/ram-backend/tablet（宿主呈现与输入面，marker 链零消费，如实记）。与 12 号 S1 桌面档同几何同 TCG 档。

### 1.3 轮矩阵与采集面

三盘 × 双跑 = 6 靴，每靴 170s（boot ~8-18s + 稳态 ≥128s），顺序跑（TCG 满核互扰禁并行）。采集：串口 chunk 到达时戳（`bootone.py`——QEMU stdout 管道读者，行到达打点）；switches 金丝雀（`sched/src/sched.rs:56-70` 每 100 切换一行 `switches=N kl=0`，kl 六靴全 0——打印锁争用不在本实验面）；QEMU CPU = proc_pid_rusage 1s 采样（24MHz timebase，12 号 §1.2 定标）+ RUSAGE_CHILDREN 差分双源（两法对拍差 <1%）。LAUNCH ok 在 TCG 下饿死（12 号 §6 既有，六靴一致）——链口径止于 LAUNCH v=。

## §2 数据（六靴全表）

### 2.1 marker 链时序（s，boot 起算；每臂 r1/r2）

| marker/段 | default r1/r2 | 250 r1/r2 | 1000 r1/r2 | 判读 |
| --- | --- | --- | --- | --- |
| ROOTFS ok | 5.868/5.860 | 5.888/5.863 | 5.865/5.853 | 恒定——固件+内核 init 相，slice 生效前+纯计算 |
| ktest done | 6.082/6.073 | 6.097/6.072 | 6.080/6.063 | 恒定——ktest 计算面 |
| AIDEMO ok | 7.110/7.118 | 6.300/6.270 | 6.260/6.232 | 缺省慢 ~0.85s——AIDEMO 自睡 2×slice（`kernel/src/main.rs:3999-4009`：200ms vs 8ms/2ms）+ 争用 RT |
| FBW ok | 8.008/8.570 | 7.841/7.969 | 8.193/8.120 | 恒定——guifbd fbinit 计算面 |
| FBSERVE→WM | 2.452/2.194 | 0.286/0.312 | 0.084/0.100 | **26× 塌缩**——等待相 ∝ 片长 |
| WM→GFXF | 1.104/0.845 | 1.684/1.763 | 1.847/1.831 | **反向 +77%/+89%**——计算相切换税 |
| GFXF→GFX | 2.618/2.579 | 1.470/1.429 | 0.971/0.957 | 2.7×——帧服务等待相 |
| GFX→LAUNCH | 3.103/4.028 | 1.079/0.929 | 0.530/0.389 | 7.8×——launcher 装载等待相 |
| **FBW→LAUNCH v= 合计** | **9.278/9.646** | **4.518/4.433** | **3.456/3.276** | 2.1×/2.8×——总账 |
| boot 段 QEMU CPU | 12.19/13.21 | 6.45/6.47 | 5.49/5.47 | 与墙钟同向（第二源对拍） |

### 2.2 稳态面（窗 [40,168]s；括号 = 晚期相 [90,168]s——缺省档有衰变分相记）

| 量 | default | 250 | 1000 | 判读 |
| --- | --- | --- | --- | --- |
| switches/s | 124.0/123.3（105/101） | 333.3/299.5（339/290） | 1076.4/1078.5（1069/1074） | 1000 档贴 timer 底（≥1 tick/slice 硬底）；10Hz 档主体块驱动 |
| 矩阵字符 B/s | 219.7/214.1（201/192） | 269.3/258.7（282/239） | 416.5/421.1（414/422） | 吞吐 +26%/+91%——同负载同窗 |
| QEMU CPU 稳态 % | 100.0/99.8 | 83.0/84.5 | 84.6/84.8 | 缺省档满核自旋；两臂多 15pp park 窗 |
| QEMU user 合计（170s） | 164.7/164.5s | 136.5/138.9s | 138.3/138.3s | 两源对拍（采样/差分）差 <1% |
| run-length 均值（p50/p90/max） | 8.64/8.29（8/8；18/15；288/128） | 1.37/1.37（1/1；2/2；10/6） | 1.00/1.00（1/1；1/1；8/8） | 独占簇直接读数——10Hz 档任务倾泻整串 |

### 2.3 副指标

| 量 | default | 250 | 1000 | 判读 |
| --- | --- | --- | --- | --- |
| HDRWAKE p50（cycles，n=128） | 63/63 | 63/62 | 62/63 | busy-poll deadline 探针（ktest 单核窗零调度参与）——按构造与 slice 无关；250-r2 单靴 p99=187 尖峰如实记 |
| AIDEMO rt1/rt2（µs） | 12684/13625 与 12494/14701 | 5816/5853 与 5934/5984 | 6574/5682 与 6484/6468 | 争用 RT 减半后走平——地板=块驱动环节拍（推断） |
| `kl=` 打印锁争用 | 全 0 | 全 0 | 全 0 | 本实验不在打印锁面 |
| pager watchdog fired | 1/1 | 2/1 | 2/2 | 各臂均有 boot 期 1-2 次——非臂间差异项 |

## §3 因果分析（slice_hz↔GUI 帧链机制分解）

### 3.1 等待相 ∝ 片长（超时唤醒粒度 + 无事件 CPU 让渡）

三个塌缩段（FBSERVE→WM 26×、GFX→LAUNCH 7.8×、GFXF→GFX 2.7×）的公共结构 = 等待方靠**超时/节拍**推进而非消息唤醒（gui 桌面档的服务装配序：gfxdemo 等 WM 槽、launcher 等 GETINFO/BLK gift、帧循环等 serve 轮到）：`arm_slice` 夹取后 tick 硬底 = 最早 1 个 SLICE 内必响（`arch/aarch64/src/timer.rs:126-136`），超时扫描只在 tick 跑（`docs/显示管线调度集成指南-v1.0.md:128-129`）——等待方的最坏滞留 = 1 个片长，10Hz 档 = 100ms 级。帧预算口径：16.67ms 内超时机会 10Hz=0.17 次 / 250=4.2 次 / 1000=16.7 次——「缺省配置下一个帧预算内连 1 次超时调度机会都不保」（指南原句）在本链实测兑现为段耗时 26× 差。消息唤醒路径本身不等 slice（try_preempt 直切，`sched/src/sched.rs:43`）——**slice_hz 的真实作用面 = 超时等待者 + 无唤醒事件的 CPU 让渡**，不是唤醒即时性 [高·数据+机制源码；段归因推断]。

### 3.2 计算相的切换税（唯一反向项）

WM→GFXF（gfxdemo 帧准备+guifbd serve 计算密集段）随 hz 单调变慢 0.97→1.72→1.84s：boot 期演示常驻面在跑，1000 档下计算任务每 1ms 被 tick 抢占让位矩阵任务（1077 切换/s），每次切换 = 上下文保存/恢复 + TCG 下 cache/TLB 局部性损失。税的量级：+0.75s（250）/+0.87s（1000）≈ 链总收益的 15-25%——被等待相塌缩盖过但真实存在 [高·实测；成本分解推断]。250→1000 该段只 +0.12s（切换率 ×3.6 而税近饱和）——税对 hz 的弹性在高位收平，推断与 tick 硬底行为（1000 后每片必抢）有关。

### 3.3 稳态自旋饥饿与 park（缺省档 100% CPU 之谜）

三角证据（§2.2）：10Hz 档 CPU 100% 但吞吐最低 + run-length 8.6 字符倾泻 + 40s 慢衰变；250/1000 档 84% CPU、run-length ≈1、吞吐最高且恒定。机制侧写（推断）：console-sender 重试环 spin→svc→block（`arch/aarch64/src/timer.rs:132` 注释自况）——占核者自旋等对端（console 排水/对方回合），对端在就绪队列但拿不到核；片长 100ms 时每次独占烧满整整一片，等待方最坏饿 100ms；4ms/1ms 片下独占窗口塌缩、对方很快够到核、整管线前进，且全员同时阻塞的瞬间出现 → 内核 park（WFI）→ QEMU vCPU 睡 → 那 15pp CPU 差。与 098ca8bf「boot 演示常驻面永不退出 + tickless 机构被饿死」的定性判一致——本实验给出定量侧写：**长片不只是延迟，是让饿死模式驻留**。

### 3.4 与调度理论对拍（时间片 vs 交互延迟）

教科书权衡（片长↑ 切换开销↓ 但交互延迟↑；片长↓ 反之）在本数据上的形态：开销轴可见（计算相 +77-89%、切换率 ×8.7）但廉价（每次切换微秒级，被 TCG 放大仍小）；延迟轴在等待相 26× 塌缩 + 吞吐 +91% + CPU −15pp 三面同向兑现——**本负载族的最优点远在缺省 10Hz 的右侧**，且直到词表顶格 1000 未见面。两个修正项让经典图景不完整（标注推断）：①唤醒抢占（G3，`sched/src/sched.rs:43` try_preempt 直切）已把消息交互延迟从「等 slice」解耦——slice 的剩余作用面收窄到超时/让渡；②自旋型等待（非阻塞自旋等对方）把过长片的代价从「延迟」放大为「吞吐+功耗双损」——真实 GUI 服务族（壳/合成器/输入管线间的 IPC 环）同构暴露于此面。

## §4 sweet spot 侧写与推荐（供 bootparams 默认值/IntentProfile 档参考）

| 档 | 链 FBW→LAUNCHv | 稳态吞吐 | 稳态 CPU | 切换/s | 侧写 |
| --- | --- | --- | --- | --- | --- |
| 10（编译缺省） | 9.47s | 217 B/s | 100% | 124 | GUI 桌面全轴最差——自旋饥饿驻留；保留给「无交互常驻面」概念档（未测） |
| 250 | 4.48s | 264 B/s | 84% | 316 | **均衡默认推荐**——链收益 82%、税温和、帧预算内 ≥4 次超时机会 |
| 1000 | 3.37s | 419 B/s | 85% | 1077 | **响应优先档**——链/吞吐最优、切换贴 timer 底、计算相税 +0.87s 可见；词表顶格 |

- **推荐一（产品默认）**：desktop/interactive 产品的 `sched_slice_hz` 默认落 250（`product.toml [platform.boot.params]`，G5 链已通）；1000 留给 latency_critical 档（IntentProfile 按档选值的思路同构——`docs/调度器设计-v1.2.md` §3.8 intent 直驱参数轴，slice_hz 是其正交的全局基座）。10Hz 缺省在 GUI 档应视为 bug 级错配。
- **推荐二（指南 128 建议的定位）**：`docs/显示管线调度集成指南-v1.0.md:129` 建议 128（帧预算内 ≥2 次超时机会）——本实验 250 档 ≥4 次、实测全轴优于缺省，128 落在 [10,250] 单调带内方向一致（未测点，推断）；取 250 与 128 的增量差在等待相塌缩的对数尾上（26× 已在 250 拿到），实测分辨率内两者大概率同级（推断）。
- **不外推**：>1000 词表外未测；且 1000 档切换已贴 timer 硬底、计算相税近饱和——更高 hz 的边际收益递减、tick 摊销成本续涨（`arm_slice` 每 slice 至少一 tick 的机制保证），按机制推演不建议 [中·推断]。
- **真机注记**：本数据 TCG 档（切换/中断成本被虚拟化放大，真机每片成本更低——税更小、收益保留）；HVF 档 ktest fatal 既有缺口修复后可复跑本矩阵复核量级（限制如实 §5）。

## §5 环境限制如实

- **TCG 单档**：HVF 档既有 boot ktest fatal（12 号 §6 同族）未复跑；绝对值含 TCG 税，跨模式/跨机不可比（icount 铁律同族——本实验零 icount 面，QEMU CPU 面仅同机同模式内三臂互比有效）。
- **稳态负载=演示常驻面**：boot 演示矩阵永不退出是 098ca8bf 定谳的待修面（修窗五件在册）——稳态吞吐/CPU/run-length 的**量级**是此负载特定的；等待相/计算相的**分解方向**机制面普适。修窗落地后稳态栏需复测。
- **LAUNCH ok 饿死**：TCG 既有（12 号 §6），链口径止于 LAUNCH v=——「首帧呈现」指标不可测，页链（PAGE ok）未进窗。
- **单机单窗**：并行会话在场（背景负载可能）；双跑一致率——链合计 ≤5%、稳态率 ≤10%（switches 250 档 333/300 最大散、吞吐 <3%、CPU <2pp）、单段最大散度 26%（default GFX→LAUNCH 3.103/4.028，缺省档等待相本身抖动大、与片长效应同面）——支持结论稳健，绝对值有背景抬底。
- **run-length 口径**：矩阵字母集 `PpuCBSAsWd` 串长（marker 行已滤）；p50/max 单口径统计（双跑对齐佐证）。
- **bootparams eco 相邻性**（ROOTFS ok→eco +0.1ms）为三臂六靴一致观察，非独立复验轮（G5 42/43 号已有 fw_cfg/AAVMF 双路七靴+双靴先例）。

## §6 来源索引

- **本轮实测产物**：`/tmp/slice-gui/`——`bootone.py`（单靴 harness：QEMU 复刻命令+串口打点+CPU 双源采样）/`analyze.py`/`agg.py`（分析器三件）；三盘 `img-{default,250,1000}.img` + `rootfs-{check,250,1000}.cpio`（check 件 = cmp 恒等保真腿）+ `prod{250,1000}/`（--emit 产件）；六靴数据 `*-r{r1,r2}/{serial.tsv,serial.log,cpu.tsv,summary.json,cmd.txt}` + `agg.json` 总表。
- **仓内（路径:行号）**：`arch/aarch64/src/timer.rs` :29（`SLICE_HZ=10` 缺省——本实验 default 档的真身）/:71-72（override 消费）/:118-146（`arm_slice` 绝对编程+S-6 夹取+tick 硬底）/:320-327（`slice_cycles_for_hz` 单源算式）/:332-340（`set_slice_hz`）；`kernel/src/bootparams.rs` :41/:62-63/:306-334（消费端全链+eco 打点）；`tools/berry_assemble.py` :171-174（词表 u32 [16,1024]）/:1020 起（序列化单点）；`sched/src/sched.rs` :56-70（switches 金丝雀每 100 切换）/:132（console-sender 自旋环注释——§3.3 机制锚）/:1743（tick 到期抢占）；`kernel/src/main.rs` :3994-4009（AIDEMO 睡 2×slice）/:4071-4081（AIDEMO eco）；`kernel/src/ktest.rs` :7610-7634（HDRWAKE busy-poll 环）；`tools/berry/src/qemu.rs` `machine_args`/`spawn_image`（命令形复刻源）。
- **文档**：`docs/显示管线调度集成指南-v1.0.md` :128-129（超时唤醒粒度=1 slice 与 128 建议的理论靶心）；`docs/调度器设计-v1.2.md` §3.8（IntentProfile 档位消费面）；`docs/产品配置层与槽位合同设计-v1.1.md` §1.6-补（bootparams 投放合同）。
- **谱系**：12 号（§1.2 环境口径/24MHz 定标、§2.1 marker 链口径与 S1 桌面档、§6 LAUNCH 饿死与 HVF 缺口——本文直接承）；`research/厂商定制化深研/42-实施轮十六复核.md` §1 与 `43-实施轮十七复核与G系终账.md` §1（G5 全链验真与 eco 行口径——三盘投放的先例）；07/09 号（TCG/满核背景与 A/B 方法先例）；README.md（目录索引）。
