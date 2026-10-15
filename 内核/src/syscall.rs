//! 真 syscall 语义层（Phase 3-4b：channel 三号——非阻塞消息 + handle 随消息
//! 迁移，`系统调用ABI设计.md` §4/§8）。
//!
//! 分层：HAL [`syscall`]（`架构相关/aarch64/`）只做 nr 界检 + 派发 + 统一错误面；
//! 真语义 handler 在本模块，boot 期经 [`install`] 注册进 HAL 表（同 IRQ 槽反转：
//! HAL 不依赖内核 crate）。**3-1 政策**（撤 2b-4「EL0 畸形号杀」）：未知/未实现
//! 编号一律 `-ENOTSUP` 存活、绝不 panic；错误由用户态自理。
//!
//! v1 27 号编号全部在册（下方 NR_* 常量组）；3-4b 实现 19 个——handle 三操作 +
//! clock_read（3-1）+ VMO/Vmar 九号（3-2）+ process_create/thread_create/
//! process_start（3-3）+ channel_create/send/recv（3-4b，全部经 [`HandleTable`]
//! 能力门控——**权限在 handle 不在对象**）。其余（wait/signal、process/thread
//! 退出与睡眠、event）留后续步，未注册槽由 HAL 默认返 `-ENOTSUP`。
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

use crate::channel::{Message, CHAN_HND_MAX, CHAN_MSG_MAX, ChannelEnd};
use crate::handle::{
    HandleTable, KernelObj, ObjRef, ObjType, Rights, ERR_ACCESS_DENIED, ERR_AGAIN, ERR_BAD_HANDLE,
    ERR_BUSY, ERR_BAD_STATE, ERR_FAULT, ERR_INVALID_ARG, ERR_NOT_FOUND, ERR_NOT_SUPPORTED,
    ERR_NO_MEMORY, ERR_OUT_OF_RANGE, RIGHT_DUPLICATE, RIGHT_MANAGE, RIGHT_MAP, RIGHT_READ,
    RIGHT_RECV, RIGHT_SEND, RIGHT_WRITE,
};
use crate::vmar::{Perm, Vmar, VmarError};
use crate::vmo::{Vmo, VmoError};
use alloc::boxed::Box;
use bos_hal_aarch64::trap::TrapFrame;
use bos_hal_aarch64::KERNEL_VA_BASE;
use bos_mm::va::PAGE_SIZE;
use core::ptr::NonNull;

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
#[allow(dead_code)] // 3-4c 落地
pub const NR_EVENT_CREATE: usize = 0x54;
// 时间（1）
pub const NR_CLOCK_READ: usize = 0x61;

/// `clock_read` 的 clock_id：v1 只有单调钟（`bos_time_t` = 纳秒；壁钟留 v2）。
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
        let tag = bos_sched::current_tag();
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

/// `vmo_create` 尺寸上限（v1）：页记账 Vec ≈ size/4096×10B，巨参会把堆分配
/// 推向 abort（分配失败 = 内核 panic——**EL0 可触发**，违 no-panic 纪律）；
/// 上限内 Vec ≤ 40KB 恒可分配。真机/堆扩大后上调（ABI 注记）。
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
        let l1 = bos_hal_aarch64::current_ttbr0_pa();
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
            Ok(()) => 0,
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

// ---- VMO / Vmar（3-2；rights 矩阵见各 handler 注释）----

/// VmoError → ABI 错误码（ABI §8.2）。
fn voe(e: VmoError) -> i32 {
    match e {
        VmoError::OutOfRange => ERR_OUT_OF_RANGE,
        VmoError::Unaligned => ERR_INVALID_ARG,
        VmoError::NotCommitted => ERR_BAD_STATE,
        VmoError::NoFrames => ERR_NO_MEMORY,
        VmoError::Mapped => ERR_BUSY,
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
    match caller_table(tf) {
        Ok(t) => match t.insert(
            KernelObj::Vmo(Vmo::create(size)),
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
/// （x1..x6）——C 原型 7 参（ABI §8.3），options 由 bos-sys stub 折叠（v1 恒 0，
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
    let vo = match t.get_raw(oh, ObjType::Vmo, RIGHT_MAP | RIGHT_WRITE) {
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
    let vmar = Vmar::new_root(crate::mm::frames(), crate::aspace::alloc_asid());
    let (ttbr0, asid) = (vmar.ttbr0_pa(), vmar.asid());
    let hv = match t.insert(
        KernelObj::Vmar(vmar),
        RIGHT_MAP | RIGHT_MANAGE | RIGHT_DUPLICATE,
    ) {
        Ok(h) => h,
        Err(e) => return e as i64, // 表满/gen 尽：Vmar 连 Box 泄漏（v1 不可达，安全向）
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
        Err(e) => e as i64, // 同上泄漏向（v1 不可达）
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
        Err(e) => e as i64, // Thread 已钉 Process；泄漏向同上（v1 不可达）
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
    thrd.started = true;
    // SAFETY: vr 在表（get_raw 过）；任务钉住（v1 永不释放，见上）。
    unsafe { (*vr.as_ptr()).retain() };
    let ht_ptr = &raw mut proc.ht as *mut HandleTable as usize;
    bos_sched::spawn_user(
        entry as usize,
        child_x0,
        sp as usize,
        b"user_child",
        proc.ttbr0_pa,
        proc.asid,
        ht_ptr,
    );
    0
}

// ---- channel（3-4b；对象 = `channel.rs`，端点互钉与环泄漏政策见其模块注）----

/// `channel_create(*out0, *out1)`：一次建一对端点（`channel.rs` 两端互指互钉
/// ——一端活则对端对象必活）。两端均授 **SEND|RECV|DUPLICATE**（对称权利；
/// TRANSFER 位同 DUPLICATE 的 vocab 政策：已定义不执法）。
extern "C" fn sys_channel_create(tf: &mut TrapFrame) -> i64 {
    let (out0, out1) = (tf.regs[1], tf.regs[2]);
    let el1 = caller_is_el1(tf);
    if let Err(e) = copy_out_u64(el1, out0, 0) {
        return e as i64;
    }
    if let Err(e) = copy_out_u64(el1, out1, 0) {
        return e as i64;
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let rights = RIGHT_SEND | RIGHT_RECV | RIGHT_DUPLICATE;
    let h0 = match t.insert(KernelObj::Channel(ChannelEnd::unpaired()), rights) {
        Ok(h) => h,
        Err(e) => return e as i64,
    };
    match t.insert(KernelObj::Channel(ChannelEnd::unpaired()), rights) {
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
            // 第二端入表失败（表满/gen 尽，v1 不可达）：回滚第一端——未配对端点
            // Drop 走 None 分支不碰任何指针（channel.rs），安全。
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
    // 对端指针 + 满检（对端与本端是不同对象——pair 互指保证，&mut 无别名）。
    // SAFETY: get_raw 已门控；对端由互钉保活（本端活 ⇒ 对端对象必在堆）；
    // 单核 IRQ 屏蔽无并发。
    let peer_ptr = unsafe {
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
            return ERR_BUSY as i64; // v1 非阻塞：满即返（等待排队留 P4 object_wait）
        }
        peer
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
    let msg = Message {
        bytes: Box::from(&bbuf[..nbytes]),
        handles: Box::from(&toks[..nhandles]),
    };
    // SAFETY: peer_ptr 互钉在活；队列有界（满检过，单核无并发插队）。
    unsafe {
        let Some(peer_end) = (*peer_ptr.as_ptr()).obj.as_channel_mut() else {
            return ERR_BAD_HANDLE as i64; // 上面刚解包过——不可达防御面（消息泄漏，安全向）
        };
        peer_end.push(msg);
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
            None => return ERR_AGAIN as i64, // v1 非阻塞：空即返（等待排队留 P4）
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
    if msg.bytes.len() > cap_bytes || msg.handles.len() > cap_hnd {
        // SAFETY: 同上。
        unsafe { recv_rollback(t, self_ptr, msg, &[]) };
        return ERR_INVALID_ARG as i64;
    }
    if let Err(e) = copy_to_user(el1, bytes, &msg.bytes) {
        // SAFETY: 同上。
        unsafe { recv_rollback(t, self_ptr, msg, &[]) };
        return e as i64;
    }
    // 令牌入调用者表（新值收集；任一失败全量回滚——表里已入的 take 回）。
    let mut newvals = [0u32; CHAN_HND_MAX];
    for i in 0..msg.handles.len() {
        match t.insert_ref(msg.handles[i].0, msg.handles[i].1) {
            Ok(nh) => newvals[i] = nh,
            Err(e) => {
                // SAFETY: 同上（msg 内令牌未动，回滚只需压回）。
                unsafe { recv_rollback(t, self_ptr, msg, &[]) };
                return e as i64;
            }
        }
    }
    let n = msg.handles.len();
    let mut obuf = [0u8; CHAN_HND_MAX * 4];
    for i in 0..n {
        obuf[i * 4..i * 4 + 4].copy_from_slice(&newvals[i].to_le_bytes());
    }
    if let Err(e) = copy_to_user(el1, handles, &obuf[..n * 4]) {
        // SAFETY: 同上（回滚含 take 回已入表的 n 个令牌）。
        unsafe { recv_rollback(t, self_ptr, msg, &newvals[..n]) };
        return e as i64;
    }
    if let Err(e) = copy_out_u64(el1, nbytesp, msg.bytes.len() as u64) {
        // SAFETY: 同上。
        unsafe { recv_rollback(t, self_ptr, msg, &newvals[..n]) };
        return e as i64;
    }
    if let Err(e) = copy_out_u64(el1, nhandlesp, n as u64) {
        // SAFETY: 同上。
        unsafe { recv_rollback(t, self_ptr, msg, &newvals[..n]) };
        return e as i64;
    }
    drop(msg); // 消费：令牌已入表（Message 的 Box 只释放数组内存，不碰 refs）
    0
}

// ---- boot 期注册（rust_main 在 ktest 之前调用；HAL install 见其 # Panics）----

/// 注册 3-4b 已实现的 19 号（3-1 四号 + VMO/Vmar 九号 + process/thread 三号 +
/// channel 三号；0x04–0x06 等待/信号留 3-4c，0x33/0x35..0x37 退出与睡眠随后续
/// 步）。未列号走 HAL 默认 `-ENOTSUP`。
pub fn install() {
    use bos_hal_aarch64::syscall::install as reg;
    reg(NR_HANDLE_CLOSE, sys_handle_close);
    reg(NR_HANDLE_DUPLICATE, sys_handle_duplicate);
    reg(NR_HANDLE_REPLACE, sys_handle_replace);
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
    reg(NR_CHANNEL_CREATE, sys_channel_create);
    reg(NR_CHANNEL_SEND, sys_channel_send);
    reg(NR_CHANNEL_RECV, sys_channel_recv);
}
