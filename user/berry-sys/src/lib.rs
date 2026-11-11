//! # berry-sys —— BerryOS raw syscall 绑定（v1 FFI 镜像）
//!
//! ## Overview (for crates.io users)
//!
//! Raw syscall bindings for the [BerryOS](https://github.com/miuiadmin/BerryOS) microkernel — an
//! experimental, aarch64-first research OS. This crate mirrors the frozen
//! v1 syscall ABI: `#[repr(C)]` base types, generated constants (single
//! source: `tools/abi.toml` in the repo), and thin `svc #0` wrappers.
//! The raw layer never panics and never allocates; higher-level runtime
//! lives in berry-libc.
//!
//! Builds for **aarch64 targets only**; useful only on BerryOS or its
//! QEMU image. Status: experimental (0.1.0) — the API moves with the
//! kernel ABI. Repo docs are mostly zh-CN.
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
//! 数值事实与内核侧对账（2026-10，T-1 起 31 号在册 31 号已实现——0x35
//! `thread_start` 随多线程首刀落地（entry/sp/arg 四参，Zircon zx_thread_start
//! 同形）；0x33/0x36 已真实现且 `-> !` 不返，「调用者存活」对它们不成立）。

#![no_std]
#![allow(clippy::missing_safety_doc)] // FFI 边界：指针语义在 ABI 文档成文

use core::arch::asm;

// —— 基础类型（§8.4；宽度属 G2 冻结面：handle u32 / rights u64 / status i32 / time u64）——

pub type Handle = u32;
pub type Rights = u64;
pub type Status = i32; // 0 = Ok，负 = 错误码
pub type Time = u64; // MONOTONIC 纳秒

/// `berry_sched_attr`（调度 1.1 ABI §7.2，56B 定长）。
///
/// `size` 由调用方填 `size_of::<SchedAttr>()`（内核接受更大且尾部全零的
/// 前向兼容形态）；`version` 目前恒 1；`flags` 决定字段生效，未置位 = 继承。
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SchedAttr {
    pub size: u32,
    pub version: u16,
    pub flags: u16,
    pub weight: u32,
    pub policy: u8,
    pub intent: u8,
    pub freeze_clock: u8,
    pub reserved0: u8,
    pub min_ns: u64,
    pub max_ns: u64,
    pub deadline_ns: u64,
    pub budget_ns: u64,
    pub period_ns: u64,
}
const _: () = assert!(core::mem::size_of::<SchedAttr>() == 56);
const _: () = assert!(core::mem::align_of::<SchedAttr>() == 8);

/// `SchedAttr.flags`：字段生效位（未置位 = 继承；bit4 只对 Thread 有效）。
pub const SCHED_FLAG_WEIGHT: u16 = 1 << 0;
pub const SCHED_FLAG_INTENT: u16 = 1 << 1;
pub const SCHED_FLAG_POLICY: u16 = 1 << 2;
pub const SCHED_FLAG_FREEZE_CLOCK: u16 = 1 << 3;
pub const SCHED_FLAG_FREEZE_READY: u16 = 1 << 4;
pub const SCHED_FLAG_RESERVE: u16 = 1 << 5;
pub const SCHED_FLAG_DEADLINE: u16 = 1 << 6;
pub const SCHED_FLAG_BUDGET: u16 = 1 << 7;

/// `process_suspend(options)`：bit0 = 只冻结目标 Thread（需 FREEZE_READY）。
pub const SUSPEND_OPT_SINGLE_THREAD: u32 = 1 << 0;

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

/// `pager_supply_pages(vmo, src, src_off)`（0x21，G4-2 v2→v1 激活）：用户态
/// pager 给 demand VMO 供给一页——**阻塞收货**（无武装 EL0 缺页请求时睡
/// EL0 pager 键，fault 探针武装请求即递送）。门控：`vmo` 须 SET_POLICY
///（`VMO_OPT_DEMAND` 创建附加权——普通 dup 面不能冒充 pager）；`src` = 0
/// → 零页供给，否则须 Vmo+READ + `src_off` 页对齐界内 + 源页已提交
///（供给链零缺页）。出口恒唤 faulter：供给失败 = 其重试再 fault 再武装，
/// pager 重调本号自愈；不再调 = faulter 永久阻塞（fail loud-on-hang）。
#[no_mangle]
pub extern "C" fn berry_pager_supply_pages(vmo: Handle, src: Handle, src_off: u64) -> Status {
    unsafe { sys(nr::PAGER_SUPPLY_PAGES, vmo as usize, src as usize, src_off as usize, 0, 0, 0) }
}

/// `pager_supply_mapped(vmo, src, delta)`（0x23，G4-4 X-3，门 C 裁 A add-only）：
/// 0x21 的恒等选源变体——门控/阻塞收货/供给/唤醒链全同（见
/// `berry_pager_supply_pages`），唯源页偏不由调用方钉死、按**请求页现算**：
/// `src_off = 目标页偏 + delta`（页对齐 i64，可负——congruent ELF 段装载的
/// vaddr 偏置典型为负）。exec 装载 demand 化供给面：pager 无法预知下一缺页
/// 页位（0x21 impasse 根因），本号以常量 delta 换「任意页请求一次供给」。
#[no_mangle]
pub extern "C" fn berry_pager_supply_mapped(vmo: Handle, src: Handle, delta: i64) -> Status {
    unsafe { sys(nr::PAGER_SUPPLY_MAPPED, vmo as usize, src as usize, delta as usize, 0, 0, 0) }
}

/// `PagerWindow`（0x24 表批窗，X-6 刀① 冻结面——`X6批供提速设计-v1.0.md`
/// §2.1：布局一次定全即冻）：`dv` = 目标 demand VMO 句柄（门控
/// Vmo+SET_POLICY，同 0x23）；`sv` = 源 VMO 句柄（0 = 零页臂）；`delta` =
/// 恒等选源常数（页对齐 i64 可负——同 0x23）。repr(C) 16B/项，内核侧逐项
/// 小端拼装（`kernel/src/syscall.rs` 手写双镜像——§8.3 原型层纪律）。
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PagerWindow {
    pub dv: Handle,
    pub sv: Handle,
    pub delta: i64,
}
const _: () = assert!(core::mem::size_of::<PagerWindow>() == 16);
const _: () = assert!(core::mem::align_of::<PagerWindow>() == 8);

/// `pager_supply_mapped_table(tbl, n)`（0x24，X-6 刀① 批供表批）：0x23 的
/// 多窗一次收货形——`tbl` = PagerWindow 数组、`n` ∈ [1,32]。rc =
/// **count-wins**：>0 = 供给成功页数（部分成功部分计、不回滚；失败槽
/// faulter 恒唤自愈）；**0 供才报 errno**（-EINVAL = n 越界 / delta 非页
/// 对齐 / 聚合 NoneForVd；-EBADF = 门败）。阻塞二分同 0x23：池空且已
/// 供>0 → 即返计数；池空且 0 供 → 泊键阻塞醒后重扫表。boot 探测形
/// `tbl=[{dv:0,sv:0,delta:0}], n=1` → 新核 -EBADF / 旧核 -ENOTSUP。
#[no_mangle]
pub extern "C" fn berry_pager_supply_mapped_table(tbl: *const PagerWindow, n: u32) -> Status {
    unsafe { sys(nr::PAGER_SUPPLY_MAPPED_TABLE, tbl as usize, n as usize, 0, 0, 0, 0) }
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

/// 0x3B（C-4b）：外部终止进程（kill/Ctrl-C 内核原语）。`code` 落为该进程
/// 退出码（父侧收割同 process_exit 面：wait TERMINATED → get_exit_code）。
/// 门控 Process MANAGE；自杀 -EBADSTATE（退自己走 berry_process_exit）；
/// 已终止 -EBADSTATE。
#[no_mangle]
pub extern "C" fn berry_process_terminate(process: Handle, code: i32) -> Status {
    unsafe { sys(nr::PROCESS_TERMINATE, process as usize, code as usize, 0, 0, 0, 0) }
}

/// 调度 1.1（0x3E）：冻结 Process（options=0）或 Thread（bit0
/// `SINGLE_THREAD`，目标须先以 `SCHED_FLAG_FREEZE_READY` 自证）。出参为
/// SuspendToken；最后一个 token 经 `berry_handle_close` 关闭时解冻。
#[no_mangle]
pub extern "C" fn berry_process_suspend(
    target: Handle,
    options: u32,
    token: *mut Handle,
) -> Status {
    unsafe {
        sys(
            nr::PROCESS_SUSPEND,
            target as usize,
            options as usize,
            token as usize,
            0,
            0,
            0,
        )
    }
}

/// options 低 8 位 = berry_intent（ADR #42 占位；v1 内核只收 0）。
#[no_mangle]
pub extern "C" fn berry_thread_create(process: Handle, options: u32, out: *mut Handle) -> Status {
    unsafe { sys(nr::THREAD_CREATE, process as usize, options as usize, out as usize, 0, 0, 0) }
}

/// T-1 落地（多线程首刀）：投入运行**非首线程**（首线程随 `process_start`；
/// Zircon zx_thread_start 同形四参）。门控 Thread MANAGE；所属 Process 须已
/// start 且 Alive（否则 -EBADSTATE）；`arg` 落新线程 x0（与首线程 bootstrap
/// 迁移值对称——pthread 层递 (fn, arg) 的载体）；线程入口不得返回（返回 =
/// fault 杀兜底）。
#[no_mangle]
pub extern "C" fn berry_thread_start(thread: Handle, entry: usize, sp: usize, arg: usize) -> Status {
    unsafe { sys(nr::THREAD_START, thread as usize, entry, sp, arg, 0, 0) }
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

/// 调度 1.1（0x3C）：声明/更新调度属性。目标可为 Thread（覆盖）或 Process
///（该进程线程默认）；需 `RIGHT_SET_POLICY`。S1 存储面，S2/S5 消费。
#[no_mangle]
pub extern "C" fn berry_sched_setattr(target: Handle, attr: *const SchedAttr) -> Status {
    unsafe { sys(nr::SCHED_SETATTR, target as usize, attr as usize, 0, 0, 0, 0) }
}

/// 调度 1.1（0x3D）：查询 effective 调度属性。目标可为 Thread/Process；
/// 需 `RIGHT_INSPECT`。`attr.size` 入参 = 调用方缓冲长度。
#[no_mangle]
pub extern "C" fn berry_sched_getattr(target: Handle, attr: *mut SchedAttr) -> Status {
    unsafe { sys(nr::SCHED_GETATTR, target as usize, attr as usize, 0, 0, 0, 0) }
}

/// 调度 1.1（0x3F）：主动让出当前时间片；无 handle/参数；无可跑任务时立即
/// 返回 0。
#[no_mangle]
pub extern "C" fn berry_thread_yield() -> Status {
    unsafe { sys(nr::THREAD_YIELD, 0, 0, 0, 0, 0, 0) }
}

// —— 策略注入（0x83：机制在内核、策略可插拔的带内接口——表形面（COMMIT/
// SHADOW_COMMIT）+ STATUS 面逐 op 包装）——

/// 用户态策略表条目（16B——与内核 `PolicyEntryABI`（`kernel/src/syscall.rs`）
/// 逐字段一致，`#[repr(C)]` 镜像；内核侧按 LE 字节流手工解码，偏移即 ABI）。
///
/// `weight` 内核侧夹紧 ±64（包络执法，不信任注入方自律）；0 键/重复/溢出由
/// `bind_gen` first-wins 语义丢弃；`_rsv`/未定义 `epp_flags` 位非 0 → -EINVAL
/// （options 纪律，调用方可清零修复）；单次收表 ≤ 128 条（`TBL_MAX` 限流 →
/// -E2BIG，诚实拒收不静默截断）。
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PolicyEntry {
    pub key: u64,
    pub weight: i32,
    pub placement: u8,
    /// bit0 = `POLICY_EPP_HAS`（带 EPP 覆盖），bit1 = `POLICY_EPP_VAL`
    /// （0 = Efficiency / 1 = Perf）；bit2.. 未定义（非 0 → -EINVAL）。
    pub epp_flags: u8,
    pub _rsv: u16,
}
const _: () = assert!(core::mem::size_of::<PolicyEntry>() == 16);
const _: () = assert!(core::mem::align_of::<PolicyEntry>() == 8);
// C-2a 钉板：偏移逐字段钉死（对内核 copy_from_user 后 from_le_bytes 的
// 手工解码位对位——key@0/weight@8/placement@12/epp_flags@13/_rsv@14）。
const _: () = {
    use core::mem::offset_of;
    assert!(offset_of!(PolicyEntry, key) == 0);
    assert!(offset_of!(PolicyEntry, weight) == 8);
    assert!(offset_of!(PolicyEntry, placement) == 12);
    assert!(offset_of!(PolicyEntry, epp_flags) == 13);
    assert!(offset_of!(PolicyEntry, _rsv) == 14);
};

/// `PolicyEntry.epp_flags` bit0：本条带 EPP 覆盖（bit1 才有意义）。
/// 镜像内核 `PE_EPP_HAS`（其又镜像 `sched/policy.rs` 私有 EPP_HAS）。
pub const POLICY_EPP_HAS: u8 = 1 << 0;
/// `PolicyEntry.epp_flags` bit1：EPP 取值（0 = Efficiency / 1 = Perf）。
/// 镜像内核 `PE_EPP_VAL`。
pub const POLICY_EPP_VAL: u8 = 1 << 1;

/// op 词汇表（内核 0x83 dispatch 已扩至六臂，S6-e）：1 COMMIT / 2 STATUS /
/// 3 PROMOTE / 4 DEMOTE / 5 UNBIND / 6 SHADOW_COMMIT——本 crate 按消费面逐
/// op 包装（先 COMMIT/STATUS，其余随需求落）。
///
/// `op = 1`：COMMIT——整表提交为活跃策略快照（表形参走
/// [`berry_policy_bind`]；op=6 SHADOW_COMMIT 同形，比照对象换 shadow 谱系）。
pub const POLICY_OP_COMMIT: u64 = 1;

/// `op = 2`：STATUS——双谱系世代 + 调用者自身策略键一次读全（见
/// [`berry_policy_status`]）。
pub const POLICY_OP_STATUS: u64 = 2;

/// 0x83：v3 号提前实现（S6-b 最小 COMMIT 面）。abi.toml 在册但 stage=v3——
/// 常量层只产 v1/1.1 行（gen_abi.py defined 集），`nr::POLICY_BIND` 不存在，
/// 故本文件私有字面量封 svc（v3 常量层落地后换 `nr::` 常量名——同内核派发注）。
const POLICY_BIND_NR: usize = 0x83;

/// `berry_policy_bind(auth, op, tbl, count, gen, rsv)`（0x83 表形面：op=1
/// COMMIT / op=6 SHADOW_COMMIT）：用户态策略表注入（x1..x6 依次 = 六参）。
///
/// op=COMMIT：`tbl` = `&[PolicyEntry]` 首址、`count` = 条目数（0 = 空表提交
/// 合法——「接口在位、表空」）；内核 copy_from_user 整表拷入（无 TOCTOU）后
/// 逐条转换提交。`gen` = 世代数，**陈旧拒收**：gen ≤ 活跃世代 → -EAGAIN
/// （写者重放/乱序到达的旧表不得覆盖新表；活跃快照未动，调用方对账 gen 后
/// 携新 gen 重试）。v1 无 handle 需求（系统全局单通道；SET_POLICY 门控随
/// S6-c Job 对象落地）——`auth` 预留不消费，`rsv` 须 0（非 0 → -EINVAL）。
#[no_mangle]
pub extern "C" fn berry_policy_bind(
    auth: Handle,
    op: u64,
    tbl: *const PolicyEntry,
    count: u64,
    gen: u64,
    rsv: u64,
) -> Status {
    unsafe {
        sys(
            POLICY_BIND_NR,
            auth as usize,
            op as usize,
            tbl as usize,
            count as usize,
            gen as usize,
            rsv as usize,
        )
    }
}

/// `berry_policy_status(packed, self_key)`（0x83 op=2，S6-e STATUS 面）：
/// 双谱系世代 + 调用者自身策略键一次读全（写者对账/遥测）。`auth` 预留
/// 不消费（v1 全局单通道）；p4 保留恒 0（非 0 → -EINVAL，options 纪律）。
///
/// `packed`（u64 出参，**必填**——内核无条件写出，NULL → -EFAULT）：
/// bit 0-31 = active gen、bit 32-63 = shadow gen（各截低 32 位）——内核
/// 单读窗打包（双谱系无撕裂），位布局是 ABI 冻结面（内核 ktest SH-6 钉板）。
/// `self_key`（u64 出参，**可选**——NULL 跳过）= 调用者自身策略键：v1 key
/// 载体即 `Task::proc_tag`（内核态指针，用户态不可合成、只可经此查询），
/// 内核经 `current_tag` 与 apply/weight_of 消费点同源读取——表里给自己
/// 加权前先对账「我是几号键」；内核任务/boot 期 = 0（哨兵键——表侧本就
/// 丢弃，诚实透出不伪装）。
///
/// 内核 op=2 另有 **p3 出参**（divergence 对账遥测：bit 0-31 =
/// divergence_count、bit 32-63 = shadow_eval_count——A/B 评估面，内核
/// ktest DV-5 钉板、宿主 `tools/policy_ab_eval.py` 消费）；本 wrapper 尚
/// 未暴露（恒传 0 = NULL 跳过），用户态 A/B 评估器落地时扩参。
#[no_mangle]
pub extern "C" fn berry_policy_status(packed: *mut u64, self_key: *mut u64) -> Status {
    unsafe {
        sys(
            POLICY_BIND_NR,
            0, // auth：v1 全局单通道预留（同 berry_policy_bind 不消费）
            POLICY_OP_STATUS as usize,
            packed as usize,
            self_key as usize,
            0, // p3 保留
            0, // p4 保留（rsv）
        )
    }
}

// —— IPC / event ——

/// 一次建一对端点（两端均授 SEND|RECV|DUPLICATE）；出参先置 0 再写新值
/// （create 种子约定）。options：bit0 = `CHAN_OPT_NO_INTENT`，其余位 -EINVAL。
#[no_mangle]
pub extern "C" fn berry_channel_create(options: u32, out0: *mut Handle, out1: *mut Handle) -> Status {
    unsafe { sys(nr::CHANNEL_CREATE, options as usize, out0 as usize, out1 as usize, 0, 0, 0) }
}

/// v1 非阻塞：满 -EBUSY；nbytes ≤ 256 / nhandles ≤ 8（超 -EINVAL；A2-3
/// 6→8——kernel `CHAN_HND_MAX` 同源）；随行
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

// —— 信号（A7-4 段②，协同式用户态投递——libc `signal.rs` 消费）——

/// 向进程 pending 集位置位（幂等 = POSIX 标准信号合并）。`h=0` → 当前进程
/// （raise 面，零 handle 需求）；`h≠0` 门控 Process **MANAGE**。sig ∈ 1..=64
/// （else -EINVAL）；目标非 Alive -EBADSTATE。**不唤醒目标**——投递执行在
/// 用户态 libc（延迟档，A7-4 §2 方案 B）。
#[no_mangle]
pub extern "C" fn berry_signal_send(process: Handle, sig: u32) -> Status {
    unsafe { sys(nr::SIGNAL_SEND, process as usize, sig as usize, 0, 0, 0, 0) }
}

/// 原子取走当前进程 pending 集（swap(0) 语义——取后内核位清零）。`out`
/// = u64 位图（信号 n = 位 n-1）；出参写失败不消费。零 handle 面。
#[no_mangle]
pub extern "C" fn berry_signal_take(pending: *mut u64) -> Status {
    unsafe { sys(nr::SIGNAL_TAKE, pending as usize, 0, 0, 0, 0, 0) }
}
