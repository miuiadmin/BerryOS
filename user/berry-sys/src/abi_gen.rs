//! ABI 常量层——**生成文件，勿手改**。
//! 单源 = `tools/abi.toml`；再生 = `just abi-gen`；
//! 守卫 = `just abi-check`（`tools/gen_abi.py --check`，挂 build/smoke 前置）。
//! 与内核侧 `kernel/src/abi_gen.rs` 同源生成（两端 u32/u64 类型约定见
//! `tools/gen_abi.py` 头注——那是 ABI 决策不是漂移）。

use crate::{Handle, Rights, Status, Time};

/// 无效 handle（表槽 0 恒空；`berry_handle_t` = 0）。
pub const INVALID_HANDLE: Handle = 0;

// ---- rights 位（12 位；权限在 handle 不在对象）----
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
/// duplicate 的「不裁剪」哨兵（内核同值 u64::MAX；实际授权仍受 handle 既有位约束）。
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

// ---- options 位 / clock id ----
/// `channel_create` options bit0（4-3c，五件套之 4）：intent opt-out——置位通道
/// 的 recv 不结算 intent（QNX `_NTO_CHF_FIXED_PRIORITY` 等价，系统服务/看门狗
/// 类 server 逃生门）。其余位保留（非 0 尾 → -EINVAL，options 纪律）。
pub const CHAN_OPT_NO_INTENT: u32 = 1 << 0;
/// `clock_read` 的 clock_id：v1 只有单调钟（壁钟留 v2）。
pub const CLOCK_MONOTONIC: u32 = 0;

// ---- 错误码（G2 冻结面；`berry_status_t` = i32，0 = Ok）----
/// EBADF：坏 handle / 类型不符 / 代号过期。
pub const BERRY_ERR_BAD_HANDLE: Status = -9;
/// EINVAL：参数非法（如 replace 两值相同）。
pub const BERRY_ERR_INVALID_ARG: Status = -22;
/// ENOMEM：无空间（表满 / gen 耗尽 / 超尺寸）。
pub const BERRY_ERR_NO_MEMORY: Status = -12;
/// EACCES：权限位不足（能力拒绝）。
pub const BERRY_ERR_ACCESS_DENIED: Status = -13;
/// ENOENT：区间不存在（unmap/protect 找不到精确匹配）。
pub const BERRY_ERR_NOT_FOUND: Status = -2;
/// EBADSTATE：对象状态不满足（读/映射未提交页——v1 无 demand paging）。
pub const BERRY_ERR_BAD_STATE: Status = -37;
/// ETIMEDOUT：阻塞等待到点未命中（object_wait_one deadline 到期——4-1）。
pub const BERRY_ERR_TIMEDOUT: Status = -110;
/// EBUSY：资源占用（映射未解除的 VMO 不能 close/decommit——悬空 PTE 防线）。
pub const BERRY_ERR_BUSY: Status = -16;
/// EAGAIN：非阻塞操作当下无货可取（channel recv 空——3-4b；posix-ish）。
pub const BERRY_ERR_AGAIN: Status = -11;
/// ENOTSUP：未知/未实现编号或能力（ABI §8.4「永不 panic」统一错误面）。
pub const BERRY_ERR_NOT_SUPPORTED: Status = -95;
/// EOVERFLOW-style：offset/len 越出对象容量或区间管辖界。
pub const BERRY_ERR_OUT_OF_RANGE: Status = -34;
/// EFAULT：用户缓冲不可访问（未映射/只读页要写/越出用户区）——3-2 copy 路径。
pub const BERRY_ERR_FAULT: Status = -14;

// ---- syscall 号（v1 在册；私有——用户程序不写编号，只经本 crate 的
// `berry_*` 封 `svc`，§8.3；未实现号 wrapper 照发、内核回落 -ENOTSUP）----
pub(crate) mod nr {
    pub(crate) const HANDLE_CLOSE: usize = 0x01;
    pub(crate) const HANDLE_DUPLICATE: usize = 0x02;
    pub(crate) const HANDLE_REPLACE: usize = 0x03;
    /// 4-1 阻塞化（poll-first + 醒来重验 + -ETIMEDOUT）；4-2 扩 Event|Channel。
    pub(crate) const OBJECT_WAIT_ONE: usize = 0x04;
    /// 4-2b 落地：任一命中 → *idx + *observed；WAIT_ANY 哨兵睡眠。
    pub(crate) const OBJECT_WAIT_MANY: usize = 0x05;
    /// 3-4c 落地：v1 仅 Event 可 signal（thread/process 信号面随后续步）。
    pub(crate) const OBJECT_SIGNAL: usize = 0x06;
    pub(crate) const VMO_CREATE: usize = 0x11;
    pub(crate) const VMO_COMMIT: usize = 0x12;
    pub(crate) const VMO_DECOMMIT: usize = 0x13;
    pub(crate) const VMO_READ: usize = 0x14;
    pub(crate) const VMO_WRITE: usize = 0x15;
    pub(crate) const VMAR_MAP: usize = 0x16;
    pub(crate) const VMAR_UNMAP: usize = 0x17;
    pub(crate) const VMAR_PROTECT: usize = 0x18;
    pub(crate) const VMAR_ALLOCATE: usize = 0x19;
    /// G-eco（svcmgr 消费 rootfs 需要 cpio 总长；通用元数据查询不止 initrd 受益）。
    pub(crate) const VMO_GET_SIZE: usize = 0x1A;
    /// P6 A-3（用户态 DMA 面：virtqueue 描述符地址是 guest PA，驱动须能查自己 VMO 页的后备；无 IOMMU 板上 PA 披露面 = 已授 MMIO 窗口的设备本可 DMA 全内存，v1 接受并文档化——IOMMU/SMMU 随 `安全与硬件隔离设计.md`）。
    pub(crate) const VMO_GET_PHYS: usize = 0x1B;
    pub(crate) const PROCESS_CREATE: usize = 0x31;
    pub(crate) const PROCESS_START: usize = 0x32;
    /// Plan B v0.5 落地：真退出（不返——`exit_current_with` 经退出钩子 drain+回收）。
    pub(crate) const PROCESS_EXIT: usize = 0x33;
    pub(crate) const THREAD_CREATE: usize = 0x34;
    /// thread_start：v2 多线程面（v1 单线程，start 即随 process_start）。
    pub(crate) const THREAD_START: usize = 0x35;
    /// Plan B v0.5 落地：v1 单线程 ⇒ 等价 process_exit(0)（Zircon last-thread 语义）。
    pub(crate) const THREAD_EXIT: usize = 0x36;
    pub(crate) const THREAD_SLEEP: usize = 0x37;
    /// P5 shell v0 收官（`&&` 的真退出码源）：**ABI 单源管线首次真实扩号**。门控仅类型不查权利位（need=0，同 vmo_get_size「对象几何非内容」惯例）。
    pub(crate) const PROCESS_GET_EXIT_CODE: usize = 0x3A;
    pub(crate) const CHANNEL_CREATE: usize = 0x51;
    pub(crate) const CHANNEL_SEND: usize = 0x52;
    pub(crate) const CHANNEL_RECV: usize = 0x53;
    /// 3-4c 落地：`options` 恒 0（非 0 → -EINVAL），授 SIGNAL|DUPLICATE。
    pub(crate) const EVENT_CREATE: usize = 0x54;
    pub(crate) const CLOCK_READ: usize = 0x61;
}

// ---- 宽度冻结断言（abi.toml [types]；G2 冻结面的机器执法）----
const _: () = assert!(core::mem::size_of::<Handle>() == 4);
const _: () = assert!(core::mem::size_of::<Rights>() == 8);
const _: () = assert!(core::mem::size_of::<Status>() == 4);
const _: () = assert!(core::mem::size_of::<Time>() == 8);
