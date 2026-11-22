# berry-sys vmo_cache_op wrapper 合并取侧丢弃修复（跨线地基补齐）

Status: implemented

一句话说明：2026-11-22 15:36 大合流合并 66ae6906 在 `user/berry-sys/src/lib.rs` 的冲突解算中整文件取了对端（origin/wip）侧，把我侧在案的 `berry_vmo_cache_op` wrapper（F0-4 0x1C 缓存维护的用户态绑定，[f0-4 刀](2026-11-18-f0-4-物理连续dma与缓存维护及virtio自动复位.md)件）静默丢弃——vnet 消费端（main.rs 14 处引用）自此 E0432，GUI 会话（86e0e497）当日 07:46 起被此卡死至上下文耗尽；修复=从合并前我侧（66ae6906^1，文本同 fceb03ff 原版）原样恢复 wrapper 至 `berry_vmo_get_phys` 之后，净丢失终审确认该 wrapper 是唯一真实语义丢失（`TELEMETRY_OP_KLOG` 常量系搬家进协议 crate 非丢失），`just image` 全绿收口。

## Problem

GUI 会话终局卡点：`just image` 在 vnet 上 E0432 `unresolved import berry_sys::berry_vmo_cache_op`——vnet HEAD 版（经 NET 批次多刀演化）引用该符号，但本分支 berry-sys 无此函数。kernel 侧 `sys_vmo_cache_op`（0x1C，syscall.rs:1034 注册在案）与 berry-sys `abi_gen.rs` 的 `nr::VMO_CACHE_OP` 常量均完好，唯 lib.rs wrapper 单点缺失。owner 裁决：修就要彻底修，把地基打好，去协调。

## Decision

真源对照修复，不重写不存根：`git show 66ae6906^1:user/berry-sys/src/lib.rs` 取合并前我侧在案版本（与 fceb03ff 引入版逐字同文），整段恢复至 `berry_vmo_get_phys` 之后（含 `#[no_mangle]` 与 F0-4 文档注释），调用模式与同族 wrapper（`sys(nr, vmo, op, offset, len, 0, 0)`）一致。

判例面：三方取证定谳丢弃机制——ours（66ae6906^1）有 wrapper、theirs（66ae6906^2=origin/wip）无、merged（66ae6906）无，即冲突解算整文件取了对端侧而非语义并集。这是[跨根收口合并笔记](2026-11-16-跨根收口合并.md)「禁直接信任三方标记流」立法的又一活案：大歧文件合并的取侧决定可以静默丢掉一侧的合法新增，且编译期符号检查（消费端与提供端分属不同 crate）要到下一次全量构建才炸响——中间的合并提交本身是「绿」的。

## Alternatives considered

- 给 berry-sys 手写新实现：拒——真源在案（我侧 fceb03ff 版），重写引入漂移面；原样恢复才可审计。
- 回退 vnet 到不用该符号的版本：拒——vnet 的 DMA 缓存维护是 F0-4 安全几何的一部分，回退消费端等于拆已验收的安全面。
- 重做合并（revert 66ae6906 重解）：过重——净丢失终审已证唯一真实丢失就 wrapper 一件，外科恢复即达重解的语义效果。

## Consequences

- 全量构建解锁：修复后 `just image` rc=0（target-campaign 隔离目录，2026-11-22 16:11 镜像出炉）。
- 审计套路入册：跨线合并后对大歧文件跑「ours/theirs/merged 三方 grep 计数」净丢失对拍——本次由它证明 `TELEMETRY_OP_KLOG` 是搬家（挪进协议 crate 由 import 引入）而非第二件丢失。
- 环境债旁证一笔：同一棵树在默认 `./target` 里内核 rust-lld 链接失败、在隔离目录全绿——陈旧 target 投毒形态（gate 争用五形态族），与本次代码修复无关，换隔离目录即分。

## Related

- [f0-4 物理连续 DMA 与缓存维护刀](2026-11-18-f0-4-物理连续dma与缓存维护及virtio自动复位.md)——wrapper 与 0x1C 的出身。
- [跨根收口合并](2026-11-16-跨根收口合并.md)——「禁直接信任三方标记流」立法原文。

## 验证

- `just image` rc=0 全绿（CARGO_TARGET_DIR=target-campaign，out/berryos.img 16:11 重烘，含 wrapper 的 berry-sys 进链）。
- 净丢失终审：`git diff 66ae6906^1 -- user/berry-sys/src/lib.rs` 仅余 5 行 minus（TELEMETRY_OP_KLOG 旧位定义+段注释，已由协议 crate import 取代）——无第二件真实丢失。
- gui8 双靴交互冒烟：boot A 全靴标记绿（BERRYBOOT/ROOTFS/APP n=10/LAUNCH/PAGE n=2），点击段 120s 未等 `CLICK ok n=1`——**存量回归非本刀所致**：合并会话 15:34/15:39 两次前置跑（15:30 镜像、无本刀 wrapper）在同一精确点位同形失败（spawn 入队改造回归，另案追）。本刀构建面验证独立成立（just image rc=0）。
