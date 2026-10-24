//! ABI 常量层——**生成文件，勿手改**。
//! 单源 = `tools/abi.toml`；再生 = `just abi-gen`；
//! 守卫 = `just abi-check`（`tools/gen_abi.py --check`，挂 build/smoke 前置）。
//! 消费：`syscall.rs` / `handle.rs` 经 `pub use` 转发，既有路径
//! （`crate::syscall::NR_*`、`crate::handle::RIGHT_*` 等）零改动。

use crate::handle::Rights;

// ---- syscall 号（ABI §4 编号表；v1 30 号在册、29 已实现）----
// 在册未实现：0x35 thread_start（派发回落 -ENOTSUP，永不 panic）
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
/// P6 A-3（用户态 DMA 面：virtqueue 描述符地址是 guest PA，驱动须能查自己 VMO 页的后备；无 IOMMU 板上 PA 披露面 = 已授 MMIO 窗口的设备本可 DMA 全内存，v1 接受并文档化——IOMMU/SMMU 随 `安全与硬件隔离设计.md`）。
pub const NR_VMO_GET_PHYS: usize = 0x1B;
pub const NR_PROCESS_CREATE: usize = 0x31;
pub const NR_PROCESS_START: usize = 0x32;
/// Plan B v0.5 落地：真退出（不返——`exit_current_with` 经退出钩子 drain+回收）。
pub const NR_PROCESS_EXIT: usize = 0x33;
pub const NR_THREAD_CREATE: usize = 0x34;
/// thread_start：v2 多线程面（v1 单线程，start 即随 process_start）。
pub const NR_THREAD_START: usize = 0x35;
/// Plan B v0.5 落地：v1 单线程 ⇒ 等价 process_exit(0)（Zircon last-thread 语义）。
pub const NR_THREAD_EXIT: usize = 0x36;
pub const NR_THREAD_SLEEP: usize = 0x37;
/// P5 shell v0 收官（`&&` 的真退出码源）：**ABI 单源管线首次真实扩号**。门控仅类型不查权利位（need=0，同 vmo_get_size「对象几何非内容」惯例）。
pub const NR_PROCESS_GET_EXIT_CODE: usize = 0x3A;
pub const NR_CHANNEL_CREATE: usize = 0x51;
pub const NR_CHANNEL_SEND: usize = 0x52;
pub const NR_CHANNEL_RECV: usize = 0x53;
/// 3-4c 落地：`options` 恒 0（非 0 → -EINVAL），授 SIGNAL|DUPLICATE。
pub const NR_EVENT_CREATE: usize = 0x54;
pub const NR_CLOCK_READ: usize = 0x61;

// ---- clock id（ABI §8.3 `berry_clock_id_t`）----
/// `clock_read` 的 clock_id：v1 只有单调钟（`berry_time_t` = 纳秒；壁钟留 v2）。
pub const CLOCK_MONOTONIC: u64 = 0;

// ---- options 位（ABI §8「options 纪律」：未知位一律 -EINVAL）----
/// `channel_create` options bit0（4-3c，五件套之 4）：intent opt-out——置位通道
/// 的 recv 不结算 intent（QNX `_NTO_CHF_FIXED_PRIORITY` 等价，系统服务/看门狗
/// 类 server 逃生门）。其余位保留（非 0 尾 → -EINVAL，options 纪律）。
pub const CHAN_OPT_NO_INTENT: u64 = 1 << 0;

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
/// 用户信号位掩码（Event `object_signal` 只收高 32 位用户位；低 32 位保留给
/// 内核派生信号 READABLE/WRITABLE/TERMINATED——位面冲突由内核执法拒绝）。
pub const SIGNAL_USER_MASK: u64 = 0xFFFF_FFFF_0000_0000;

// ---- 错误码（posix-ish 负数；ABI §8.2，`berry_status_t` = i32）----
/// EBADF：坏 handle / 类型不符 / 代号过期。
pub const ERR_BAD_HANDLE: i32 = -9;
/// EINVAL：参数非法（如 replace 两值相同）。
pub const ERR_INVALID_ARG: i32 = -22;
/// ENOMEM：无空间（表满 / gen 耗尽 / 超尺寸）。
pub const ERR_NO_MEMORY: i32 = -12;
/// EACCES：权限位不足（能力拒绝）。
pub const ERR_ACCESS_DENIED: i32 = -13;
/// ENOENT：区间不存在（unmap/protect 找不到精确匹配）。
pub const ERR_NOT_FOUND: i32 = -2;
/// EBADSTATE：对象状态不满足（读/映射未提交页——v1 无 demand paging）。
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
