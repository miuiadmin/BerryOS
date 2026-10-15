//! 真 syscall 语义层（Phase 3-4b：channel 三号——非阻塞消息 + handle 随消息
//! 迁移，`系统调用ABI设计.md` §4/§8）。
//!
//! 分层：HAL [`syscall`]（`架构相关/aarch64/`）只做 nr 界检 + 派发 + 统一错误面；
//! 真语义 handler 在本模块，boot 期经 [`install`] 注册进 HAL 表（同 IRQ 槽反转：
//! HAL 不依赖内核 crate）。**3-1 政策**（撤 2b-4「EL0 畸形号杀」）：未知/未实现
//! 编号一律 `-ENOTSUP` 存活、绝不 panic；错误由用户态自理。
//!
//! v1 27 号编号全部在册（下方 NR_* 常量组）；4-1 实现 23 个——handle 三操作 +
//! clock_read（3-1）+ VMO/Vmar 九号（3-2）+ process_create/thread_create/
//! process_start（3-3）+ channel_create/send/recv（3-4b，全部经 [`HandleTable`]
//! 能力门控——**权限在 handle 不在对象**）+ event 三号（3-4c；4-1 wait_one
//! 阻塞化）+ thread_sleep（4-0，首个阻塞 syscall）。其余（wait_many、
//! process/thread 退出）留后续步，未注册槽由 HAL 默认返 `-ENOTSUP`。
//!
//! **copy_from/to_user（3-2）**：EL1 调用方（ktest）→ 内核高半直映区直拷；
//! EL0 → 界检 `[0x1000, 2^39)` + 逐页走查**当前 TTBR0**（= 调用者自身表）验
//! AP 位，校验后直拷。**无 TOCTOU**：SVC 异常入口硬件置 DAIF 屏蔽 IRQ 且
//! handler 不解蔽 → syscall 主体天然不可抢占（单核）。
//!
//! **per-process 表（3-4a 拆表）**：全局单表退役——每个 Process 拥有自己的
//! [`HandleTable`]（`process.rs`），EL0 syscall 经任务 `proc_tag` 派发到所属
//! 进程的表（[`caller_table`]）；boot 期手搭的用户进程各领一张 leak 常驻表。
//! [`BOOTSTRAP_HT`] 降格为**内核侧表**（ktest 的 EL1 测试面 + 未来 init 的
//! 表雏形，能力模型 §5）。**bootstrap 迁移转正**：`process_start` 的
//! `bootstrap` 参数不再恒 0——从调用者表 `take` 出 handle、`insert_ref` 进
//! 子进程表，子入口 x0 = 子表内的新 handle 值（能力随创建流动，Zircon 同形）。
//!
//! **channel（3-4b）**：v1 = 非阻塞有界队列（send 满拒 -EBUSY / recv 空返
//! -EAGAIN——等待排队留 P4 object_wait）。对象本体在 `channel.rs`（端点对
//! 互钉、环泄漏政策）；本模块只做门控 + 拷贝 + 原子序（任一失败 = 调用者表
//! 与队列零变化的复原承诺）。
//!
//! **event（3-4c + 4-1）**：`event.rs` 纯位掩码。wait_one **阻塞化**（4-1）：
//! poll-first（命中即返不睡）→ 未命中且 `deadline≠0` 挂 [`berry_sched::block_current`]
//! 到绝对时刻，醒来**重验条件**（Timeout 时位可能刚被置——醒因不等于结果）；
//! 到点未命中 `-ETIMEDOUT`、poll（`deadline==0`）未命中 -EAGAIN。signal 置位后
//! 经 [`berry_sched::wake_matching`] 唤醒等同一对象、请求位与新置位集相交的
//! 睡眠者。signal 门控 RIGHT_SIGNAL；wait 不需权利位（12 位 rights 表无 WAIT
//! 位，持有 handle 即可查）。

use crate::channel::{Message, CHAN_HND_MAX, CHAN_MSG_MAX, ChannelEnd};
use crate::event::Event;
use crate::handle::{
    HandleTable, KernelObj, ObjRef, ObjType, Rights, ERR_ACCESS_DENIED, ERR_AGAIN, ERR_BAD_HANDLE,
    ERR_BUSY, ERR_BAD_STATE, ERR_FAULT, ERR_INVALID_ARG, ERR_NOT_FOUND, ERR_NOT_SUPPORTED,
    ERR_NO_MEMORY, ERR_OUT_OF_RANGE, ERR_TIMEDOUT, RIGHT_DUPLICATE, RIGHT_MANAGE, RIGHT_MAP,
    RIGHT_READ, RIGHT_RECV, RIGHT_SEND, RIGHT_SIGNAL, RIGHT_WRITE, SIGNAL_READABLE,
    SIGNAL_USER_MASK, SIGNAL_WRITABLE,
};
use crate::vmar::{Perm, Vmar, VmarError};
use crate::vmo::{Vmo, VmoError};
use berry_hal_aarch64::trap::TrapFrame;
use berry_hal_aarch64::KERNEL_VA_BASE;
use berry_mm::va::PAGE_SIZE;
use core::ptr::NonNull;

// ---- v1 27 号编号表（ABI §4「阶段 = v1」行；镜像 berry-sys）----

// handle / 通用对象（6）
pub const NR_HANDLE_CLOSE: usize = 0x01;
pub const NR_HANDLE_DUPLICATE: usize = 0x02;
pub const NR_HANDLE_REPLACE: usize = 0x03;
/// 4-1 阻塞化（poll-first + 醒来重验 + -ETIMEDOUT）；4-2 扩 Event|Channel。
pub const NR_OBJECT_WAIT_ONE: usize = 0x04;
/// 4-2b 落地：任一命中 → *idx + *observed；WAIT_ANY 哨兵睡眠。
pub const NR_OBJECT_WAIT_MANY: usize = 0x05;
/// 3-4c 落地：v1 仅 Event 可 signal（thread/process 信号面随后续步）。
pub const NR_OBJECT_SIGNAL: usize = 0x06;
// VMO / Vmar（9；3-2 全部实现——下方 handlers）
pub const NR_VMO_CREATE: usize = 0x11;
pub const NR_VMO_COMMIT: usize = 0x12;
pub const NR_VMO_DECOMMIT: usize = 0x13;
pub const NR_VMO_READ: usize = 0x14;
pub const NR_VMO_WRITE: usize = 0x15;
pub const NR_VMAR_MAP: usize = 0x16;
pub const NR_VMAR_UNMAP: usize = 0x17;
pub const NR_VMAR_PROTECT: usize = 0x18;
pub const NR_VMAR_ALLOCATE: usize = 0x19;
// 进程 / 线程（7）
pub const NR_PROCESS_CREATE: usize = 0x31;
pub const NR_PROCESS_START: usize = 0x32;
#[allow(dead_code)] // 进程退出路径（杀死/回收）随后续步落地
pub const NR_PROCESS_EXIT: usize = 0x33;
pub const NR_THREAD_CREATE: usize = 0x34;
#[allow(dead_code)] // 3-3
pub const NR_THREAD_START: usize = 0x35;
#[allow(dead_code)] // 3-3
pub const NR_THREAD_EXIT: usize = 0x36;
#[allow(dead_code)] // 3-3
pub const NR_THREAD_SLEEP: usize = 0x37;
// IPC / event（4；3-4b 实现 channel 三号）
pub const NR_CHANNEL_CREATE: usize = 0x51;
pub const NR_CHANNEL_SEND: usize = 0x52;
pub const NR_CHANNEL_RECV: usize = 0x53;
/// `channel_create` options bit0（4-3c，五件套之 4）：intent opt-out——置位通道
/// 的 recv 不结算 intent（QNX `_NTO_CHF_FIXED_PRIORITY` 等价，系统服务/看门狗
/// 类 server 逃生门）。其余位保留（非 0 尾 → -EINVAL，options 纪律）。
pub const CHAN_OPT_NO_INTENT: u64 = 1 << 0;
/// 3-4c 落地：`options` 恒 0（非 0 → -EINVAL），授 SIGNAL|DUPLICATE。
pub const NR_EVENT_CREATE: usize = 0x54;
// 时间（1）
pub const NR_CLOCK_READ: usize = 0x61;

/// `clock_read` 的 clock_id：v1 只有单调钟（`berry_time_t` = 纳秒；壁钟留 v2）。
const CLOCK_MONOTONIC: u64 = 0;

/// 用户 VA 合法区（Vmar root 覆盖 `[0x1000, 2^39)`，0 页 NULL 保卫——
/// `内存管理设计.md` §4；copy_out 的界检同源）。
const USER_VA_END: u64 = 1 << 39;

// ---- 表派发（3-4a per-process 拆分）----

/// 内核侧表：ktest 的 EL1 测试面 + 未来 init 进程表的雏形（能力模型 §5「第一
/// 个能力从哪来」——它将随 boot 拆分移交 init）。lazy init（首次内核侧使用）。
static mut BOOTSTRAP_HT: Option<HandleTable> = None;

/// 取内核侧表（**仅 EL1 内核路径**：ktest/boot 插对象用；懒初始化需要堆）。
pub fn bootstrap_table() -> &'static mut HandleTable {
    // SAFETY: 单核；调用点全在 EL1 同步上下文，ktest/boot 期 IRQ 未开、无并发。
    unsafe {
        let slot = &raw mut BOOTSTRAP_HT;
        if (*slot).is_none() {
            *slot = Some(HandleTable::new());
        }
        (*slot).as_mut().unwrap_unchecked()
    }
}

/// 调用方 handle 表派发（3-4a）：
/// - **EL1 调用方**（ktest/内核自调）→ 内核侧表 [`BOOTSTRAP_HT`]；
/// - **EL0 调用方** → 当前任务 `proc_tag` 指向的**所属进程表**（`spawn_user`
///   时注入；0 = 无表任务（内核线程/早期 boot）→ 一切 handle 无效）。
///
/// SAFETY（裸指针的三个不变量）：① tag 来源只有两处——boot 期 `Box::leak` 的
/// 常驻表（main.rs 手搭用户进程）与 `Process::ht`（Box'd ObjRef 内、地址稳定，
/// 且运行中的任务钉住 Process → 表随任务同寿）；② 单核 syscall 全程 IRQ 屏蔽
/// → 同一表无并发访问；③ 每个 handler 同一时刻至多持一张表的 `&mut`（跨表
/// 操作如 bootstrap 迁移是**两张不同表**的借用，无别名）。
fn caller_table(tf: &TrapFrame) -> Result<&'static mut HandleTable, i64> {
    if caller_is_el1(tf) {
        Ok(bootstrap_table())
    } else {
        let tag = berry_sched::current_tag();
        if tag == 0 {
            Err(ERR_BAD_HANDLE as i64) // 无表任务：没有 handle 可言
        } else {
            // SAFETY: 见函数注释三不变量。
            Ok(unsafe { &mut *(tag as *mut HandleTable) })
        }
    }
}

// ---- copy_from/to_user（3-2 全量；copy_out_u64 为 out 参数种子保留）----

/// 调用方来自 EL1（ktest/内核自调）？SPSR[3:2] = 异常时 EL（EL0=0b00 / EL1=0b01）。
fn caller_is_el1(tf: &TrapFrame) -> bool {
    (tf.spsr >> 2) & 0x3 == 1
}

/// `vmo_create` 尺寸上限（v1）：页记账 ≈ **18B/页**（pages = `Option<usize>`
/// 无 niche 占 16B + mapcnt u16 2B——旧注释 10B/页 ≤40KB 系算术错，3b3 H2 勘
/// 误），16MiB 上限对应 4096 页 ≈ 72KiB 记账。上限只拦巨参滥用；分配本身已
/// try 化（`Vmo::create` 折 -ENOMEM——上限内 OOM 也不再 abort）。真机/堆扩
/// 大后上调（ABI 注记）。
const VMO_MAX_SIZE: usize = 16 * 1024 * 1024;

/// 栈上拷贝分块（防长缓冲吃内核栈）。
const COPY_CHUNK: usize = 256;

/// 缓冲指针可访问性整段校验。EL1 调用方 → 只认内核高半直映区（ktest 静态
/// 缓冲）；EL0 → 用户区 `[0x1000, 2^39)` 且**逐页走查当前 TTBR0**（= 调用者
/// 自身表）验叶 AP：写需 AP∈{00,01}（EL1 可写），读需 AP≠11（11 = EL0 专属，
/// 内核读不到）。任一页缺叶/权限不符 → false（调用方折 -EFAULT）。
fn access_ok(caller_el1: bool, ptr: u64, len: u64, write: bool) -> bool {
    if len == 0 {
        return true;
    }
    let Some(end) = ptr.checked_add(len) else { return false };
    if caller_el1 {
        ptr >= KERNEL_VA_BASE as u64
    } else {
        if ptr < 0x1000 || end > USER_VA_END {
            return false;
        }
        let l1 = berry_hal_aarch64::current_ttbr0_pa();
        let first = (ptr as usize) & !(PAGE_SIZE - 1);
        let last = (end as usize - 1) & !(PAGE_SIZE - 1);
        let mut va = first;
        while va <= last {
            match crate::vmar::walk_leaf(l1, va) {
                Some(desc) => {
                    let ap = (desc >> 6) & 0b11;
                    if write && ap >= 0b10 {
                        return false; // RO(10) / EL0 专属(11)：EL1 不可写
                    }
                    if !write && ap == 0b11 {
                        return false; // EL0 专属页内核读不到
                    }
                }
                None => return false, // 未映射页
            }
            va += PAGE_SIZE;
        }
        true
    }
}

/// 从调用方缓冲拷进内核切片（`vmo_write` 源 / in 参数读）。
fn copy_from_user(caller_el1: bool, src: u64, dst: &mut [u8]) -> Result<(), i32> {
    if !dst.is_empty() {
        if !access_ok(caller_el1, src, dst.len() as u64, false) {
            return Err(ERR_FAULT);
        }
        // SAFETY: 整段界检过（EL1 = 高半直映；EL0 = 调用者表逐页 AP 验过）；
        // syscall 全程 IRQ 屏蔽 → 校验与拷贝间无任务切换（单核无 TOCTOU）。
        unsafe { core::ptr::copy_nonoverlapping(src as *const u8, dst.as_mut_ptr(), dst.len()) };
    }
    Ok(())
}

/// 从内核切片拷出到调用方缓冲（`vmo_read` 目的 / out 参数写）。
fn copy_to_user(caller_el1: bool, dst: u64, src: &[u8]) -> Result<(), i32> {
    if !src.is_empty() {
        if !access_ok(caller_el1, dst, src.len() as u64, true) {
            return Err(ERR_FAULT);
        }
        // SAFETY: 同 copy_from_user。
        unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst as *mut u8, src.len()) };
    }
    Ok(())
}

/// 读调用方 in-out u64 槽（vmar_map 的 *vaddr 等）。
fn copy_in_u64(caller_el1: bool, ptr: u64) -> Result<u64, i32> {
    if !access_ok(caller_el1, ptr, 8, false) {
        return Err(ERR_FAULT);
    }
    // SAFETY: 界检过；8 字节对齐风险由页粒度走查容许（v1 非对齐只影响性能）。
    Ok(unsafe { core::ptr::read_volatile(ptr as *const u64) })
}

/// 写一个 u64 到调用方提供的 out 指针（ABI §8.3 各 `*out` 参数）。
///
/// 界检：用户区 `[0x1000, 2^39)`（经当前 TTBR0 = 调用者自身表——EL0 syscall
/// 时不切 AS，天然翻译调用者页）；内核高半区仅 EL1 调用方可用（**EL0 传内核
/// 指针一律拒**——防伪造内核写）。3-2 起走 [`copy_to_user`] 全量校验（AP 位
/// 逐页验，未映射页 -EFAULT 不再内核 fault）。
fn copy_out_u64(caller_el1: bool, ptr: u64, val: u64) -> Result<(), i32> {
    copy_to_user(caller_el1, ptr, &val.to_le_bytes())
}

// ---- handlers（x0=号、x1..x6=参、x0=返回）----

/// `handle_close(h)`：关 handle（引用 -1，归零释放；代号 +1 旧值作废）。
extern "C" fn sys_handle_close(tf: &mut TrapFrame) -> i64 {
    let h = tf.regs[1] as u32;
    match caller_table(tf) {
        Ok(t) => match t.close(h) {
            Ok(()) => {
                // 4-3b：持有面变化——睡眠者可能刚关掉受捐边的 peer handle，
                // 全图重推让边消失（stale boost 上界 = 到下一个 block/wake
                // 事件；close 即时收口）。屏蔽窗内零分配。
                crate::pi::recompute();
                0
            }
            Err(e) => e as i64,
        },
        Err(e) => e,
    }
}

/// `handle_duplicate(h, rights, *out)`：复制（rights ⊆ 原、非空；SAME=全集）。
extern "C" fn sys_handle_duplicate(tf: &mut TrapFrame) -> i64 {
    let (h, rights, out) = (tf.regs[1] as u32, tf.regs[2] as Rights, tf.regs[3]);
    // 先验 out 指针（界检不过不浪费槽位/引用）。
    if let Err(e) = copy_out_u64(caller_is_el1(tf), out, 0) {
        return e as i64;
    }
    match caller_table(tf) {
        Ok(t) => match t.duplicate(h, rights) {
            Ok(nh) => match copy_out_u64(caller_is_el1(tf), out, nh as u64) {
                Ok(()) => 0,
                Err(e) => e as i64,
            },
            Err(e) => e as i64,
        },
        Err(e) => e,
    }
}

/// `handle_replace(h, replacement, *out)`：原子换槽（两旧值均失效）。
extern "C" fn sys_handle_replace(tf: &mut TrapFrame) -> i64 {
    let (h, repl, out) = (tf.regs[1] as u32, tf.regs[2] as u32, tf.regs[3]);
    if let Err(e) = copy_out_u64(caller_is_el1(tf), out, 0) {
        return e as i64;
    }
    match caller_table(tf) {
        Ok(t) => match t.replace(h, repl) {
            Ok(nh) => match copy_out_u64(caller_is_el1(tf), out, nh as u64) {
                Ok(()) => 0,
                Err(e) => e as i64,
            },
            Err(e) => e as i64,
        },
        Err(e) => e,
    }
}

/// `clock_read(clock_id, *out)`：单调钟纳秒（CNTVCT/CNTFRQ 换算；先商后余防
/// 中间溢出——ticks×1e9 在 ~5 分钟处溢 u64，拆两项后 ~584 年）。
extern "C" fn sys_clock_read(tf: &mut TrapFrame) -> i64 {
    if tf.regs[1] != CLOCK_MONOTONIC {
        return ERR_NOT_SUPPORTED as i64; // 壁钟等 id 留 v2
    }
    let out = tf.regs[2];
    let ticks = berry_hal_aarch64::timer::cntvct();
    let freq = berry_hal_aarch64::timer::cntfrq();
    if freq == 0 {
        return ERR_NOT_SUPPORTED as i64; // 频率不可读（真机异常）
    }
    let ns = (ticks / freq) * 1_000_000_000 + ((ticks % freq) * 1_000_000_000) / freq;
    match copy_out_u64(caller_is_el1(tf), out, ns) {
        Ok(()) => 0,
        Err(e) => e as i64,
    }
}

/// MONOTONIC 纳秒 → CNTVCT 周期（先商后余防中间溢出，与 [`sys_clock_read`]
/// 对称——`deadline_ns × 62.5MHz` 直乘在 ~29 年处溢 u64，拆两项后安全）。
/// `freq==0` → `None`（真机异常，调用方折 -ENOTSUP）。
fn ns_to_vct(ns: u64) -> Option<u64> {
    // SAFETY: 只读 CNTFRQ（timer::init 后恒有效）。
    let freq = berry_hal_aarch64::timer::cntfrq();
    if freq == 0 {
        return None;
    }
    Some((ns / 1_000_000_000) * freq + ((ns % 1_000_000_000) * freq) / 1_000_000_000)
}

/// CNTVCT → MONOTONIC 纳秒（`sys_clock_read` 同式抽出；4-3c intent 两道时间帽
/// 的计时源）。`freq==0` → 0（真机异常语境——帽永不触发，方向安全）。
fn vct_ns() -> u64 {
    // SAFETY: 只读 CNTVCT/CNTFRQ（timer::init 后恒有效）。
    let freq = berry_hal_aarch64::timer::cntfrq();
    if freq == 0 {
        return 0;
    }
    let ticks = berry_hal_aarch64::timer::cntvct();
    (ticks / freq) * 1_000_000_000 + ((ticks % freq) * 1_000_000_000) / freq
}

/// `thread_sleep(deadline)`（0x37，4-0 首个阻塞 syscall）：睡到绝对时刻
/// `deadline` 纳秒（MONOTONIC——`clock_read` 同坐标系，无「无限等」哨兵，
/// 客户端传 `clock_read()+δ`；`IPC设计.md` Phase 4 前置对齐 2）。
/// 时刻已过 = 立即返回（poll 语义）。睡眠经 `berry_sched::block_current`（纯
/// sleep：obj=0 只能超时自醒）；SVC 语境天然屏蔽窗，续行后经 eret 自然解蔽——
/// syscall 主体不可抢占（免锁根基）不被破坏。
extern "C" fn sys_thread_sleep(tf: &mut TrapFrame) -> i64 {
    let deadline_ns = tf.regs[1] as u64;
    let Some(target_vct) = ns_to_vct(deadline_ns) else {
        return ERR_NOT_SUPPORTED as i64; // 频率不可读（真机异常，同 clock_read）
    };
    // SAFETY: 只读 CNTVCT。
    if target_vct > berry_hal_aarch64::timer::cntvct() {
        // 未到点才睡：已过点立即返回（屏蔽窗契约由 SVC 入口满足）。
        berry_sched::block_current(0, 0, target_vct);
    }
    0
}

// ---- VMO / Vmar（3-2；rights 矩阵见各 handler 注释）----

/// VmoError → ABI 错误码（ABI §8.2）。
fn voe(e: VmoError) -> i32 {
    match e {
        VmoError::OutOfRange => ERR_OUT_OF_RANGE,
        VmoError::Unaligned => ERR_INVALID_ARG,
        VmoError::NotCommitted => ERR_BAD_STATE,
        VmoError::NoFrames => ERR_NO_MEMORY,
        VmoError::Mapped => ERR_BUSY,
        VmoError::NoMem => ERR_NO_MEMORY, // 页记账 try 化（3b3 H2）
    }
}

/// VmarError → ABI 错误码（ABI §8.2）。
fn vme(e: VmarError) -> i32 {
    match e {
        VmarError::Unaligned => ERR_INVALID_ARG,
        VmarError::OutOfBounds => ERR_OUT_OF_RANGE,
        VmarError::Overlap => ERR_INVALID_ARG, // 请求区间撞现有映射/预留
        VmarError::NotMapped => ERR_NOT_FOUND,
        VmarError::VmoNotCommitted => ERR_BAD_STATE,
        VmarError::NoFrames => ERR_NO_MEMORY,
        VmarError::NoMem => ERR_NO_MEMORY, // regions try 化（3b3 H2）
    }
}

/// `vmo_create(size, options, *out)`：建匿名 VMO。新对象授 READ|WRITE|MAP|
/// DUPLICATE（全 Rights 面减 TRANSFER——transfer 随 3-4 channel 落地）；v1
/// options 恒 0。`size` 上限 [`VMO_MAX_SIZE`]（防 EL0 巨参推爆堆——no-panic）。
extern "C" fn sys_vmo_create(tf: &mut TrapFrame) -> i64 {
    let (size, options, out) = (tf.regs[1] as usize, tf.regs[2] as u32, tf.regs[3]);
    if options != 0 {
        return ERR_INVALID_ARG as i64;
    }
    if size == 0 {
        return ERR_INVALID_ARG as i64;
    }
    let el1 = caller_is_el1(tf);
    if size > VMO_MAX_SIZE {
        return ERR_NO_MEMORY as i64;
    }
    if let Err(e) = copy_out_u64(el1, out, 0) {
        return e as i64;
    }
    // create try 化（3b3 H2）：NoMem → -ENOMEM（原 create 内嵌 vec! 双分配，
    // OOM abort 曾是 EL0 可触发 panic）。
    let vmo = match Vmo::create(size) {
        Ok(v) => v,
        Err(e) => return voe(e) as i64,
    };
    match caller_table(tf) {
        Ok(t) => match t.insert(
            KernelObj::Vmo(vmo),
            RIGHT_READ | RIGHT_WRITE | RIGHT_MAP | RIGHT_DUPLICATE,
        ) {
            Ok(h) => match copy_out_u64(el1, out, h as u64) {
                Ok(()) => 0,
                Err(e) => e as i64,
            },
            Err(e) => e as i64,
        },
        Err(e) => e,
    }
}

/// `vmo_commit(vmo, offset, len)`：立即出页清零（v1 无 demand paging）。
/// 门控：handle 类型 Vmo + **WRITE**（提交即写入清零内容）。
extern "C" fn sys_vmo_commit(tf: &mut TrapFrame) -> i64 {
    let (h, off, len) = (tf.regs[1] as u32, tf.regs[2] as usize, tf.regs[3] as usize);
    match caller_table(tf) {
        Ok(t) => match t.get_raw(h, ObjType::Vmo, RIGHT_WRITE) {
            Ok((obj, _)) => {
                let Some(vmo) = obj_vmo_mut(&obj) else { return ERR_BAD_HANDLE as i64 };
                match vmo.commit(crate::mm::frames(), off, len) {
                    Ok(()) => 0,
                    Err(e) => voe(e) as i64,
                }
            }
            Err(e) => e as i64,
        },
        Err(e) => e,
    }
}

/// `vmo_decommit(vmo, offset, len)`：归还已提交且**未被映射**的页（映射中 →
/// -EBUSY，先 unmap——悬空 PTE 防线）。门控同 commit（WRITE）。
extern "C" fn sys_vmo_decommit(tf: &mut TrapFrame) -> i64 {
    let (h, off, len) = (tf.regs[1] as u32, tf.regs[2] as usize, tf.regs[3] as usize);
    match caller_table(tf) {
        Ok(t) => match t.get_raw(h, ObjType::Vmo, RIGHT_WRITE) {
            Ok((obj, _)) => {
                let Some(vmo) = obj_vmo_mut(&obj) else { return ERR_BAD_HANDLE as i64 };
                match vmo.decommit(crate::mm::frames(), off, len) {
                    Ok(()) => 0,
                    Err(e) => voe(e) as i64,
                }
            }
            Err(e) => e as i64,
        },
        Err(e) => e,
    }
}

/// `vmo_read(vmo, buf, offset, len)`：内核中介读（非映射访问）。门控：READ。
/// 整段缓冲校验前置（fail-fast，纯读无半截副作用）；256B 分块防吃内核栈。
extern "C" fn sys_vmo_read(tf: &mut TrapFrame) -> i64 {
    let (h, buf, off, len) = (tf.regs[1] as u32, tf.regs[2], tf.regs[3] as usize, tf.regs[4] as usize);
    let el1 = caller_is_el1(tf);
    if !access_ok(el1, buf, len as u64, true) {
        return ERR_FAULT as i64;
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let vmo = match t.get(h, ObjType::Vmo, RIGHT_READ) {
        Ok((obj, _)) => match obj.as_vmo() {
            Some(v) => v,
            None => return ERR_BAD_HANDLE as i64, // 类型门控后不可达；防御面
        },
        Err(e) => return e as i64,
    };
    let mut chunk = [0u8; COPY_CHUNK];
    let mut pos = 0;
    while pos < len {
        let n = core::cmp::min(COPY_CHUNK, len - pos);
        if let Err(e) = vmo.read(off + pos, &mut chunk[..n]) {
            return voe(e) as i64;
        }
        if let Err(e) = copy_to_user(el1, buf + pos as u64, &chunk[..n]) {
            return e as i64;
        }
        pos += n;
    }
    0
}

/// `vmo_write(vmo, buf, offset, len)`：内核中介写。门控：WRITE。分块同 read。
extern "C" fn sys_vmo_write(tf: &mut TrapFrame) -> i64 {
    let (h, buf, off, len) = (tf.regs[1] as u32, tf.regs[2], tf.regs[3] as usize, tf.regs[4] as usize);
    let el1 = caller_is_el1(tf);
    if !access_ok(el1, buf, len as u64, false) {
        return ERR_FAULT as i64;
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let vmo = match t.get_raw(h, ObjType::Vmo, RIGHT_WRITE) {
        Ok((obj, _)) => {
            let Some(v) = obj_vmo_mut(&obj) else { return ERR_BAD_HANDLE as i64 };
            v
        }
        Err(e) => return e as i64,
    };
    let mut chunk = [0u8; COPY_CHUNK];
    let mut pos = 0;
    while pos < len {
        let n = core::cmp::min(COPY_CHUNK, len - pos);
        if let Err(e) = copy_from_user(el1, buf + pos as u64, &mut chunk[..n]) {
            return e as i64;
        }
        if let Err(e) = vmo.write(off + pos, &chunk[..n]) {
            return voe(e) as i64;
        }
        pos += n;
    }
    0
}

/// 门控后的 Vmo `&mut` 解包（get_raw 类型门控后防御性取本体，不符折 None →
/// 调用方返 -EBADF——syscall 边界无 panic 纪律，不可达分支也不 panic）。
fn obj_vmo_mut(obj: &core::ptr::NonNull<crate::handle::ObjRef>) -> Option<&'static mut Vmo> {
    // SAFETY: get_raw 已验指针在表、gen 匹配、类型为 Vmo；&mut 期间无并发
    // 访问（单核 + syscall 全程 IRQ 屏蔽）。
    unsafe { &mut (*obj.as_ptr()).obj }.as_vmo_mut()
}

/// `vmar_map`：内核 6 参形态 `(vmar, vmo, vmo_offset, *vaddr, len, rights)`
/// （x1..x6）——C 原型 7 参（ABI §8.3），options 由 berry-sys stub 折叠（v1 恒 0，
/// stub 校验后不放行非 0）。`*vaddr` 为 in-out：进 = 请求 VA（v1 须显式、页
/// 对齐），出 = 实际映射基 VA。
///
/// rights 面（v1 只支持恰 `READ|WRITE`=3 = `Perm::UserRw` 映射；`UserRo` 留
/// v2 AP=11）：外位 → -EINVAL；缺 WRITE → -ENOTSUP。对象门控：**vmar 需
/// MAP、vmo 需 MAP|WRITE**（映射进去的页可写——WRITE 不外借给映射权）。
extern "C" fn sys_vmar_map(tf: &mut TrapFrame) -> i64 {
    let (vh, oh, voff, vaddrp, len, rights) = (
        tf.regs[1] as u32,
        tf.regs[2] as u32,
        tf.regs[3] as usize,
        tf.regs[4],
        tf.regs[5] as usize,
        tf.regs[6] as Rights,
    );
    if rights & !(RIGHT_READ | RIGHT_WRITE) != 0 {
        return ERR_INVALID_ARG as i64;
    }
    if rights != RIGHT_READ | RIGHT_WRITE {
        return ERR_NOT_SUPPORTED as i64;
    }
    let el1 = caller_is_el1(tf);
    let req = match copy_in_u64(el1, vaddrp) {
        Ok(v) => v,
        Err(e) => return e as i64,
    };
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    // 双对象同借：两 get_raw 类型门控互异 → 必不同对象 → &mut 无别名（论证
    // 见 HandleTable::get_raw 文档）。
    let vr = match t.get_raw(vh, ObjType::Vmar, RIGHT_MAP) {
        Ok((o, _)) => o,
        Err(e) => return e as i64,
    };
    // VMO 门控逐位要求（3b3 M6）：映射产出可读 UserRw 叶 → READ 必须在句柄上，
    // 否则 duplicate 合法收紧掉 READ 后仍能经映射读（READ 权逃逸）。v1 恒
    // READ|WRITE 映射，故等价 MAP|READ|WRITE；UserRo 映射留 v2 再按权限拆。
    let vo = match t.get_raw(oh, ObjType::Vmo, RIGHT_MAP | RIGHT_READ | RIGHT_WRITE) {
        Ok((o, _)) => o,
        Err(e) => return e as i64,
    };
    // SAFETY: 指针经 get_raw 门控；类型互异保证对象互异；单核 IRQ 屏蔽。
    let pair = unsafe {
        ((*vr.as_ptr()).obj.as_vmar_mut(), (*vo.as_ptr()).obj.as_vmo_mut())
    };
    let (Some(vmar), Some(vmo)) = pair else {
        return ERR_BAD_HANDLE as i64; // 类型门控后不可达；防御面
    };
    match vmar.map(crate::mm::frames(), vmo, req as usize, voff, len, Perm::UserRw) {
        Ok(va) => match copy_out_u64(el1, vaddrp, va as u64) {
            Ok(()) => 0,
            Err(e) => e as i64,
        },
        Err(e) => vme(e) as i64,
    }
}

/// `vmar_unmap(vmar, vaddr, len)`：解除映射（精确匹配）。门控：vmar **MAP**。
extern "C" fn sys_vmar_unmap(tf: &mut TrapFrame) -> i64 {
    let (vh, vaddr, len) = (tf.regs[1] as u32, tf.regs[2] as usize, tf.regs[3] as usize);
    match caller_table(tf) {
        Ok(t) => match t.get_raw(vh, ObjType::Vmar, RIGHT_MAP) {
            Ok((obj, _)) => {
                // SAFETY: get_raw 已门控；单核 IRQ 屏蔽。
                let Some(vmar) = (unsafe { &mut (*obj.as_ptr()).obj }).as_vmar_mut() else {
                    return ERR_BAD_HANDLE as i64;
                };
                match vmar.unmap(vaddr, len) {
                    Ok(()) => 0,
                    Err(e) => vme(e) as i64,
                }
            }
            Err(e) => e as i64,
        },
        Err(e) => e,
    }
}

/// `vmar_protect(vmar, rights, vaddr, len)`：改映射权限。rights 面同 map
/// （v1 只支持恰 READ|WRITE）。门控：vmar **MANAGE**（改管辖界内页权）。
extern "C" fn sys_vmar_protect(tf: &mut TrapFrame) -> i64 {
    let (vh, rights, vaddr, len) =
        (tf.regs[1] as u32, tf.regs[2] as Rights, tf.regs[3] as usize, tf.regs[4] as usize);
    if rights & !(RIGHT_READ | RIGHT_WRITE) != 0 {
        return ERR_INVALID_ARG as i64;
    }
    if rights != RIGHT_READ | RIGHT_WRITE {
        return ERR_NOT_SUPPORTED as i64;
    }
    match caller_table(tf) {
        Ok(t) => match t.get_raw(vh, ObjType::Vmar, RIGHT_MANAGE) {
            Ok((obj, _)) => {
                // SAFETY: 同上。
                let Some(vmar) = (unsafe { &mut (*obj.as_ptr()).obj }).as_vmar_mut() else {
                    return ERR_BAD_HANDLE as i64;
                };
                match vmar.protect(vaddr, len, Perm::UserRw) {
                    Ok(()) => 0,
                    Err(e) => vme(e) as i64,
                }
            }
            Err(e) => e as i64,
        },
        Err(e) => e,
    }
}

/// `vmar_allocate(vmar, options, *vaddr, len, *child)`：切子 Vmar（预留区间）。
/// `*vaddr` in-out（进 = 请求基 VA，出 = 实际基 = 请求——v1 绝对 VA 语义）。
/// 门控：vmar **MANAGE**；子 Vmar 挂表授**父 handle rights 全集**（派生只紧
/// 不松：= 父，调用方再按需 duplicate 收紧）。
extern "C" fn sys_vmar_allocate(tf: &mut TrapFrame) -> i64 {
    let (vh, options, vaddrp, len, child_out) =
        (tf.regs[1] as u32, tf.regs[2] as u32, tf.regs[3], tf.regs[4] as usize, tf.regs[5]);
    if options != 0 {
        return ERR_INVALID_ARG as i64;
    }
    let el1 = caller_is_el1(tf);
    if let Err(e) = copy_out_u64(el1, child_out, 0) {
        return e as i64;
    }
    let req = match copy_in_u64(el1, vaddrp) {
        Ok(v) => v,
        Err(e) => return e as i64,
    };
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let (obj, parent_rights) = match t.get_raw(vh, ObjType::Vmar, RIGHT_MANAGE) {
        Ok(x) => x,
        Err(e) => return e as i64,
    };
    // SAFETY: get_raw 已门控；借用在 insert 前结束（下方块作用域）。
    let child = {
        // SAFETY: 同上。
        let Some(vmar) = (unsafe { &mut (*obj.as_ptr()).obj }).as_vmar_mut() else {
            return ERR_BAD_HANDLE as i64;
        };
        match vmar.allocate(req as usize, len) {
            Ok(c) => c,
            Err(e) => return vme(e) as i64,
        }
    };
    match t.insert(KernelObj::Vmar(child), parent_rights) {
        Ok(ch) => {
            // vaddr/child 双回写（两指针上面各验过一遍，此处失败面为不可达防御）。
            let r1 = copy_out_u64(el1, vaddrp, req);
            let r2 = copy_out_u64(el1, child_out, ch as u64);
            match (r1, r2) {
                (Ok(()), Ok(())) => 0,
                (Err(e), _) | (_, Err(e)) => e as i64,
            }
        }
        Err(e) => e as i64,
    }
}

// ---- process / thread（3-3；对象 = `process.rs`，钉住语义见其模块注释）----

/// `process_create(job, options, *process, *vmar)`：建空进程。**双出参**
/// （Zircon `zx_process_create` 同形——ABI §8.3 v1 修正）：父进程靠返回的
/// root Vmar handle 才能向子地址空间映射代码/栈，只给 process handle 不够用。
/// 出权：process = MANAGE|DUPLICATE，root Vmar = MAP|MANAGE|DUPLICATE（与
/// boot root 同面）。v1 参数面：`job` 恒 0（Job 对象 v2——**创建权未设防的
/// 已知豁口**，落地记录见能力模型）、`options` 恒 0。
extern "C" fn sys_process_create(tf: &mut TrapFrame) -> i64 {
    let (job, options, p_out, v_out) = (tf.regs[1], tf.regs[2] as u32, tf.regs[3], tf.regs[4]);
    if job != 0 || options != 0 {
        return ERR_INVALID_ARG as i64;
    }
    let el1 = caller_is_el1(tf);
    if let Err(e) = copy_out_u64(el1, p_out, 0) {
        return e as i64;
    }
    if let Err(e) = copy_out_u64(el1, v_out, 0) {
        return e as i64;
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    // 帧耗尽折 -ENOMEM（3b3 H1：new_root 曾内嵌 expect——EL0 一次 process_create
    // 即可让内核 panic 停机）。
    let vmar = match Vmar::new_root(crate::mm::frames(), crate::aspace::alloc_asid()) {
        Ok(v) => v,
        Err(e) => return vme(e) as i64,
    };
    let (ttbr0, asid) = (vmar.ttbr0_pa(), vmar.asid());
    let hv = match t.insert(
        KernelObj::Vmar(vmar),
        RIGHT_MAP | RIGHT_MANAGE | RIGHT_DUPLICATE,
    ) {
        Ok(h) => h,
        Err(e) => return e as i64, // 表满/gen 尽：-ENOMEM，vmar 正常 drop（先占槽后装箱，无泄漏）
    };
    let Ok((vobj, _)) = t.get_raw(hv, ObjType::Vmar, 0) else {
        // 刚插入即取失败 = 不可达防御面；关掉防泄漏。
        let _ = t.close(hv);
        return ERR_BAD_HANDLE as i64;
    };
    // SAFETY: vobj 在表（刚插入且类型 Vmar）；Process 构造即钉住 vmar。
    let proc = unsafe { crate::process::Process::new(vobj, ttbr0, asid) };
    match t.insert(KernelObj::Process(proc), RIGHT_MANAGE | RIGHT_DUPLICATE) {
        Ok(hp) => {
            let r1 = copy_out_u64(el1, p_out, hp as u64);
            let r2 = copy_out_u64(el1, v_out, hv as u64);
            match (r1, r2) {
                (Ok(()), Ok(())) => 0,
                (Err(e), _) | (_, Err(e)) => e as i64, // 出参失效：两 handle 已建，调用方重试态自理
            }
        }
        Err(e) => e as i64, // Process 入表失败：对象正常 drop（先占槽后装箱，无泄漏）
    }
}

/// `thread_create(process, options, *out)`：在进程下建**未启动**线程描述
/// （入口/栈留给 `process_start`——首线程启动路径，Zircon 同形）。门控：
/// process **MANAGE**。Thread 构造即钉住 Process（父 handle 关光仍可 start）。
extern "C" fn sys_thread_create(tf: &mut TrapFrame) -> i64 {
    let (ph, options, out) = (tf.regs[1] as u32, tf.regs[2] as u32, tf.regs[3]);
    if options != 0 {
        return ERR_INVALID_ARG as i64;
    }
    let el1 = caller_is_el1(tf);
    if let Err(e) = copy_out_u64(el1, out, 0) {
        return e as i64;
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let Ok((pobj, _)) = t.get_raw(ph, ObjType::Process, RIGHT_MANAGE) else {
        return ERR_BAD_HANDLE as i64;
    };
    // SAFETY: get_raw 已验在表且类型 Process；Thread 构造即钉住它。
    let thrd = unsafe { crate::process::Thread::new(pobj) };
    match t.insert(KernelObj::Thread(thrd), RIGHT_MANAGE | RIGHT_DUPLICATE) {
        Ok(h) => match copy_out_u64(el1, out, h as u64) {
            Ok(()) => 0,
            Err(e) => e as i64,
        },
        Err(e) => e as i64, // Thread 入表失败：对象 drop 时 release 钉住计数，无泄漏
    }
}

/// `process_start(process, thread, entry, sp, bootstrap)`：启动首线程
/// （把 thread 的执行流放进 process 的地址空间）。门控：process 与 thread
/// 各 **MANAGE**。
///
/// **bootstrap 迁移（3-4a 转正，v1 不再恒 0）**：非 0 时从**调用者表** `take`
/// 出该 handle、`insert_ref` 进**子进程表**——子入口 x0 = 子表内的新 handle
/// 值（能力随创建流动，Zircon `zx_process_start` 同形；3-3「恒 0」过渡约定
/// 退役）。迁移原子性：take 失败（-EBADF）时子表未动；insert 失败（表满，
/// v1 不可达）回滚重插调用者表（handle **值**可能变化——代际推进，语义仍活）。
///
/// 校验序：`entry`==0 / `sp` 非 16 对齐 → -EINVAL；bootstrap 非 0 但无效 →
/// -EBADF；thread 非本 process 所属 → -EACCES；已启动 → -EBADSTATE。全过后
/// `started=true`、**替运行中的任务再钉一次 Process**（v1 任务不退出，
/// 永不释放——防 handle 全关后页表根被销毁），`spawn_user` 入队（tag =
/// 子进程表指针——子的 EL0 syscall 从此派发到自己的表）。
extern "C" fn sys_process_start(tf: &mut TrapFrame) -> i64 {
    let (ph, th, entry, sp, bootstrap) = (
        tf.regs[1] as u32,
        tf.regs[2] as u32,
        tf.regs[3],
        tf.regs[4],
        tf.regs[5] as u32,
    );
    if entry == 0 || sp == 0 || sp % 16 != 0 {
        return ERR_INVALID_ARG as i64;
    }
    let t = match caller_table(tf) {
        Ok(t) => t,
        Err(e) => return e,
    };
    // 双对象同借：类型互异 → 两指针必不同对象 → 无别名（论证同 vmar_map）。
    let vr = match t.get_raw(ph, ObjType::Process, RIGHT_MANAGE) {
        Ok((o, _)) => o,
        Err(e) => return e as i64,
    };
    let vt = match t.get_raw(th, ObjType::Thread, RIGHT_MANAGE) {
        Ok((o, _)) => o,
        Err(e) => return e as i64,
    };
    // SAFETY: 双 get_raw 门控过；单核 IRQ 屏蔽。proc 取 &mut（要写子表 ht）。
    let (proc, thrd) = unsafe {
        ((*vr.as_ptr()).obj.as_process_mut(), (*vt.as_ptr()).obj.as_thread_mut())
    };
    let (Some(proc), Some(thrd)) = (proc, thrd) else {
        return ERR_BAD_HANDLE as i64; // 类型门控后不可达；防御面
    };
    if thrd.owner() != vr {
        return ERR_ACCESS_DENIED as i64; // thread 不归该 process 管
    }
    if thrd.started {
        return ERR_BAD_STATE as i64;
    }
    // bootstrap 迁移：调用者表 → 子进程表（子入口 x0 = 子表新值）。
    // 两张不同表的 &mut（t = 调用者表、proc.ht = 子表）无别名。
    let child_x0: usize = if bootstrap != 0 {
        match t.take(bootstrap) {
            Ok((obj, rights)) => match proc.ht.insert_ref(obj, rights) {
                Ok(h) => h as usize,
                Err(e) => {
                    // 表满回滚（v1 不可达）：重插调用者表；handle 值或变（代号
                    // 推进）但语义存活——错误码原样上抛。
                    let _ = t.insert_ref(obj, rights);
                    return e as i64;
                }
            },
            Err(e) => return e as i64, // 坏 handle：子表未动（原子）
        }
    } else {
        0
    };
    thrd.entry = entry as usize;
    thrd.sp = sp as usize;
    let ht_ptr = &raw mut proc.ht as *mut HandleTable as usize;
    // 任务装配 try 化 + **前置于一切不可逆变更**（3b3 H2 重排）：栈/TCB OOM →
    // 回滚迁移（take 回子表令牌、重插调用者表——值或变、语义存活，同 insert
    // 失败回滚）→ -ENOMEM；thrd/started/retain 全未动。单核 IRQ 屏蔽：已入队
    // 任务在 eret 前不会运行，下面字段补写无竞态。
    if berry_sched::try_spawn_user(
        entry as usize,
        child_x0,
        sp as usize,
        b"user_child",
        proc.ttbr0_pa,
        proc.asid,
        ht_ptr,
    )
    .is_none()
    {
        if bootstrap != 0 {
            // 回滚迁移（子表刚插入、单核无插队 → take 必成；防御面下令牌泄漏，
            // 安全向）。
            if let Ok((obj, rights)) = proc.ht.take(child_x0 as u32) {
                let _ = t.insert_ref(obj, rights);
            }
        }
        return ERR_NO_MEMORY as i64;
    }
    thrd.started = true;
    // SAFETY: vr 在表（get_raw 过）；任务钉住（v1 永不释放，见上）。
    unsafe { (*vr.as_ptr()).retain() };
    0
}

// ---- channel（3-4b；对象 = `channel.rs`，端点互钉与环泄漏政策见其模块注）----

/// `channel_create(options, *out0, *out1)`（4-3c 起 options 首参，Zircon/
/// `event_create` 同形）：一次建一对端点（`channel.rs` 两端互指互钉——一端
/// 活则对端对象必活）。两端均授 **SEND|RECV|DUPLICATE**（对称权利；TRANSFER
/// 位同 DUPLICATE 的 vocab 政策：已定义不执法）。options：bit0 =
/// [`CHAN_OPT_NO_INTENT`]（intent opt-out，两端同置——通道级不传播）；其余位
/// → -EINVAL（options 纪律）。
extern "C" fn sys_channel_create(tf: &mut TrapFrame) -> i64 {
    let (options, out0, out1) = (tf.regs[1], tf.regs[2], tf.regs[3]);
    if options & !CHAN_OPT_NO_INTENT != 0 {
        return ERR_INVALID_ARG as i64;
    }
    let no_intent = options & CHAN_OPT_NO_INTENT != 0;
    let el1 = caller_is_el1(tf);
    if let Err(e) = copy_out_u64(el1, out0, 0) {
        return e as i64;
    }
    if let Err(e) = copy_out_u64(el1, out1, 0) {
        return e as i64;
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let rights = RIGHT_SEND | RIGHT_RECV | RIGHT_DUPLICATE;
    // 端点装配 try 化（3b3 H2）：队列预储 CHAN_DEPTH × Message ≈ 2.7KB/端，
    // 失败先于一切入表——零回滚面，折 -ENOMEM。
    let (Some(end0), Some(end1)) =
        (ChannelEnd::try_unpaired(no_intent), ChannelEnd::try_unpaired(no_intent))
    else {
        return ERR_NO_MEMORY as i64;
    };
    let h0 = match t.insert(KernelObj::Channel(end0), rights) {
        Ok(h) => h,
        Err(e) => return e as i64,
    };
    match t.insert(KernelObj::Channel(end1), rights) {
        Ok(h1) => {
            let (p0, p1) = (
                t.get_raw(h0, ObjType::Channel, 0).ok().map(|(o, _)| o),
                t.get_raw(h1, ObjType::Channel, 0).ok().map(|(o, _)| o),
            );
            match (p0, p1) {
                (Some(a), Some(b)) => {
                    // SAFETY: 两指针刚 get_raw 过（在表、类型 Channel）。
                    unsafe { ChannelEnd::pair(a, b) };
                    let r0 = copy_out_u64(el1, out0, h0 as u64);
                    let r1 = copy_out_u64(el1, out1, h1 as u64);
                    match (r0, r1) {
                        (Ok(()), Ok(())) => 0,
                        (Err(e), _) | (_, Err(e)) => e as i64, // 两端已建——调用方重试态自理
                    }
                }
                _ => ERR_BAD_HANDLE as i64, // 刚插入即取失败 = 不可达防御面；两端泄漏（安全向）
            }
        }
        Err(e) => {
            // 第二端入表失败（表满/gen 尽——真实可达，敌意/失控进程可自刷本表）：
            // 回滚第一端——未配对端点 Drop 走 None 分支不碰任何指针（channel.rs），安全。
            let _ = t.close(h0);
            e as i64
        }
    }
}

/// `channel_send(ch, bytes, nbytes, handles, nhandles)`：消息压**对端**队列
/// （非阻塞：满 → -EBUSY）。门控：本端 **SEND**。随行 handle 逐个 `take` 出
/// 调用者表（发送方失去原值）、令牌随消息挂起，接收方 recv 时 `insert_ref`
/// 落进**它的**表——能力随消息跨表迁移（能力模型 §4）。
///
/// 原子序（任一步失败 = 调用者表零变化）：参数上限（no-panic 前置）→ 门控 →
/// 对端满检 → 随行值全量预检（活值 + 数组内无重复——重复值先 take 会作废后值，
/// 回滚也换值）→ 载荷拷入 → 全量 take → 压队列。take 失败仅剩不可达防御面
/// （预检后单核无插队），回滚重插（值或变、语义活）。
///
/// **4-4 fastpath/slowpath 分层**（`IPC设计.md` §7 检查链）：共享前缀（上列
/// 全部错误面）之后按检查链 fork——「无随行 handle（迁移面留 slowpath）+
/// 对端队列空 + 恰一个非哨兵 READABLE 等待者且无哨兵在场」全过 → fastpath
/// 定点唤醒（`fast_wake`，与 slowpath 共用同一内部唤醒序列）+ 直切判定
///（`try_preempt`：严格更高 eff 才切，seL4 direct switch 的 v1 形态）；
/// 任一不满足 → slowpath（`wake_matching` 全匹配唤醒）。**两路语义逐条对齐，
/// 不长第二套**：错误码由共享前缀天然恒等；唤醒集恒等（多匹配者/任一哨兵
/// 在场一律退 slowpath——单点唤醒的集合恰等于全匹配集时才走快路）；消息
/// 可见性/顺序恒等（两路都 push 对端队列，recv 侧无感知）。链项中「单次
/// handle 查找 / 消息 ≤ inline 上限（nargs 前置检）/ 同核（v1 单核）/ 无
/// fault 挂起（v1 无 pager）」= v1 结构性成立。intent 快照两路同代码
/// （一次 cmp+sel，零记账——§7「intent 传播不破线」）。
extern "C" fn sys_channel_send(tf: &mut TrapFrame) -> i64 {
    let (h, bytes, nbytes, handles, nhandles) = (
        tf.regs[1] as u32,
        tf.regs[2],
        tf.regs[3] as usize,
        tf.regs[4],
        tf.regs[5] as usize,
    );
    if nbytes > CHAN_MSG_MAX || nhandles > CHAN_HND_MAX {
        return ERR_INVALID_ARG as i64;
    }
    let el1 = caller_is_el1(tf);
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let self_ptr = match t.get_raw(h, ObjType::Channel, RIGHT_SEND) {
        Ok((o, _)) => o,
        Err(e) => return e as i64,
    };
    // 对端指针 + 满检 + 队列空快照（4-4 检查链项——对端与本端是不同对象，
    // pair 互指保证，&mut 无别名；满检过且此刻空 = 「队列空」链项成立）。
    // SAFETY: get_raw 已门控；对端由互钉保活（本端活 ⇒ 对端对象必在堆）；
    // 单核 IRQ 屏蔽无并发。
    let (peer_ptr, peer_empty) = unsafe {
        let Some(end) = (*self_ptr.as_ptr()).obj.as_channel_mut() else {
            return ERR_BAD_HANDLE as i64;
        };
        let Some(peer) = end.peer() else {
            return ERR_BAD_STATE as i64; // 未配对（v1 不可达——create 即配对）
        };
        let Some(peer_end) = (*peer.as_ptr()).obj.as_channel_mut() else {
            return ERR_BAD_HANDLE as i64;
        };
        if peer_end.queue_full() {
            return ERR_BUSY as i64; // 非阻塞：满即返（阻塞等空位 = wait_one WRITABLE，4-2）
        }
        (peer, !peer_end.has_message())
    };
    // 随行 handle 值拷入 + 全量预检（无副作用——失败 = 表零变化）。
    let mut hbuf = [0u8; CHAN_HND_MAX * 4];
    if let Err(e) = copy_from_user(el1, handles, &mut hbuf[..nhandles * 4]) {
        return e as i64;
    }
    let mut hvals = [0u32; CHAN_HND_MAX];
    for i in 0..nhandles {
        hvals[i] = u32::from_le_bytes([hbuf[i * 4], hbuf[i * 4 + 1], hbuf[i * 4 + 2], hbuf[i * 4 + 3]]);
    }
    for i in 0..nhandles {
        if !t.alive(hvals[i]) {
            return ERR_BAD_HANDLE as i64;
        }
        for j in 0..i {
            if hvals[j] == hvals[i] {
                return ERR_INVALID_ARG as i64; // 数组内重复值（见函数注释原子序论证）
            }
        }
    }
    let mut bbuf = [0u8; CHAN_MSG_MAX];
    if let Err(e) = copy_from_user(el1, bytes, &mut bbuf[..nbytes]) {
        return e as i64;
    }
    // 全量 take（bounded ≤4——占位数组免堆分配；slice 进消息只取前 n 项）。
    let mut toks: [(NonNull<ObjRef>, Rights); CHAN_HND_MAX] = [(NonNull::dangling(), 0); CHAN_HND_MAX];
    for i in 0..nhandles {
        match t.take(hvals[i]) {
            Ok(tok) => toks[i] = tok,
            Err(e) => {
                for tok in &toks[..i] {
                    let _ = t.insert_ref(tok.0, tok.1); // 回滚（v1 不可达防御面）
                }
                return e as i64;
            }
        }
    }
    // 定长内联消息（3b3 H2 try 化：曾在此双 Box 分配——take 之后 OOM 即 abort
    // 于迁移中途；内联 + 队列预储后 send 全链零分配）。
    let mut msg = Message::new();
    msg.bytes[..nbytes].copy_from_slice(&bbuf[..nbytes]);
    msg.nbytes = nbytes;
    msg.handles[..nhandles].copy_from_slice(&toks[..nhandles]);
    msg.nhandles = nhandles;
    // 4-3c：消息头快照发送者 intent_effective（ADR #42 五件套——fastpath 增量
    // = 一次结构体填充；记账/帽判定全在 recv 结算 slowpath，「fastpath 零记账」）。
    // CURRENT 无效（ktest/init 前语境）→ 零值头（UNSPECIFIED 首手）照常发送。
    if let Some((meta, snap)) = berry_sched::intent_snapshot_current(vct_ns()) {
        msg.meta = meta;
        if snap == berry_sched::SnapOut::Capped {
            // 每跳预算帽：降级为不传播（消息头已装自己的 declared）+ 计数 + trace。
            crate::telemetry::intent_budget_capped();
            berry_hal_aarch64::uart_puts(b"intent: hop budget cap (send)\n");
        }
    }
    // 4-4 检查链判定（push 前取条件——「对端队列空」链项此后不变：单核 IRQ
    // 屏蔽无插队）：无随行 handle + 队列空 + 恰一个非哨兵 READABLE 等待者且
    // 无哨兵在场 → fastpath；否则 slowpath（检查链其余项 v1 结构性成立，见
    // 函数注释）。
    let waiter = if nhandles == 0 && peer_empty {
        berry_sched::find_single_waiter(peer_ptr.as_ptr() as usize, SIGNAL_READABLE)
    } else {
        None
    };
    // SAFETY: peer_ptr 互钉在活；队列有界（满检过，单核无并发插队）。
    unsafe {
        let Some(peer_end) = (*peer_ptr.as_ptr()).obj.as_channel_mut() else {
            return ERR_BAD_HANDLE as i64; // 上面刚解包过——不可达防御面（消息泄漏，安全向）
        };
        peer_end.push(msg);
    }
    match waiter {
        Some(t) => {
            // fastpath：定点唤醒（同一内部唤醒序列 + PI 收尾重推）→ 直切判定
            //（严格更高 eff 才切——切走后从 try_preempt 续行、IRQ 仍屏蔽）。
            // SAFETY: t 来自同一屏蔽窗 find_single_waiter（此刻仍在 SLEEPQ 上）。
            unsafe { berry_sched::fast_wake(t) };
            crate::telemetry::ipc_fast_hit();
            if berry_sched::try_preempt() {
                crate::telemetry::ipc_direct_switch();
            }
        }
        None => {
            // slowpath：4-2 唤醒点原样——消息落对端队列 → 对端 READABLE 置位，
            // 唤醒等「对端可读」的全部睡眠者（键 = 对端 ObjRef 地址；等价于
            // Zircon send 侧的可读信号；无等待者时本遍历近零开销）。
            crate::telemetry::ipc_fast_miss();
            berry_sched::wake_matching(peer_ptr.as_ptr() as usize, SIGNAL_READABLE);
        }
    }
    0
}

/// recv 失败回滚：`inserted` 里的新 handle 值逐个 take 回（调用者表复原——
/// 令牌本就还在消息里，insert_ref 不搬走它），消息压回**队首**（FIFO 序复原）。
/// 令牌引用计数全程未动（take/insert_ref 都不碰 refs）→ 复原后系统与调用前一致。
///
/// # Safety
/// `endp` 须为本端在表对象（get_raw 门控过）；`msg` 须为从该端队列 pop 出的
/// 消息；单核 IRQ 屏蔽上下文。
unsafe fn recv_rollback(t: &mut HandleTable, endp: NonNull<ObjRef>, msg: Message, inserted: &[u32]) {
    for &nh in inserted {
        let _ = t.take(nh);
    }
    // SAFETY: 见函数注释；解包失败 = 不可达防御面（消息泄漏，安全向）。
    if let Some(end) = (*endp.as_ptr()).obj.as_channel_mut() {
        end.push_front(msg);
    }
}

/// `channel_recv(ch, bytes, *nbytes, handles, *nhandles)`：弹**本端**队首
/// （非阻塞：空 → -EAGAIN）。门控：本端 **RECV**。`*nbytes`/`*nhandles`
/// in-out：进 = 调用方容量，出 = 实际字节数 / 随行 handle 数；容量不足 →
/// -EINVAL 且消息保原样（v1 无「回写所需尺寸」面，ABI §8.3 注）。随行令牌
/// `insert_ref` 落调用者表、**新值**写 `handles` 数组（值必变——表不同/槽不同）。
///
/// 原子序：空队先检（空时零用户内存读 = 零 fault 面）→ pop 消费 → 容量/载荷/
/// 令牌入表/回写各步任一失败 → [`recv_rollback`]（表 + 队列全复原）。
extern "C" fn sys_channel_recv(tf: &mut TrapFrame) -> i64 {
    let (h, bytes, nbytesp, handles, nhandlesp) = (
        tf.regs[1] as u32,
        tf.regs[2],
        tf.regs[3],
        tf.regs[4],
        tf.regs[5],
    );
    let el1 = caller_is_el1(tf);
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let self_ptr = match t.get_raw(h, ObjType::Channel, RIGHT_RECV) {
        Ok((o, _)) => o,
        Err(e) => return e as i64,
    };
    // SAFETY: get_raw 已门控；单核 IRQ 屏蔽。pop 消费，失败路径经回滚压回队首。
    let msg = unsafe {
        let Some(end) = (*self_ptr.as_ptr()).obj.as_channel_mut() else {
            return ERR_BAD_HANDLE as i64;
        };
        match end.front() {
            Some(_) => end.pop(),
            None => return ERR_AGAIN as i64, // 非阻塞：空即返（阻塞等消息 = wait_one READABLE，4-2）
        }
    };
    let cap_bytes = match copy_in_u64(el1, nbytesp) {
        Ok(v) => v as usize,
        Err(e) => {
            // SAFETY: 见 recv_rollback 契约（endp 门控过、msg 刚 pop、无令牌入表）。
            unsafe { recv_rollback(t, self_ptr, msg, &[]) };
            return e as i64;
        }
    };
    let cap_hnd = match copy_in_u64(el1, nhandlesp) {
        Ok(v) => v as usize,
        Err(e) => {
            // SAFETY: 同上。
            unsafe { recv_rollback(t, self_ptr, msg, &[]) };
            return e as i64;
        }
    };
    if msg.nbytes > cap_bytes || msg.nhandles > cap_hnd {
        // SAFETY: 同上。
        unsafe { recv_rollback(t, self_ptr, msg, &[]) };
        return ERR_INVALID_ARG as i64;
    }
    if let Err(e) = copy_to_user(el1, bytes, &msg.bytes[..msg.nbytes]) {
        // SAFETY: 同上。
        unsafe { recv_rollback(t, self_ptr, msg, &[]) };
        return e as i64;
    }
    // 令牌入调用者表（新值收集；任一失败全量回滚——表里已入的 take 回）。
    let mut newvals = [0u32; CHAN_HND_MAX];
    for i in 0..msg.nhandles {
        match t.insert_ref(msg.handles[i].0, msg.handles[i].1) {
            Ok(nh) => newvals[i] = nh,
            Err(e) => {
                // SAFETY: 同上（msg 内令牌未动，回滚只需压回）。
                unsafe { recv_rollback(t, self_ptr, msg, &[]) };
                return e as i64;
            }
        }
    }
    let n = msg.nhandles;
    let mut obuf = [0u8; CHAN_HND_MAX * 4];
    for i in 0..n {
        obuf[i * 4..i * 4 + 4].copy_from_slice(&newvals[i].to_le_bytes());
    }
    if let Err(e) = copy_to_user(el1, handles, &obuf[..n * 4]) {
        // SAFETY: 同上（回滚含 take 回已入表的 n 个令牌）。
        unsafe { recv_rollback(t, self_ptr, msg, &newvals[..n]) };
        return e as i64;
    }
    if let Err(e) = copy_out_u64(el1, nbytesp, msg.nbytes as u64) {
        // SAFETY: 同上。
        unsafe { recv_rollback(t, self_ptr, msg, &newvals[..n]) };
        return e as i64;
    }
    if let Err(e) = copy_out_u64(el1, nhandlesp, n as u64) {
        // SAFETY: 同上。
        unsafe { recv_rollback(t, self_ptr, msg, &newvals[..n]) };
        return e as i64;
    }
    // 4-2：本端弹出一条 → 对端队列腾出一个槽 → 对端 WRITABLE 置位，唤醒等
    //「对端未满」的睡眠者（A 的可写性由 B 的队列决定——键 = 对端 ObjRef 地址；
    // 仅成功路径唤醒：回滚路径状态全复原，唤醒即虚假）。
    // SAFETY: 本端门控过、互钉在活；只读对端指针。
    if let Some(peer) = (unsafe { (*self_ptr.as_ptr()).obj.as_channel() }).and_then(|e| e.peer()) {
        berry_sched::wake_matching(peer.as_ptr() as usize, SIGNAL_WRITABLE);
    }
    // 4-3c slowpath 记账：dequeue 即结算 intent（每消息重导——借入/opt-out/
    // 超帽的语义与遥测见 `berry_sched::intent`；CURRENT 无效则跳过不记）。
    // SAFETY: 本端门控过；只读 no_intent 标志。
    let opt_out = (unsafe { (*self_ptr.as_ptr()).obj.as_channel() }).map_or(false, |e| e.no_intent());
    if let Some(out) = berry_sched::intent_settle_current(msg.meta, opt_out, vct_ns()) {
        match out {
            berry_sched::SettleOut::Borrowed => crate::telemetry::intent_borrowed(),
            berry_sched::SettleOut::CappedHop => {
                crate::telemetry::intent_hop_capped_inc();
                berry_hal_aarch64::uart_puts(b"intent: hop cap (recv)\n");
            }
            berry_sched::SettleOut::CappedBudget => {
                crate::telemetry::intent_budget_capped();
                berry_hal_aarch64::uart_puts(b"intent: chain budget cap (recv)\n");
            }
            berry_sched::SettleOut::Own => {}
        }
    }
    drop(msg); // 消费：令牌已入表（Message 定长内联——drop 无堆动作，不碰 refs）
    0
}

// ---- event_create（3-4c；对象 = `event.rs` 纯用户位掩码）----

/// `event_create(options, *out)`：建一个 Event（出生信号全零）。`options` v1
/// 恒 0（非 0 → -EINVAL，ABI §8.3 注记）；授 **SIGNAL|DUPLICATE**——持有者可
/// 置/清位、可复制授权给同伴（等的一方 dup 到只读即可，wait 不需权利位）。
extern "C" fn sys_event_create(tf: &mut TrapFrame) -> i64 {
    let (options, out) = (tf.regs[1], tf.regs[2]);
    if options != 0 {
        return ERR_INVALID_ARG as i64;
    }
    let el1 = caller_is_el1(tf);
    if let Err(e) = copy_out_u64(el1, out, 0) {
        return e as i64;
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let h = match t.insert(KernelObj::Event(Event::new()), RIGHT_SIGNAL | RIGHT_DUPLICATE) {
        Ok(h) => h,
        Err(e) => return e as i64, // 表满/gen 尽/装箱 OOM——对象未入表，零回滚面
    };
    match copy_out_u64(el1, out, h as u64) {
        Ok(()) => 0,
        Err(e) => e as i64, // 对象已入表——调用方重试态自理（同 channel_create 政策）
    }
}

// ---- object_wait/signal 面（3-4c 事件三号 → 4-1 阻塞化 → 4-2 扩 Channel）----

/// 对象当前信号状态（4-2 wait 面统一查询点）：
/// - Event → 用户位掩码原样；
/// - Channel → 内核派生：`READABLE`（**本端**队列非空）| `WRITABLE`（**对端**
///   队列未满；未配对端点对端恒「满」——运行期不可达，create 即配对）；
/// - 其余类型 → `None`（wait 面只收 Event|Channel）。
///
/// # Safety
/// `obj` 须为在表/钉住的活对象；Channel 分支解引用互钉对端（互钉保活）。
/// 只读；调用点均在 IRQ 屏蔽串行语境。
unsafe fn signal_state(obj: NonNull<ObjRef>) -> Option<u64> {
    unsafe {
        match &(*obj.as_ptr()).obj {
            KernelObj::Event(e) => Some(e.signals),
            KernelObj::Channel(c) => {
                let mut s = if c.has_message() { SIGNAL_READABLE } else { 0 };
                if let Some(peer) = c.peer() {
                    if let KernelObj::Channel(pe) = &(*peer.as_ptr()).obj {
                        if !pe.queue_full() {
                            s |= SIGNAL_WRITABLE;
                        }
                    }
                }
                Some(s)
            }
            _ => None,
        }
    }
}

/// `object_signal(h, clear_mask, set_mask)`：`signals = (signals & !clear) | set`
/// （置/清一条原子）。v1 仅 **Event** 可 signal（thread/process 信号面随后续
/// 步）；门控 RIGHT_SIGNAL。返回 0（非错误面不回写掩码新值——读用 wait_one）。
///
/// 4-2 位空间：clear/set 掩码**只收高 32 位用户位**（低 32 位保留给内核派生
/// READABLE/WRITABLE）——越界 → -EINVAL（检在 get_raw 门控**之后**：坏 handle
/// /类型不符仍 -EBADF，错误面优先级不变）。
///
/// 4-1：置位后经 `wake_matching` 唤醒等本对象、请求位与**新置位集**相交的
/// 睡眠者（清位不唤醒——位没新增）。匹配键 = ObjRef 地址（同一 Event 的
/// handle 跨表 insert_ref 共享同一 ObjRef，双表 signal/wait 天然互通）。
/// 被唤醒者**重验条件**兜底（重置已置位等边角 → 假唤醒无害，见 wait_one）。
extern "C" fn sys_object_signal(tf: &mut TrapFrame) -> i64 {
    let (h, clear, set) = (tf.regs[1] as u32, tf.regs[2], tf.regs[3]);
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let obj = match t.get_raw(h, ObjType::Event, RIGHT_SIGNAL) {
        Ok((o, _)) => o,
        Err(e) => return e as i64, // 坏值/类型不符 -EBADF；缺 SIGNAL 位 -EACCES
    };
    if (clear | set) & !SIGNAL_USER_MASK != 0 {
        return ERR_INVALID_ARG as i64; // 低 32 位内核派生保留（4-2 位空间）
    }
    // SAFETY: get_raw 已三门控（在表、类型 Event、权利足）；单核 IRQ 屏蔽。
    unsafe {
        let Some(ev) = (*obj.as_ptr()).obj.as_event_mut() else {
            return ERR_BAD_HANDLE as i64; // 不可达防御面（类型刚验过）
        };
        ev.signal(clear, set);
    }
    // 纯清位（set=0）无新增位 → 无人该醒（wake_matching 对掩码 0 无命中）。
    berry_sched::wake_matching(obj.as_ptr() as usize, set);
    0
}

/// `object_wait_one(h, signals, deadline, *observed)`（4-1 阻塞化；4-2 扩
/// Channel）：等待 `current & signals != 0`。命中 → `*observed = current`
/// （**全满足集**，Zircon 同形）返 0；poll（`deadline==0`）未命中 → -EAGAIN；
/// `deadline≠0`（MONOTONIC 绝对时刻）未命中 → 挂 [`berry_sched::block_current`]
/// 睡到点。
///
/// **wait 面对象 = Event | Channel**（4-2）：Event 状态 = 用户位掩码；Channel
/// 状态 = 内核派生 READABLE/WRITABLE（[`signal_state`] 统一查询——用户 signal
/// 不出 Channel 的位，等用户位 = 等一个永不置位的位，poll -EAGAIN / 到点
/// -ETIMEDOUT，同 Zircon 不设门）。
///
/// **poll-first**：信号已到即返不睡（无丢信号）。醒来**重验条件**（醒因 ≠
/// 结果：Timeout 时位可能刚被置、Signaled 时位可能又被清——清位竞态下重验
/// 未命中且未到点则**再睡**，循环兜底）；到点未命中 → -ETIMEDOUT（时刻已过
/// 的 deadline 走同一路径立即返，不睡）。
///
/// **睡眠钉住**：进入循环前 `ObjRef::retain` 一次、覆盖全部睡眠轮次——睡眠
/// 期间调用者表乃至全表的 handle 可能被关光（waker 只持有另一表的 handle），
/// 裸 ObjRef 悬空 = use-after-free（3-3 钉住纪律的跨睡眠版）；唯一出口
/// release（若期间归零，此处 drop——屏蔽窗内、Drop 链不回表，安全）。
///
/// 等待**不需权利位**（12 位 rights 表本无 WAIT 位；持有 handle 即可查）。
extern "C" fn sys_object_wait_one(tf: &mut TrapFrame) -> i64 {
    let (h, signals, deadline, observed) =
        (tf.regs[1] as u32, tf.regs[2], tf.regs[3], tf.regs[4]);
    let el1 = caller_is_el1(tf);
    if let Err(e) = copy_out_u64(el1, observed, 0) {
        return e as i64;
    }
    let deadline_vct = if deadline != 0 {
        match ns_to_vct(deadline) {
            Some(v) => v,
            None => return ERR_NOT_SUPPORTED as i64, // 频率不可读（真机异常）
        }
    } else {
        0
    };
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    // 4-2：wait 面扩 Channel——Event 未中再查 Channel（权利面同形：wait 不需
    // 权利位）。两拒合并回**首个**错误（-EBADF/-EACCES 原样透传）。
    let obj = match t.get_raw(h, ObjType::Event, 0) {
        Ok((o, _)) => o,
        Err(e) => match t.get_raw(h, ObjType::Channel, 0) {
            Ok((o, _)) => o,
            Err(_) => return e as i64,
        },
    };
    // 统一循环：挂队前 + 每次醒来都重验（醒因不等于结果）。钉住**一次覆盖
    // 全部睡眠轮次**（清位竞态下可能再睡多轮），release 收口在唯一出口——
    // release 后不再触 obj。
    // SAFETY: 活对象（表内 handle 钉着）；睡眠期间全表 handle 被关光也不销毁
    //（3-3 钉住纪律的跨睡眠版）。
    unsafe { (*obj.as_ptr()).retain() };
    let ret = loop {
        // SAFETY: 钉住在身对象必活；只读（signal/send/recv 写路径与本读点同
        // 为屏蔽串行）。
        let Some(current) = (unsafe { signal_state(obj) }) else {
            break ERR_BAD_HANDLE as i64; // 不可达防御面
        };
        if current & signals != 0 {
            break match copy_out_u64(el1, observed, current) {
                Ok(()) => 0,
                Err(e) => e as i64,
            };
        }
        if deadline == 0 {
            break ERR_AGAIN as i64; // poll 语义（3-4c 不变）
        }
        // SAFETY: 只读 CNTVCT。
        if deadline_vct <= berry_hal_aarch64::timer::cntvct() {
            break ERR_TIMEDOUT as i64; // 到点未命中（含传入即已过——不睡）
        }
        // 匹配键 = ObjRef 地址；请求位 = signals（signal 侧以新置位集匹配）。
        // 醒因丢弃：Signaled/Timeout 都只信重验——Timeout 时位可能刚被置、
        // Signaled 时位可能又被清；未命中且未到点则再睡，循环兜底。
        let _reason =
            berry_sched::block_current(obj.as_ptr() as usize, signals, deadline_vct);
    };
    // SAFETY: 与上面的 retain 成对（唯一出口）；若等待期间 handle 全关光，
    // 此处归零 drop（屏蔽窗内、Drop 链纯指针操作不回表）。
    unsafe { ObjRef::release(obj) };
    ret
}

/// `object_wait_many(handles, signals, n, deadline, *idx, *observed)`（4-2b）：
/// 等 n 个对象中**任一**满足 `current & signals[i] != 0`。命中 → `*idx = i`
/// （入参序——多对象同时就绪取最前）+ `*observed = 该对象全满足集`，返 0；
/// poll（`deadline==0`）未命中 -EAGAIN；`deadline≠0` 未命中睡到点 -ETIMEDOUT。
/// wait 面（Event|Channel）/位空间/醒来重验/睡眠钉住纪律与
/// [`sys_object_wait_one`] 同源——钉住 = **全部**对象各 retain 一次、唯一出口
/// 逐个 release。
///
/// **[`berry_sched::WAIT_ANY`] 哨兵**：Task 只有单一 `wait_obj` 匹配键，多对象
/// 等待者挂哨兵——任何对象的 wake_matching 都命中（虚假唤醒由重验循环兜底；
/// v1 等待者个位数，全队列误醒开销可忽略）。
///
/// n ∈ [1, [`WAIT_MANY_MAX`]=16]（栈上定长数组零分配、no-panic；越界 -EINVAL）。
/// handles/signals 两数组**先整段拷入内核栈**再解析（copy_from_user 界检整段
/// 走查；syscall 全程 IRQ 屏蔽 → 校验与解析间无 TOCTOU）。数组内重复 handle
/// 合法（各项独立 retain/release，引用计数守恒）。
/// wait_many 的 n 上限（ABI §8.3；栈上定长数组零分配的边界——v1 无堆路径，
/// 16 与 Zircon 同量级）。
const WAIT_MANY_MAX: usize = 16;

extern "C" fn sys_object_wait_many(tf: &mut TrapFrame) -> i64 {
    let (handles_p, signals_p, n, deadline, idx_p, observed_p) = (
        tf.regs[1],
        tf.regs[2],
        tf.regs[3] as usize,
        tf.regs[4],
        tf.regs[5],
        tf.regs[6],
    );
    let el1 = caller_is_el1(tf);
    if n == 0 || n > WAIT_MANY_MAX {
        return ERR_INVALID_ARG as i64;
    }
    // 出参预清（失败路径调用方拿确定值；命中覆盖为真值。idx=usize::MAX=未命中）。
    if let Err(e) = copy_out_u64(el1, observed_p, 0).and_then(|()| copy_out_u64(el1, idx_p, u64::MAX)) {
        return e as i64;
    }
    let deadline_vct = if deadline != 0 {
        match ns_to_vct(deadline) {
            Some(v) => v,
            None => return ERR_NOT_SUPPORTED as i64, // 频率不可读（真机异常）
        }
    } else {
        0
    };
    // 两数组整段拷入（同 channel_send 的随行值拷入模式：字节缓冲 + 逐项小端拼）。
    let mut hraw = [0u8; WAIT_MANY_MAX * 4];
    if let Err(e) = copy_from_user(el1, handles_p, &mut hraw[..n * 4]) {
        return e as i64;
    }
    let mut sraw = [0u8; WAIT_MANY_MAX * 8];
    if let Err(e) = copy_from_user(el1, signals_p, &mut sraw[..n * 8]) {
        return e as i64;
    }
    let mut hbuf = [0u32; WAIT_MANY_MAX];
    for i in 0..n {
        hbuf[i] = u32::from_le_bytes([
            hraw[i * 4],
            hraw[i * 4 + 1],
            hraw[i * 4 + 2],
            hraw[i * 4 + 3],
        ]);
    }
    let mut sbuf = [0u64; WAIT_MANY_MAX];
    for i in 0..n {
        let mut b = [0u8; 8];
        b.copy_from_slice(&sraw[i * 8..i * 8 + 8]);
        sbuf[i] = u64::from_le_bytes(b);
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    // 解析全部对象（Event|Channel 双类型，同 wait_one；wait 不需权利位）；任一
    // 失败 = 整体拒绝、首错透传——此时尚无 retain，无回滚面。
    let mut objs: [Option<NonNull<ObjRef>>; WAIT_MANY_MAX] = [None; WAIT_MANY_MAX];
    for i in 0..n {
        let o = match t.get_raw(hbuf[i], ObjType::Event, 0) {
            Ok((o, _)) => o,
            Err(e) => match t.get_raw(hbuf[i], ObjType::Channel, 0) {
                Ok((o, _)) => o,
                Err(_) => return e as i64,
            },
        };
        objs[i] = Some(o);
    }
    // 钉住全部对象一次、覆盖全部睡眠轮次（wait_one 纪律的多数组版——睡眠期间
    // 全表 handle 被关光也不销毁）。
    for o in objs[..n].iter().flatten() {
        // SAFETY: 活对象（表内 handle 钉着）。
        unsafe { (*o.as_ptr()).retain() };
    }
    let ret = loop {
        // 逐项重验（醒来从头扫——醒因 ≠ 结果；多对象同时就绪取入参序最前者）。
        let mut hit: Option<Result<(usize, u64), i64>> = None;
        for i in 0..n {
            let o = objs[i].unwrap();
            // SAFETY: 钉住在身对象必活；只读（signal/send/recv 写路径与本读点
            // 同为屏蔽串行）。
            let Some(current) = (unsafe { signal_state(o) }) else {
                hit = Some(Err(ERR_BAD_HANDLE as i64)); // 不可达防御面
                break;
            };
            if current & sbuf[i] != 0 {
                hit = Some(Ok((i, current)));
                break;
            }
        }
        match hit {
            Some(Ok((i, current))) => {
                break match copy_out_u64(el1, idx_p, i as u64)
                    .and_then(|()| copy_out_u64(el1, observed_p, current))
                {
                    Ok(()) => 0,
                    Err(e) => e as i64,
                };
            }
            Some(Err(code)) => break code, // 防御面 -EBADF
            None => {}                     // 全未命中 → 走睡眠/超时判定
        }
        if deadline == 0 {
            break ERR_AGAIN as i64; // poll 语义（同 wait_one）
        }
        // SAFETY: 只读 CNTVCT。
        if deadline_vct <= berry_hal_aarch64::timer::cntvct() {
            break ERR_TIMEDOUT as i64; // 到点未命中（含传入即已过——不睡）
        }
        // 哨兵 + 全位掩码：任何对象的任何新置位都唤醒本任务，醒来重验定夺。
        let _reason =
            berry_sched::block_current(berry_sched::WAIT_ANY, !0, deadline_vct);
    };
    for o in objs[..n].iter().flatten() {
        // SAFETY: 与上面的 retain 成对（唯一出口）；release 后不再触 obj。
        unsafe { ObjRef::release(*o) };
    }
    ret
}

// ---- boot 期注册（rust_main 在 ktest 之前调用；HAL install 见其 # Panics）----

/// 注册已实现号（3-1 四号 + VMO/Vmar 九号 + process/thread 三号 + channel 三号 +
/// event 三号 = 3-4c 起 22 号；4-0 加 thread_sleep(0x37) = 23 号；4-1 wait_one
/// 阻塞化 / 4-2 wait 面扩 Channel 均为改板；4-2b 加 wait_many(0x05) = **24 号**。
/// 0x33/0x35/0x36 退出随后续步）。
/// 未列号走 HAL 默认 `-ENOTSUP`。
pub fn install() {
    use berry_hal_aarch64::syscall::install as reg;
    reg(NR_HANDLE_CLOSE, sys_handle_close);
    reg(NR_HANDLE_DUPLICATE, sys_handle_duplicate);
    reg(NR_HANDLE_REPLACE, sys_handle_replace);
    reg(NR_OBJECT_WAIT_ONE, sys_object_wait_one);
    reg(NR_OBJECT_WAIT_MANY, sys_object_wait_many);
    reg(NR_OBJECT_SIGNAL, sys_object_signal);
    reg(NR_CLOCK_READ, sys_clock_read);
    reg(NR_VMO_CREATE, sys_vmo_create);
    reg(NR_VMO_COMMIT, sys_vmo_commit);
    reg(NR_VMO_DECOMMIT, sys_vmo_decommit);
    reg(NR_VMO_READ, sys_vmo_read);
    reg(NR_VMO_WRITE, sys_vmo_write);
    reg(NR_VMAR_MAP, sys_vmar_map);
    reg(NR_VMAR_UNMAP, sys_vmar_unmap);
    reg(NR_VMAR_PROTECT, sys_vmar_protect);
    reg(NR_VMAR_ALLOCATE, sys_vmar_allocate);
    reg(NR_PROCESS_CREATE, sys_process_create);
    reg(NR_PROCESS_START, sys_process_start);
    reg(NR_THREAD_CREATE, sys_thread_create);
    reg(NR_THREAD_SLEEP, sys_thread_sleep);
    reg(NR_CHANNEL_CREATE, sys_channel_create);
    reg(NR_CHANNEL_SEND, sys_channel_send);
    reg(NR_CHANNEL_RECV, sys_channel_recv);
    reg(NR_EVENT_CREATE, sys_event_create);
}
