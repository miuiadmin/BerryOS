# 波 2 收尾三小刀：virtio_example 接 HAL + 分区 display() + 驱动表补全

Status: implemented

## Problem

[驱动子系统综合研究报告](../../research/2026-11-18-驱动子系统综合研究报告.md) 2026-11-20 终审后仍留三处可小刀落地的消费面缺口：其一，berry-driver 的 virtio_example 自持 rd32/wr32 volatile 对——berry-hal 落地、vprobe 已接线（首真消费者）之后，仓里只剩它一份裸 volatile MMIO 面，HAL「volatile 收敛在实现内部」的承诺留了破口；其二，berry-partition 只有 parse 产数据、无格式化面，vblk 打分区日志靠 14 行手拼循环，形状散在消费点不可复用（教学可见性是 P0-2 的立身命题之一）；其三，devmgr DRIVER_TABLE 只收 §4.2 四类 did，console=3/rng=4 无绑定——rng 设备挂上即落「未知设备类」WARN，观察面分不清「did 无主」与「did 有主缺服务」。

## Decision

三刀同批（互不依赖、各 20 行域内）：

1. **virtio_example 接 berry-hal**（[vprobe 接线](2026-11-20-vprobe接线berry-hal首真消费者.md)同款、第二消费者）：`VirtioMmio` 窗对象进结构体（`Option<VirtioMmio>` 替裸指针字段），probe 授窗、start/stop 走 `set_status`/`status` 语义便捷面——比 vprobe 的 `read32(mmio::MAGIC)` 偏移面更进一步（语义面免在消费点铺偏移常量）；本地 rd32/wr32 退役，文件零 unsafe。窗长 0x200 本地镜像（devmgr 几何常量同款惯例——引 berry-exec 取 `VIRTIO_MMIO_STRIDE` 会拖进 syscall 面，违背本 crate 零环境面纪律）。与并行「债务清理刀」在两 crate 头注释新下的「正交组合」定谳相容的一半：Driver trait 契约面仍零 HAL 依赖（probe 裸指针总线中立不动），依赖只落在示例模块——恰是其所称「实现侧自选——可直接引 berry-hal 的 `Mmio` 实现兑现」的形态；但该注释另称「两 crate 非 crate 级依赖、各自只依赖 berry-virtio」，与本刀的示例模块 dep 有表述张力（示例住在 berry-driver crate 内），跨 lane 待对齐。
2. **berry-partition 增 `display(&[Partition]) -> String`**：单行「头段 scheme+条数 + 逐条 `[index] 名/type/LBA 闭区间`」，形状 = vblk 现行分区日志逐字收口（消费面零漂移）；空表退 `0 partitions`（无 scheme 可判）；尾不带换行（行终止归消费侧 print/println）。vblk 换用留后续刀。
3. **devmgr 驱动表补全**：console=3→console、rng=4→berry-rng 先行绑定——表是 did→服务名单源，先行入表后「设备在而服务缺」呈现为 absent WARN（did 有主），与「未知设备类」（did 无主）区分；rng 服务本体仍归报告交付 1.5 未动。rng 账本行名取 rngd：psproto `NAME_MAX=8` 帽，berry-rng 9B 超帽，按 gui-inputd→inputd 先例缩名。

## Alternatives considered

display 实现 `fmt::Display` trait（弃——列表头段+逐条拼装非单值格式化，自由函数贴 parse 姊妹形态）；virtio_example 存裸指针、调用点临时包 HAL（弃——窗对象随状态机生命周期互证，Unprobed 态恒 None 即物理不可读写）；rng 账本行名直用 berry-rng（弃——9B 超 `NAME_MAX=8`，账本查无此行恒 absent 假告警）；console/rng 同批放行 vprobe 白名单（弃——vprobe 是 boot 链守门件，拓扑漂移 loud trap 是其职责，白名单随各自服务落地刀放行，本批只动 devmgr 观察面）；did=4 继续当 devmgr「未知 did」测试样例（必改——入表后 4 有主，样例换 9=virtio-9p 真未知类）。

## Consequences

2026-11-20 验收全绿：berry-driver host 7 测、berry-partition host 18 测（16+2 新）、devmgr host `cargo test --lib` 11 测（10+1 新）零败，berry-hal 回归 8 测零败；`just build-user` 全链通过（rootfs.cpio x64 条目，tlsscan 全过）。justfile 注释随批同步（partition 18 测、devmgr 11 测、匹配表六类）；研究报告 §4.2 交付 1.4/1.5 行与 §4.3 波 2.2 行状态标注同批更新。下一刀：vblk 换用 display() 收口手拼循环；vblk/vnet 按 berry-driver + berry-hal 全量接线（[波 2 三件套](2026-11-20-驱动波2三件套.md)已排）；console/rng 服务本体落地时随刀放行 vprobe 白名单与 ps 账本行。

2026-11-23 尾账（总审复核补）：devmgr 的 host 测此前只在本篇与 justfile 注释自述，`test-host` 与 CI 零执法——`justfile` 驱动块末补 `cargo test --lib --manifest-path user/bins/devmgr/Cargo.toml --target {{host}}` 一行入闸；带 `--lib` 是因 bin 面 volatile 读与 psproto RPC 全在 `cfg(target_os = "linux")` 依赖下，host 整测必 E0433（devmgr Cargo.toml 自述契约；gui-wmd/guifbd 靠 main.rs 全文 cfg 门控才免 `--lib`，devmgr 不为此改源）。同批勘正研究报告 §4.2 交付 1.1 行的 host 测数——上批只同步了 1.4/1.5 与 §4.3 波 2.2 行，漏了 1.1 行（停在驱动表补全刀新增 1 测之前）。

## Related

- [驱动子系统综合研究报告](../../research/2026-11-18-驱动子系统综合研究报告.md)——§4.2/§4.3 未动清单与先行件语境。
- [vprobe 接线 berry-hal 首真消费者](2026-11-20-vprobe接线berry-hal首真消费者.md)——同款接线先例（本批第二消费者）。
- [berry-driver 统一 Driver trait 框架 crate](2026-11-20-berry-driver统一Driver-trait框架crate.md)——virtio_example 所属框架件。
- [berry-partition 分区表解析 crate](2026-11-20-berry-partition分区表解析crate.md)、[vblk 接入 berry-partition 启动分区探测](2026-11-20-vblk接入berry-partition启动分区探测.md)——display() 的数据源与形状出处。
- [devmgr v1 设备枚举与服务状态报告](2026-11-20-devmgr-v1设备枚举与服务状态报告.md)——驱动表补全的所属件。
- [驱动波 2 三件套](2026-11-20-驱动波2三件套.md)——HAL/框架件的波次伞笔记。
