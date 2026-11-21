# F1-3：无环 `ChannelShared`、在途句柄 `Message::Drop`、`SIGNAL_PEER_CLOSED` 与算术溢出收口

Status: implemented

## Problem

旧版 [channel.rs](../../../kernel/src/channel.rs) 在 `ChannelEnd::pair` 中通过双端互相调用 `ObjRef::retain` 维持指针存活，同时 `Message` 结构体未实现 `Drop`，导致三项结构性缺陷：① 每一对创建的通道形成 `ObjRef` 自引用死环，双端句柄关闭后通道对象与双向 `Vec<Message>` 预储缓冲永久泄漏于内核堆；② 发送方塞入 `Vmo` 或子 `Channel` 随行句柄后若对端未调 `recv` 即关闭，所有在途句柄及其后备物理页永久泄漏；③ 对端进程崩溃或关闭句柄时缺乏 `SIGNAL_PEER_CLOSED` 状态位与唤醒通知，阻塞在 `object_wait_one` 上的客户端只能空等超时。此外，[syscall.rs](../../../kernel/src/syscall.rs) 的 `access_ok_window`、`ns_to_vct`、`sys_telemetry_open`、`sys_cbox_drain` 与 `sys_vmo_read/write` 在 `u64::MAX` 邻域存在整数加法或乘法溢出隐患。

## Decision

1. 在 [channel.rs](../../../kernel/src/channel.rs) 改由双端 `ChannelEnd` 共享单一 `Arc<ChannelShared>`（内含叶级中断安全锁保护的双向消息队列 `queues`、双端 `frozen_owner`、句柄级开闭计数 `open_ends: [AtomicU32; 2]`、幂等关闭门闩 `closed_once: [AtomicBool; 2]` 与不增加引用计数的弱等待键 `end_objs: [AtomicUsize; 2]`），彻底废除双向 `ObjRef::retain` 自引用环。`HandleTable::duplicate` 同步递增 `open_ends[side]`，`close`/`drain`/`Message::Drop` 同步递减，最后一枚句柄关闭时即使本端仍有 `wait_one` 临时钉住引用也能当场触发对端关闭语义。
2. 为 `Message` 实现 `Drop`，自动对前 `nhandles` 枚未消费句柄执行 `release_handle` 与 `ObjRef::release`，并在释放队列锁之后再析构 `Vec<Message>`，从根源消除嵌套或自指通道重入死锁；`sys_channel_recv` 完成随行句柄入表与用户态写回后调用 `msg.disarm_handles()` 清零所有权。
3. 在 [abi.toml](../../../tools/abi.toml)、[evface/src/lib.rs](../../../evface/src/lib.rs) 与 [chanface/src/lib.rs](../../../chanface/src/lib.rs) 新增派生位 `SIGNAL_PEER_CLOSED = 1 << 4`；进程退出钩子 `task_exit_hook` 在全谱锁窗内同步执行 `drain_ipc_on_exit()` 立即关闭死亡进程持有的全部通道与冻结令牌句柄，使客户端在微秒级内收到 `SIGNAL_PEER_CLOSED` 唤醒；同时将 `NR_HANDLE_REPLACE` 升为 `SVC_CLASS_SQ_RQ_TABLE` 并在 [process.rs](../../../kernel/src/process.rs) `ht_drain_flush` 增设锁外延迟唤醒缓冲，严守 `SQ -> RQ -> TABLE` 全局锁序。本端未读消息在对端关闭后仍可由 `channel_recv` 完整排空，排空后再 `recv` 或向已关对端 `send` 返回 `-EBADSTATE`（`-37`），并同步更新 [io.rs](../../../user/berry-libc/src/io.rs) 标准输入与控制台断管处理。
4. 在 [syscall.rs](../../../kernel/src/syscall.rs) 对 `access_ok_window`、`sys_telemetry_open`、`sys_cbox_drain`、`sys_vmo_read` 与 `sys_vmo_write` 全面改用 `checked_add`/`saturating_add`，并将 `ns_to_vct` 升级为 `u128` 饱和换算，根绝 `u64::MAX` 边界触发 EL1 算术 Panic。

## Alternatives considered

- 仅依赖 `ObjRef.refs` 归零触发通道关闭而不维护句柄级 `open_ends[side]`：拒绝——当服务端阻塞在 `sys_object_wait_one` 内被 `process_terminate` 强杀时，睡眠任务的内核栈帧不会正常展开执行尾部的 `ObjRef::release`，仅靠 `ObjRef.refs` 会导致死亡服务端端点滞留不触发 `SIGNAL_PEER_CLOSED`；独立的句柄级 `open_ends[side]` 在最后一枚句柄摘除瞬间即可确定性清空队列并唤醒对端。
- 将通道句柄关闭全部留给异步 `HT_DRAINQ` 在 `idle` 时排空：拒绝——双核忙载下 `idle` 排水可能延后数十毫秒，违反服务端崩溃后 `< 1ms` 唤醒客户端的硬实时要求，且 `ht_drain_flush` 仅持 `KL(TABLE)` 直接调用 `wake_matching(SQ)` 会违反 `SQ -> TABLE` 锁序；在 `task_exit_hook` 全谱锁窗内同步执行 O(槽) 的 `drain_ipc_on_exit()` 配合 `ht_drain_flush` 锁外唤醒缓冲，兼具零延迟与严格锁序合规。

## Consequences

彻底根除 BerryOS 通道对自引用内存泄漏与未读在途句柄物理页泄漏，补齐微内核标准 `SIGNAL_PEER_CLOSED` 即时故障感知契约与系统调用边界算术安全。宿主机 `berry-evface`（14 测）与 `berry-chanface`（17 测）全绿；`ktest` `test_f1_3` 实测 1000 轮满载 `128MiB` VMO 与嵌套通道未读关闭后 `mm::allocated_pages()` 净增 `0` 页，服务端强杀后客户端在微秒级（`< 1ms`）被唤醒并读得 `SIGNAL_PEER_CLOSED`。

## Related

- 缺陷来源：[2026-11-18-全栈架构与代码缺陷深研.md](../../research/2026-11-18-全栈架构与代码缺陷深研.md) K5、K6
- 前序底座：[2026-11-18-f1-2-两阶段拆页表与页表帧全回收及废除leak分支.md](2026-11-18-f1-2-两阶段拆页表与页表帧全回收及废除leak分支.md)
- 实施规格与计划：[2026-11-18-微内核与全栈底座彻底重构规格.md](../../plans/2026-11-18-微内核与全栈底座彻底重构规格.md)、[2026-11-18-全栈底座彻底重构计划.md](../../plans/2026-11-18-全栈底座彻底重构计划.md)
