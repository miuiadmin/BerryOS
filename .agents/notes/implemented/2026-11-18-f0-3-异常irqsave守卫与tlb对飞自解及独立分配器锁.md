# F0-3：异常 `IrqSaveGuard`、`tlbshoot` 对飞自解与独立第 5 层 `ALLOC_LOCK`

Status: implemented

## Problem

全栈审计检出异常中断掩码、跨核 TLB shootdown 与内核内存分配器三处并发正确性缺陷：

1. [dpg.rs](../../../kernel/src/dpg.rs) `probe` / `probe_el0` 在同步数据异常探针内直接调用 `klock_enter` / `klock_exit`，而 `klock_exit` 无条件执行 `enable_irq()` 把 `PSTATE.I` 清 0，导致异常返回 `eret` 前指令流暴露在嵌套时钟 IRQ 下，可能覆写当前核 `ELR_EL1` / `SPSR_EL1`；[vmar.rs](../../../kernel/src/vmar.rs) `Vmar::Drop` 同样存在未保存 `DAIF` 裸取 `KERN_LOCK` 的中断重入死锁风险。
2. [tlbshoot.rs](../../../arch/aarch64/src/tlbshoot.rs) `wait_pending` 纯自旋等待对核 SGI 回执，而当双核各自在 `PSTATE.I == 1` 下同时向对方发起 `broadcast` 并进入回执等待时，SGI 均被硬件挂起无法送达，`spins >= 400_000_000` 后必然双核互锁 Panic；此外 `probe_el0` ZFOD 直供与 `dpg_pager` 供给路径在广播后未调用 `sched_exit_check()` 排干本核 `PENDING` 闩。
3. [mm.rs](../../../kernel/src/mm.rs) `mm::frames()` 直接向全树返回裸 `&'static mut FrameAllocator` 而无自持锁，且 [heap.rs](../../../kernel/src/heap.rs) `SlabHeap` 未加分配器锁；当 `HT_DRAINQ` 异步回收或无 `KL` 路径析构 `Box`/`Vec`/`Vmo` 时，与对核 `vmo_commit`/`vmar_map` 形成并发 `&mut` 别名 UB 与伙伴链/空闲链腐坏。

## Decision

1. 在 [kern_lock.rs](../../../arch/aarch64/src/kern_lock.rs) 新增保存并精确恢复 `DAIF` 的 RAII 守卫 `KlockIrqSaveGuard`（`klock_enter_irqsave`）与支持 `kl_held()` 同核重入退化的 `TableIrqSaveGuard`（`kern_acquire_irqsave_reentrant`），全面替换 `dpg::probe`、`dpg::probe_el0` 与 `Vmar::Drop` 中的裸取放锁，确保异常出口全程保持 `PSTATE.I == 1` 直至 `eret` 原子恢复用户态 `SPSR_EL1`。
2. 在 [tlbshoot.rs](../../../arch/aarch64/src/tlbshoot.rs) 引入每核 `REQ_PENDING` 原子标志：`broadcast` 对目标核置位，`wait_pending` 自旋环内每次迭代轮询 `REQ_PENDING[me].swap(false, AcqRel)`，命中即当场执行 `flush_local_all()` 并自增 `ACKS[me]`，将双核关中断对飞死锁化为单拍内联互助放行；同时公开 `sched_exit_check()` 并在 `probe_el0` ZFOD 放锁后与 `dpg_pager` 放锁后补齐排干。
3. 在 [mm.rs](../../../kernel/src/mm.rs) 激活 `berry-lkface` 预留的第 5 层 `CLASS_ALLOC`（rank 5：位于 `TABLE`/`PT` 之内、`ASID`/`PRINT` 之外）自旋锁 `ALLOC_LOCK`，提供 `alloc_lock()`、`with_frames()`、`frames_locked()` 与 `allocated_pages()`；[heap.rs](../../../kernel/src/heap.rs) `SlabHeap::{alloc, dealloc}` 统一纳入 `ALLOC_LOCK` 保护并复用 `frames_locked` 消除锁重入；[vmo.rs](../../../kernel/src/vmo.rs)、[vmar.rs](../../../kernel/src/vmar.rs)、[syscall.rs](../../../kernel/src/syscall.rs)、[dpg.rs](../../../kernel/src/dpg.rs) 与 [main.rs](../../../kernel/src/main.rs) 全量切入 `*_global` 安全接口（`zero_frame_global` 在锁外执行 4KiB `DC ZVA`，`decommit_global` 在锁外释放临时 `Vec`）。

## Alternatives considered

- 将物理帧与内核堆继续绑在 rank 4 `KERN_LOCK`（`TABLE`）上，仅给 `HT_DRAINQ` 补 `KERN_LOCK`：拒绝——任何持 `PT` 锁或调度路径内的 `Vec`/`Box` 析构一旦反向请求 rank 4 `KERN_LOCK` 即构成同层重入或逆序死锁；独立 rank 5 `ALLOC_LOCK` 严丝合缝嵌在 `TABLE/PT (4) -> ALLOC (5) -> ASID (6) -> PRINT (7)` 既有全序链中，允许持 `TABLE`/`PT` 者顺向分配/释放内存且临界区缩短至纯指针/位图操作。
- 在 `wait_pending` 自旋前临时开中断接收 SGI：拒绝——部分调用点（如异常探针 ZFOD 返回前）严禁提前开中断暴露 `ELR_EL1`/`SPSR_EL1`；原子标志内联代消费无需开中断即可在零锁状态下完成跨核互助失效。

## Consequences

异常探针进出锁窗严格守恒 `DAIF`，双核同时关中断发起 TLB shootdown 零死锁自解，全部运行时帧分配、归还与 `SlabHeap` 堆操作由第 5 层 `ALLOC_LOCK` + `lockdep` 在线执法保护。开启 `--features lockdep` 实跑双核 rootfs 舰队时当场抓获一处潜伏锁序倒置：[lib.rs](../../../svcface/src/lib.rs) 与 [syscall.rs](../../../kernel/src/syscall.rs) 原将零表零指针的 `0x3F thread_yield` 误挂在 `SVC_CLASS_RQ_TABLE`（预取 rank 4 `TABLE`），随后 [sched.rs](../../../sched/src/sched.rs) `yield_now_keep_irq` 尾段 `reap_drain` 反向请求 rank 3 `REAPQ`（`chain=[RQ0, RQ1, TABLE] -> REAPQ` AB-BA 死锁隐患），已将 `0x3F` 对齐 `0x37` 收归 `SVC_CLASS_SQ_RQ` 并在 `yield_now_impl` 加 `!held_test(HELD_TABLE)` 双保险。复验 `berry-svcface` 宿主机单测、双核 `TLBS ok n=10`、`ALLOC ok n=320`、`HT ok n=320`、双核 rootfs 按需调页舰队（`DPG`/`DPGP`/`DPGS`/`DPGN`）与单核确定性 `ktest`（含 `test_f0_3`）+ `BENCH ok n=8` 全绿，零 lockdep 违例、零帧泄漏。

## Related

- 缺陷来源：[2026-11-18-全栈架构与代码缺陷深研.md](../../research/2026-11-18-全栈架构与代码缺陷深研.md) K1–K3、K9、K10
- 关联历史笔记：[2026-11-13-p24-smp-dual-window-not-applicable.md](2026-11-13-p24-smp-dual-window-not-applicable.md)
- 实施规格与计划：[2026-11-18-微内核与全栈底座彻底重构规格.md](../../plans/2026-11-18-微内核与全栈底座彻底重构规格.md)、[2026-11-18-全栈底座彻底重构计划.md](../../plans/2026-11-18-全栈底座彻底重构计划.md)
