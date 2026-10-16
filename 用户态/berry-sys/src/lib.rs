//! # berry-sys —— BerryOS raw syscall 绑定（v1 FFI 镜像）
//!
//! 合约源 = `文档/系统调用ABI设计.md` §8.3（原型）+ §8.4（本镜像的规范文本）。
//! 调用约定：ARM64 `SVC #0`，`x0` = syscall 号，`x1..x6` ≤ 6 参，返回 `x0`
//! （`Status`：0 = Ok，负 = 错误码）。**用户程序不直接写编号**——编号是
//! `nr` 模块私有常量，只在本文件封 `svc` 时出现（§8.3 头注）。
//!
//! 镜像纪律（§8.4 约定）：改任何签名 = 同时改 ABI 文档与内核侧；两端
//! `#[repr(C)]`/C-ABI 保证布局一致。v1 手写，IDL 单源生成是终态（欠债挂注）。
//!
//! 纯 raw 层：**绝不 panic、绝不分配**；上层 `berry` crate 负责
//! `Status → Result<(), Status>` 包装与运行时（console/argv 等随 G-eco-d）。
//!
//! 数值事实与内核侧对账（2026-10，24 号已实现；0x33/0x35/0x36 内核回落
//! `-ENOTSUP`，wrapper 照发——内核执法，调用者存活）。

#![no_std]
#![allow(clippy::missing_safety_doc)] // FFI 边界：指针语义在 ABI 文档成文

use core::arch::asm;

// —— 基础类型（§8.4；宽度属 G2 冻结面：handle u32 / rights u64 / status i32 / time u64）——

pub type Handle = u32;
pub type Rights = u64;
pub type Status = i32; // 0 = Ok，负 = 错误码
pub type Time = u64; // MONOTONIC 纳秒

pub const INVALID_HANDLE: Handle = 0;

// —— rights 位（镜像 内核/src/handle.rs：12 位，权限在 handle 不在对象）——

pub const RIGHT_READ: Rights = 1 << 0;
pub const RIGHT_WRITE: Rights = 1 << 1;
pub const RIGHT_EXECUTE: Rights = 1 << 2;
pub const RIGHT_MAP: Rights = 1 << 3;
pub const RIGHT_DUPLICATE: Rights = 1 << 4;
pub const RIGHT_TRANSFER: Rights = 1 << 5;
pub const RIGHT_INSPECT: Rights = 1 << 6;
pub const RIGHT_SIGNAL: Rights = 1 << 7;
pub const RIGHT_SEND: Rights = 1 << 8;
pub const RIGHT_RECV: Rights = 1 << 9;
pub const RIGHT_MANAGE: Rights = 1 << 10;
pub const RIGHT_SET_POLICY: Rights = 1 << 11;
/// duplicate 的「不裁剪」哨兵（内核同值 u64::MAX；实际授权仍受 handle 既有位约束）。
pub const RIGHTS_SAME: Rights = u64::MAX;

// —— wait 信号位（4-2 位空间：低 32 位内核派生 + 高 32 位用户）——

pub const SIGNAL_READABLE: u64 = 1 << 0; // Channel 派生：本端队列非空
pub const SIGNAL_WRITABLE: u64 = 1 << 1; // Channel 派生：对端队列未满
pub const SIGNAL_USER_MASK: u64 = 0xFFFF_FFFF_0000_0000; // Event/object_signal 专用

// —— options 位 ——

/// `channel_create`：opt-out 通道（置位端 recv 不结算 msg_intent；4-3c）。
pub const CHAN_OPT_NO_INTENT: u32 = 1 << 0;

/// `clock_read` 的 clock_id：v1 只有单调钟（壁钟留 v2）。
pub const CLOCK_MONOTONIC: u32 = 0;

// —— 错误码（数值 = G2 冻结面，镜像 内核/src/handle.rs ERR_*）——

pub const BERRY_ERR_NOT_FOUND: Status = -2;
pub const BERRY_ERR_BAD_HANDLE: Status = -9;
pub const BERRY_ERR_NO_MEMORY: Status = -12;
pub const BERRY_ERR_ACCESS_DENIED: Status = -13;
pub const BERRY_ERR_FAULT: Status = -14;
pub const BERRY_ERR_BUSY: Status = -16;
pub const BERRY_ERR_OUT_OF_RANGE: Status = -34;
pub const BERRY_ERR_BAD_STATE: Status = -37;
pub const BERRY_ERR_INVALID_ARG: Status = -22;
pub const BERRY_ERR_NOT_SUPPORTED: Status = -95;
pub const BERRY_ERR_AGAIN: Status = -11;
pub const BERRY_ERR_TIMEDOUT: Status = -110;

/// `Status` → 是否成功（raw 层不建 Result 类型，上层 `berry` crate 包）。
#[inline]
pub const fn is_ok(s: Status) -> bool {
    s == 0
}

// —— syscall 号（私有：用户程序不直接写编号，§8.3）——

mod nr {
    pub(super) const HANDLE_CLOSE: usize = 0x01;
    pub(super) const HANDLE_DUPLICATE: usize = 0x02;
    pub(super) const HANDLE_REPLACE: usize = 0x03;
    pub(super) const OBJECT_WAIT_ONE: usize = 0x04;
    pub(super) const OBJECT_WAIT_MANY: usize = 0x05;
    pub(super) const OBJECT_SIGNAL: usize = 0x06;

    pub(super) const VMO_CREATE: usize = 0x11;
    pub(super) const VMO_COMMIT: usize = 0x12;
    pub(super) const VMO_DECOMMIT: usize = 0x13;
    pub(super) const VMO_READ: usize = 0x14;
    pub(super) const VMO_WRITE: usize = 0x15;
    pub(super) const VMAR_MAP: usize = 0x16;
    pub(super) const VMAR_UNMAP: usize = 0x17;
    pub(super) const VMAR_PROTECT: usize = 0x18;
    pub(super) const VMAR_ALLOCATE: usize = 0x19;
    pub(super) const VMO_GET_SIZE: usize = 0x1A; // G-eco 入机侧第二刀（add-only）

    pub(super) const PROCESS_CREATE: usize = 0x31;
    pub(super) const PROCESS_START: usize = 0x32;
    pub(super) const PROCESS_EXIT: usize = 0x33; // 未实现（-ENOTSUP）
    pub(super) const THREAD_CREATE: usize = 0x34;
    pub(super) const THREAD_START: usize = 0x35; // 未实现（-ENOTSUP）
    pub(super) const THREAD_EXIT: usize = 0x36; // 未实现（-ENOTSUP）
    pub(super) const THREAD_SLEEP: usize = 0x37;

    pub(super) const CHANNEL_CREATE: usize = 0x51;
    pub(super) const CHANNEL_SEND: usize = 0x52;
    pub(super) const CHANNEL_RECV: usize = 0x53;
    pub(super) const EVENT_CREATE: usize = 0x54;

    pub(super) const CLOCK_READ: usize = 0x61;
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

/// ABI 形状 `-> !`（§8.4：不返回）。内核 0x33 v1 未实现（-ENOTSUP）——
/// 到达此分支即自旋（不 panic：raw 层纪律；实现落地后不可达）。
#[no_mangle]
pub extern "C" fn berry_process_exit(code: i32) -> ! {
    unsafe { sys(nr::PROCESS_EXIT, code as usize, 0, 0, 0, 0, 0) };
    loop {
        core::hint::spin_loop();
    }
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

/// ABI 形状 `-> !`；内核 0x36 v1 未实现（同 `process_exit` 自旋留桩）。
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
