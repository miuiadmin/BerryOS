//! ABI 常量层——**生成文件，勿手改**。
//! 单源 = `tools/abi.toml`；再生 = `just abi-gen`；
//! 守卫 = `just abi-check`（`tools/gen_abi.py --check`，挂 build/smoke 前置）。
//! 消费：`syscall.rs` / `handle.rs` 经 `pub use` 转发，既有路径
//! （`crate::syscall::NR_*`、`crate::handle::RIGHT_*` 等）零改动。
//! 预算四元组（生成器执法，ADR #11 帽）：nr_total=37（v1 在册）/
//! nr_defined=41（v1+1.1 常量层）/ nr_registered=40（已挂派发）/
//! nr_ceiling=45（nr_defined ≤ 帽 = `tools/gen_abi.py` load 执法）。

use crate::handle::Rights;

// ---- 平台 API level（abi.toml [platform]；VER-01 提案第 4 条：单调整数
// level 轨道——只增不减、不兼容 ABI 变更 +1、兼容新增不 +1。内核侧
// 在册无消费者（装载期门在 svcmgr），照在册未实现号先例豁免 dead_code）----
/// svcmgr 装载期与 .bap `[compat] min_level` 比较的平台级基线
///（用户侧同名常量同源同值，两端在册防漂移）。
#[allow(dead_code)]
pub const BERRY_PLATFORM_LEVEL: u32 = 1;

// ---- syscall 号（ABI §4 编号表；v1 37 号在册、36 已实现）----
// 在册未实现：0x85 berry_attest（派发回落 -ENOTSUP，永不 panic）
pub const NR_HANDLE_CLOSE: usize = 0x01;
pub const NR_HANDLE_DUPLICATE: usize = 0x02;
pub const NR_HANDLE_REPLACE: usize = 0x03;
/// 4-1 阻塞化（poll-first + 醒来重验 + -ETIMEDOUT）；4-2 扩 Event|Channel。
pub const NR_OBJECT_WAIT_ONE: usize = 0x04;
/// 4-2b 落地：任一命中 → *idx + *observed；WAIT_ANY 哨兵睡眠。
pub const NR_OBJECT_WAIT_MANY: usize = 0x05;
/// 3-4c 落地：v1 仅 Event 可 signal（thread/process 信号面随后续步）。
pub const NR_OBJECT_SIGNAL: usize = 0x06;
pub const NR_VMO_CREATE: usize = 0x11;
pub const NR_VMO_COMMIT: usize = 0x12;
pub const NR_VMO_DECOMMIT: usize = 0x13;
pub const NR_VMO_READ: usize = 0x14;
pub const NR_VMO_WRITE: usize = 0x15;
pub const NR_VMAR_MAP: usize = 0x16;
pub const NR_VMAR_UNMAP: usize = 0x17;
pub const NR_VMAR_PROTECT: usize = 0x18;
pub const NR_VMAR_ALLOCATE: usize = 0x19;
/// G-eco（svcmgr 消费 rootfs 需要 cpio 总长；通用元数据查询不止 initrd 受益）。
pub const NR_VMO_GET_SIZE: usize = 0x1A;
/// P6 A-3（用户态 DMA 面：virtqueue 描述符地址是 guest PA，驱动须能查自己 VMO 页的后备；无 IOMMU 板上 PA 披露面 = 已授 MMIO 窗口的设备本可 DMA 全内存，v1 接受并文档化——IOMMU/SMMU 随 `安全与硬件隔离设计-v1.0.md`）。
pub const NR_VMO_GET_PHYS: usize = 0x1B;
/// G4-2 首形（§5-③ 裁决）＝阻塞收货：无武装缺页请求时 poll-first 后睡 pager
/// 裸键，fault 探针武装 EL0 请求槽 + 唤醒即递送——ADR #9 缺页路径零分配。供给 =
/// pt_lock 窗 commit + 填叶 + tlbi/broadcast（vmar_map 同款）+ 唤 faulter。
/// 源数据 VMO 页须 pre-commit（§2.2 防线 4）。pager 崩溃语义 = faulter 永久阻塞
/// （§5-⑤ 裁决）。
pub const NR_PAGER_SUPPLY_PAGES: usize = 0x21;
/// X-3（exec 装载 demand 化）供给面：0x21 收货前钉死 src_off 单偏移，真 ELF
/// 多段装载需按请求页选源页——源偏 = demand 页偏 + 常量 delta（门 C 裁 A 形
/// 2026-11-07：新号 add-only 破冻，先例 0x86/0x87；0x21 原义零漂移）。收货/
/// 唤醒/供给路径与 0x21 同形（阻塞收货 + poll-first 睡 pager 裸键）；源数据
/// VMO 页须 pre-commit（§2.2 防线 4）。
pub const NR_PAGER_SUPPLY_MAPPED: usize = 0x23;
/// X-6 刀①（批供提速）表批形，冻结裁决 2026-11-08（设计稿
/// `X6批供提速设计-v1.0.md` §7 主裁③「全套照提案冻」）：号 0x24 / N=32
/// 三源镜像（EREQ_SLOTS / PAGER_SLOTS / 表帽）/ count-in-x0 / PagerWindow
/// 布局一次定全即冻。每窗门控/供给/唤醒链与 0x23 同构（阻塞收货 +
/// poll-first 睡 pager 裸键）；整批 ≤32 页一 KL 窗；池空且已供>0 → 即返
/// 计数不阻塞，池空且 0 供 → 泊键阻塞醒后重扫表。boot 探测回退：
/// 0x24(dv:0,n:1) 新核 -EBADF / 旧核 -ENOTSUP。
pub const NR_PAGER_SUPPLY_MAPPED_TABLE: usize = 0x24;
pub const NR_PROCESS_CREATE: usize = 0x31;
pub const NR_PROCESS_START: usize = 0x32;
/// Plan B v0.5 落地：真退出（不返——`exit_current_with` 经退出钩子 drain+回收）。
pub const NR_PROCESS_EXIT: usize = 0x33;
pub const NR_THREAD_CREATE: usize = 0x34;
/// T-1 落地：门控 Thread MANAGE；所属 Process 须已 start 且 Alive（否则 -EBADSTATE）；线程入口不得返回（返回 = fault 杀兜底）——pthread/线程库的内核地基。
pub const NR_THREAD_START: usize = 0x35;
/// Plan B v0.5 落地：v1 单线程 ⇒ 等价 process_exit(0)（Zircon last-thread 语义）。
pub const NR_THREAD_EXIT: usize = 0x36;
pub const NR_THREAD_SLEEP: usize = 0x37;
/// P5 shell v0 收官（`&&` 的真退出码源）：**ABI 单源管线首次真实扩号**。门控仅类型不查权利位（need=0，同 vmo_get_size「对象几何非内容」惯例）。
pub const NR_PROCESS_GET_EXIT_CODE: usize = 0x3A;
/// C-4b（Ctrl-C/kill 面）落地：VM 草案 R5 预批的唯一扩号。bsh Ctrl-C 杀前台子进程、`kill <pid>` 的内核原语；未 start 的进程可终止（收口免任务面）。
pub const NR_PROCESS_TERMINATE: usize = 0x3B;
/// S1 收口：56B attr 解析/校验/存储（Thread 覆盖 + Process 默认）已落；S2/S5 消费随后续刀。
pub const NR_SCHED_SETATTR: usize = 0x3C;
/// S1 收口：effective 组合 + 前向兼容回写（尾部清零）已落。
pub const NR_SCHED_GETATTR: usize = 0x3D;
/// S3-b 收口：进程全冻 + Thread SINGLE_THREAD（FREEZE_READY 自证、末 token thaw 自动清位）均落。
pub const NR_PROCESS_SUSPEND: usize = 0x3E;
/// S1 收口：SVC 面 direct yield_now（含切换锁舞蹈）已落。
pub const NR_THREAD_YIELD: usize = 0x3F;
pub const NR_CHANNEL_CREATE: usize = 0x51;
pub const NR_CHANNEL_SEND: usize = 0x52;
pub const NR_CHANNEL_RECV: usize = 0x53;
/// 3-4c 落地：`options` 恒 0（非 0 → -EINVAL），授 SIGNAL|DUPLICATE。
pub const NR_EVENT_CREATE: usize = 0x54;
pub const NR_CLOCK_READ: usize = 0x61;
/// SEC-01 §4.5 件 11（2026-10）：号位按在册模式冻结预留（度量链零实现——底料 SEC-3；防 ABI 冻结后再挤号）；ktest B 段「在册未实现真号」-ENOTSUP 样本挂 kernel 解冻（ktest.rs 兄弟 WIP 冻结中；沿用既有纪律勿拿未知号 0x99 冒充——二者 -ENOTSUP 成因不同）。
pub const NR_BERRY_ATTEST: usize = 0x85;
/// A7-4 段② 翻真（2026-10-31，门 C 裁定 A add-only）：h=0 当前进程 raise 面 / h≠0 门控 MANAGE；sig ∈ 1..=64 界检；幂等位置位、不唤醒目标（协同式用户态投递）。设计 = `docs/事件循环A7-4信号面实施设计草案-v1.0.md` §3.1。
pub const NR_SIGNAL_SEND: usize = 0x86;
/// A7-4 段② 翻真（2026-10-31，与 0x86 同刀）：swap(0) 语义取走当前进程 pending 集；出参写失败不消费（先写后清）。投递中新到位置回内核 pending（下轮 take）——竞态免设计。
pub const NR_SIGNAL_TAKE: usize = 0x87;

// ---- clock id（ABI §8.3 `berry_clock_id_t`）----
/// `clock_read` 的 clock_id：v1 只有单调钟（`berry_time_t` = 纳秒；壁钟留 v2）。
pub const CLOCK_MONOTONIC: u64 = 0;

// ---- options 位（ABI §8「options 纪律」：未知位一律 -EINVAL）----
/// `channel_create` options bit0（4-3c，五件套之 4）：intent opt-out——置位通道
/// 的 recv 不结算 intent（QNX `_NTO_CHF_FIXED_PRIORITY` 等价，系统服务/看门狗
/// 类 server 逃生门）。其余位保留（非 0 尾 → -EINVAL，options 纪律）。
pub const CHAN_OPT_NO_INTENT: u64 = 1 << 0;
/// `vmo_create` options bit0（G4-2，§5-① 裁决）：按需后备 VMO——创建即
/// demand 对象属性 + 额外授 RIGHT_SET_POLICY（在册未消费位首消费；0x21 pager 面
/// 门权）。映射走 vmar_map 对象级分流 map_demand（首触缺页由 EL0 pager 供给）。
/// 其余位保留（非 0 尾 → -EINVAL，options 纪律）。
pub const VMO_OPT_DEMAND: u64 = 1 << 0;

// ---- rights 位（`能力模型设计.md` §2；位表冻结，未消费位全量在册）----
pub const RIGHT_READ: Rights = 1 << 0;
pub const RIGHT_WRITE: Rights = 1 << 1;
pub const RIGHT_EXECUTE: Rights = 1 << 2;
pub const RIGHT_MAP: Rights = 1 << 3;
pub const RIGHT_DUPLICATE: Rights = 1 << 4;
pub const RIGHT_TRANSFER: Rights = 1 << 5;
pub const RIGHT_INSPECT: Rights = 1 << 6;
/// 3-4c 起消费：`object_signal` 门控（Event 置/清位）。
pub const RIGHT_SIGNAL: Rights = 1 << 7;
/// 3-4b 起消费：`channel_send` 门控。
pub const RIGHT_SEND: Rights = 1 << 8;
/// 3-4b 起消费：`channel_recv` 门控。
pub const RIGHT_RECV: Rights = 1 << 9;
pub const RIGHT_MANAGE: Rights = 1 << 10;
pub const RIGHT_SET_POLICY: Rights = 1 << 11;
/// `handle_duplicate`「取原权利全集」哨兵（= `BERRY_RIGHT_SAME_RIGHTS`，全 1）。
pub const RIGHTS_SAME: Rights = u64::MAX;

// ---- wait 信号位（4-2 位空间：低 32 内核派生 + 高 32 用户）----
/// Channel 端点可读：**本端**队列非空（内核派生位——用户 signal 不出此位）。
pub const SIGNAL_READABLE: u64 = 1 << 0;
/// Channel 端点可写：**对端**队列未满（内核派生位）。
pub const SIGNAL_WRITABLE: u64 = 1 << 1;
/// Process 已退出（Plan B v0.5 内核派生位；Exited 置位时唤醒，永久置位——
/// 同 Zircon TASK_TERMINATED 口径；父进程 wait 到该位后 unmap/close 回收）。
pub const SIGNAL_TERMINATED: u64 = 1 << 2;
/// 冻结进程收讯挂起位（AM-1.5 内核派生位——SuspendToken 的 wait 面唯一可
/// 命中位）：目标进程冻结期间任一 send 成功入队即置位（一次语义防风暴，同窗
/// 多跳不重复通知），全解冻（最后一枚 token close）清零；所有在册 token 同刻
/// 同视（signal_state 直接读目标 Process.wake_pending 派生，零 per-token 存储）。
/// 消费形：object_wait_one(token, SIGNAL_WAKE_PENDING, deadline, &observed)——
/// deadline=0 即 poll 探针；零新 syscall。
pub const SIGNAL_WAKE_PENDING: u64 = 1 << 3;
/// 用户信号位掩码（Event `object_signal` 只收高 32 位用户位；低 32 位保留给
/// 内核派生信号 READABLE/WRITABLE/TERMINATED/WAKE_PENDING——位面冲突由内核
/// 执法拒绝）。
pub const SIGNAL_USER_MASK: u64 = 0xFFFF_FFFF_0000_0000;

// ---- 错误码（posix-ish 负数；ABI §8.2，`berry_status_t` = i32）----
/// EBADF：坏 handle / 类型不符 / 代号过期。
pub const ERR_BAD_HANDLE: i32 = -9;
/// EINVAL：参数非法（如 replace 两值相同）。
pub const ERR_INVALID_ARG: i32 = -22;
/// E2BIG：调度属性长度超界（size > 内核结构且尾部非零；1.1/S1）。
pub const ERR_E2BIG: i32 = -7;
/// ENOMEM：无空间（表满 / gen 耗尽 / 超尺寸）。
pub const ERR_NO_MEMORY: i32 = -12;
/// EACCES：权限位不足（能力拒绝）。
pub const ERR_ACCESS_DENIED: i32 = -13;
/// ENOENT：区间不存在（unmap/protect 找不到精确匹配）。
pub const ERR_NOT_FOUND: i32 = -2;
/// EBADSTATE：对象状态不满足（读/映射未提交页——eager VMO 面；demand 页 G4-1 起缺页供给）。
pub const ERR_BAD_STATE: i32 = -37;
/// ETIMEDOUT：阻塞等待到点未命中（object_wait_one deadline 到期——4-1）。
pub const ERR_TIMEDOUT: i32 = -110;
/// EBUSY：资源占用（映射未解除的 VMO 不能 close/decommit——悬空 PTE 防线）。
pub const ERR_BUSY: i32 = -16;
/// EAGAIN：非阻塞操作当下无货可取（channel recv 空——3-4b；posix-ish）。
pub const ERR_AGAIN: i32 = -11;
/// ENOTSUP：未知/未实现编号或能力（ABI §8.4「永不 panic」统一错误面）。
pub const ERR_NOT_SUPPORTED: i32 = -95;
/// EOVERFLOW-style：offset/len 越出对象容量或区间管辖界。
pub const ERR_OUT_OF_RANGE: i32 = -34;
/// EFAULT：用户缓冲不可访问（未映射/只读页要写/越出用户区）——3-2 copy 路径。
pub const ERR_FAULT: i32 = -14;

// ---- 宽度冻结断言（abi.toml [types]；G2 冻结面的机器执法）----
const _: () = assert!(core::mem::size_of::<crate::handle::Handle>() == 4);
const _: () = assert!(core::mem::size_of::<crate::handle::Rights>() == 8);
