# vblk 接入 berry-partition：启动分区表探测（存储 P0-2 刀 2）

Status: implemented

## Problem

[驱动波 1 三件套](2026-11-20-驱动波1三件套.md)交付了 [berry-partition 解析 crate](2026-11-20-berry-partition分区表解析crate.md) 但消费面留刀：vblk 启动自验后直接转常驻服务，把整盘当一个分区暴露——盘上有没有分区表、几 个分区、什么布局，服务与控制台全看不见，教学场景「分区 → 格式化 → 多文件系统对照」的第一环（看见分区）仍缺。

## Decision

**方案 = vblk 启动序最小接入（探测 + 一行日志，服务面零动）**：

1. **插点**：main() 自验（LBA0 印章对拍 + LBA1 写读往返）之后、`VBLK ok` 汇总行之前——探测属启动探测面，汇总行收尾；探测引入的 blk_op 设备失败仍走既有 trap（loud），标记行缺失 = smoke FAIL 兜底不变。
2. **读盘闭包注入**：`berry_partition::parse(closure, SECTOR_SZ)` 的 block_reader = `blk_op(BLK_T_IN, lba)` + IOB bounce 拷出（serve() 读路径同款）；LBA↔盘容对账归消费侧（库边界不做盘尾对账）——闭包内 `lba >= cap` 即返 `Error::Reader`，防病态表把读面引出盘界。
3. **结果只打一行日志**：`vblk: GPT 2 partitions: [1] root 2048-10239 [2] ...`（scheme 由首条 kind 判——0xEE 即整面转 GPT，同表恒同面；index = 表内槽位号与 UEFI 工具显示对齐；LBA 域闭区间含端，库语义原样转述）；MBR 面条目打 `type=0x83`，GPT 空名打裸 LBA 域。
4. **失败不杀服务**：解析 Err/空表只打一行事实（`vblk: 分区表解析 ...（按整盘服务）`）——smoke 印章盘是 superfloppy 形合法面，trap 会杀掉 rootfs 相第 13/14 标记链；`VBLK ok`/`BLKD ok` 判据零扰动。
5. **RPC 面不加**：vblk 协议 = berry-blkproto 冻结面（Read/Write 两型、严格串行、应答 1B），加分区查询需动协议 crate + svcmgr client 联动，超出最小刀；教学可见性日志已足，多分区按 RPC 暴露归 v2。
6. **接线**：跨 workspace path 依赖照 vnet→berry-libc 先例（`../../../user/berry-partition`，drivers 单锁入册）；berry-partition 的 alloc 面（Vec/String）走 berry-libc 静态 1MiB 堆（malloc 族，bsh 同链 std musl bin 大量用 Vec/String 先例）——分区表 ≤512 条远不及堆帽。

## Alternatives considered

协议加分区查询 RPC（弃——冻结面三处联动：协议 crate 新 Req 变体、应答帧形态、svcmgr client 消费，与 v2 多分区暴露同刀再做）；探测失败 trap（弃——印章盘无分区表，Err 是合法面非链断面）；每分区一行日志（弃——行数随盘变，smoke 洪流面判据不友好，一行打全）；`format!` 拼 String 再打印（弃——多段 `print!` 零额外分配，std LineWriter 攒到换行一次吐出）；探测后按分区改 serve() 的扇区映射（弃——行为变更面，vblk v1 语义 = 整盘 LBA 直通，多分区暴露归 v2）。

## Consequences

`just build-user` 全链通过（rootfs.cpio 重建、tlsscan 三查全过）；vblk 增量构建零自身警告，产物含新字符串常量（`partitions:` 等）；drivers/Cargo.lock 增量两处（berry-partition 包条目 + vblk 依赖表）。改动面 = vblk Cargo.toml +1 依赖、main.rs +60 行（含注释）——架构零动（find_device/negotiate/blk_op/serve 全不碰）。真 GPT 盘接入即打分区布局行；smoke 印章盘预期打 `no partitions（空表）` 或 MBR 空表行，不挡标记。下一刀（[研究报告](../../research/2026-11-18-驱动子系统综合研究报告.md) P0-2 第二断层）：fsmgr 按 `first_lba` 定卷替代 BPB 约定式探测；v2 面：vblk 多分区按 RPC 暴露 + GPT CRC32 可选执法。Related：[驱动波 1 三件套](2026-11-20-驱动波1三件套.md)（本刀兑现其留刀）、[berry-partition 分区表解析 crate](2026-11-20-berry-partition分区表解析crate.md)（API 冻结面）、[vblk 接线 berry-hal：HAL crate 的第二个真消费者](2026-11-23-vblk接线berry-hal.md)（后续刀——MMIO 面翻 HAL，本刀的分区探测块一字未动）。2026-11-24 勘正：探测插点已前移到一切写之前，有表时跳过 LBA1 自验写——本篇 Decision 1 的「自验之后」插点不再是现状，现行顺序与理由见 [vblk 启动自检顺序缺陷笔记](2026-11-24-vblk启动序探测前移与有表跳过LBA1自验写.md)。
