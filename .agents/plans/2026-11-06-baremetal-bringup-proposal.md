# 真机 bring-up 预研（云 VM → RPi5 三台阶）

Type: proposal
Status: draft
Updated: 2026-11-07

一句话说明：把「QEMU→真机」从无限期风险变成有里程碑的工程——三台阶（云 VM 关 → RPi5 三件套关 → rootfs 真机通道），零架构分叉。

> **2026-11-07 延后条款（仓主裁）**：暂不购板——台阶二/三挂起，**台阶一（云 VM，按小时租用非购板）照常独立开工**；目标板并线裁决（RPi5 翻案 vs RPi4 bench 规程）**随采购推迟**，待台阶一云 VM 数据回来后重评（届时对真机缺口面的判断更精确，板型选择可能超出 RPi 系）。再购板触发条件：① 台阶一全绿且裸机面成为当前瓶颈；② sched-1.3 需要 DVFS/功耗真数据；③ i-cache 雷（G4-3 C4）已无板先修、就位待验。无板期间不受影响的：仿真面全部、i-cache 修码、云 VM 真时钟/持久/网络验证。

> **2026-11-07 事实修正（调研 14 号 §1.2，urthr 板级矩阵实证）**：台阶二目标板 RPi4 → **RPi5 翻案**——① GICv2 两板相同（`gic.rs` 复用判断不变）；② SDHC **只在 RPi5 有完整先例**（RPi4B 的 SD 通道 urthr 亦未铺）；③ PSCI 只有 RPi5 有（RPi4B SMP 须 spin-table，前置雷 6 自动消失于 RPi5）；④ PCIe/xHCI/Cadence 网卡/RP1 南桥全套活样本在 RPi5（Zig 实现可参照语义）。原拒 RPi5 理由（BCM2712 陌生）被实证推翻一半；真机 CI 形态先例同步获得（Build RPi5 + Runtime Test 分行 Actions）。翻案随延后条款一并冻结，采购时复评。

## 背景

- 调研 12 号 §4：真机鸿沟是 BerryOS 最大风险；swift-os（云 VM 跑 nginx+ACME）与 TrustOS（真机 GPU）已过此关。
- 库内既有真机意识（零散在册，本 proposal 汇总）：G4-3 C4 **i-cache 维护全树缺失**（TCG 不建模掩盖，真机前置雷）；测试策略 §已知非内核因素 **TCG CNTVCT 2× 失真**（真机应 10Hz）；测试策略 G5 **DVFS/EAS 验收=真机功耗计**；FAT 评估 §扇区内撕裂=真机面 v3+；G3 草案 TCG 绝对值跨机不可比。
- arch 层 QEMU 依赖面侦察（2026-11-06 实况）：`gic.rs` GICv2 + QEMU virt 地址硬编码（GICD `0x0800_0000`）；PL011 UART（RPi4 有真件，GPIO14/15 复用）；PL061/wake_gen 电源键链 **QEMU 特有**；`fwcfg.rs` **QEMU 特有**（rootfs 通道）；virtio-blk/net 全家 QEMU 面；`timer.rs` 读 `CNTFRQ_EL0` 不硬编码 ✓（架构级，真机直接适用）。

## 目标（三台阶）

1. **台阶一 · 云 VM 关**：aarch64 KVM 云 VM（Hetzner CAX 类，swift-os 同形态先例）跑 `berryos.img`——设备面不变（virtio），验证**真时钟**（KVM 无 TCG 2×）、**真存储持久性**、**真网络/TLS 出网**。近乎零移植。
2. **台阶二 · RPi5 裸机三件套关**（2026-11-07 由 RPi4 翻案，见头部修正注）：pftf RPi UEFI（成熟度待核实）+ BerryBoot EFI 链直落 SD 卡；三件套 = PL011 串口 → ARM generic timer（固件设 CNTFRQ）→ GIC-400（GICv2，`gic.rs` 理论复用、中断拓扑待板级 DTB 核实）；参照 urthr（Zig 同板全套外设先例）与真机 CI 形态（Build 镜像与真机运行时测试分行）；ktest + 最小裸相 smoke。
3. **台阶三 · rootfs 真机通道**：EMMC2 SD 驱动替代 fw_cfg rootfs；UDP netconsole/log/screencap 预埋（TrustOS 三通道形态，调研 13 号）。

## 成功判据

- 台阶一：KVM 下 boot 28/28 全绿 + CNTVCT 实测 10Hz（2× 失真消失实证）+ 一次真出网（TLS 握手留证）。
- 台阶二：串口 banner → GIC timer 驱动调度起 → ktest 绿 → 裸相 smoke 最小集（标记数按真机面裁剪，另行立门）。
- 全程红线：**零架构分叉**——不动冻结常量，平台差异收敛在 `arch/aarch64` 内（平台表/条件编译），不 fork 第二内核。

## 方案选项

- **A（推荐）：云 VM 先行 → RPi5 跟进**。最大验证面/最低成本先行；RPi5 GICv2 兼容使 `gic.rs` 复用面大，SDHC/PSCI/PCIe/网卡全套 urthr 先例；每台阶独立可验，符合小步纪律。
- B：跳过云 VM 直上 RPi5——首验即同时动内存图/设备树/SD 驱动/i-cache 雷全深水，拒。
- ~~C：RPi4~~（2026-11-07 降级）：SDHC 与 PSCI 双缺（urthr 实证，14 号 §1.2），SD 通道无人铺路——降为 RPi5 之后的次选；TrustOS 教训（陌生平台 = 死亡前置）对 4B 的 SD/启动协议面同样适用。

## 前置雷清单（实施前逐项落刀）

1. **i-cache 维护**（G4-3 C4 已登记）：per-fault `ic ivau` vs 换页粒度 `iallu`，RPi4 A72 VIPT 首当其冲。
2. **DMA 缓存一致性**：BCM2711 IOCOHERENT 状态待核实；swift-os 坑一（连续分配低区越界，调研 13 号 §2）同型风险——P1-e 对抗性分配器测试先行。
3. **TCG 绝对值重标**：lat/bench 全部 TCG 标定值在真机面作废重跑（B-2 纪律已有）。
4. **fw_cfg 断链面**：rootfs 注入 + 相关 smoke 断言在真机的替代通道。
5. **wake 源空缺**：PL061 电源键链 QEMU 特有，真机唤醒链另裁。
6. **SMP 启动协议**：QEMU PSCI vs RPi 固件路径（spin-table/PSCI，待核实 RPi UEFI 供何种）。

## 适用边界

本 proposal 只立项不实施；实施刀按台阶另立 plan（判据各自挂 smoke/ktest 门）。台阶一成本最低，可与 G 线并行不冲突。

## 参考

- 调研 12 号 §4/§5、13 号 §2/§4、14 号 §1.2（urthr 板级矩阵）；G4 草案 §7-9；测试策略 §G5/§已知非内核因素；swift-os NOTES.md（Hetzner/分配器坑）；TrustOS README（UDP 三通道）
- **并线提示（2026-11-07）**：sched-1.3 预研真机线（`1a6eb25`，bench 视角：seL4bench 骨架/HDR 1024 桶/RPi4 规程 GIC/PL011/DVFS）与本 proposal（bring-up 视角）共享 PL011/GIC 三件套——**目标板分歧待并线裁决**（bench 线 RPi4 vs bring-up 线 RPi5 翻案）；另注：RPi4/RPi5 均为单簇 4 核（A72/A76），真机 EAS 异构放置验证两板皆不可达，异构面另需评估。
