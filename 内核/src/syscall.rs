//! 真 syscall 语义层（Phase 3-1：ABI 骨架，`系统调用ABI设计.md` §4/§8）。
//!
//! 分层：HAL [`syscall`]（`架构相关/aarch64/`）只做 nr 界检 + 派发 + 统一错误面；
//! 真语义 handler 在本模块，boot 期经 [`install`] 注册进 HAL 表（同 IRQ 槽反转：
//! HAL 不依赖内核 crate）。**3-1 政策**（撤 2b-4「EL0 畸形号杀」）：未知/未实现
//! 编号一律 `-ENOTSUP` 存活、绝不 panic；错误由用户态自理。
//!
//! v1 27 号编号全部在册（下方 NR_* 常量组）；3-1 实现 4 个——handle_close/
//! duplicate/replace（经 3-0 [`HandleTable`] 门控）+ clock_read（MONOTONIC）。
//! 其余（wait/signal、VMO/Vmar、process/thread、channel/event）留 3-2..3-4，
//! 未注册槽由 HAL 默认返 `-ENOTSUP`。
//!
//! **并发说明（v1 免锁的边界）**：引导表 [`BOOTSTRAP_HT`] 的变更操作只发生在
//! EL1 ktest/boot（IRQ 未开，单线程）；EL0 路径（abuser 坏 handle）纯查表拒——
//! 3-1 无 EL0 可触的表变更。3-2 起 EL0 持真 handle，届时表随进程拆分 +
//! syscall 主体 preempt-disable（Zircon 同款），本模块同步重构。

use crate::handle::{
    HandleTable, Rights, ERR_BAD_HANDLE, ERR_INVALID_ARG, ERR_NOT_SUPPORTED,
};
use bos_hal_aarch64::trap::TrapFrame;
use bos_hal_aarch64::KERNEL_VA_BASE;

// ---- v1 27 号编号表（ABI §4「阶段 = v1」行；镜像 bos-sys）----

// handle / 通用对象（6）
pub const NR_HANDLE_CLOSE: usize = 0x01;
pub const NR_HANDLE_DUPLICATE: usize = 0x02;
pub const NR_HANDLE_REPLACE: usize = 0x03;
#[allow(dead_code)] // 3-4（object 等待/信号）落地
pub const NR_OBJECT_WAIT_ONE: usize = 0x04;
#[allow(dead_code)] // 同上
pub const NR_OBJECT_WAIT_MANY: usize = 0x05;
#[allow(dead_code)] // 同上
pub const NR_OBJECT_SIGNAL: usize = 0x06;
// VMO / Vmar（9）
#[allow(dead_code)] // 3-2 落地
pub const NR_VMO_CREATE: usize = 0x11;
#[allow(dead_code)] // 3-2
pub const NR_VMO_COMMIT: usize = 0x12;
#[allow(dead_code)] // 3-2（含对象销毁回收物理页——补 3-0 裁剪项）
pub const NR_VMO_DECOMMIT: usize = 0x13;
#[allow(dead_code)] // 3-2
pub const NR_VMO_READ: usize = 0x14;
#[allow(dead_code)] // 3-2
pub const NR_VMO_WRITE: usize = 0x15;
#[allow(dead_code)] // 3-2
pub const NR_VMAR_MAP: usize = 0x16;
#[allow(dead_code)] // 3-2
pub const NR_VMAR_UNMAP: usize = 0x17;
#[allow(dead_code)] // 3-2
pub const NR_VMAR_PROTECT: usize = 0x18;
#[allow(dead_code)] // 3-2
pub const NR_VMAR_ALLOCATE: usize = 0x19;
// 进程 / 线程（7）
#[allow(dead_code)] // 3-3 落地
pub const NR_PROCESS_CREATE: usize = 0x31;
#[allow(dead_code)] // 3-3
pub const NR_PROCESS_START: usize = 0x32;
#[allow(dead_code)] // 3-3
pub const NR_PROCESS_EXIT: usize = 0x33;
#[allow(dead_code)] // 3-3
pub const NR_THREAD_CREATE: usize = 0x34;
#[allow(dead_code)] // 3-3
pub const NR_THREAD_START: usize = 0x35;
#[allow(dead_code)] // 3-3
pub const NR_THREAD_EXIT: usize = 0x36;
#[allow(dead_code)] // 3-3
pub const NR_THREAD_SLEEP: usize = 0x37;
// IPC / event（4）
#[allow(dead_code)] // 3-4 落地
pub const NR_CHANNEL_CREATE: usize = 0x51;
#[allow(dead_code)] // 3-4（handle 迁移经此，能力模型 §4）
pub const NR_CHANNEL_SEND: usize = 0x52;
#[allow(dead_code)] // 3-4
pub const NR_CHANNEL_RECV: usize = 0x53;
#[allow(dead_code)] // 3-4
pub const NR_EVENT_CREATE: usize = 0x54;
// 时间（1）
pub const NR_CLOCK_READ: usize = 0x61;

/// `clock_read` 的 clock_id：v1 只有单调钟（`bos_time_t` = 纳秒；壁钟留 v2）。
const CLOCK_MONOTONIC: u64 = 0;

/// 用户 VA 合法区（Vmar root 覆盖 `[0x1000, 2^39)`，0 页 NULL 保卫——
/// `内存管理设计.md` §4；copy_out 的界检同源）。
const USER_VA_END: u64 = 1 << 39;

// ---- 引导 handle 表（3-1 全局单表雏形）----

/// 引导表：v1 预期它是 init 进程表的雏形（能力模型 §5「第一个能力从哪来」）；
/// 3-2 起挂到进程对象、随 `process_create` 拆分。lazy init（首次内核侧使用）。
static mut BOOTSTRAP_HT: Option<HandleTable> = None;

/// 取引导表（**仅 EL1 内核路径**：ktest/boot 插对象用；懒初始化需要堆）。
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

/// 只读窥表（handler 用）：表未建 = 一切 handle 无效（-EBADF），**不触发初始化**
/// （EL0 垃圾 handle 路径不碰堆）。
fn table_peek() -> Option<&'static mut HandleTable> {
    // SAFETY: 单核顺序访问；syscall 为同步异常路径、无嵌套（见模块并发说明）。
    unsafe { (*(&raw mut BOOTSTRAP_HT)).as_mut() }
}

// ---- out 参数回写（copy_out 种子；3-2 扩全量 copy_from/to_user）----

/// 调用方来自 EL1（ktest/内核自调）？SPSR[3:2] = 异常时 EL（EL0=0b00 / EL1=0b01）。
fn caller_is_el1(tf: &TrapFrame) -> bool {
    (tf.spsr >> 2) & 0x3 == 1
}

/// 写一个 u64 到调用方提供的 out 指针（ABI §8.3 各 `*out` 参数）。
///
/// 界检：用户区 `[0x1000, 2^39)`（经当前 TTBR0 = 调用者自身表——EL0 syscall
/// 时不切 AS，天然翻译调用者页）；内核高半区仅 EL1 调用方可用（**EL0 传内核
/// 指针一律拒**——防伪造内核写）。页未映射/只读 → 内核 fault（v1 可接受：
/// bos-sys 侧指针来自合法对象；3-2 copy_user 落地 fault-safe 修补路径）。
fn copy_out_u64(caller_el1: bool, ptr: u64, val: u64) -> Result<(), i32> {
    let ok = if (0x1000..USER_VA_END).contains(&ptr) {
        true
    } else {
        caller_el1 && ptr >= KERNEL_VA_BASE as u64
    };
    if !ok {
        return Err(ERR_INVALID_ARG);
    }
    // SAFETY: 界检通过（见上）；指针属于调用者可写区或（EL1）内核直映区。
    unsafe { core::ptr::write_volatile(ptr as *mut u64, val) };
    Ok(())
}

// ---- handlers（x0=号、x1..x6=参、x0=返回）----

/// `handle_close(h)`：关 handle（引用 -1，归零释放；代号 +1 旧值作废）。
extern "C" fn sys_handle_close(tf: &mut TrapFrame) -> i64 {
    let h = tf.regs[1] as u32;
    match table_peek() {
        Some(t) => match t.close(h) {
            Ok(()) => 0,
            Err(e) => e as i64,
        },
        None => ERR_BAD_HANDLE as i64, // 表未建：一切 handle 无效
    }
}

/// `handle_duplicate(h, rights, *out)`：复制（rights ⊆ 原、非空；SAME=全集）。
extern "C" fn sys_handle_duplicate(tf: &mut TrapFrame) -> i64 {
    let (h, rights, out) = (tf.regs[1] as u32, tf.regs[2] as Rights, tf.regs[3]);
    // 先验 out 指针（界检不过不浪费槽位/引用）。
    if let Err(e) = copy_out_u64(caller_is_el1(tf), out, 0) {
        return e as i64;
    }
    match table_peek() {
        Some(t) => match t.duplicate(h, rights) {
            Ok(nh) => match copy_out_u64(caller_is_el1(tf), out, nh as u64) {
                Ok(()) => 0,
                Err(e) => e as i64,
            },
            Err(e) => e as i64,
        },
        None => ERR_BAD_HANDLE as i64,
    }
}

/// `handle_replace(h, replacement, *out)`：原子换槽（两旧值均失效）。
extern "C" fn sys_handle_replace(tf: &mut TrapFrame) -> i64 {
    let (h, repl, out) = (tf.regs[1] as u32, tf.regs[2] as u32, tf.regs[3]);
    if let Err(e) = copy_out_u64(caller_is_el1(tf), out, 0) {
        return e as i64;
    }
    match table_peek() {
        Some(t) => match t.replace(h, repl) {
            Ok(nh) => match copy_out_u64(caller_is_el1(tf), out, nh as u64) {
                Ok(()) => 0,
                Err(e) => e as i64,
            },
            Err(e) => e as i64,
        },
        None => ERR_BAD_HANDLE as i64,
    }
}

/// `clock_read(clock_id, *out)`：单调钟纳秒（CNTVCT/CNTFRQ 换算；先商后余防
/// 中间溢出——ticks×1e9 在 ~5 分钟处溢 u64，拆两项后 ~584 年）。
extern "C" fn sys_clock_read(tf: &mut TrapFrame) -> i64 {
    if tf.regs[1] != CLOCK_MONOTONIC {
        return ERR_NOT_SUPPORTED as i64; // 壁钟等 id 留 v2
    }
    let out = tf.regs[2];
    let ticks = bos_hal_aarch64::timer::cntvct();
    let freq = bos_hal_aarch64::timer::cntfrq();
    if freq == 0 {
        return ERR_NOT_SUPPORTED as i64; // 频率不可读（真机异常）
    }
    let ns = (ticks / freq) * 1_000_000_000 + ((ticks % freq) * 1_000_000_000) / freq;
    match copy_out_u64(caller_is_el1(tf), out, ns) {
        Ok(()) => 0,
        Err(e) => e as i64,
    }
}

// ---- boot 期注册（rust_main 在 ktest 之前调用；HAL install 见其 # Panics）----

/// 注册 3-1 已实现的 4 号。未列号走 HAL 默认 `-ENOTSUP`（3-2..3-4 逐批增补）。
pub fn install() {
    use bos_hal_aarch64::syscall::install as reg;
    reg(NR_HANDLE_CLOSE, sys_handle_close);
    reg(NR_HANDLE_DUPLICATE, sys_handle_duplicate);
    reg(NR_HANDLE_REPLACE, sys_handle_replace);
    reg(NR_CLOCK_READ, sys_clock_read);
}
