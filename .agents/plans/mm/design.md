# mm（内存管理与弹性调页）总体设计

Type: spec
Status: active
Updated: 2026-11-18
Version: 2.1.0

一句话说明：`mm` 是莓核（Meihe）的内存管理与弹性调页子系统——提供动态大物理地址空间、每核无争用页帧与对象弹匣、65,536 位 ASID 位图与无死锁 TLB 一致性、硬件强制 W^X 与内核栈守卫页、确定性进程退出全量回收，以及从 L0 可弃缓存到 L3 墓碑真回收的五级内存弹性闭环。

## 背景

存量内存子系统完成了伙伴分配器（`mm/src/buddy.rs`）、小对象 Slab（`kernel/src/heap.rs`）、高半直映翻转与每进程页表（`arch/aarch64/src/mmu.rs`）、VMO/Vmar 映射（`kernel/src/vmo.rs`、`vmar.rs`）及 32 槽按需调页批供（`kernel/src/dpg.rs`、`0x21`/`0x23`/`0x24`）。但如 [莓核全核深度体检报告](../../research/2026-11-18-莓核全核深度体检报告.md) M1–M7 所示，底层存在 SMP 双核 TLB Shootdown 关中断对飞死锁、ASID 回卷无活跃位图撞号击穿隔离、进程退出永久泄漏全部在映物理页与页表树、全用户态 RWX 裸奔、256MiB RAM / 4GiB PA 硬顶，以及内存压力五级梯度物理回收量为零等结构性缺陷。本规格承接 [meihe 总体设计](../meihe/design.md) 与 [莓核二代终态架构六大核心裁决](../../notes/proposed/2026-11-18-莓核2代终态架构六大核心裁决.md)，定义莓核 2.0 内存子系统的完整内部契约；对外跨层现行契约见 [内存管理设计 v1.1](../../../docs/内存管理设计-v1.1.md)。

## 范围

- **覆盖**：动态物理内存探测、PMM 伙伴分配器与每核 `PerCpuPageset`（PCP）；`SlabHeap` 每核 9 档无锁 `Magazine` 与空页退还；65,536 位 `AsidAllocator` 与跨核无死锁 TLB 一致性状态机；页表 W^X、`PAN`、VIPT 指令缓存一致性与内核栈虚拟守卫页；`Vmar` 区间平衡树（子区间切分、ASLR、`query`）与 `Teardown-Before-Drain` 退出全回收；两级基数页簇 `RadixPageLedger`、2B/帧 `FrameRefTable` 写时复制（CoW）、显式 `VMO_OPT_ZFOD`、`VMO_OPT_CONTIGUOUS`、L0 `VMO_OPT_DISCARDABLE` 与 L1 `0x22 pager_op_range` 回收协议。
- **不覆盖**：用户态 `pager` 内部的压缩算法（如 LZ4/ZSTD 字典管理）与闪存磨损日预算策略（归 [am 总体设计](../am/design.md) 与用户态 `pager` 服务）；SMMUv3 硬件设备流表寄存器编程（归 [hal 总体设计](../hal/design.md)）；跨插槽 NUMA 节点亲和分配。

## 契约

### 一、动态物理内存布局与分层无争用分配器

1. **突破 4GiB PA 与 256MiB RAM 静态墙**：
   - 启动期读取硬件支持的最大物理地址位宽（ARM64 读 `ID_AA64MMFR0_EL1.PARange` 映射至 `TCR_EL1.IPS`，至少启用 40-bit = 1TiB PA；x86_64 读 `CPUID.80000008H:EAX[7:0]`）；
   - 由 `BerryHandoff`（UEFI MemoryMap）或 DTB `/memory` 扫描可用物理内存段 `[ram_base, ram_top)`（支持非连续保留洞与顶端 16KiB `ramoops` 预留扣除），在首个足够大的常规物理段内一次性切出三块连续启动元数据数组，随后建立覆盖全部物理 RAM 的高半 `1GiB`（L1 block）/`2MiB`（L2 block）线性直映（强制置 `PXN | UXN`，严禁直映区可执行）：

```text
早期动态元数据切分（以 4GiB RAM = 1,048,576 个 4KiB 帧为例）：
  1. Buddy 元数据（阶数组 + 双级空闲头位图）：约 2.1 MiB
  2. FrameRefTable（&'static [AtomicU16]，每帧 2B 引用与锁页标志）：2.0 MiB
  3. L2 直映页表池（按需切分非 1GiB 对齐段）：≤ 64 KiB
```

2. **`FrameRefTable` 帧级引用计数编码（2B / 物理帧）**：
   - `bits [14:0]`：共享引用计数 `refcnt`（`0` = 空闲在 Buddy 中；`1` = 独占已分配帧；`2..=32767` = CoW 或跨 VMO 共享帧；达 `32767` 饱和时拒绝继续 CoW 共享并退化为深拷贝新帧，永不回绕）；
   - `bit 15`（`FRAME_PINNED`）：DMA 锁页标志（置位期间禁止 `pager_op_range` 换出或迁移）。
3. **PMM 两级分配（每核 PCP 热弹匣 + 细锁 Buddy）**：
   - **快路径（Order-0 单页，占缺页与匿名提交 98% 流量）**：经 `PerCpu<PcpList>` 在关抢占守卫 `PreemptPin` 下无锁弹出/压入本核空闲帧单链；参数定标：`low = 16`、`high = 64`、`batch = 16`（空时持 `BuddyLock` 一次批取 16 页，超 `high` 时一次批退 16 页归还 Buddy）；
   - **慢路径（`order >= 1` 或 PCP 批取/批退）**：持叶子锁 `BuddyLock`（Rank 80）操作双层位图 O(1) 伙伴分配器（`MAX_ORDER = 13`，最大单块连续物理内存 `32MiB`）；
   - **水位信号钩子**：`BuddyAllocator::free_pages()` 每次跌破 `low_watermark`（默认总帧数 `5%`）时，无锁置位系统压力事件低水位标志，唤醒用户态 `appd`/`am` 启动 L0–L3 回收，彻底消除纯 `vmo_commit` 吃空内存零预警的盲区。
4. **`SlabHeap` 每核 Magazine 与空页归还**：
   - 维持 9 档尺寸类 `[8, 16, 32, 64, 128, 256, 512, 1024, 2048]`（`> 2048B` 直走 `BuddyAllocator`）；
   - 每核每档维护双弹匣 `Magazine`（容量 32 槽），常规 `alloc`/`dealloc` 纯本核无锁；跨核释放挂入页头原子 `remote_free` 链；
   - 每张 4KiB Slab 页首内嵌 32B `SlabPageHdr { in_use: u16, class_idx: u8, flags: u8, next, prev, remote_free: AtomicPtr }`；当 `in_use` 归零且本档空闲整页缓存超过 4 页时，自动摘除该页退还 PCP/Buddy，根治小对象堆单向膨胀不退内存的顽疾。

### 二、ASID 活跃位图与无死锁跨核 TLB 一致性

1. **`AsidAllocator` 位图与代号契约（`mm/src/asid.rs`）**：
   - 数据结构：`active_bitmap: [u64; 1024]`（覆盖 16-bit ASID `0..=65535`；`bit 0` 永久置 `1` 保留给内核 `ASID 0`；8-bit 硬件模式仅扫描前 4 个 `u64` 即 `1..=255`）、游标 `cursor: u16`、全局回卷代号 `roll_gen: AtomicU64`（初始 `1`）；
   - **分配算法（持 `AsidLock` Rank 80）**：
     1. 在 `(cursor..=max_asid)` 内利用 `!word != 0` 与 `trailing_zeros()` O(1) 寻找首个 `0` 位，命中则置 `1`、推进 `cursor = id + 1` 并返回 `(id, current_gen)`；
     2. 若游标至末尾全满，执行**安全回卷**：在 `1..cursor` 范围内扫描首个 `0` 位；若全范围无 `0` 位（即当前同时存活进程数已达硬件全部可用 ASID 上限），立即返回 `None`（上层 `process_create` 返 `-ENOMEM`），**严禁分配任何正为 `1` 的在用 ASID**；
     3. 若回卷在 `1..cursor` 找到空闲位 `id`，原子递增 `roll_gen.fetch_add(1, AcqRel)`，置位 `id`，并在锁外执行一次全局 `ArchMmu::tlb_flush_all_is()`（`dsb ishst` → `tlbi vmalle1is` → `dsb ish` → `isb`）清净所有已退役旧进程残留的冷 ASID 表项。
   - **释放算法**：仅在 `Vmar::Drop` 完成全部页表回收后调用 `free_asid(id)` 清零对应 bit（不立即复写 `cursor`，最大化推迟回卷频率）。
2. **无死锁跨核 TLB Shootdown 状态机**：
   - **规则一（升权/新建免广播）**：无效描述符（`valid == 0`）建立新映射（`vmar_map` 首次落叶、`dpg` `supply_page`、`ZFOD` 填零）架构上不驻留有效旧 TLB，仅需 `dsb ishst` 保证页表写可见，**禁止发送任何跨核 TLB IPI 或全刷指令**；
   - **规则二（降权/解映硬件广播主路径）**：`vmar_unmap`、`vmar_protect`（权限收窄）、CoW 父子页降只读及 `pager_op_range` 摘页，在支持硬件广播的合规多核上直接执行区间/ASID 广播：单页或 ≤16 页逐页 `tlbi vae1is, (asid << 48 | vpn)`，>16 页或整空间拆毁执行一次 `tlbi aside1is, (asid << 48)`，前后配 `dsb ishst` 与 `dsb ish`——**全程零软件 IPI 互等，从数学上彻底根除关中断对飞死锁**；
   - **规则三（软件不信任隔离后端兜底，如旧版虚拟化层）**：若启动期 `probe_asid_tlb_isolation()` 判定硬件广播不可信而必须走 IPI 回执，发起核必须在**释放一切自旋锁且开中断（`PSTATE.I = 0`，仅持 `PreemptPin` 禁任务切换）**状态下等待远核回执，使双核同时发起 Shootdown 时均可即时响应对方 IPI，严禁关中断忙等。

### 三、硬件页权限（严格 W^X + PAN + VIPT）与内核栈守卫页

1. **页表权限位映射铁律（以 ARM64 Stage-1 4KiB 叶为例，x86_64 同构映射 `R/W`、`U/S`、`XD`、`SMAP`）**：

| 映射域与请求 `Rights` | 允许性 | `AP[2:1]` | `PXN` | `UXN` | `nG` | 附加动作 |
|---|---|---|---|---|---|---|
| 内核 `.text` 段 | 内核固定 | `10`（EL1 RO） | `0` | `1` | `0` | — |
| 内核 `.rodata` 段 | 内核固定 | `10`（EL1 RO） | `1` | `1` | `0` | — |
| 内核 `.data`/`.bss`/堆/直映 RAM/MMIO | 内核固定 | `00`（EL1 RW） | `1` | `1` | `0` | 禁直映区执行 |
| 用户只读（`READ`） | 合法 | `11`（EL0/1 RO） | `1` | `1` | `1` | CoW 共享页同此档 |
| 用户读写（`READ \| WRITE`） | 合法 | `01`（EL0/1 RW） | `1` | `1` | `1` | 栈/堆/匿名数据页 |
| 用户只读执行（`READ \| EXECUTE`） | 合法 | `11`（EL0/1 RO） | `1` | `0` | `1` | 填页必刷 VIPT I-cache |
| 任意含 `WRITE \| EXECUTE` 组合 | **非法（`-EINVAL`）** | — | — | — | — | **W^X 内核硬拒** |

2. **PAN（Privileged Access Never）与 VIPT I-cache 一致性**：
   - `SCTLR_EL1.SPAN` 置 `0`（异常进入 EL1 自动置 `PSTATE.PAN = 1`），EL1 内核除 `copy_from_user` / `copy_to_user` 显式校验窗口外，任何误触低半用户地址的读写当场触发 EL1 Permission Fault；
   - 凡向带 `RIGHT_EXECUTE` 的用户映射供给物理页（含 `vmar_map` 预提交页与 `dpg` 按需供给页），落 PTE 前必须对目标物理页缓存行逐行执行 `dc cvau` → `dsb ish` → `ic ivau` → `dsb ish`（若硬件 `CTR_EL0.DIC == 1` / `IDC == 1` 则按硬件能力自动跳过对应步骤）。
3. **内核栈虚拟区与守卫页（`KSTACK_VA_REGION`）**：
   - 在高半非直映区划定专用虚拟段 `KSTACK_VA_BASE .. KSTACK_VA_TOP`（容量 32MiB，支持 1,024 个并发内核线程槽）；
   - 每槽步长固定 `32KiB`：低地址 `[slot_base, slot_base + 4KiB)` **恒不映射（Guard Page）**，高地址 `[slot_base + 4KiB, slot_base + 20KiB)`（标准 16KiB 栈，按需可配 28KiB）映射独立物理帧；线程退出经本核 `REAPQ` 回收时解映物理帧归还 PCP，槽号入栈槽空闲位图。

### 四、`Vmar` 平衡区间树与确定性退出全回收

1. **`Vmar` 区间能力完备化**：
   - 将线性 `Vec<Region>` 升级为按 `base_va` 排序的无重叠区间树（`BTreeMap<usize, Region>` 或侵入式红黑树），查询与插入 `O(log M)`；
   - **自动选址与 ASLR**：当 `vmar_map` / `vmar_allocate` 传入 `*vaddr == 0`（无 `VMAR_OPT_SPECIFIC`）时，在可用空洞中结合硬件熵源（`RNDR` / `CNTVCT` 混合种子）随机偏移选址并回写 `*vaddr`；
   - **子区间拆分与映射自省（`0x1F berry_vmar_query`）**：`vmar_unmap(vaddr, len)` 与 `vmar_protect(rights, vaddr, len)` 支持对已映射 `Region` 的前缀、后缀或中段做精确拆分（1 段拆 2 段或 3 段），同步更新拆分段对应的 `Vmo` 页范围 `mapcnt`；新增 `0x1F berry_vmar_query(vmar_h, va, *out_info)`（门控 `RIGHT_INSPECT`），持 `VmarLock` 在区间树中 $O(\log M)$ 查找包含 `va` 的活跃映射并写出 48B `#[repr(C)] VmarRegionInfo { base_va: u64, len: u64, vmo_koid: u64, vmo_offset: u64, perms: u32, flags: u32, rsvd: u64 }`（未命中返 `-ENOENT`），供调试器 `bdbg`、栈展开器与分配器精确自省；
   - `Vmar::allocate` 全程持本空间 `VmarLock`（Rank 30），消除无锁修改区间表的竞态。
2. **Teardown-Before-Drain 退出三步铁律**：
   - 任何进程进入终结清理（自愿 `process_exit`、最后线程 `thread_exit`、`process_terminate`、`job_kill` 或 EL0 未捕获故障强杀），由所在核本地排空路径严格按以下三步执行：
     1. **`root_vmar.teardown_all()`**：持 `VmarLock` 遍历全部叶区间与子 `Vmar`，清零所有有效叶 PTE，累计每个关联 `Vmo` 的解映页数后释放 `VmarLock`，执行单次整空间 `tlbi aside1is`，再逐个持 `VmoLock`（或原子扣减）配平 `mapcnt` 与 `demand_pins`，并唤醒/取消 `DpgPool` 中隶属本 `Vmar` 的在途缺页槽；
     2. **`proc.ht.drain()`**：此时本进程对任何 `Vmo` 已无活跃页表映射，所有槽位无条件执行 `PinnedObj::release`（不再跳过任何 VMO），独占 VMO 立即触发 `Vmo::Drop` 将全部物理帧退回 PCP/Buddy 并扣减所属 `Job.ram_bytes_used`；
     3. **`Vmar::Drop`**：递归遍历 L1 → L2 → L3 页表目录，释放全部中间页表物理帧退回 PCP/Buddy（同步退还 `Job.ram_bytes_used`），最后调用 `free_asid(self.asid)`。

### 五、基数页簇 VMO、写时复制（CoW）与五级内存回收闭环

1. **两级按需基数页簇（`RadixPageLedger`）**：
   - 替代原 `Vec<Option<u64>> + Vec<u16>` 全尺寸预分配数组：一级目录按 `2MiB`（512 页）分簇，未触及簇指针为 `None`（建 128MiB 空 VMO 初始堆占用仅 **256B** 而非 576KiB）；叶簇存储紧凑 `PageSlot(u64)`：
     - `bits [35:0]`：物理页帧号 `PFN`（覆盖最高 256TiB 物理地址）；
     - `bits [51:36]`：本 VMO 内该页映射计数 `mapcnt: u16`；
     - `bits [55:52]`：页状态枚举 `Empty(0) | Committed(1) | CowShared(2) | SwappedOut(3) | Discarded(4)`；
     - `bits [63:56]`：保留标志（如脏位镜像）。
2. **VMO 创建选项与写时复制（`vmo_create_child`）**：
   - `vmo_create(size, options, *out)` 正式支持正交位：`VMO_OPT_DEMAND (1<<0)`、`VMO_OPT_DISCARDABLE (1<<1)`（仅纯匿名合法）、`VMO_OPT_ZFOD (1<<2)`（显式零填充按需页，**废除单页零供给自动污染整对象的旧 `mark_zfod`**）、`VMO_OPT_CONTIGUOUS (1<<3)`（需设备 `Resource` 授权，Buddy 按阶分配连续物理帧并置 `FRAME_PINNED`）；
   - 新增 add-only `vmo_create_child(parent_vmo, options, offset, size, *out)`（`VMO_CHILD_COW = 1<<0`）：持父 `VmoLock` 将区间内已提交叶标为 `CowShared`、`FrameRefTable[pfn].fetch_add(1)`，并将映射该父 VMO 的可写 PTE 降为只读（`AP_USER_RO`）后执行一次 `tlbi`；任一侧写触发 Permission Fault 时，若 `FrameRefTable[pfn] > 1` 则从 PCP 取新帧拷入 4KiB、原帧 `fetch_sub(1)`、重映射为 `UserRw`；若 `FrameRefTable[pfn] == 1`（对端已先分裂或释放）则原地升回 `UserRw` 免拷贝；
   - **不可逆内存封存（`0x1E berry_vmo_seal`）**：要求句柄持 `RIGHT_MANAGE`，持 `VmoLock`（Rank 40）校验：若请求置 `SEAL_WRITE (1<<0)` 且当前 `writable_mapcnt > 0`（任意地址空间仍存活跃可写映射），立即返回 `-EBUSY` 拒绝封存；校验通过后单调不可逆置位 `vmo.seals |= seal_flags`（支持 `SEAL_WRITE | SEAL_GROW (1<<1) | SEAL_SHRINK (1<<2) | SEAL_EXEC (1<<3)`），此后任何 `vmo_write`、带 `WRITE` 的 `vmar_map/protect` 或尺寸变更一律硬拒 `-EACCES`，为进程外编译器 `aotd` 交付严格满足硬件 `W^X` 的只读可执行代码 VMO 提供内核数学保证。
3. **多文件系统独立调页器（`0x25 berry_pager_create`）与五级内存回收闭环**：
   - **一等 `ObjType::Pager(15)` 异步端口投递**：新增 `0x25 berry_pager_create(port_h, key, options, *out_pager)`（要求 `port_h` 持 `RIGHT_WRITE`），允许 `BerryROFS`、`BerryFS` 与压缩换出守护进程各自创建独立调页器并绑定其专属后备 `Vmo`；目标 `Vmo` 发生缺页时，内核不再挤占单一全局 `svcmgr` 槽池，而是直接向关联 `Pager` 的 `Port` 投递 `PortPacket { key, kind = PAGER_REQ, payload = fault_pgoff | flags }` 并将缺页线程挂入该 `Pager` 等待链（同时建立 `Task -> Pager` 跨核 PI 边），服务端经 `0x21/0x23/0x24` 供给页帧即精准唤醒；
   - **内联按需根与看门狗全覆盖**：移除全局 16 槽 `DEMAND_ROOTS` 硬顶，在 `Vmar` 内直接持有按需区间计数；`dpg` 槽池扩至 64 槽并按绑定 `pager` 分离唤醒通道，`EOVERFLOW_KEY` 排队等待与槽内等待共享同一个 **30 秒绝对截止期看门狗**（严禁 `u64::MAX` 无限死等）；
   - **L0 `VMO_OPT_DISCARDABLE`**：`Vmo` 维护 `lock_count: u32`；`vmo_commit` 递增锁计数（若期间曾被内核摘页则返回特定状态位提示调用方重建缓存），`vmo_decommit` 递减锁计数并将 `lock_count == 0 && total_mapcnt == 0` 的 VMO 挂入全局 `DISCARD_LRU`；PMM 触发低水位时自动按 LRU 摘除其已提交帧归还 PCP；
   - **L1 `0x22 pager_op_range(pager_vmo, op, offset, len)`**：激活在册 `0x22` 号，支持 `PAGER_OP_EVICT = 1`（内核校验目标页无 `FRAME_PINNED`、解映关联只读/冻结进程 PTE、刷 TLB、将页状态标为 `SwappedOut` 并归还物理帧给 PCP；后续解冻访问自然走既有 `dpg` 缺页供给链恢复）与 `PAGER_OP_FAIL = 2`（pager 显式通知读盘/解压不可恢复错误，唤醒等待中的缺页线程令其精确触发故障退出，无需干等 30 秒看门狗）。

## 验收标准

1. **宿主机纯核单测（`faces/mm*`）**：覆盖 8-bit/16-bit ASID 位图满载回卷不撞活号、`RadixPageLedger` 稀疏空洞与 CoW 引用计数守恒、`Vmar` 区间任意前缀/中段/后缀拆分合并、L0 `DISCARDABLE` 锁状态机与 PCP/Magazine 水位流转，100% 零 `unsafe` 通过。
2. **隔离与硬件防线验证**：尝试 `vmar_map` / `vmar_protect` 传入 `WRITE | EXECUTE` 必返 `-EINVAL`；用户只读页写必触发 EL0 Permission Fault；EL1 内核裸读低半用户指针必触发 PAN Fault；故意递归耗尽内核线程栈必精确命中所配 `4KiB` 守卫页而非破坏堆。
3. **零泄漏与多核高并发压测**：16 核 QEMU 下并发执行 1,000 轮「进程创建 + 匿名/Demand/CoW VMO 映射 + 子区间 `unmap`/`protect` + 随机崩溃/强杀退出」，双核/16 核零 TLB 对飞楔死，压测前后 `BuddyAllocator::allocated_pages()` 与 `Job` 内存账本差值严格为 **0**；注入 `memhog` 负载时 PMM 低水位信号正常触发 L0 弃页与 L1 换出，物理空闲帧曲线真实回升。

## 适用边界

- 用户态 ELF 二进制链接脚本须保证代码段（`.text`）与可写数据段（`.data`/`.bss`）4KiB 页对齐隔离（`berry-ld` `-z max-page-size=4096` 已满足；早期内嵌汇编测试模板在启用 W^X 同刀按页拆开代码与可写区）。
- `2MiB` 透明大页（THP）不在 2.0 首轮强制面，留 `RadixPageLedger` 一级簇整簇晋升扩展位。
