# 真机 bring-up 预研（云 VM → 树莓派 4 三台阶）

Type: proposal
Status: draft
Updated: 2026-11-06

一句话说明：把「QEMU→真机」从无限期风险变成有里程碑的工程——三台阶（云 VM 关 → RPi4 三件套关 → rootfs 真机通道），零架构分叉。

## 背景

- 调研 12 号 §4：真机鸿沟是 BerryOS 最大风险；swift-os（云 VM 跑 nginx+ACME）与 TrustOS（真机 GPU）已过此关。
- 库内既有真机意识（零散在册，本 proposal 汇总）：G4-3 C4 **i-cache 维护全树缺失**（TCG 不建模掩盖，真机前置雷）；测试策略 §已知非内核因素 **TCG CNTVCT 2× 失真**（真机应 10Hz）；测试策略 G5 **DVFS/EAS 验收=真机功耗计**；FAT 评估 §扇区内撕裂=真机面 v3+；G3 草案 TCG 绝对值跨机不可比。
- arch 层 QEMU 依赖面侦察（2026-11-06 实况）：`gic.rs` GICv2 + QEMU virt 地址硬编码（GICD `0x0800_0000`）；PL011 UART（RPi4 有真件，GPIO14/15 复用）；PL061/wake_gen 电源键链 **QEMU 特有**；`fwcfg.rs` **QEMU 特有**（rootfs 通道）；virtio-blk/net 全家 QEMU 面；`timer.rs` 读 `CNTFRQ_EL0` 不硬编码 ✓（架构级，真机直接适用）。

## 目标（三台阶）

1. **台阶一 · 云 VM 关**：aarch64 KVM 云 VM（Hetzner CAX 类，swift-os 同形态先例）跑 `berryos.img`——设备面不变（virtio），验证**真时钟**（KVM 无 TCG 2×）、**真存储持久性**、**真网络/TLS 出网**。近乎零移植。
2. **台阶二 · RPi4 裸机三件套关**：pftf RPi UEFI（成熟度待核实）+ BerryBoot EFI 链直落 SD 卡；三件套 = PL011 串口 → ARM generic timer（固件设 CNTFRQ）→ GIC-400（GICv2 兼容，`gic.rs` 理论可复用、改地址与中断拓扑——中断号待 DTB 核实）；ktest + 最小裸相 smoke。
3. **台阶三 · rootfs 真机通道**：EMMC2 SD 驱动替代 fw_cfg rootfs；UDP netconsole/log/screencap 预埋（TrustOS 三通道形态，调研 13 号）。

## 成功判据

- 台阶一：KVM 下 boot 28/28 全绿 + CNTVCT 实测 10Hz（2× 失真消失实证）+ 一次真出网（TLS 握手留证）。
- 台阶二：串口 banner → GIC timer 驱动调度起 → ktest 绿 → 裸相 smoke 最小集（标记数按真机面裁剪，另行立门）。
- 全程红线：**零架构分叉**——不动冻结常量，平台差异收敛在 `arch/aarch64` 内（平台表/条件编译），不 fork 第二内核。

## 方案选项

- **A（推荐）：云 VM 先行 → RPi4 跟进**。最大验证面/最低成本先行；RPi4 GICv2 兼容使 `gic.rs` 复用面大；每台阶独立可验，符合小步纪律。
- B：直上 RPi4 UEFI——首验即同时动内存图/设备树/SD 驱动/i-cache 雷全深水，拒。
- C：RPi5（BCM2712）——更新、资料少、GIC 拓扑陌生（待核实项更多）；TrustOS 教训（调研 13 号 §4）：铺陌生平台 = 单人项目死亡前置信号，拒。

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

- 调研 12 号 §4/§5、13 号 §2/§4；G4 草案 §7-9；测试策略 §G5/§已知非内核因素；swift-os NOTES.md（Hetzner/分配器坑）；TrustOS README（UDP 三通道）
