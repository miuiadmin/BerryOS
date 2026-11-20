# svcmgr 接入 devmgr 拉起链

Status: implemented

## Problem

[devmgr v1](2026-11-20-devmgr-v1设备枚举与服务状态报告.md)已入 rootfs（`bin/devmgr`）但 svcmgr 点名制不识——串口看不到枚举报告，端到端链（枚举 → 匹配 → 服务状态）缺最后一环。v1 笔记以「平像 160KiB 恰满帽零余量」为由把接线留独立刀，本刀就是那把刀。

## Decision

主走查加 `bin/devmgr` 点名 arm（vprobe arm 之后、`app/` 收集之前），形态三件：

1. **vprobe 同款拉起**：`mmio_vmo: Some(H_MMIO)` 授窗（EL0 永不自造物理窗）+ `stdout: console` 直打 + spawn 后**阻塞 reap**（跑完即退自检件——vprobe 同口径，退出码不判，窗几何错由 devmgr 自打诊断行后 exit 1）；窗口缺席 = `eco_skip(b"mmio absent")` 可见降级。
2. **gift[2] = ps 查询端**（v1 笔记契约半边）：`extra_gifts: &[q_b]` 递通道，svcmgr 侧服务一笔 PS_LIST（`encode_table_reply` 单源 + bc_run_sub 同款收发循环，帽 60s 对 devmgr 侧 30s RPC 帽无竞速）。报告 ✓/✗ 因此有真值：mkcpio 名序本条目在预拉族（vblk/vnet/PM 双服务/guifbd/inputd/fsmgr）入账之后到访。
3. **两顶帽子明示不动**：账本不入册（hexyl/vprobe 同例——18 行恰满 256B 帧帽零漂移）；smoke 标记不加（接线约束——`DEVMGR ok` 仍留开口，报告行是诊断面非断言面）。

**位置锚点勘误**：任务书定位「vprobe 之后、vblk 之前」在代码序里互斥——vprobe 在主走查末（名序 v > s），vblk 在预拉段首（P6 A-5 先于 fsmgr）。实际落点是二者的**之间**：走查 'd' 位到访（预拉族毕、vprobe 前），恰为报告真值最优窗。

**几何实测推翻「装不下」预判**：「平像恰满帽零余量」的真义是文件尺寸被 pager_entry 钉位（0x28000）恒定格 163840B——增长被段间零垫吸收，文件大小不动；真生长墙是链接钉「.bss 终点 < 0x28000」（link-user.ld 位置计数器回退即链接失败），本刀实测余量 0x3ED8。本刀落段 +0x1BC（.text +0x184、.rodata +0x28，.bss 终点 0x24128 → 0x242D8），平像仍恰 163840、`--max` 帽零违。

## Alternatives considered

纯 vprobe 形态不递 gift（弃——枚举报告完整但服务状态恒 `?` 降级，v1 契约半边落空，且 bc_run_sub 模板现成代价 ~0.6KB）；预拉段先于 vblk 拉起（弃——账本空表报告全是 WARN 缺失告警，观察价值最低）；fw_cfg `devmgr=on` 开关（弃——空间恐惧解除后无需退档）；抬高 160KiB 帽（弃——余量充足零必要，且涉内核 USER_CODE_MAX 双改超最小面）。

## Consequences

`just build-user` 全链绿（rootfs.cpio x64 条目零漂、tlsscan x62 ELF 全过）；串口 boot 序新增 `devmgr: N devices: …` 报告行（走查 'd' 位，hexyl/bsh 输出之前、vprobe `VIRTIO ok` 之前）。走查因 spawn+reap 多一次 std 级阻塞（ps/berryctl 同档先例 ×4 既有），TCG 慢窗最坏 60s 帽可见降级行不挡链。剩余开口：`DEVMGR ok` smoke 标记（报告 §4.2 验收谓词）与 guest 实跑验证（QEMU e2e）留后续刀。

## Related

- [devmgr v1：MMIO 槽枚举 + 驱动匹配 + 服务状态报告](2026-11-20-devmgr-v1设备枚举与服务状态报告.md)——拉起契约（授窗 + gift）与两硬墙背景；其「下一刀」由本刀兑现。
- [驱动子系统综合研究报告](../../research/2026-11-18-驱动子系统综合研究报告.md)——§4.2 交付 1.1 与波次划分。
