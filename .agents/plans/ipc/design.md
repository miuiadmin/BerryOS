# ipc（异步端口、定向让片通道与分层能力沙箱）总体设计

Type: spec
Status: active
Updated: 2026-11-18
Version: 2.1.0

一句话说明：`ipc` 是莓核（Meihe）的对象句柄、进程间通信、多路事件通知与层次化能力沙箱子系统——通过 `ChannelShared` 无环双端、`Port` 侵入式观察者链、`channel_call` 同带定向让片、真共享 VMO 环形通道，以及 `Job` 树配额 + 内联 `Badge` + `RevokeEpoch` 纪元撤销，实现微秒级零惊群通信与不可绕过的纯能力安全边界。

## 背景

存量 IPC 与能力实现（`kernel/src/handle.rs`、`channel.rs`、`event.rs`、`pi.rs`、`process.rs`）跑通了单进程槽位表、非阻塞双端队列、信号掩码与基础优先级继承。但如 [莓核全核深度体检报告](../../research/2026-11-18-莓核全核深度体检报告.md) I1–I7 及 S3 所示，`wait_many` 依赖全局 `WAIT_ANY` 哨兵使常驻服务令全系统 Channel Fastpath 稳态命中率归零并引发全表虚假唤醒；Channel 双端互持 `ObjRef` 成环与强杀阻塞线程造成永久泄漏；`MsgBilling` 置于 per-CPU 静态槽引发跨任务串账且应答反向扣费；缺乏 `PEER_CLOSED`/`ERR_DEAD(-52)`、`Job` 树形物理内存配额、多客鉴别 `Badge` 与能力撤销机制；PI 在双核跨核与多路等待链上失明。本规格依据 [meihe 总体设计](../meihe/design.md) 与 [莓核二代终态架构六大核心裁决](../../notes/proposed/2026-11-18-莓核2代终态架构六大核心裁决.md)，确立莓核 2.0 `ipc` 独立核心 crate 的完整契约；跨层现行契约见 [IPC 设计 v1.0](../../../docs/IPC设计-v1.0.md)、[能力模型设计 v1.0](../../../docs/能力模型设计-v1.0.md) 与 [系统调用 ABI 设计 v1.0](../../../docs/系统调用ABI设计-v1.0.md)。

## 范围

- **覆盖**：`PinnedObj<T>`（原 `ObjRef`）原子生命周期与 `Task::sleep_pins` 强杀配平；`HandleTable` 槽位布局、空闲链、64 位 `Rights` 全宽执法、内联不可伪造 `Badge` 与 `RevokeEpoch` O(1) 撤销；`ObjType::Job(11)` 树容器、父链无锁物理内存/进程/能耗配额与 `job_kill`；`ChannelShared` 无环双端与 `PEER_CLOSED`/`ERR_DEAD` 状态机；`ObjType::Port` 与侵入式 `ObserverList`（含 `wait_many` 栈节点改造）；`channel_call` 事务匹配、同带定向让片（Directed Handoff）与 TCB 级精确受益者计费；PM/AI/遥测真共享 VMO 环协议；跨 16 核跨原语有界增量 PI 算法；新增 add-only 系统调用签名与错误码矩阵。
- **不覆盖**：用户态路径式命名空间守护服务内部路由表结构；网络套接字协议栈（`vnet`）与文件系统（`fsmgr`）上层 wire 报文定义。

## 契约

### 一、原子对象生命周期、句柄槽内联 `Badge` 与 `RevokeEpoch`

1. **`PinnedObj<T>` 原子引用计数与阻塞钉住零泄漏铁律**：
   - 所有内核对象统一包裹于 `PinnedObj { refs: AtomicUsize, koid: u64, obj: KernelObj }`（`koid` 为全局单调唯一 64 位内核对象 ID，永不复用，供遥测、策略表键与调试反查）；
   - 内存序：`retain` 执行 `refs.fetch_add(1, Ordering::Relaxed)`（前置断言旧值 `>= 1`）；`release` 执行 `if refs.fetch_sub(1, Ordering::Release) == 1 { fence(Ordering::Acquire); drop_box(self); }`——跨核共享对象增减引用**全程零锁**；
   - **`Task::sleep_pins` 强杀配平**：线程在 `wait_one`、`wait_many`、`port_wait`、`channel_call` 进入阻塞前，将所有临时 `retain` 的对象指针写入 `Task::sleep_pins: SleepPinSet`（内联容量 16）；无论正常唤醒返回还是被 `process_terminate` / `job_kill` 从睡眠队列强行摘除推入本核 `REAPQ`，均由统一的 `Task::release_sleep_pins()` 幂等清槽释放，根绝强杀泄漏；未捕获硬件异常杀死进程的退出码固定为负错误码（如 `-EFAULT = -14`），与正常 `process_exit(0)` 严格区分。
2. **`HandleTable` 空闲链、64 位权限全宽执法与死槽隔离**：
   - 每进程 `HandleTable` 受独立 `HandleTableLock`（Rank 20）保护，维护侵入式 `free_head: u32` O(1) 分配槽位（废除 `O(N)` 线性扫表）；
   - 句柄 32 位编码维持 `[31:30]=00 保留 | [29:16]=14 位 gen (1..=16383) | [15:0]=16 位 slot (1..=65535)`；当某槽经历 16,383 次关闭到达代号上限时，归入冷隔离区不再参与热链扫描，且长命驱动改用持久共享 VMO + `Port` 后不再每 I/O 创建销毁临时句柄；
   - 全路径（含 `hdlface`、`channel_send`、`process_start` bootstrap）统一以 64 位 `u64` 校验 `Rights`：**`process_start` 迁移非零 `bootstrap` 必须校验源句柄持 `RIGHT_TRANSFER`**（缺则返 `-EACCES`）；`process_create` 初始授予父进程对子 `Process` 的 `RIGHT_MANAGE | RIGHT_INSPECT | RIGHT_DUPLICATE | RIGHT_TRANSFER`；`0x88 power_off`、`0x84 telemetry_open`、`0x42 cbox_drain` 全部收归 `Resource` / `root_job` 能力门控。
3. **内联不可伪造 `Badge` 与 `RevokeEpoch` O(1) 撤销**：
   - `SlotState::Used` 定长结构：`{ gen: u16, flags: u16, epoch_id: u32, epoch_expected: u64, badge: u64, rights: u64, obj: NonNull<PinnedObj> }`；
   - **不可伪造 `Badge`（解决多客户端单通道鉴别）**：新增 `handle_mint(src_h, new_rights, badge, epoch_h, *out_h)`——要求源句柄持 `RIGHT_DUPLICATE`，且仅当源句柄当前 `badge == 0` 并持 `RIGHT_SET_POLICY` 时允许铸造非零 `badge`（已带非零 `badge` 的句柄再次 `duplicate`/`mint` 只能原值继承该 `badge`，尝试改值返 `-EACCES`）；服务端建立监听通道后，为每个客户端 `mint` 一枚带专属客户端 ID `badge` 且去掉 `RIGHT_SET_POLICY` 的发送端句柄交付对方；客户端发消息时内核从发送槽读取 `badge` 写入消息信封，服务端 `recv` / `channel_call` 直接拿到内核背书的 `sender_badge`；
   - **`RevokeEpoch` 纪元撤销（无跨表锁子树回收）**：全局/分片维护无锁 `EpochArena`（每槽 `AtomicU64` 单调代号，由 `RevokeEpoch` 内核对象持有所有权）；`handle_mint` 若传入非零 `epoch_h`（需 `RIGHT_MANAGE`），新句柄记录 `(epoch_id, current_epoch_val)`，且其后续一切 `duplicate` 与跨通道 `transfer` 子孙句柄强制继承同一 `(epoch_id, epoch_expected)`；每次句柄查表（`HandleTable::lookup`）若 `epoch_id != 0` 则执行一次 `Acquire` 原子读比对 `EpochArena[epoch_id] == epoch_expected`，一旦授权方调用 `epoch_revoke(epoch_h)` 递增该原子计数器或销毁 `epoch_h`，所有散落在任意进程表中的派生句柄下一次系统调用立即 O(1) 返回 `BERRY_ERR_DEAD (-52)`，无需扫描或锁定任何他进程句柄表。

### 二、`ObjType::Job(11)` 层次化沙箱与物理资源守恒

1. **严格树拓扑**：内核启动创建唯一根容器 `root_job`（经引导槽交付 `svcmgr`）；每个 `Job` 持父指针、子 `Job` 链表与成员 `Process` 链表（结构变更持 `JobLock` Rank 10，父先于子；树深硬帽 `MAX_JOB_DEPTH = 8`）。`sys_process_create(job_h, ...)` 强制要求 `job_h` 为持 `RIGHT_MANAGE` 的存活 `Job`（废除 `job == 0` 免票入口），从根源封死关闭子进程句柄绕过配额的进程炸弹。
2. **沿父链无锁原子账本**：每个 `Job` 内含缓存行对齐的原子配额组：
   - `ram_bytes_used / ram_bytes_max: AtomicU64`：任何隶属该 `Job` 的进程执行 `vmo_commit`、按需缺页填帧、CoW 分裂或分配页表帧前，沿当前 `Job` 向 `root_job` 逐级 CAS 预扣字节数（任一层超 `ram_bytes_max` 立即回滚已扣祖先层并返 `-ENOMEM`）；物理帧释放时沿同链原子退账；
   - `live_procs / max_procs: AtomicU32`：按**存活进程实体**而非句柄槽数计数（进程彻底 `drain` 完毕才减 `1`）；
   - `cpu_runtime_ns: AtomicU64` 与 `energy_uj: AtomicU64`：调度器 `tick` / 切换尾按 EM 功率表累加，供策略与电源服务读取；
   - `policy_flags: u32`：控制子树是否允许声明 `BERRY_INTENT_LATENCY_CRITICAL` 等特权策略位。
3. **`job_kill(job_h)` 两阶段原子清场**：持 `JobLock` 将本 `Job` 及递归子孙 `Job` 状态翻为 `Killing`（当场阻断一切新 `process_create` / `job_create`），收集全部成员 `Process` 引用后释放 `JobLock`，逐一触发与 `process_terminate` 同源的强杀与 `Teardown-Before-Drain` 回收，最后置 `SIGNAL_TERMINATED` 唤醒监督者。

### 三、`ChannelShared` 无环双端、`Port` 观察者链与 `channel_call` 定向让片

1. **`ChannelShared` 无环控制块与死活状态机**：
   - `channel_create` 仅分配单个 `PinnedObj<ChannelShared>`，内含两套单向定长环形缓冲 `RingQueue<MessageSlot, 16>`（按实际 `nbytes` 与 `nhandles` 切片拷贝，零整包预清零、零 `Vec::remove(0)` 搬移）、双端打开句柄计数 `open_cnt: [AtomicU32; 2]`、双端观察者链 `observers: [ObserverList; 2]` 与单把 `ObjStateLock`（Rank 50）；`HandleTable` 用 `SlotState::flags` bit0 标记本句柄代表端 `0` 还是端 `1`；
   - 某端最后一个句柄关闭（`open_cnt[side].fetch_sub(1) == 1`）时：在 `ObjStateLock` 下清退该端无人再读的入站队列残留消息（自动 `release` 消息内随行句柄，零泄漏），对另一端置派生信号 `SIGNAL_PEER_CLOSED (1 << 4)`，唤醒另一端所有阻塞线程、`call_waiters` 与 `Port` 观察者；当 `open_cnt[0] == 0 && open_cnt[1] == 0` 时，`PinnedObj` 引用归零当场释放 `ChannelShared`；
   - **死端错误码与强杀管道 `EOF` 唤醒契约**：对 `peer_closed` 的通道执行 `channel_send` 或 `channel_call` 立即返回 `BERRY_ERR_DEAD (-52)`；执行 `channel_recv` 时若本端环内仍有对端临终前发出的消息则正常返回消息，环空后返回 `BERRY_ERR_DEAD (-52)`。当上游进程自愿退出或被 `process_terminate` / `job_kill` 强杀时，内核 `Teardown-Before-Drain` 执行 `HandleTable::drain()` 关闭其持有的全部通道与共享 `IoRing` 关联句柄，必定同步触发 `SIGNAL_PEER_CLOSED` 唤醒下游管道与伪终端读者立即返回 `0 (EOF)`，根除被杀进程无用户态 `exit` 导致的下游永久挂死。
   - **内核对象类型（`ObjType`）2.0 权威扩编**：在既有 `1..=12`（至 `SuspendToken = 12`）基础上，统一锁定新增对象枚举值：`Port = 13`、`Epoch = 14`、`Pager = 15`、`Bti = 16`、`Pmt = 17`、`Guest = 18`、`Vcpu = 19`，供 `0x07 object_get_info` 与终端 `lscap` 单源识别。
2. **`ObjType::Port` 与侵入式观察者链（废除 `WAIT_ANY` 哨兵）**：
   - 可等待内核对象（`ChannelShared` 双端、`Event`、`Process`、`Thread`、`SuspendToken`、`Interrupt`、`Timer`）统一内嵌 16B `ObserverList` 头；
   - **异步端口路径**：`object_wait_async(obj_h, port_h, key, signals, options)`（支持 `WAIT_ASYNC_ONCE = 0` 单次自动摘除与 `WAIT_ASYNC_EDGE = 1` 边沿持续投递）分配或复用绑定槽挂入目标 `ObserverList`；目标对象信号跃迁满足 `observed & signals != 0` 时，在 `ObjStateLock` 内直接向目标 `Port` 的环形就绪队列压入 32B 定长包并唤醒 `port_wait` 等待者：

```text
PortPacket 布局（32B，#[repr(C)]，编译期 offset_of 断言钉死）：
  偏移  宽度  字段       语义
  0     8     key        调用方注册时传入的不透明上下文键（如 fd / 指针 / 会话号）
  8     8     signals    触发时刻该对象的完整满足信号位掩码
  16    4     kind       1 = SIGNAL_ONE | 2 = SIGNAL_EDGE | 3 = USER_PACKET | 4 = INTERRUPT
  20    4     status     0（BERRY_OK）或负状态码（如对端关闭时的附加诊断）
  24    8     payload    附加字（如 Interrupt 触发时间戳 CNTVCT 或用户自定义投递字）
```

   - **兼容 `object_wait_many`（≤16 路）零分配改造**：在 `sys_object_wait_many` 内核栈帧上声明 `[StackObserver; 16]`，逐个挂入目标对象各自的 `ObserverList`（均指向当前 `Task`），未命中则挂入当前任务专属睡眠槽；任一目标对象置位即精确唤醒本任务，返回前统一从最多 16 个对象的链上 O(1) 摘除——**全系统彻底删除 `WAIT_ANY` 哨兵与 `WAIT_ANY_WAITERS` 全 `SLEEPQ` 扫描，常驻服务在线时无关通道的 Channel Fastpath 100% 保持畅通**。
3. **`channel_call` 同步 RPC、同带定向让片与 TCB 精确计费**：
   - **事务匹配免混包**：客户端调 `channel_call` 时，内核原子生成非零单调 `txid: u64`（置高位标识 Call 请求）填入消息头，若服务端恰好阻塞在等待该通道新请求上，直接准备直切；服务端 `channel_recv` 收到请求后，处理完毕通过带应答标志的发送（或 `channel_reply`）回传相同 `txid`，内核直接在通道 `call_waiters` 链上 O(1) 匹配到等待该 `txid` 的客户端线程，将应答字节与随行句柄直接拷入客户端接收缓冲，免经公共入站队列（彻底消灭多客户端共享通道时的外来应答串包）；
   - **同带定向时间片借予（Directed Handoff）**：当 `channel_call` 唤醒唯一等待中的服务端线程 `S` 时，即便 `S.prio_effective == C.prio_effective`（如常规用户态服务皆为 `0`）且同属一个未处于 `THROTTLE` 的 `FairBand`，调用方 `C` 将自身当前剩余物理时间片 `slice_left` 的一半（或上限 `2ms`）临时借予 `S` 并立即在当前核（若 `S` 可运行于本核）执行上下文直切——既把同级同步 RPC 延迟压至单次上下文切换级，又因借出的是 `C` 已获准的片内额度而严格不破坏 EEVDF 公平性与 CBS 带包络；
   - **TCB 级受益者计费闭环**：移除 per-CPU 全局 `BILLING_RECV_*`；在 `Task` TCB 内置 `billing_donor_job: u64` 与 `billing_enter_runtime_cycles: u64`（取本线程自身在 CPU 上累计执行的真实周期数而非挂钟，免疫中途抢占穿插！）；服务端 `recv` 请求消息时记录客户端所属 `Job` 域与自身当前运行周期，发出应答消息时按「本线程期间实际消耗周期」精确计入客户端 `Job` 账下并清空借入态，且客户端收到应答包（带 `REPLY` 位）绝不反向开启计费。
4. **真共享 VMO 无锁环形通道（PM / AI / 遥测热路径）**：
   - 兑现 [架构综合纲领](../../../docs/架构综合纲领.md) 第 4 轴硬约束：定义统一 64B 缓存行对齐头 `SharedRingHdr { magic: u32, ver: u16, slot_size: u16, capacity: u32, flags: u32, head: AtomicU64, tail: AtomicU64, dropped: AtomicU64, ... }`（驻留共享 VMO 首缓存行，数据槽紧随其后）；
   - 新增遥测/策略环映射出口（如 `telemetry_open` 扩展返回只读共享 VMO 句柄，PM `util` 环与 `policy` 环同构），用户态 `berry-power` 与 `advisor` 直接 `vmar_map` 零系统调用、零拷贝读取实时指标。

### 四、跨 16 核增量 PI、物理统一 Futex（`0x55/0x56`）与对象内省（`0x07`）

1. **显式等待边替代全表盲扫**：每个阻塞中的 `Task` 维护显式等待目标 `blocked_on: Option<PiEdge>`，覆盖四类原语：
   - **Channel 边**：通道维护当前绑定服务端执行者指针（最近在该端 `recv` 或绑定监听 `Port` 的服务线程/进程池主线程），客户端阻塞在满队列 `WRITABLE` 或 `channel_call` 应答等待时指向该服务端；
   - **Port 边**：阻塞在 `port_wait` 的工作线程登记为该 `Port` 的活跃消费者，挂入该 `Port` 的通道收到高优阻塞请求时沿 `Port` 边传导；
   - **Pager（`dpg`）边**：高优线程发生按需缺页阻塞在 `DpgSlot` / `Pager` 时，边指向该按需 `Vmo` 绑定的用户态 `pager` 服务线程；
   - **Futex 边**：持 PI 模式的用户态互斥锁等待线程指向当前原子字记录的锁属主线程 TID。
2. **跨核增量传播协议（≤8 跳）**：
   - 当线程 `T` 阻塞在边 `T -> Owner` 且 `T` 的有效调度序（`(class_rank, prio_effective, -intent_effective)`）高于 `Owner` 时，沿 `blocked_on` 链向后走查最多 `PI_MAX_HOPS = 8` 跳（遇环或达 8 跳安全终止不 panic）；
   - 若某跳受捐者 `Owner` 正处于任意核 `cpu ∈ 0..online_cpus()` 的 `RQ[cpu]` 上，持该核 `RunqueueLock`（Rank 70）原地抬升其 `pi_inherited` 与借入 `intent_effective`，若 `Owner` 在远端核就绪且新有效序高于该核当前运行线程，立即向 `cpu` 发送 `SGI_RESCHEDULE` 触发远核抢占；
   - 当服务端完成应答/`pager` 完成供给或互斥锁释放时，清退对应捐赠槽并重取剩余等待者最高值立即回落。
3. **用户态快速同步原语（`0x55 berry_futex_wait` / `0x56 berry_futex_wake`）**：
   - 校验用户指针 4 字节对齐且可读；通过 `Vmar` 查询该虚拟页后备——若属跨进程共享 `Vmo` 则生成物理统一键 `FutexKey::Shared { vmo_koid, byte_offset }`（跨进程映射不同虚拟地址亦精确命中同一锁），若属进程私有匿名页则生成快速键 `FutexKey::Private { aspace_koid, va }`；
   - 散列落入 64 桶 `SleepBucketLock`（Rank 60），在桶锁保护下经 `PAN` 安全窗口执行单次原子 32 位比较：若 `*addr != expected` 立即解锁返回 `-EAGAIN`（绝无丢唤醒竞态），相等则挂入桶链阻塞至 `berry_futex_wake(addr, count)` 或 `deadline` 到期，作为 `berry-libc` `pthread` 与 Rust `std::sync` 的零系统调用无争用底座。
4. **统一内核对象内省契约（`0x07 berry_object_get_info`）**：
   - 签名：`berry_object_get_info(handle, topic, buf, cap, *actual)`（门控 `RIGHT_INSPECT`；缓冲不足返 `-ENOMEM` 并回写所需总字节至 `*actual`，全部输出结构 `#[repr(C)]` 尾部垫零防信息泄露），锁定六大权威 `topic` 供 `ps / free / btop / lscap` 零猜测消费：
     - `INFO_HANDLE_BASIC (1)`：返回 `{ koid, related_koid, obj_type (1..=19), rights: u64, badge: u64 }`（40B）；
     - `INFO_PROCESS_STATS (2)`：返回进程状态、退出码、线程数、所属 `job_koid`、累计 CPU 纳秒与常驻物理页 `rss_bytes`；
     - `INFO_PROCESS_HANDLES (3)`：批量导出目标进程句柄表的 `CapInfoRec { slot, gen, obj_type, rights, badge, koid, peer_koid, queue_len }` 数组（`lscap` 单一数据源）；
     - `INFO_JOB_STATS (4)`：导出 `Job` 当前 `ram_bytes_used / max`、`live_procs / max`、子树累计能耗 `energy_uj` 与子 `Job` Koid 列表；
     - `INFO_VMO_INFO (5)`：导出 `size_bytes`、`committed_bytes`、`share_count`、`seals`、`pager_koid` 与创建标志；
     - `INFO_KMEM_STATS (6)`（需 `root_job` 或诊断 `Resource` 的 `RIGHT_INSPECT`）：导出 `MemstatRec { total_pages, free_buddy_pages, pcp_pages, slab_pages, vmo_pages, discardable_pages, wired_pages }`（`free / vmstat` 单一数据源）。

### 五、新增与激活系统调用契约总表（预登记 `tools/abi.toml`）

| 拟定号位 | 符号名 | C-ABI 参数签名（≤6 参） | 所需权利门控 | 核心错误码 |
|---|---|---|---|---|
| `0x07` | `berry_object_get_info` | `(handle, topic, buf, cap, *actual)` | `handle`: `INSPECT`（`KMEM_STATS` 需根 `Job`/诊断资源） | `-EBADF`、`-EACCES`、`-EINVAL`、`-ENOMEM`、`-EFAULT` |
| `0x08` | `berry_handle_mint` | `(src, rights, badge, epoch, *out)` | `src`: `DUPLICATE`（铸非零 `badge` 另需 `SET_POLICY`）；`epoch≠0`: `MANAGE` | `-EBADF`、`-EACCES`、`-EINVAL`、`-ENOMEM` |
| `0x09` | `berry_epoch_create` | `(options, *out)` | 无（受所属 `Job` 对象配额约束） | `-EINVAL`、`-ENOMEM` |
| `0x0A` | `berry_epoch_revoke` | `(epoch)` | `epoch`: `MANAGE` | `-EBADF`、`-EACCES` |
| `0x1C` | `berry_vmo_create_child` | `(parent, options, offset, size, *out)` | `parent`: `DUPLICATE \| READ` | `-EBADF`、`-EACCES`、`-EINVAL`、`-ENOMEM` |
| `0x30` | `berry_job_create` | `(parent_job, options, *out)` | `parent_job`: `MANAGE` | `-EBADF`、`-EACCES`、`-EINVAL`（超深 8 层）、`-ENOMEM` |
| `0x43` | `berry_job_set_quota` | `(job, max_procs, max_ram_bytes)` | `job`: `SET_POLICY \| MANAGE` | `-EBADF`、`-EACCES`、`-EINVAL`（大于父限额） |
| `0x44` | `berry_job_kill` | `(job)` | `job`: `MANAGE` | `-EBADF`、`-EACCES` |
| `0x55` | `berry_futex_wait` | `(addr, expected, deadline)` | 当前用户地址空间 4B 对齐可读页 | `-EAGAIN`、`-ETIMEDOUT`、`-EINVAL`、`-EFAULT` |
| `0x56` | `berry_futex_wake` | `(addr, count)` | 当前用户地址空间 4B 对齐可读页 | `-EINVAL`、`-EFAULT` |
| `0x58` | `berry_port_create` | `(options, *out)` | 无（受所属 `Job` 配额约束） | `-EINVAL`、`-ENOMEM` |
| `0x59` | `berry_object_wait_async` | `(handle, port, key, signals, options)` | `port`: `WRITE`；`handle`: 有效存活句柄 | `-EBADF`、`-EACCES`、`-EDEAD(-52)`、`-EINVAL` |
| `0x5A` | `berry_port_wait` | `(port, deadline, *out_packet)` | `port`: `READ` | `-EBADF`、`-EACCES`、`-EAGAIN`、`-ETIMEDOUT`、`-EFAULT` |
| `0x5B` | `berry_channel_reply` | `(reply_token, bytes_ptr, n_b, hdls_ptr, n_h)` | 一次性事务 `ReplyToken` | `-EBADF`、`-EDEAD(-52)`、`-EINVAL`、`-EFAULT` |
| `0x5C` | `berry_channel_call` | `(ch, args_ptr, deadline, *actual_bytes, *actual_handles)` | `ch`: `SEND \| RECV`（避让已用 `0x57 channel_recv_n`；`args_ptr` 指 `ChannelCallArgs`） | `-EBADF`、`-EACCES`、`-EDEAD(-52)`、`-ETIMEDOUT`、`-EFAULT` |

## 验收标准

1. **宿主机纯核与并发模型检验**：`faces/portface`、`faces/chanface`、`faces/jobface` 宿主机单测 100% 覆盖观察者挂摘、环形队列回滚、`Job` 父链配额并发扣退与 `RevokeEpoch` 状态机；用 `loom` 对 `PinnedObj` 引用计数、`ChannelShared` 双端同时关闭及 `Job` 树并发 `job_kill` 做全交错模型检验零死锁、零悬垂。
2. **快路径与零惊群实测**：系统桌面与控制台全量服务在线时，两独立进程 `channel_send` 的 Fastpath 命中率 **> 95%** 且无关 `Port`/`wait_many` 服务零虚假唤醒；同优先级两用户进程 `channel_call` 1,000 轮往返 100% 触发定向让片直切；`MsgBilling` 在强抢占干扰下跨进程零串账、应答零反扣。
3. **沙箱与撤销防线**：恶意进程尝试 `process_create(0, ...)` 或循环创建关闭子柄突破 `Job.max_procs` / `Job.ram_bytes_max` 100% 被拒；客户端尝试篡改已烙印 `badge` 100% 返 `-EACCES`；`epoch_revoke` 后跨进程孙代派生句柄当次调用 100% 返回 `BERRY_ERR_DEAD (-52)`。

## 适用边界

- `Port` 单端口未消费就绪包队列设硬上限（默认 1,024 包）：边缘模式同 `(obj, key)` 合并已有未读包不重复占槽；用户显式注入包满队列时返 `-EBUSY`，防止无人收割的端口耗尽内核内存。
- `channel_call` 仅面向请求-应答语义明确的同步 RPC；流式单向推送仍走非阻塞 `channel_send` + `Port` 背压，大块载荷仍走共享 `VMO`。
