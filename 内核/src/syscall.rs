//! 真 syscall 语义层（Phase 3-2：VMO/Vmar 全量门控，`系统调用ABI设计.md` §4/§8）。
//!
//! 分层：HAL [`syscall`]（`架构相关/aarch64/`）只做 nr 界检 + 派发 + 统一错误面；
//! 真语义 handler 在本模块，boot 期经 [`install`] 注册进 HAL 表（同 IRQ 槽反转：
//! HAL 不依赖内核 crate）。**3-1 政策**（撤 2b-4「EL0 畸形号杀」）：未知/未实现
//! 编号一律 `-ENOTSUP` 存活、绝不 panic；错误由用户态自理。
//!
//! v1 27 号编号全部在册（下方 NR_* 常量组）；3-2 实现 13 个——handle 三操作 +
//! clock_read（3-1）+ VMO/Vmar 九号（create/commit/decommit/read/write/map/
//! unmap/protect/allocate，全部经 [`HandleTable`] 能力门控——**权限在 handle
//! 不在对象**）。其余（wait/signal、process/thread、channel/event）留 3-3..3-4，
//! 未注册槽由 HAL 默认返 `-ENOTSUP`。
//!
//! **copy_from/to_user（3-2）**：EL1 调用方（ktest）→ 内核高半直映区直拷；
//! EL0 → 界检 `[0x1000, 2^39)` + 逐页走查**当前 TTBR0**（= 调用者自身表）验
//! AP 位，校验后直拷。**无 TOCTOU**：SVC 异常入口硬件置 DAIF 屏蔽 IRQ 且
//! handler 不解蔽 → syscall 主体天然不可抢占（单核）。
//!
//! **并发说明（v1 免锁的边界）**：引导表 [`BOOTSTRAP_HT`] 是全局单表（3-2 起
//! good 的 root Vmar 入表 = 首能力）；表的一切访问都在 syscall 同步异常路径
//! （IRQ 屏蔽，单核无并发）或 boot 期（IRQ 未开）。3-3 `process_create` 拆
//! per-process 表时本模块同步重构。

use crate::handle::{
    HandleTable, KernelObj, ObjType, Rights, ERR_BAD_HANDLE, ERR_BUSY, ERR_BAD_STATE, ERR_FAULT,
    ERR_INVALID_ARG, ERR_NOT_FOUND, ERR_NOT_SUPPORTED, ERR_NO_MEMORY, ERR_OUT_OF_RANGE,
    RIGHT_DUPLICATE, RIGHT_MANAGE, RIGHT_MAP, RIGHT_READ, RIGHT_WRITE,
};
use crate::vmar::{Perm, VmarError};
use crate::vmo::{Vmo, VmoError};
use bos_hal_aarch64::trap::TrapFrame;
use bos_hal_aarch64::KERNEL_VA_BASE;
use bos_mm::va::PAGE_SIZE;

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

/// 表未建的统一拒绝面（表 lazy init 只在 EL1 boot 路径触发——EL0 垃圾 handle
/// 不该有能力把堆带起来）。
fn table_or_ebadf() -> Result<&'static mut HandleTable, i64> {
    match table_peek() {
        Some(t) => Ok(t),
        None => Err(ERR_BAD_HANDLE as i64),
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
    match table_or_ebadf() {
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
    match table_or_ebadf() {
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
    match table_or_ebadf() {
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
    let Ok(t) = table_or_ebadf() else { return ERR_BAD_HANDLE as i64 };
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
    let Ok(t) = table_or_ebadf() else { return ERR_BAD_HANDLE as i64 };
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
    let Ok(t) = table_or_ebadf() else { return ERR_BAD_HANDLE as i64 };
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
    match table_or_ebadf() {
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
    match table_or_ebadf() {
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
    let Ok(t) = table_or_ebadf() else { return ERR_BAD_HANDLE as i64 };
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

// ---- boot 期注册（rust_main 在 ktest 之前调用；HAL install 见其 # Panics）----

/// 注册 3-2 已实现的 13 号（3-1 四号 + VMO/Vmar 九号）。未列号走 HAL 默认
/// `-ENOTSUP`（3-3..3-4 逐批增补）。
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
}
