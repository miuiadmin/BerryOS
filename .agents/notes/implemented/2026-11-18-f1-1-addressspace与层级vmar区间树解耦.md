# F1-1：`AddressSpace` 与层级 `Vmar` 区间树解耦、每空间独立 `pt_lock` 及 `DEMAND_ROOTS` 收口

Status: implemented

## Problem

全栈底座审计检出虚拟地址空间与区间管理层存在四项耦合缺陷：

1. [vmar.rs](../../../kernel/src/vmar.rs) 将硬件页表根 `l1_pa`、ASID 所有权与区间线性表混装在单一 `Vmar` 结构内：一方面 `unmap` 与 `protect` 仅支持精确整段匹配，无法支持用户态对连续映射做局部切片解除或改权；另一方面 `allocate` 切出的子 `Vmar` 释放（`Drop`）后，父 `Vmar` 中的 `Reserved` 占位永久残留，父区间无法重新映射。
2. 中间页表物理帧（L2/L3）在分配后未归集到统一对象账本，阻碍进程退出时的确定性页表帧全回收。
3. 页表锁采用 64KiB 全局静态数组 `PT_LOCKS: [AtomicBool; 0x1_0000]` 按 ASID 索引，不仅常驻占用 64KiB 内核 BSS，且在 ASID 演进与跨空间并发时缺乏对象级内聚。
4. 按需调页反查表 `DEMAND_ROOTS` 原登记 `*mut Vmar`，导致同一地址空间下的子 `Vmar` 调用 `vmar_map` 映射按需页时与根 `Vmar` 触发 `OwnerConflict` 误拒，且 `AtomicRootSlots::slot` 误用 `Ordering::Relaxed` 读配对写端 `Ordering::Release`，在 ARM64 弱内存序下缺少同步保证，同时栈对象搬移还会破坏裸指针地址耦合。

## Decision

1. 新建宿主机可测纯算法 crate `berry-vmarface`（`vmarface/src/lib.rs`）：基于双层 `BTreeMap`（每作用域 `VmarScope` + 全空间扁平叶索引 `active_leaves`）实现层级区间树 `VmarTree<M>`，严格固定 `Unaligned -> OutOfBounds -> Overlap` 判序优先级，原生支持任意连续子区间/跨相邻多段无副作用预检与原子切分 `unmap_range`、`protect_range`、O(log N) 全树 `demand_hit` 以及递归释放父级占位的 `destroy_vmar` 与整树排空 `drain_all`。
2. 在 [aspace.rs](../../../kernel/src/aspace.rs) 落地实体对象 `AddressSpace`：独占 `l1_pa`、`asid: AtomicU16`、对象内嵌 `pt_lock: AtomicBool`（集中挂接 lockdep `CLASS_PT`，彻底删除 [vmar.rs](../../../kernel/src/vmar.rs) 的 64KiB `PT_LOCKS` 全局数组）、L1/L2/L3 页表物理帧账本 `page_tables: SpinLock<Vec<usize>>` 及 `tree: SpinLock<VmarTree<VmoRegionMeta>>`；[process.rs](../../../kernel/src/process.rs) `Process` 直接持有 `Arc<AddressSpace>`，使 `pt_lock_current()` 经 `current_pinned()` O(1) 直达本进程 `AddressSpace.pt_lock`（引导期裸 `UserProcess` 经 16 槽 `BOOT_ASPACES` 兜底）。
3. 将 [vmar.rs](../../../kernel/src/vmar.rs) 的 `Vmar` 降级为指向 `Arc<AddressSpace>` 的轻量句柄投影 `{ aspace, vmar_id, base, len }`：子 `Vmar` 析构时自动调 `aspace.destroy_vmar(vmar_id)` 递归清叶、退还映射钉与按需钉并释放父级 `SubVmar` 预留。
4. 将 `DEMAND_ROOTS`（[vmar.rs](../../../kernel/src/vmar.rs)、[syscall.rs](../../../kernel/src/syscall.rs)、[dpg.rs](../../../kernel/src/dpg.rs)）全面切换为登记堆上稳定锚定的 `*const AddressSpace`，根与任意层级子 `Vmar` 共享同一指针天然幂等零冲突，在最后一个 `Arc<AddressSpace>` 析构或拆解时统一注销；并将 `AtomicRootSlots` 读写升级为严格配对的 `Release` / `Acquire` 内存序。

## Alternatives considered

- 在 `AddressSpace` 中缓存根 `Vmar` 裸指针继续以 `*mut Vmar` 登记 `DEMAND_ROOTS`：拒绝——若子 `Vmar` 先于根 `Vmar` 触发按需映射或根 `Vmar` 句柄先于子 `Vmar` 关闭，裸 `*mut Vmar` 会产生悬垂指针或提前清槽；直接登记 `Arc::as_ptr(&aspace)` 在全部句柄与进程存续期内恒定有效且与栈搬移彻底解耦。
- 保留 64KiB `PT_LOCKS` 全局数组而不迁入 `AddressSpace`：拒绝——`Process` 已直接持有 `Arc<AddressSpace>`，锁跟随对象既消除 64KiB 静态 BSS 死重，又从根源杜绝跨代同号空间的锁槽假共享。

## Consequences

根与多层嵌套子 `Vmar` 完全共享同一 `AddressSpace` 页表树、对象级 `pt_lock` 与按需调页身份，子 `Vmar` 关闭后父区间立即可重映射，连续映射支持页粒度局部 `protect` 与 `unmap` 切分，且全量 L1/L2/L3 页表帧账本与 `AddressSpace::teardown()` 已就位，为下一刀 F1-2 进程退出两阶段自动拆表与废除 `leak = true` 铺平底座。内核静态 BSS 净减约 64KiB，`berry-vmarface` 宿主机单测、`ktest` `test_f1_1` 与单双核 `lockdep` 冒烟全绿。

## Related

- 缺陷来源：[2026-11-18-全栈架构与代码缺陷深研.md](../../research/2026-11-18-全栈架构与代码缺陷深研.md) K5、K6、K7
- 前序 PT 锁背景：[2026-11-13-p24-smp-dual-window-not-applicable.md](2026-11-13-p24-smp-dual-window-not-applicable.md)
- 实施规格与计划：[2026-11-18-微内核与全栈底座彻底重构规格.md](../../plans/2026-11-18-微内核与全栈底座彻底重构规格.md)、[2026-11-18-全栈底座彻底重构计划.md](../../plans/2026-11-18-全栈底座彻底重构计划.md)
