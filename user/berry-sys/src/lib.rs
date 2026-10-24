//! # berry-sys —— BerryOS raw syscall 绑定（v1 FFI 镜像）
//!
//! 合约源 = `docs/系统调用ABI设计.md` §8.3（原型）+ §8.4（本镜像的规范文本）。
//! 调用约定：ARM64 `SVC #0`，`x0` = syscall 号，`x1..x6` ≤ 6 参，返回 `x0`
//! （`Status`：0 = Ok，负 = 错误码）。**用户程序不直接写编号**——编号是
//! `nr` 模块私有常量，只在本文件封 `svc` 时出现（§8.3 头注）。
//!
//! 镜像纪律（§8.4 约定）：改任何签名 = 同时改 ABI 文档与内核侧；两端
//! `#[repr(C)]`/C-ABI 保证布局一致。**常量层（号/位/码/宽度）已单源化**：
//! 本 crate 的 `abi_gen` 与内核 `kernel/src/abi_gen.rs` 同由 `tools/abi.toml`
//! 生成（`just abi-gen` 再生、`just abi-check` 守卫）；原型层（`berry_*`
//! 签名）仍手写双镜像——v2 挂账（见 ABI 文档 §8 引言）。
//!
//! 纯 raw 层：**绝不 panic、绝不分配**；上层 `berry` crate 负责
//! `Status → Result<(), Status>` 包装与运行时（console/argv 等随 G-eco-d）。
//!
//! 数值事实与内核侧对账（2026-10，P5 v0.5 起 29 号在册 28 号已实现——
//! 余 0x35 `thread_start`（v2 多线程）内核回落 `-ENOTSUP`，wrapper 照发；
//! 0x33/0x36 已真实现且 `-> !` 不返，「调用者存活」对它们不成立）。

#![no_std]
#![allow(clippy::missing_safety_doc)] // FFI 边界：指针语义在 ABI 文档成文

use core::arch::asm;

// —— 基础类型（§8.4；宽度属 G2 冻结面：handle u32 / rights u64 / status i32 / time u64）——

pub type Handle = u32;
pub type Rights = u64;
pub type Status = i32; // 0 = Ok，负 = 错误码
pub type Time = u64; // MONOTONIC 纳秒

// —— ABI 常量层（生成文件 `src/abi_gen.rs`，单源 = `tools/abi.toml`）——
// glob 转发维持既有路径（`berry_sys::RIGHT_*`/`BERRY_ERR_*` 等）；`nr` 模块
// 是 pub(crate)——glob 不带出，编号仍只在下方 wrapper 的 `svc` 里出现（§8.3）。
mod abi_gen;
pub use crate::abi_gen::*;
use crate::abi_gen::nr;

/// `Status` → 是否成功（raw 层不建 Result 类型，上层 `berry` crate 包）。
#[inline]
pub const fn is_ok(s: Status) -> bool {
    s == 0
}

/// SVC 封装：`x0` = 号（入）/返回值（出），`x1..x6` = 参。
/// `nostack`：SVC 不动用户 SP（内核侧自切栈，eret 原样恢复）。
#[inline]
unsafe fn sys(
    n: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
    a6: usize,
) -> Status {
    let ret: usize;
    // SAFETY: 调用方（下方 wrapper）保证参数 marshalling 与 ABI §8.3 一致；
    // svc 是 EL0 进入内核的唯一合法入口，无内存副作用发生在用户侧。
    unsafe {
        asm!(
            "svc #0",
            inlateout("x0") n => ret,
            in("x1") a1, in("x2") a2, in("x3") a3,
            in("x4") a4, in("x5") a5, in("x6") a6,
            options(nostack)
        );
    }
    ret as Status
}

// —— handle / 通用对象（§8.3「handle / 通用对象」段）——

#[no_mangle]
pub extern "C" fn berry_handle_close(h: Handle) -> Status {
    unsafe { sys(nr::HANDLE_CLOSE, h as usize, 0, 0, 0, 0, 0) }
}

/// rights ⊆ 原（dup 只紧不松；`RIGHTS_SAME` = 不裁剪）。
#[no_mangle]
pub extern "C" fn berry_handle_duplicate(h: Handle, rights: Rights, out: *mut Handle) -> Status {
    unsafe { sys(nr::HANDLE_DUPLICATE, h as usize, rights as usize, out as usize, 0, 0, 0) }
}

#[no_mangle]
pub extern "C" fn berry_handle_replace(h: Handle, replacement: Handle, out: *mut Handle) -> Status {
    unsafe { sys(nr::HANDLE_REPLACE, h as usize, replacement as usize, out as usize, 0, 0, 0) }
}

/// 4-1 阻塞化 wait：poll-first + 醒来重验；`*observed` = 全满足集（非请求子集）。
/// `deadline`：0 = poll（未命中 -EAGAIN）；非 0 = MONOTONIC 绝对时刻（到点
/// 未命中 -ETIMEDOUT）。等待不需权利位。
#[no_mangle]
pub extern "C" fn berry_object_wait_one(
    h: Handle,
    signals: u64,
    deadline: Time,
    observed: *mut u64,
) -> Status {
    unsafe {
        sys(
            nr::OBJECT_WAIT_ONE,
            h as usize,
            signals as usize,
            deadline as usize,
            observed as usize,
            0,
            0,
        )
    }
}

/// 4-2b 多对象等待：n ∈ [1,16]，任一命中出 `*idx`（入参序最前命中项）+
/// `*observed`（该对象全满足集）。数组先整段拷入内核再解析（无 TOCTOU）。
#[no_mangle]
pub extern "C" fn berry_object_wait_many(
    handles: *const Handle,
    signals: *const u64,
    n: usize,
    deadline: Time,
    idx: *mut usize,
    observed: *mut u64,
) -> Status {
    unsafe {
        sys(
            nr::OBJECT_WAIT_MANY,
            handles as usize,
            signals as usize,
            n,
            deadline as usize,
            idx as usize,
            observed as usize,
        )
    }
}

/// Event 信号面：`signals = (s & !clear) | set`；clear/set 须 ⊆ 用户半
/// （`SIGNAL_USER_MASK`），低 32 位内核派生 → -EINVAL。门控 RIGHT_SIGNAL。
#[no_mangle]
pub extern "C" fn berry_object_signal(h: Handle, clear: u64, set: u64) -> Status {
    unsafe { sys(nr::OBJECT_SIGNAL, h as usize, clear as usize, set as usize, 0, 0, 0) }
}

// —— VMO / Vmar ——
//
// 注：`vmar_map` 内核 6 参形态无 options 槽（§8.3 注：berry-sys stub 折叠），
// 本 wrapper 收 7 参后折叠——v1 options 须 0，非 0 在此拒 -ENOTSUP
// （语义 = 「v1 只支持 0」，与内核 rights 面缺档同口径）。

#[no_mangle]
pub extern "C" fn berry_vmo_create(size: u64, options: u32, out: *mut Handle) -> Status {
    unsafe { sys(nr::VMO_CREATE, size as usize, options as usize, out as usize, 0, 0, 0) }
}

#[no_mangle]
pub extern "C" fn berry_vmo_commit(vmo: Handle, offset: u64, len: u64) -> Status {
    unsafe { sys(nr::VMO_COMMIT, vmo as usize, offset as usize, len as usize, 0, 0, 0) }
}

/// 映射中页 -EBUSY（悬空 PTE 防线，先 unmap）。
#[no_mangle]
pub extern "C" fn berry_vmo_decommit(vmo: Handle, offset: u64, len: u64) -> Status {
    unsafe { sys(nr::VMO_DECOMMIT, vmo as usize, offset as usize, len as usize, 0, 0, 0) }
}

/// 内核中介读（rights 门控 READ；越界 -EINVAL/-FAULT 面）。
#[no_mangle]
pub extern "C" fn berry_vmo_read(vmo: Handle, buf: *mut u8, offset: u64, len: usize) -> Status {
    unsafe { sys(nr::VMO_READ, vmo as usize, buf as usize, offset as usize, len, 0, 0) }
}

#[no_mangle]
pub extern "C" fn berry_vmo_write(vmo: Handle, buf: *const u8, offset: u64, len: usize) -> Status {
    unsafe { sys(nr::VMO_WRITE, vmo as usize, buf as usize, offset as usize, len, 0, 0) }
}

/// VMO **页对齐容量**（add-only 0x1A，G-eco 入机侧第二刀）。注意非 blob
/// 实长——cpio 遍历以 newc 头自描述长度为准（尾零页 = 页垫）。
#[no_mangle]
pub extern "C" fn berry_vmo_get_size(vmo: Handle, out: *mut u64) -> Status {
    unsafe { sys(nr::VMO_GET_SIZE, vmo as usize, out as usize, 0, 0, 0, 0) }
}

/// 查字节物理地址（P6 A-3 用户态 DMA 面——virtqueue 描述符地址是 guest PA）：
/// `*pa = 所在页后备 PA + 页内偏移`（精确到字节，描述符 addr 直用）。匿名
/// VMO 该页须已 commit（-EBADSTATE）；MMIO 窗口 = pa_base+offset 恒可查。
/// 门控仅类型 need=0（get_size 同「几何非内容」惯例）；pa 是 u64 出参。
#[no_mangle]
pub extern "C" fn berry_vmo_get_phys(vmo: Handle, offset: u64, out: *mut u64) -> Status {
    unsafe { sys(nr::VMO_GET_PHYS, vmo as usize, offset as usize, out as usize, 0, 0, 0) }
}

/// v1 rights 须恰 READ|WRITE（UserRo 留 v2）；options 折叠（见段首注）。
#[no_mangle]
pub extern "C" fn berry_vmar_map(
    vmar: Handle,
    options: u32,
    vmo: Handle,
    vmo_offset: u64,
    vaddr: *mut usize,
    len: usize,
    rights: Rights,
) -> Status {
    if options != 0 {
        return BERRY_ERR_NOT_SUPPORTED;
    }
    unsafe {
        sys(
            nr::VMAR_MAP,
            vmar as usize,
            vmo as usize,
            vmo_offset as usize,
            vaddr as usize,
            len,
            rights as usize,
        )
    }
}

#[no_mangle]
pub extern "C" fn berry_vmar_unmap(vmar: Handle, vaddr: usize, len: usize) -> Status {
    unsafe { sys(nr::VMAR_UNMAP, vmar as usize, vaddr, len, 0, 0, 0) }
}

#[no_mangle]
pub extern "C" fn berry_vmar_protect(vmar: Handle, rights: Rights, vaddr: usize, len: usize) -> Status {
    unsafe { sys(nr::VMAR_PROTECT, vmar as usize, rights as usize, vaddr, len, 0, 0) }
}

#[no_mangle]
pub extern "C" fn berry_vmar_allocate(
    vmar: Handle,
    options: u32,
    vaddr: *mut usize,
    len: usize,
    child: *mut Handle,
) -> Status {
    unsafe {
        sys(
            nr::VMAR_ALLOCATE,
            vmar as usize,
            options as usize,
            vaddr as usize,
            len,
            child as usize,
            0,
        )
    }
}

// —— 进程 / 线程 ——

/// 双出参（`*process, *vmar`——Zircon 同形：父须拿子地址空间根才能装代码）。
/// job v1 恒 0（Job 对象 v2）。
#[no_mangle]
pub extern "C" fn berry_process_create(
    job: Handle,
    options: u32,
    process: *mut Handle,
    vmar: *mut Handle,
) -> Status {
    unsafe {
        sys(
            nr::PROCESS_CREATE,
            job as usize,
            options as usize,
            process as usize,
            vmar as usize,
            0,
            0,
        )
    }
}

/// bootstrap 非 0 时从调用者表 take 迁入子表（原值立即 -EBADF），子入口
/// `x0` = 子表内新 handle 值。
#[no_mangle]
pub extern "C" fn berry_process_start(
    process: Handle,
    thread: Handle,
    entry: usize,
    sp: usize,
    bootstrap: Handle,
) -> Status {
    unsafe {
        sys(
            nr::PROCESS_START,
            process as usize,
            thread as usize,
            entry,
            sp,
            bootstrap as usize,
            0,
        )
    }
}

/// ABI 形状 `-> !`（§8.4：不返回）。内核 0x33 已随 Plan B v0.5 实现：置
/// Process Exited + 唤醒 TERMINATED 等待者 + drain 子表，任务不返。svc 后
/// 的自旋仅防御面（内核执法正确即不可达；raw 层不 panic）。
#[no_mangle]
pub extern "C" fn berry_process_exit(code: i32) -> ! {
    unsafe { sys(nr::PROCESS_EXIT, code as usize, 0, 0, 0, 0, 0) };
    loop {
        core::hint::spin_loop();
    }
}

/// 0x3A（P5 shell v0 收官）：查进程退出码。未退出 → -EAGAIN（poll 语义——
/// 调用方先 `berry_object_wait_one(SIGNAL_TERMINATED)` 再查）。`*code` 预清
/// 由内核出参路径保证；code 宽 = i32（4B，宽度跟 ABI 类型走）。
#[no_mangle]
pub extern "C" fn berry_process_get_exit_code(process: Handle, code: *mut i32) -> Status {
    unsafe { sys(nr::PROCESS_GET_EXIT_CODE, process as usize, code as usize, 0, 0, 0, 0) }
}

/// options 低 8 位 = berry_intent（ADR #42 占位；v1 内核只收 0）。
#[no_mangle]
pub extern "C" fn berry_thread_create(process: Handle, options: u32, out: *mut Handle) -> Status {
    unsafe { sys(nr::THREAD_CREATE, process as usize, options as usize, out as usize, 0, 0, 0) }
}

/// 未实现（内核 -ENOTSUP）——ABI 形状照发。
#[no_mangle]
pub extern "C" fn berry_thread_start(thread: Handle) -> Status {
    unsafe { sys(nr::THREAD_START, thread as usize, 0, 0, 0, 0, 0) }
}

/// ABI 形状 `-> !`；内核 0x36 已随 Plan B v0.5 实现（v1 单线程 ⇒ 等价
/// process_exit(0)，Zircon last-thread 语义——任务不返）。自旋同上仅防御。
#[no_mangle]
pub extern "C" fn berry_thread_exit() -> ! {
    unsafe { sys(nr::THREAD_EXIT, 0, 0, 0, 0, 0, 0) };
    loop {
        core::hint::spin_loop();
    }
}

/// deadline = 绝对 MONOTONIC 纳秒（与 clock_read 对称；已过点立即返 0）。
#[no_mangle]
pub extern "C" fn berry_thread_sleep(deadline: Time) -> Status {
    unsafe { sys(nr::THREAD_SLEEP, deadline as usize, 0, 0, 0, 0, 0) }
}

// —— IPC / event ——

/// 一次建一对端点（两端均授 SEND|RECV|DUPLICATE）；出参先置 0 再写新值
/// （create 种子约定）。options：bit0 = `CHAN_OPT_NO_INTENT`，其余位 -EINVAL。
#[no_mangle]
pub extern "C" fn berry_channel_create(options: u32, out0: *mut Handle, out1: *mut Handle) -> Status {
    unsafe { sys(nr::CHANNEL_CREATE, options as usize, out0 as usize, out1 as usize, 0, 0, 0) }
}

/// v1 非阻塞：满 -EBUSY；nbytes ≤ 256 / nhandles ≤ 4（超 -EINVAL）；随行
/// handle 逐个迁出调用者表（原值立即 -EBADF）；数组内重复值 -EINVAL；
/// 任一步失败 = 调用者表零变化。
#[no_mangle]
pub extern "C" fn berry_channel_send(
    ch: Handle,
    bytes: *const u8,
    nbytes: usize,
    handles: *const Handle,
    nhandles: usize,
) -> Status {
    unsafe {
        sys(
            nr::CHANNEL_SEND,
            ch as usize,
            bytes as usize,
            nbytes,
            handles as usize,
            nhandles,
            0,
        )
    }
}

/// v1 非阻塞：空 -EAGAIN；`*nbytes`/`*nhandles` in-out（进 = 容量、出 =
/// 实际值；容量不足 -EINVAL 且消息保原样）；随行令牌入调用者表。
#[no_mangle]
pub extern "C" fn berry_channel_recv(
    ch: Handle,
    bytes: *mut u8,
    nbytes: *mut usize,
    handles: *mut Handle,
    nhandles: *mut usize,
) -> Status {
    unsafe {
        sys(
            nr::CHANNEL_RECV,
            ch as usize,
            bytes as usize,
            nbytes as usize,
            handles as usize,
            nhandles as usize,
            0,
        )
    }
}

/// v1：options 须 0（非 0 -EINVAL）；授 SIGNAL|DUPLICATE；出生信号全零。
#[no_mangle]
pub extern "C" fn berry_event_create(options: u32, out: *mut Handle) -> Status {
    unsafe { sys(nr::EVENT_CREATE, options as usize, out as usize, 0, 0, 0, 0) }
}

// —— 时间 ——

/// 单调钟纳秒（`clock_id` 须 `CLOCK_MONOTONIC`）。
#[no_mangle]
pub extern "C" fn berry_clock_read(clock_id: u32, out: *mut Time) -> Status {
    unsafe { sys(nr::CLOCK_READ, clock_id as usize, out as usize, 0, 0, 0, 0) }
}
