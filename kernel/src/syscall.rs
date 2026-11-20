//! 真 syscall 语义层（Phase 3-4b：channel 三号——非阻塞消息 + handle 随消息
//! 迁移，`docs/系统调用ABI设计-v1.0.md` §4/§8）。
//!
//! 分层：HAL [`syscall`]（`arch/aarch64/`）只做 nr 界检 + 派发 + 统一错误面；
//! 真语义 handler 在本模块，boot 期经 [`install`] 注册进 HAL 表（同 IRQ 槽反转：
//! HAL 不依赖内核 crate）。**3-1 政策**（撤 2b-4「EL0 畸形号杀」）：未知/未实现
//! 编号一律 `-ENOTSUP` 存活、绝不 panic；错误由用户态自理。
//!
//! v1 40 号编号全部在册（`abi_gen` 单源生成、经 `pub use` 转发——`tools/abi.toml`）；**39 号已实现（唯 0x85 `berry_attest` 预留位恒 `-ENOTSUP`——T-1 后在册真语义全实现 + A7-4 信号两号 + G4-2 0x21 pager 首形 + G4-4 X-3b 0x23 恒等选源 + X-6 刀① 0x24 批供表批 + 桌面UX刀D 0x88 power_off + CP-08 0x40 util 快照读 + cbox 刀1 0x42 排液〔跨根并回〕）**——
//! handle 三操作 + clock_read（3-1）+ VMO/Vmar 十一号（3-2 + G-eco get_size +
//! P6 get_phys 用户态 DMA 面）+
//! process_create/start/exit + process_get_exit_code（P5——`&&` 真退出码源）+
//! process_terminate（C-4b——外部终止，kill/Ctrl-C 内核原语）+
//! thread_create/exit（3-3 + Plan B 退出路径）+
//! thread_start（T-1 多线程首刀——四参扩形 entry/sp/arg，Zircon zx_thread_start
//! 同形；非首线程投入运行 + Thread TERMINATED join 面 + 末线程进程收口）+
//! channel 三号（3-4b）+ event 三号（3-4c；4-1 wait_one 阻塞化）+ wait_many
//! （4-2b）+ thread_sleep（4-0，首个阻塞 syscall），全部经 [`HandleTable`]
//! 能力门控——**权限在 handle 不在对象**。未注册槽由 HAL 默认返
//! `-ENOTSUP`。**任务退出钩子**（Plan B → T-1 线程版）：任一任务
//! 退出（含 fault 杀，码 0）经 [`task_exit_hook`] 做 Thread 簿记（join 唤醒）+
//! 活线程计数——末线程再 Process 收口（Exited + 唤醒 TERMINATED 同步 +
//! drain 子表异步移交 HT_DRAINQ + release 双钉住——P2-2）——见下方 hook
//! 注释。
//!
//! **copy_from/to_user（3-2）**：EL1 调用方（ktest）→ 内核高半直映区直拷；
//! EL0 → 界检 `[0x1000, 2^39)` + 逐页走查**当前 TTBR0**（= 调用者自身表）验
//! AP 位，校验后直拷。**无 TOCTOU**：SVC 异常入口硬件置 DAIF 屏蔽 IRQ，
//! handler 多数号不解蔽（唯 send/recv 拷贝段局部解蔽——窗内表零触达 + 段
//! 计数 >0 禁切，见 `irq_window`）→ syscall 主体天然不可抢占（单核）。
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
    RIGHT_TRANSFER, RIGHT_INSPECT,
    RIGHT_READ, RIGHT_RECV, RIGHT_SEND, RIGHT_SIGNAL, RIGHT_WRITE, SIGNAL_READABLE,
    SIGNAL_TERMINATED, SIGNAL_WRITABLE,
};
use crate::process::ProcState;
use crate::vmar::{Perm, Vmar, VmarError};
use crate::vmo::{Vmo, VmoError};
use berry_hal_aarch64::trap::TrapFrame;
use berry_hal_aarch64::KERNEL_VA_BASE;
use berry_mm::va::PAGE_SIZE;
use core::ptr::NonNull;

// ---- v1 37 号编号表（ABI §4「阶段 = v1」行）——单源生成 ----
// 常量本体在 `crate::abi_gen`（`tools/abi.toml` 生成文件）；此处 re-export
// 维持既有路径（ktest/main 直接写 `crate::syscall::NR_*`）。改号/加号先改
// abi.toml 再 `just abi-gen`——`gen_abi.py --check` 守卫挂 build/smoke 前置。
// T-1 起 31 号全注册（0x35 随 sys_thread_start 消费）——豁免保留为冻结位
// 纪律（v2 批再入册未同步实现的号仍随转发触发 unused_imports）；G4-2 起
// 0x21 pager_supply_pages 激活（v2 号 → v1，add-only）。
#[allow(unused_imports)]
pub use crate::abi_gen::{
    CHAN_OPT_NO_INTENT, NR_CHANNEL_CREATE, NR_CHANNEL_RECV, NR_CHANNEL_RECV_N, NR_CHANNEL_SEND,
    NR_CLOCK_READ,
    NR_CBOX_DRAIN, NR_EVENT_CREATE, NR_HANDLE_CLOSE, NR_HANDLE_DUPLICATE, NR_HANDLE_REPLACE, NR_OBJECT_SIGNAL,
    NR_OBJECT_WAIT_MANY, NR_OBJECT_WAIT_ONE, NR_PAGER_SUPPLY_MAPPED, NR_PAGER_SUPPLY_MAPPED_TABLE,
    NR_PAGER_SUPPLY_PAGES, NR_PROCESS_CREATE,
    NR_PROCESS_EXIT, NR_PROCESS_GET_EXIT_CODE, NR_PROCESS_START, NR_PROCESS_SUSPEND,
    NR_SCHED_GETATTR, NR_SCHED_SETATTR, NR_SCHED_UTIL_SNAPSHOT, NR_THREAD_YIELD,
    NR_PROCESS_TERMINATE,
    NR_POWER_OFF, NR_SIGNAL_SEND, NR_SIGNAL_TAKE, NR_THREAD_CREATE, NR_THREAD_EXIT,
    NR_THREAD_SLEEP,
    NR_THREAD_START, NR_VMAR_ALLOCATE, NR_VMAR_MAP,
    NR_VMAR_PROTECT, NR_VMAR_UNMAP, NR_VMO_COMMIT, NR_VMO_CREATE, NR_VMO_DECOMMIT,
    NR_VMO_GET_PHYS, NR_VMO_GET_SIZE, NR_VMO_READ, NR_VMO_WRITE, NR_VMO_CACHE_OP, VMO_CACHE_OP_CLEAN, VMO_CACHE_OP_INVALIDATE, VMO_CACHE_OP_CLEAN_INVALIDATE, VMO_OPT_CONTIGUOUS, VMO_OPT_PINNED};
use crate::abi_gen::{CLOCK_MONOTONIC, CLOCK_REALTIME, ERR_E2BIG, RIGHT_SET_POLICY, VMO_OPT_DEMAND};

/// 用户 VA 合法区（Vmar root 覆盖 `[0x1000, 2^39)`，0 页 NULL 保卫——
/// `内存管理设计.md` §4；copy_out 的界检同源）。W3-Q70：值域常量单源迁
/// berry-aokface，此处 re-export 对齐（消费点引用不变）。
const USER_VA_END: u64 = berry_aokface::USER_VA_END;

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

/// 调用方 handle 表派发（3-4a；4-5 前置补丁：tag 优先，与 pi::holder_table 同源）：
/// - **tag ≠ 0**（EL0 所属进程表 / EL1 `spawn_with_tag` 常驻内核服务私有表）→
///   `proc_tag` 指向的 leak 常驻表（`spawn_user`/`spawn_with_tag` 时注入）；
/// - **EL1 + tag == 0**（ktest/内核自调/共享表短命任务）→ 内核侧表 [`BOOTSTRAP_HT`]；
/// - **EL0 + tag == 0** → 一切 handle 无效。
///
/// SAFETY（裸指针的三个不变量）：① tag 来源只有三处——boot 期 `Box::leak` 的
/// 常驻表（main.rs 手搭用户进程 + `spawn_with_tag` 的常驻内核服务）与
/// `Process::ht`（Box'd ObjRef 内、地址稳定，且运行中的任务钉住 Process →
/// 表随任务同寿）；② 单核 syscall 多数号 IRQ 屏蔽（唯 send/recv 拷贝窗局部
/// 解蔽——窗不触表且段计数 >0 禁切，见 `irq_window`）→ 同一表无并发访问；
/// ③ 每个 handler 同一时刻至多持一张表的 `&mut`（跨表操作如 bootstrap 迁移
/// 是**两张不同表**的借用，无别名）。
fn caller_table(tf: &TrapFrame) -> Result<&'static mut HandleTable, i64> {
    // tag 优先（与 pi::holder_table 同派发）：EL1 常驻内核服务（spawn_with_tag
    // 挂私有表）→ 其表；tag==0（ktest/boot 期/共享表短命任务）→ 内核侧表。
    // ktest 在 sched::init 前跑 → current_tag()==0 → 落共享表，既有 EL1 面不变。
    let tag = berry_sched::current_tag();
    if caller_is_el1(tf) {
        if tag != 0 {
            // SAFETY: 见函数注释三不变量。
            Ok(unsafe { &mut *(tag as *mut HandleTable) })
        } else {
            Ok(bootstrap_table())
        }
    } else if tag == 0 {
        Err(ERR_BAD_HANDLE as i64) // 无表任务：没有 handle 可言
    } else {
        // SAFETY: 见函数注释三不变量。
        Ok(unsafe { &mut *(tag as *mut HandleTable) })
    }
}

// ---- copy_from/to_user（3-2 全量；copy_out_u64 为 out 参数种子保留）----

/// 调用方来自 EL1（ktest/内核自调）？SPSR[3:2] = 异常时 EL（EL0=0b00 / EL1=0b01）。
fn caller_is_el1(tf: &TrapFrame) -> bool {
    (tf.spsr >> 2) & 0x3 == 1
}

/// `vmo_create` 尺寸上限（v1）：页记账 ≈ **18B/页**（pages = `Option<usize>`
/// 无 niche 占 16B + mapcnt u16 2B——旧注释 10B/页 ≤40KB 系算术错，3b3 H2 勘
/// 误），128MiB 上限对应 32768 页 ≈ 576KiB 记账。上限只拦巨参滥用；分配本身已
/// try 化（`Vmo::create` 折 -ENOMEM——上限内 OOM 也不再 abort）。**A1-2 帽分家**
/// （`docs/容量工程前置-A1设计草案-v1.0.md` §3.1）：C-2c 曾与 `ROOTFS_MAX`/`MAX_ORDER`
/// 同口径 16→32MiB（rootfs 装载同因）；大 blob 装载形态迁 VMO 逐页 commit 后帽
/// 独立抬至 128MiB（逐页无堆单块地形压力——A5/A6 的 80-120MB 静态面），
/// `MAX_ORDER` 不再联动。
/// bench S-2（`docs/基准与压力测试设计-v1.0.md` §12）引用本常量做超帽/边界断言——
/// 帽值单一事实源，勿在 bench 侧另写裸数字。
pub(crate) const VMO_MAX_SIZE: usize = 128 * 1024 * 1024;

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
    if ptr.checked_add(len).is_none() {
        return false; // 回卷拒（原判序在 EL1/EL0 分派之前——两路共门）
    }
    if caller_el1 {
        // W3-Q70：EL1 高半单比较判定律单源（berry-aokface——el1_span_ok
        // 自带回卷门，此处前置共门后幂等零漂移）。
        berry_aokface::el1_span_ok(ptr, len, KERNEL_VA_BASE as u64)
    } else {
        // W3-Q70：EL0 值域四门判定律单源（berry-aokface——len==0 恒过/
        // checked_add 回卷拒/null 页下界/USER_VA_END 上界）；逐页走查
        // 物理臂留壳（walk_leaf 读 TTBR0 三级表）。
        if !berry_aokface::el0_span_ok(ptr, len) {
            return false;
        }
        let l1 = berry_hal_aarch64::current_ttbr0_pa();
        for va in berry_aokface::span_pages(ptr, len) {
            match crate::vmar::walk_leaf(l1, va as usize) {
                // W3-Q70：AP 位域读写拒律单源（berry-aokface::leaf_admits）。
                Some(desc) if berry_aokface::leaf_admits(desc, write) => {}
                _ => return false, // 缺叶或权限不符
            }
        }
        true
    }
}

/// 窗内同页缓存哨兵：用户 VA < 2^39、EL1 高半直映 ≥ KERNEL_VA_BASE，
/// u64::MAX 两界之外——首块必走查。W3-Q70：哨兵单源迁 berry-aokface，
/// 此处 re-export 对齐（消费点引用不变）。
const SAME_PAGE_NONE: u64 = berry_aokface::SAME_PAGE_NONE;

/// **单一 PT 锁窗内**的逐页校验（P1 同页跳过）。`last_page` = 本窗局部
/// 缓存：块覆盖的每个页若 == 缓存页，该页已在本窗走查过且写者（他核
/// map/unmap/protect 须取同 AS PT 锁）被本窗挡在表外 ⇒ 表冻结 ⇒ 免重复
/// 三级走查；新页按**本块在该页内的实际字节段**走 [`access_ok`]（边界
/// 语义与整段调用逐位同源）。逐页咨询而非只比块首页：非页对齐缓冲的
/// 256B 块可跨页界，只比首页会漏验跨界页（未映射页直拷 = 内核 fault）。
/// 缓存生命周期**严格 = 锁窗**：跨窗/跨调用复用即绕过 §A113 walk+store
/// 同窗铁律（他核 unmap 撤页 TOCTOU）——故只经调用方局部变量传递，
/// **禁提升 static**。
fn access_ok_window(
    caller_el1: bool,
    ptr: u64,
    len: u64,
    write: bool,
    last_page: &mut u64,
) -> bool {
    if len == 0 {
        return true;
    }
    // W3-Q70：同页跳过律单源（berry-aokface::window_should_walk——
    // SAME_PAGE_NONE 哨兵首块必查；缓存只记最近页）。页序同源 span_pages。
    for va in berry_aokface::span_pages(ptr, len) {
        if berry_aokface::window_should_walk(va, &mut *last_page) {
            // 本块在本页内的字节段 [max(va, ptr), min(页尾, 块尾))。
            let start = va.max(ptr);
            let end = (va + PAGE_SIZE as u64).min(ptr + len);
            if !access_ok(caller_el1, start, end - start, write) {
                return false;
            }
        }
    }
    true
}

/// 窗内校验 + 拷贝：调用方**已持**本 AS PT 锁（EL1 路免锁高半直映）。
/// 分块块流（`sys_vmo_read/write` 的 256B 块）在外层单窗内传递同一
/// `last_page` 复用本函数——同页块免重复走查（64KB = 256 块：原每块
/// 一窗一走查 256 次 → 每页 1 次 = 16 次）。
fn copy_from_user_locked(
    caller_el1: bool,
    src: u64,
    dst: &mut [u8],
    last_page: &mut u64,
) -> Result<(), i32> {
    if dst.is_empty() {
        return Ok(());
    }
    if !access_ok_window(caller_el1, src, dst.len() as u64, false, last_page) {
        return Err(ERR_FAULT);
    }
    // SAFETY: 整段界检过（EL1 = 高半直映；EL0 = 调用者表逐页 AP 验过且
    // 同窗持锁，见 [`access_ok_window`]）；无本核任务切换切进校验-拷贝间
    // ——g3-1c-2 起双面：全屏蔽窗号（多数——屏蔽即无切换）；管道拷贝段
    // 局部解蔽窗（send/recv——段级 preempt_disable 保无切换 §A113，窗在
    // 调用点非此处内置，见 `irq_window`——内置对 TABLE 类号（vmo 面）还是
    // tick 面活体违约：SQ 位不在集 ⇒ tick 入窗 expire_timeouts 物理取 SQ
    // 于 KL 下 = §A126 同款七类锁序逆序，不只是切换面）。
    unsafe { core::ptr::copy_nonoverlapping(src as *const u8, dst.as_mut_ptr(), dst.len()) };
    Ok(())
}

/// 从调用方缓冲拷进内核切片（`vmo_write` 源 / in 参数读）。单发整段拷贝
/// （自带 PT 锁窗 + 全段走查）。**256B 分块块流勿逐块调本函数**——每块
/// 重取锁 + 重走查（同页 16 块 = 16 次三级走查，P1 瓶颈）——改由外层持
/// 单窗循环调 [`copy_from_user_locked`]（同页跳过）。
fn copy_from_user(caller_el1: bool, src: u64, dst: &mut [u8]) -> Result<(), i32> {
    if !dst.is_empty() {
        // S-4 4-2（判例 §A113 walk+copy 同窗铁律）：EL0 路走查与 store 同一
        // per-AS PT 锁窗——封死他核 unmap/protect 在两步之间撤页的 TOCTOU
        // （原「单核无 TOCTOU」假设随 SMP 退役）。EL1 路高半直映不取。
        let _pt = if caller_el1 { None } else { Some(crate::vmar::pt_lock_current()) };
        let mut last_page = SAME_PAGE_NONE;
        copy_from_user_locked(caller_el1, src, dst, &mut last_page)?;
    }
    Ok(())
}

/// 窗内校验 + 拷出（同 [`copy_from_user_locked`]——调用方已持锁，同页
/// 块免重复走查）。
fn copy_to_user_locked(
    caller_el1: bool,
    dst: u64,
    src: &[u8],
    last_page: &mut u64,
) -> Result<(), i32> {
    if src.is_empty() {
        return Ok(());
    }
    if !access_ok_window(caller_el1, dst, src.len() as u64, true, last_page) {
        return Err(ERR_FAULT);
    }
    // SAFETY: 同 copy_from_user_locked。
    unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst as *mut u8, src.len()) };
    Ok(())
}

/// 从内核切片拷出到调用方缓冲（`vmo_read` 目的 / out 参数写）。单发整段
/// 拷贝；分块块流同 [`copy_from_user`] 注——外层单窗 +
/// [`copy_to_user_locked`]。
fn copy_to_user(caller_el1: bool, dst: u64, src: &[u8]) -> Result<(), i32> {
    if !src.is_empty() {
        // S-4 4-2：同 copy_from_user——EL0 路走查与 store 同一 PT 锁窗。
        let _pt = if caller_el1 { None } else { Some(crate::vmar::pt_lock_current()) };
        let mut last_page = SAME_PAGE_NONE;
        copy_to_user_locked(caller_el1, dst, src, &mut last_page)?;
    }
    Ok(())
}

/// 读调用方 in-out u64 槽（vmar_map 的 *vaddr 等）。
fn copy_in_u64(caller_el1: bool, ptr: u64) -> Result<u64, i32> {
    // S-4 4-2：同 copy 链同窗铁律（§A113）——EL0 路持锁走查 + 读。
    let _pt = if caller_el1 { None } else { Some(crate::vmar::pt_lock_current()) };
    if !access_ok(caller_el1, ptr, 8, false) {
        return Err(ERR_FAULT);
    }
    // SAFETY: 界检过；8 字节对齐风险由页粒度走查容许（v1 非对齐只影响性能）。
    Ok(unsafe { core::ptr::read_volatile(ptr as *const u64) })
}

/// 窗内读一个 u64（调用方**已持**本 AS PT 锁 + 解蔽窗——W3-Q92 recv 容量
/// 双读并窗的消费面）：同 [`copy_in_u64`] 的走查 + volatile 读，锁窗由
/// 调用方持有（同 [`copy_from_user_locked`] 纪律）。
fn copy_in_u64_locked(caller_el1: bool, ptr: u64, last_page: &mut u64) -> Result<u64, i32> {
    if !access_ok_window(caller_el1, ptr, 8, false, last_page) {
        return Err(ERR_FAULT);
    }
    // SAFETY: 界检过（同 copy_in_u64——8 字节对齐风险由页粒度走查容许）。
    Ok(unsafe { core::ptr::read_volatile(ptr as *const u64) })
}

/// 窗内写一个 u64 到调用方 out 指针（调用方**已持** PT 锁——W3-Q92 recv
/// 出参并窗的消费面）：同 [`copy_out_u64`] 的走查 + 写，锁窗由调用方持有。
fn copy_out_u64_locked(caller_el1: bool, ptr: u64, val: u64, last_page: &mut u64) -> Result<(), i32> {
    copy_to_user_locked(caller_el1, ptr, &val.to_le_bytes(), last_page)
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

/// handle 出参写回（**4 字节**——ABI §8 冻结基础类型宽度 handle=u32，见
/// `docs/系统调用ABI设计-v1.0.md`「冻结的是形状」节）。G-eco-c 实踩：曾用
/// `copy_out_u64` 写 8 字节，砸穿调用方 4 字节 `Handle` 槽的**邻接局部**——
/// 编译 Rust（编译器紧排局部）下 svcmgr 的 `saddr` 槽被 thread_create 出参
/// 低半清零，hello 以 sp0=0x1000（应 0x3000）入 EL0 即写 0xff0 自陷；asm
/// 模板槽位 8 字节间距手排、ktest 出参静态零置 + `as u32` 截读，两者碰巧
/// 免疫。**非 handle 的出参（ns/size/vaddr/nbytes/idx/observed）仍 8 字节**
/// ——宽度跟着 ABI 类型走，不跟着「凑巧能跑」走。
fn copy_out_u32(caller_el1: bool, ptr: u64, val: u32) -> Result<(), i32> {
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
/// P3-1 起源 handle 须持 `RIGHT_DUPLICATE`——缺位 -EACCES（授予方去位 =
/// 禁复制，Zircon `zx_handle_duplicate` 同形；与「请求 ⊆ 原、非空」两既有
/// 拒面叠乘）。执法边界 = 本 syscall 面（不受信面）：`duplicate` 对一切
/// 对象类型开放，故经类型无关权利探针 [`HandleTable::rights_of`] 查位
/// （`get_raw` 的具体 `ObjType` 门不适配此面；坏 handle 仍 -EBADF 走
/// lookup 语义）；内核内部直调表 API（boot 插表/ktest）是授能语境不经
/// 此门。
extern "C" fn sys_handle_duplicate(tf: &mut TrapFrame) -> i64 {
    let (h, rights, out) = (tf.regs[1] as u32, tf.regs[2] as Rights, tf.regs[3]);
    // 先验 out 指针（界检不过不浪费槽位/引用）。
    if let Err(e) = copy_out_u32(caller_is_el1(tf), out, 0) {
        return e as i64;
    }
    match caller_table(tf) {
        Ok(t) => match t.rights_of(h) {
            // P3-1：RIGHT_DUPLICATE 执法——get_raw 等效检查处的 need 位
            //（类型无关面经 rights_of 探针，缺位 -EACCES）。
            Ok(r) if r & RIGHT_DUPLICATE != 0 => match t.duplicate(h, rights) {
                Ok(nh) => match copy_out_u32(caller_is_el1(tf), out, nh) {
                    Ok(()) => 0,
                    Err(e) => e as i64,
                },
                Err(e) => e as i64,
            },
            Ok(_) => ERR_ACCESS_DENIED as i64, // 源 handle 无 DUP 位 = 授予方明令禁复制
            Err(e) => e as i64,
        },
        Err(e) => e,
    }
}

/// `handle_replace(h, replacement, *out)`：原子换槽（两旧值均失效）。
extern "C" fn sys_handle_replace(tf: &mut TrapFrame) -> i64 {
    let (h, repl, out) = (tf.regs[1] as u32, tf.regs[2] as u32, tf.regs[3]);
    if let Err(e) = copy_out_u32(caller_is_el1(tf), out, 0) {
        return e as i64;
    }
    match caller_table(tf) {
        Ok(t) => match t.replace(h, repl) {
            Ok(nh) => match copy_out_u32(caller_is_el1(tf), out, nh) {
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
/// 波一刀 B：id=1 `CLOCK_REALTIME` = 宿主 fw_cfg epoch 注入面（`crate::rtc`
/// ——原「壁钟留 v2」提前落地为既有 0x61 的 id 域扩展，零新号；id 0 行为
/// 逐位不变）。注入缺席 → REALTIME 仍 -ENOTSUP（用户态降级静态占位）。
extern "C" fn sys_clock_read(tf: &mut TrapFrame) -> i64 {
    let id = tf.regs[1];
    if id != CLOCK_MONOTONIC && id != CLOCK_REALTIME {
        return ERR_NOT_SUPPORTED as i64; // 未知 id（ktest 负样本 = 2）
    }
    let out = tf.regs[2];
    let ticks = berry_hal_aarch64::timer::cntvct();
    let freq = berry_hal_aarch64::timer::cntfrq();
    if freq == 0 {
        return ERR_NOT_SUPPORTED as i64; // 频率不可读（真机异常）
    }
    let ns = (ticks / freq) * 1_000_000_000 + ((ticks % freq) * 1_000_000_000) / freq;
    let ns = if id == CLOCK_REALTIME {
        match crate::rtc::realtime_ns() {
            Some(v) => v,
            None => return ERR_NOT_SUPPORTED as i64, // 无注入（-kernel 冒烟拓扑）
        }
    } else {
        ns
    };
    match copy_out_u64(caller_is_el1(tf), out, ns) {
        Ok(()) => 0,
        Err(e) => e as i64,
    }
}

/// `power_off()`（0x88，桌面UX刀D——guest 内关机最小闭环）：PSCI
/// SYSTEM_OFF（0x84000008）经 HVC conduit。成功 = 整机下电**不归**（QEMU
/// TCG/KVM 走 `qemu_system_shutdown_request`——PM-10 §1.6 实读）；返回 =
/// 失败恒 -ENOTSUP（两成因同形：conduit 死——EFI handoff / 非 EL1 入口 /
/// 无 PSCI（probe 缓存 `off_supported` 门，不盲发 HVC）；或固件拒收
/// （SYSTEM_OFF 无 PSCI_FEATURES 面，SMCCC 返回任何值即失败）。零参数零
/// 出参。v1 无门控（单用户仿真系统，消费面 launcher 关机键 + berryctl
/// power；门控与优雅关机序随审计 02 号 I3 刀）。
extern "C" fn sys_power_off(_tf: &mut TrapFrame) -> i64 {
    // SMCCC：SYSTEM_OFF 成功不归；任何返回 = 失败（conduit 死 / 固件拒），
    // 错误码面恒 -ENOTSUP（调用方诚实降级——launcher 提示行 / berryctl 退 1）。
    let _ = berry_hal_aarch64::psci::system_off_hvc();
    ERR_NOT_SUPPORTED as i64
}

// ---- 遥测环读口（0x84 telemetry_open——klog 快照首域〔阶段五第 16 步〕＋
// memstat 定长快照〔遥测 v3 首件〕＋双族环历史快照〔op=2 环读口刀——
// per-CPU 切换环＋唤醒环四段一体导出〕＋WakeSrc 槽表纯镜像〔op=3 槽表
// 读口刀——40B 轻头＋128×64B 平数据全表导出，本文件 wsrc 臂＋
// telemetry::wsrc_table_snap_into，berryctl powertop 消费〕；op 域
// add-only，后续波次自 op=4 起按落地序取号）----

/// 0x84 op 域常量**单源 = berry-telface**（klog=0 / memstat=1 / rings=2 /
/// wsrc=3——内核派发与 berry-sys 包装同引此，消内核/用户双写）；后续波次
/// 自 op=4 起按落地序取号（add-only 纪律下预留号无执法值）。
use berry_telface::{TELEMETRY_OP_KLOG, TELEMETRY_OP_MEMSTAT, TELEMETRY_OP_RINGS, TELEMETRY_OP_WSRC};

/// op=1 memstat 定长快照**冻结下限**（v1 形 40B = `MemstatRec` 5×u64）。
/// len ≥ 此值即服务：实写 40B 返 40——尾追追加律（未来扩形旧调用方收
/// 40B v1 前缀、返回值自证所得；跨大改走新 op 值）。字面冻结不随 size_of
/// 动：本臂恒写 v1 前缀。
const MEMSTAT_REC_V1: usize = 40;

/// klog 快照读帽（op=0 臂专属——= 单环容量 16KiB，再大也只装得下单环最新
/// 段；bsh dmesg 消费面 4KiB 尾帽远在其下。op=1 memstat 定长 40B，len 门
/// 是 [`MEMSTAT_REC_V1`] 冻结下限——len 帽语义 op 专属，各在臂内）。
const TELEMETRY_SNAPSHOT_MAX: usize = berry_hal_aarch64::klog_ring::KLOG_RING_SIZE;

/// klog 快照暂存（ring 快照 → copy_out 之间的稳定缓冲）。**单写者 = KL**：
/// 本 handler 恒 TABLE 类（KL 全局互斥跨核串行）+ ktest EL1 面同经 dispatch
/// 取锁——无第二并发写者，免独立锁。
static mut KLOG_SNAP_STAGE: [u8; TELEMETRY_SNAPSHOT_MAX] = [0; TELEMETRY_SNAPSHOT_MAX];

/// op=2 双族环快照**冻结下限**（v1 轻头 32B = `RingsHdrV1`）。len ≥ 此值
/// 即服务（len=32 可单读头探几何——util count-wins 探测先例同精神）；
/// **无上帽**（变长填充返回值自证——16KiB 帽是 op=0 臂专属语义不移植，
/// memstat 尾追追加律同族）。字面冻结不随 size_of 动：本臂恒写 v1 前缀。
const RINGS_SNAP_HDR_V1: usize = 32;

/// op=3 WakeSrc 槽表快照**冻结下限**（v1 轻头 40B = `WsrcHdrV1`）。len ≥
/// 此值即服务（len=40 可单读头探几何——op=2 探针先例同精神）；**无上帽**
///（全表定长 8232B 上界天然，变长仅整行截尾、返回值自证——16KiB 帽是
/// op=0 臂专属语义不移植，`RINGS_SNAP_HDR_V1` 注族）。字面冻结不随
/// size_of 动：本臂恒写 v1 前缀。
const WSRC_SNAP_HDR_V1: usize = 40;

/// 环快照暂存（窗口读 → copy_out 之间的稳定缓冲；容量 = telface
/// `TELEMETRY_RINGS_MAX` 满载上界 7296B：32 头＋4×24 段＋2×64×24 切换＋
/// 2×64×32 唤醒）。**单写者 = KL**：本 handler 恒 TABLE 类（KL 全局互斥
/// 跨核串行）+ ktest EL1 面同经 dispatch 取锁——`KLOG_SNAP_STAGE` 同纪律
/// **新立不共用**（命名与注释语义绑 klog——stage 命名诚实，裁决 6）。
static mut RING_SNAP_STAGE: [u8; berry_telface::TELEMETRY_RINGS_MAX] =
    [0; berry_telface::TELEMETRY_RINGS_MAX];

/// WakeSrc 槽表快照暂存（全表纯镜像装配 → copy_out 之间的稳定缓冲；容量
/// = telface `TELEMETRY_WSRC_MAX` 满载 8232B：40 头＋128×64 行——与
/// `WAKE_SRC_TABLE` 本身 8KB 同量级）。**单写者 = KL**：本 handler 恒
/// TABLE 类（KL 全局互斥跨核串行）+ ktest EL1 面同经 dispatch 取锁——
/// `KLOG_SNAP_STAGE`/`RING_SNAP_STAGE` 同纪律**新立不共用**（RING_SNAP_STAGE
/// 裁决 6 判例直移：单发整段无半写面——半写面即用户缓冲出现不完整行＋
/// 无头的中间态）。
static mut WSRC_SNAP_STAGE: [u8; berry_telface::TELEMETRY_WSRC_MAX] =
    [0; berry_telface::TELEMETRY_WSRC_MAX];

/// op=2 单段装配步：`read` 窗口读进 stage 段位（整条粒度 cap 截尾）→
/// 段描述符**紧随其窗口读落笔**（count = 实拷数——段描述符形下 counts
/// 恒真，两遍探测法的「头 counts 可撒谎」面结构性不存在〔裁决 3/6〕）。
/// 余量 < 24 即止（整段粒度：描述符都放不下则该段与其后段全部缺席，
/// 返回值自证截尾）。`limit` = 用户 len 与满载容量的 min（输出总界）。
fn rings_seg_put(
    stage: &mut [u8],
    cursor: &mut usize,
    limit: usize,
    family: u32,
    cpu: u32,
    read: fn(usize, &mut [u8]) -> (u32, u64),
) {
    let rem = limit - *cursor;
    if rem < core::mem::size_of::<berry_telface::RingSegV1>() {
        return; // 余量 < 24 即止——调用方以 cursor 停位判缺席
    }
    let rec_size = match family {
        berry_telface::RING_FAMILY_SWITCH => core::mem::size_of::<berry_telface::SwitchRec>(),
        _ => core::mem::size_of::<berry_telface::WakeRec>(),
    };
    let cap = (rem - 24) / rec_size; // 描述符先扣，余量整条粒度分给条目
    // SAFETY: 单写者 = KL（见 RING_SNAP_STAGE 注）；段位在 stage/cursor/limit 界内。
    let (count, total) = read(cpu as usize, &mut stage[*cursor + 24..*cursor + 24 + cap * rec_size]);
    let seg = berry_telface::RingSegV1 {
        family,
        cpu,
        total,
        count,
        rec_size: rec_size as u32,
    };
    let mut desc = [0u8; 24];
    berry_telface::ring_seg_write(&seg, &mut desc);
    stage[*cursor..*cursor + 24].copy_from_slice(&desc);
    *cursor += 24 + count as usize * rec_size; // 实拷数前进（count<cap 时零空洞）
}

/// `berry_telemetry_open(op, buf, len)`（0x84）：op 域 add-only 分发——
/// match 首键 op、len 门各自在臂内（拒序 op 先行——klog 翻真时判序保形）。
///
/// op=0 klog 快照（阶段五第 16 步首域）——读两核 klog 字节环拼接 copy_out 到
/// `buf`，返回实写字节数（>0）。拼接契约：CPU 序（环 0 → 环 1）、环内旧→新、
/// 每环贡献其**最新** `min(环内有效字节, 余量)` 字节（先到环优先占满——裸相
/// 串口主力在 CPU0，dmesg 尾部语义天然成立；两环拼接非时序归并，读方自知）。
/// len 门：`len` 为 0 或 > [`TELEMETRY_SNAPSHOT_MAX`] → -EINVAL。
///
/// op=1 memstat 定长快照（遥测 v3 首件）——`MemstatRec` 40B 五域 RAM 面经
/// [`crate::telemetry::memstat_snapshot`] 装配（free 读点派生使
/// total==alloc+free+reserved 按构造闭合）。len 门：`len` <
/// [`MEMSTAT_REC_V1`] → -EINVAL；≥ 则实写 40B 返 40（尾追追加律见彼注）。
///
/// op=2 双族环快照（环读口刀）——per-CPU 切换环＋唤醒环四段一体导出：
/// 32B 头（`RingsHdrV1`：version/hdr_bytes/cntfrq/ring_len/ring_cpus）＋
/// 每环 24B 段描述符（`RingSegV1`）＋变长载荷（CPU 序段序 switch c0→c1→
/// wake c0→c1、段内旧→新、整条粒度截尾）。**一致性语义 = 逐环一致＋尽力
/// 一致**：每环内单 seqlock 版本窗整块一致或整环诚实弃（count=0）；四环间
/// 无一致保证（各环独立快照时刻——跨段非时序归并，读方自知；klog 臂 CPU
/// 序拼接同形）。count 三态判据：空（total==0）／降级（total>0 ∧ count==0
/// ∧ 余量足——64 次重试上界后读窗不稳的诚实弃段，保证前进免 EAGAIN
/// 重试风暴）／截尾（余量尽，返回值自证）。丢包派生：每环
/// total − min(total, ring_len) = 轮转淘汰数。trace_off 形态环让位 →
/// totals 全 0 → 快照诚实空头空段（天然真空，无特判）。len 门：`len` <
/// [`RINGS_SNAP_HDR_V1`] → -EINVAL；无上帽（变长填充返回值自证，≥32 恒
/// 返 ≥32）。
///
/// op=3 WakeSrc 槽表纯镜像（槽表读口刀）——40B 头（`WsrcHdrV1`：
/// version/hdr_bytes/slots/rec_size/cntfrq/now_vct/wake_acct_n）＋128×64B
/// 定长平数据行（`WakeSrcRecV1`——**行号即源值**，值直索引零 src 域）。
/// 全表镜像导出、变长仅整行截尾（len=40 单读头探几何；无上帽——全表定长
/// 8232B 上界天然，返回值 40+行数×64 自证）。一致性语义＝逐槽逐字段独立
/// 原子读**非一致快照**与装配序双不变式（Σ行wakes ≥ wake_acct_n＋
/// now_vct ≥ 任一行时戳）见臂内注（G6 全文）＋
/// [`crate::telemetry::wsrc_table_snap_into`] doc。trace_off 表照出（槽写
/// 与 N 递增在 trace_off 早退前——计数器族不随关，双 trace 态皆真数据无
/// 空头形）。len 门：`len` < [`WSRC_SNAP_HDR_V1`] → -EINVAL。
///
/// 拒面：op 域外 → -ENOTSUP；copy_out 失败 → -EFAULT（klog 臂逐环分段拷贝，
/// 失败前的段已写——诚实部分读，调用方重试；memstat/rings/wsrc 臂单发
/// 整段无半写面）。零 handle 零权利位（系统级遥测读面——统计信号非对象
/// 内容，`sched_util_snapshot` 同族口径）。
///
/// 锁类 TABLE：klog 环热尾容错读无锁无共享串行面；memstat 三元组单闭包
/// 一致窗、水位皆 Relaxed 原子读（统计信号无序角色）；rings 臂 seqlock
/// 单版本窗整块拷（重试持 KL 有界：64 次 × ~2KB 拷贝，写方永不取 KL 无
/// 死锁面）+ 7296B static stage 单写者；wsrc 臂纯原子读零重试面（表无
/// seqlock——op=2 最大风险〔TCG 降级〕在本刀结构性不存在）+ 8232B
/// static stage 单写者；copy_out 链走查调用方页表 = §A113 读者面（同
/// 0x61 clock_read）。
extern "C" fn sys_telemetry_open(tf: &mut TrapFrame) -> i64 {
    let (op, ptr, len) = (tf.regs[1], tf.regs[2], tf.regs[3]);
    match op {
        // op=0 klog（G1 逐字节保形：原单臂 if 体原序原码随臂下放）。
        TELEMETRY_OP_KLOG => {
            if len == 0 || len as usize > TELEMETRY_SNAPSHOT_MAX {
                return ERR_INVALID_ARG as i64;
            }
            let el1 = caller_is_el1(tf);
            // SAFETY: 单写者 = KL（见 KLOG_SNAP_STAGE 注）；本 handler 全程 KL 窗。
            let stage = unsafe { &mut *core::ptr::addr_of_mut!(KLOG_SNAP_STAGE) };
            let mut total: usize = 0;
            for cpu in 0..berry_hal_aarch64::klog_ring::KLOG_NCPUS {
                let rem = len as usize - total;
                if rem == 0 {
                    break;
                }
                let n = berry_hal_aarch64::klog_ring::snapshot_into(cpu, &mut stage[..rem]);
                if n == 0 {
                    continue; // 空环/读窗不稳——跳过该环（诚实空，不重试占锁）
                }
                match copy_to_user(el1, ptr + total as u64, &stage[..n]) {
                    Ok(()) => total += n,
                    Err(e) => return e as i64, // -EFAULT 透传（此前段已写=诚实部分读）
                }
            }
            total as i64
        }
        // op=1 memstat（v3 首件）：len 冻结下限门 → 栈上装配 → 单发整段拷出。
        TELEMETRY_OP_MEMSTAT => {
            if (len as usize) < MEMSTAT_REC_V1 {
                return ERR_INVALID_ARG as i64;
            }
            let el1 = caller_is_el1(tf);
            let rec = crate::telemetry::memstat_snapshot();
            // 栈上 40B raw 组装（sched_util_snapshot 同款；定长小件零 static
            // stage——免暂存免锁）。
            let words = [
                rec.total_pages,
                rec.reserved_pages,
                rec.alloc_pages,
                rec.free_pages,
                rec.committed_pages,
            ];
            let mut raw = [0u8; MEMSTAT_REC_V1];
            for (slot, w) in raw.chunks_exact_mut(8).zip(words) {
                slot.copy_from_slice(&w.to_le_bytes());
            }
            match copy_to_user(el1, ptr, &raw) {
                Ok(()) => MEMSTAT_REC_V1 as i64,
                Err(e) => e as i64,
            }
        }
        // op=2 双族环快照（环读口刀）：len 冻结下限门 → 单遍装配 → 单发整段
        // 拷出。段描述符紧随其窗口读落笔 = counts 恒真（rings_seg_put）；
        // 头最后落笔 offset 0；四段 canon 序 CPU 序段序（klog 臂拼接先例
        // 同构）。尽力一致语义与 count 三态判据见函数 doc（G6 全文）。
        TELEMETRY_OP_RINGS => {
            if (len as usize) < RINGS_SNAP_HDR_V1 {
                return ERR_INVALID_ARG as i64;
            }
            let el1 = caller_is_el1(tf);
            // 输出总界 = 用户 len（无上帽）；stage 只用到满载容量。
            let limit = (len as usize).min(berry_telface::TELEMETRY_RINGS_MAX);
            // SAFETY: 单写者 = KL（见 RING_SNAP_STAGE 注）；本 handler 全程 KL 窗。
            let stage = unsafe { &mut *core::ptr::addr_of_mut!(RING_SNAP_STAGE) };
            let mut cursor = RINGS_SNAP_HDR_V1; // 头区预留——头最后落笔
            rings_seg_put(stage, &mut cursor, limit, berry_telface::RING_FAMILY_SWITCH, 0,
                crate::telemetry::ring_snap_into);
            rings_seg_put(stage, &mut cursor, limit, berry_telface::RING_FAMILY_SWITCH, 1,
                crate::telemetry::ring_snap_into);
            rings_seg_put(stage, &mut cursor, limit, berry_telface::RING_FAMILY_WAKE, 0,
                crate::telemetry::wake_ring_snap_into);
            rings_seg_put(stage, &mut cursor, limit, berry_telface::RING_FAMILY_WAKE, 1,
                crate::telemetry::wake_ring_snap_into);
            let mut hbuf = [0u8; RINGS_SNAP_HDR_V1];
            berry_telface::rings_hdr_write(&crate::telemetry::rings_hdr(), &mut hbuf);
            stage[..RINGS_SNAP_HDR_V1].copy_from_slice(&hbuf);
            match copy_to_user(el1, ptr, &stage[..cursor]) {
                Ok(()) => cursor as i64, // ≥32 头自证
                Err(e) => e as i64,      // -EFAULT 透传（单发整段无半写面）
            }
        }
        // op=3 WakeSrc 槽表纯镜像（槽表读口刀）：len 冻结下限门 → 全表装配
        //（telemetry::wsrc_table_snap_into——装配序双不变式：N 先读 Acquire
        // →逐槽逐字段 Relaxed 读→now_vct 行读毕后取→头最后落笔）→ 单发
        // 整段拷出。**一致性契约（G6 全文）**：逐槽逐字段独立原子读、非一
        // 致快照（wake_src_snapshot 单槽先例扩全表）——单调族（wakes/
        // irq_count/no_waiter fetch_add、first_ts fetch_min、last_ts
        // fetch_max、flags fetch_or、merge_hal 单调 store）中途读恒得某个
        // 真实持过的值无回退无撕裂；非单调族（last_woken_id/last_ack_ts
        // 定宽 store）可旧可新不撕；跨字段错位可能（wakes 新于 last_ts 等）
        // ——差分只许用单调族（区间差精确自愈、不丢不双计）；全记录一致窗
        // 不承诺（挂账维持）。Σ行wakes ≥ wake_acct_n 恒立（N 先读——写序
        // 「槽先 N 末位 Release」的读者侧对偶；一次广播可醒任意多任务，
        // 不做固定差值帽）；now_vct ≥ 任一行内时戳（行读毕后取，CNTVCT
        // 单调律——消费方区间分母可安全夹上界）。first_ts==u64::MAX 哨兵＝
        // 未触。整行粒度截尾：余量按 64B 整行分、不足一行即止，返回值
        // 40+行数×64 自证（len=40 可单读头探几何）。trace_off 表照出零特判
        //（on_wake 槽写与 N 递增都在 trace_off 早退之前——计数器族不随关；
        // 表导出双 trace 态皆真数据、无空头形）。并账点边界：SGI×2/UNKNOWN
        // 的 irq_count 是 G1.5 boot 尾单点并账快照非连续并账——差分对该
        // 三槽恒 0 是 v1 已知边界（连续并账挂 v2）。
        TELEMETRY_OP_WSRC => {
            if (len as usize) < WSRC_SNAP_HDR_V1 {
                return ERR_INVALID_ARG as i64;
            }
            let el1 = caller_is_el1(tf);
            // 输出总界 = 用户 len 与满载容量的 min（全表定长——len>8232
            // 恒满载）；stage 只用到满载容量。
            let limit = (len as usize).min(berry_telface::TELEMETRY_WSRC_MAX);
            // SAFETY: 单写者 = KL（见 WSRC_SNAP_STAGE 注）；本 handler 全程 KL 窗。
            let stage = unsafe { &mut *core::ptr::addr_of_mut!(WSRC_SNAP_STAGE) };
            let n = crate::telemetry::wsrc_table_snap_into(stage, limit);
            match copy_to_user(el1, ptr, &stage[..n]) {
                Ok(()) => n as i64,  // ≥40 头自证
                Err(e) => e as i64,  // -EFAULT 透传（单发整段无半写面）
            }
        }
        _ => ERR_NOT_SUPPORTED as i64, // op 域外（后续波次自 op=4 起按落地序取号）
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

/// CNTVCT → MONOTONIC 纳秒（4-3c intent 两道时间帽 + S5-a 相对 deadline 的
/// 计时源）。换算 = `timer::ticks_to_ns`：init 预计算倒数乘-移对（下一代
/// 优化 #1——热路径 1 mul + 1 shift 消解 2×udiv；send/recv 3 处换算每往返
/// 省 54-138 cycles）。慢路两除精确式与 `sys_clock_read` 同形；QEMU virt
/// 62.5MHz（ns/tick=16 二进精准）两路逐位一致。`freq==0` → 0（真机异常
/// 语境——帽永不触发，方向安全）。
fn vct_ns() -> u64 {
    berry_hal_aarch64::timer::ticks_to_ns(berry_hal_aarch64::timer::cntvct())
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
        // MMIO 窗口内容面（P6 A-2）：设备只能经映射访问。
        VmoError::NotSupported => ERR_NOT_SUPPORTED,
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

// ---- S-5e M1：soakload churn 进度观测（端到端防假绿）----
// 对抗审查实录（S-5e M1）：载荷线程被 G1.5 kill / churn svc 恒败（gen 顶格）
// 时写者仍自旋满载形态——调度/内存/console 断言照绿（载荷业务进度零观测）。
// 观测面：soakload master 舰队 spawn 后 arm（task_soakload）；此后 vmo_create
// 成功（载荷每环首 svc）计数。Relaxed 即可——观测计数只要原子性与最终可见
//（boot 尾毫秒级漏计无关紧要），非同步原语。soakmon 心跳 v2.1 第 9 字段
// churn= 读出（soak-smp.sh 断言严格递增；非 soakload boot 恒 0 不入断言面）。
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
static SOAKLOAD_ACTIVE: AtomicBool = AtomicBool::new(false);
static SOAK_CHURN_OK: AtomicU64 = AtomicU64::new(0);

/// soakload master 舰队 spawn 后置位（churn 成功计数开始）。
pub fn soakload_churn_arm() {
    SOAKLOAD_ACTIVE.store(true, Ordering::Relaxed);
}

/// churn 成功计数读出（soakmon 心跳 v2.1 churn= 字段）。
pub fn soakload_churn_readout() -> u64 {
    SOAK_CHURN_OK.load(Ordering::Relaxed)
}

/// `vmo_create(size, options, *out)`：建匿名 VMO。默认授 READ|WRITE|MAP|
/// DUPLICATE|TRANSFER（Zircon 模型：新 handle 默认携 TRANSFER——send 随行
/// 预检 4-4b 起执法，去位迁移须显式 dup 收紧）。
/// options：bit0 = `VMO_OPT_DEMAND`（G4-2，§5-① 裁决）——按需后备 VMO
/// （demand 对象属性，映射走 vmar_map 对象级分流 map_demand，首触缺页由
/// EL0 pager 经 0x21 供给）+ **额外授 SET_POLICY**（0x21 门权——普通 dup
/// 面不能冒充 pager）；其余位 → -EINVAL（options 纪律；**全宽 u64 检**，
/// G4-2 修批 MECH-2——x2 高 32 位不截断，channel_create 同口径）。
/// `size` 上限 [`VMO_MAX_SIZE`]（防 EL0 巨参推爆堆——no-panic）。
extern "C" fn sys_vmo_create(tf: &mut TrapFrame) -> i64 {
    let (size, options, out) = (tf.regs[1] as usize, tf.regs[2], tf.regs[3]);
    let legal_opts = VMO_OPT_DEMAND | VMO_OPT_CONTIGUOUS | VMO_OPT_PINNED;
    if options & !legal_opts != 0 {
        return ERR_INVALID_ARG as i64;
    }
    let demand = options & VMO_OPT_DEMAND != 0;
    let contiguous = options & VMO_OPT_CONTIGUOUS != 0;
    let pinned = options & VMO_OPT_PINNED != 0;
    // F0-4：按需缺页 VMO 与创建期锁死连续/钉死物理页互斥。
    if demand && (contiguous || pinned) {
        return ERR_INVALID_ARG as i64;
    }
    if size == 0 {
        return ERR_INVALID_ARG as i64;
    }
    let el1 = caller_is_el1(tf);
    if size > VMO_MAX_SIZE {
        return ERR_NO_MEMORY as i64;
    }
    if let Err(e) = copy_out_u32(el1, out, 0) {
        return e as i64;
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    // per-Process VMO 配额帽（Medium 缺口收口，`process.rs` 帽常量注）：本表
    // 在册 VMO handle 数 ≥ 帽先拒 -ENOMEM——fail-fast 先于 create 的页记账
    // 分配（128MiB VMO 记账 Vec ≈ 576KiB/个，无帽单进程循环 create 可把共享
    // 堆耗到全机 -ENOMEM）。记账 = `count_type` 表扫描（S5.12 token 帽同款；
    // 随 close/take/drain/迁移天然配平，sidecar 计数器在跨表迁移下失配——
    // `pager_load` 同口径）。admission 只查不写（沿革 #109 纪律）。
    if t.count_type(ObjType::Vmo) >= crate::process::MAX_VMO_PER_PROC {
        return ERR_NO_MEMORY as i64;
    }
    // create try 化（3b3 H2）：NoMem → -ENOMEM（原 create 内嵌 vec! 双分配，
    // OOM abort 曾是 EL0 可触发 panic）。
    let vmo_res = if demand {
        Vmo::create_demand(size)
    } else if contiguous {
        Vmo::create_contiguous_global(size)
    } else if pinned {
        Vmo::create_pinned(size)
    } else {
        Vmo::create(size)
    };
    let vmo = match vmo_res {
        Ok(v) => v,
        Err(e) => return voe(e) as i64,
    };
    // G4-2：demand 位附加 SET_POLICY（§5-① 裁决——demand VMO 首消费，
    // 0x21 双验之一；默认面权利集逐位不变）+ TRANSFER（Zircon 默认携位）。
    let rights = if demand {
        RIGHT_READ | RIGHT_WRITE | RIGHT_MAP | RIGHT_DUPLICATE | RIGHT_SET_POLICY
            | RIGHT_TRANSFER
    } else {
        RIGHT_READ | RIGHT_WRITE | RIGHT_MAP | RIGHT_DUPLICATE | RIGHT_TRANSFER
    };
    match t.insert(KernelObj::Vmo(vmo), rights) {
        Ok(h) => match copy_out_u32(el1, out, h) {
            Ok(()) => {
                // S-5e M1：soakload 载荷 churn 进度计数（svc 恒败/gen 顶格
                // → 心跳 churn 冻红；非 soakload boot 零成本分支直过）。
                if SOAKLOAD_ACTIVE.load(Ordering::Relaxed) {
                    SOAK_CHURN_OK.fetch_add(1, Ordering::Relaxed);
                }
                0
            }
            Err(e) => e as i64,
        },
        Err(e) => e as i64,
    }
}

/// `vmo_commit(vmo, offset, len)`：立即出页清零（eager VMO 面；demand
/// VMO 供给链自 commit——G4-1 起）。
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

/// `vmo_cache_op(vmo, op, offset, len)`（0x1C，F0-4）：非一致性 DMA PoC 数据缓存维护。
/// 门控：`CLEAN` 需 `RIGHT_READ`；`INVALIDATE` / `CLEAN_INVALIDATE` 需 `RIGHT_WRITE`。
extern "C" fn sys_vmo_cache_op(tf: &mut TrapFrame) -> i64 {
    let (h, op, off, len) = (
        tf.regs[1] as u32,
        tf.regs[2],
        tf.regs[3] as usize,
        tf.regs[4] as usize,
    );
    let (need_rights, clean_only) = match op {
        VMO_CACHE_OP_CLEAN => (RIGHT_READ, true),
        VMO_CACHE_OP_INVALIDATE | VMO_CACHE_OP_CLEAN_INVALIDATE => (RIGHT_WRITE, false),
        _ => return ERR_INVALID_ARG as i64,
    };
    let Ok(t) = caller_table(tf) else {
        return ERR_BAD_HANDLE as i64;
    };
    let vmo = match t.get(h, ObjType::Vmo, need_rights) {
        Ok((obj, _)) => match obj.as_vmo() {
            Some(v) => v,
            None => return ERR_BAD_HANDLE as i64,
        },
        Err(e) => return e as i64,
    };
    match vmo.cache_op(clean_only, off, len) {
        Ok(()) => 0,
        Err(e) => voe(e) as i64,
    }
}

/// `vmo_read(vmo, buf, offset, len)`：内核中介读（非映射访问）。门控：READ。
/// 整段缓冲校验前置（fail-fast，纯读无半截副作用）；256B 分块防吃内核栈。
/// S-4 4-2：前置 access_ok 是纯走查 fail-fast（不 store），**不是**安全边界
/// ——真正的 §A113 读者窗在下方分块拷贝（单一 PT 锁窗包整段块流 + 每新页
/// 重查，前置被跨核 TOCTOU 绕过仅导致分块时 -EFAULT；P1 同页跳过见窗注）。
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
    // P1 同页跳过：整段分块拷贝并成**单一 PT 锁窗**（§A113 不变量保持——
    // 窗内每新页走查与 store 仍同窗；同页后续块免重复三级走查：64KB 载荷
    // 256 块，原每块一窗一走查 = 256 次 → 每页 1 次 = 16 次）。窗含
    // `vmo.read`：纯无锁 direct map 拷贝（界检 + 已提交检 + memcpy——不
    // 取锁、不分配、不阻塞）⇒ 无锁序新边、持有有界；同 AS 写者自旋与本
    // 窗串行——与既有单发整段 `copy_*_user`（一窗到底）同语义。
    let _pt = if el1 { None } else { Some(crate::vmar::pt_lock_current()) };
    let mut last_page = SAME_PAGE_NONE;
    while pos < len {
        let n = core::cmp::min(COPY_CHUNK, len - pos);
        if let Err(e) = vmo.read(off + pos, &mut chunk[..n]) {
            return voe(e) as i64;
        }
        if let Err(e) = copy_to_user_locked(el1, buf + pos as u64, &chunk[..n], &mut last_page) {
            return e as i64;
        }
        pos += n;
        // §A126 墓碑（原 g3-1b 落点②块间检查点，已死）：本号 = TABLE 类
        // （§A115 降档，KL-only 无 RQ 位）——preempt_checkpoint 真探形
        // try_preempt→rq_lock 位不在集不退化 = 物理取 RQ 于 KL 下 = 七类
        // 锁序（队列>表）逆序（双核窄概率 ABBA 窗，无实测挂死）。g3-1b
        // 「空探」论证只盖切换行为、没盖探峰动作本身的物理取锁。切换诉求
        // 由落点①派发尾检查点（svc_exit_class 毕 = 零锁点）承接；vmo 中介
        // 拷贝解蔽/体内检查点留专门刀（升档翻 §A115 或包络分裂，两解均开
        // PT 锁序新验证边——未采证据不入刀）。
    }
    0
}

/// `vmo_write(vmo, buf, offset, len)`：内核中介写。门控：WRITE。分块同 read
/// （前置 access_ok 同为 fail-fast 纯走查，§A113 窗在下方单一 PT 锁分块窗）。
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
    // P1 同页跳过：同 sys_vmo_read——单一 PT 锁窗包整段块流，`last_page`
    // 窗内缓存，同页块免重复三级走查；窗含 `vmo.write`（纯无锁 direct map
    // 拷贝——不取锁、不分配、不阻塞，无锁序新边）。
    let _pt = if el1 { None } else { Some(crate::vmar::pt_lock_current()) };
    let mut last_page = SAME_PAGE_NONE;
    while pos < len {
        let n = core::cmp::min(COPY_CHUNK, len - pos);
        if let Err(e) = copy_from_user_locked(el1, buf + pos as u64, &mut chunk[..n], &mut last_page) {
            return e as i64;
        }
        if let Err(e) = vmo.write(off + pos, &chunk[..n]) {
            return voe(e) as i64;
        }
        pos += n;
        // §A126 墓碑：原落点②块间检查点已死——类面分析见 sys_vmo_read 同位注。
    }
    0
}

/// `vmo_get_size(vmo, *size)`：查 VMO 容量（G-eco 入机侧：svcmgr 消费 rootfs
/// 需要 cpio 总长；add-only 0x1A 顺延）。门控：**仅类型不查权利位**（need=0）
/// ——size 是对象几何非内容，同 `object_wait`「等待不需权利」惯例（12 位
/// rights 无 SIZE 位；零权利 handle 本就进不了表）。注意返回**页对齐容量**
/// （create 即取整）——cpio 遍历以 newc 头自描述长度为准，别当 blob 实长。
extern "C" fn sys_vmo_get_size(tf: &mut TrapFrame) -> i64 {
    let (h, out) = (tf.regs[1] as u32, tf.regs[2]);
    let el1 = caller_is_el1(tf);
    // 出参预清 + 界检（fail-fast，同 vmo_create）。size 是 u64 出参——预清
    // 宽度跟 ABI 类型走（G-eco-c 宽度纪律的例外提醒：0x1A 返回的不是 handle）。
    if let Err(e) = copy_out_u64(el1, out, 0) {
        return e as i64;
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let vmo = match t.get(h, ObjType::Vmo, 0) {
        Ok((obj, _)) => match obj.as_vmo() {
            Some(v) => v,
            None => return ERR_BAD_HANDLE as i64, // 类型门控后不可达；防御面
        },
        Err(e) => return e as i64,
    };
    match copy_out_u64(el1, out, vmo.size() as u64) {
        Ok(()) => 0,
        Err(e) => e as i64,
    }
}

/// `vmo_get_phys(vmo, offset, *pa)`：查字节物理地址（P6 A-3 用户态 DMA 面：
/// virtqueue 描述符地址是 guest PA，用户态驱动须能查自己 VMO 页的后备）。
/// 门控同 [`sys_vmo_get_size`]：**仅类型不查权利位**（need=0）——后备几何
/// 非内容（12 位 rights 无 INSPECT 消费语义，零权利观察者同 get_size 可查）。
/// 语义：`*pa = 所在页后备 PA + 页内偏移`（精确到字节——描述符 addr 直用，
/// 免调用方再拼）；匿名 VMO 该页须已 commit（-EBADSTATE，voe NotCommitted
/// 同码）——未提交页无后备可查；MMIO 窗口后备 = `pa_base + offset` 恒可查
/// （`page_pa` 统一取）。offset ∈ [0, size)（-EOVERANGE）；pa 是 u64 出参
/// 8 字节宽（宽度跟 ABI 类型走）。
extern "C" fn sys_vmo_get_phys(tf: &mut TrapFrame) -> i64 {
    let (h, off, out) = (tf.regs[1] as u32, tf.regs[2] as usize, tf.regs[3]);
    let el1 = caller_is_el1(tf);
    // 出参预清 + 界检（fail-fast，同 get_size；pa 是 u64 非 handle——8 字节）。
    if let Err(e) = copy_out_u64(el1, out, 0) {
        return e as i64;
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    // 安全修复（arch-r2 #5）：need 0→READ——零权利观察者不可查询物理地址
    //（侧信道重建内存布局 / 跨进程共享帧检测）。P6 DMA 动机只需有 READ 的
    // 合法持有者。get_size 维持 need=0（纯容量无物理信息）。
    let vmo = match t.get(h, ObjType::Vmo, RIGHT_READ) {
        Ok((obj, _)) => match obj.as_vmo() {
            Some(v) => v,
            None => return ERR_BAD_HANDLE as i64, // 类型门控后不可达；防御面
        },
        Err(e) => return e as i64,
    };
    if off >= vmo.size() {
        return ERR_OUT_OF_RANGE as i64; // 字节语义：[0, size)，尾后一字节也拒
    }
    match vmo.page_pa(off / PAGE_SIZE) {
        Some(pa) => match copy_out_u64(el1, out, (pa + off % PAGE_SIZE) as u64) {
            Ok(()) => 0,
            Err(e) => e as i64,
        },
        None => ERR_BAD_STATE as i64, // 未提交页：无后备（MMIO 恒有——page_pa 恒 Some）
    }
}

/// 门控后的 Vmo `&mut` 解包（get_raw 类型门控后防御性取本体，不符折 None →
/// 调用方返 -EBADF——syscall 边界无 panic 纪律，不可达分支也不 panic）。
fn obj_vmo_mut(obj: &core::ptr::NonNull<crate::handle::ObjRef>) -> Option<&'static mut Vmo> {
    // SAFETY: get_raw 已验指针在表、gen 匹配、类型为 Vmo；&mut 期间无并发
    // 访问（单核 + 本函数调用面（vmo 族号）全程 IRQ 屏蔽——send/recv 解蔽
    // 窗不经此函数）。
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
    // G4-2（§5-① 裁决）：demand VMO 对象级分流——登记 Demand 区间不填叶
    //（首触缺页走 EL0 探针 → 0x21 供给）；eager VMO 原路径逐位不变（全
    // 提交执法点在 `Vmar::map` 内）。perm 同为 UserRw（供给叶取区间登记权）。
    // demand 分支另登记 asid 反查表（§5-④——探针第一段 TTBR0 现读 asid →
    // root Vmar 的唯一来源）；**先登记后映射**：登记失败零副作用早返。映射
    // 失败留幂等残项（同 (asid, root) 重试 map 复占同槽）——G4-2 修批后
    // 语义改口（MECH-3 翻案）：残项**不再是终身占位**（`Vmar::Drop` 注销面
    // 已落，槽随对象销毁可回收），stale 残项最坏 = 探针 stage-2 region 查
    // miss → 原杀链。
    let r = if vmo.is_demand() {
        match crate::vmar::register_demand_root(vmar.asid(), vmar as *mut Vmar) {
            Ok(()) => vmar.map_demand(vmo, req as usize, voff, len, Perm::UserRw),
            // 满表 = 供给面不可达（静默丢 = demand 页死循环杀链）→ 拒映射；
            // 异 root 同 asid = 装配面不变量破（防御拒）。
            Err(crate::vmar::DemandRootError::Full) => return ERR_NO_MEMORY as i64,
            Err(crate::vmar::DemandRootError::OwnerConflict) => {
                return ERR_NOT_SUPPORTED as i64
            }
        }
    } else {
        vmar.map(crate::mm::frames(), vmo, req as usize, voff, len, Perm::UserRw)
    };
    match r {
        Ok(va) => match copy_out_u64(el1, vaddrp, va as u64) {
            Ok(()) => 0,
            Err(e) => e as i64,
        },
        Err(e) => vme(e) as i64,
    }
}

/// `pager_supply_pages(vmo, src, src_off)`（0x21，G4-2 首形）：用户态 pager
/// 给 demand VMO 供给一页——**阻塞收货**（§5-③ 裁决）：无武装 EL0 缺页
/// 请求时 poll-first 后睡 EL0 pager 裸键（fault 探针武装请求槽 + 唤醒即
/// 递送——ADR #9 缺页路径零分配）。
///
/// 门控：目标 vmo 须 Vmo + **SET_POLICY**（demand 创建附加权，§5-① 裁决
/// ——普通 dup 面不能冒充 pager）；src = 0 → 零页供给（供给页即清零页）；
/// 否则须 Vmo + READ + `src_off` 页对齐且 `src_off+PAGE_SIZE` 界内 + 源页
/// 已提交（§2.2 防线 4：supply 链零缺页——未提交源 read 报错拒）。
///
/// 供给 = pt_lock 窗 commit + 填叶 + tlbi/broadcast（vmar_map 同款，
/// dpg_pager 同链）→ 源拷贝（静态 4K 缓冲，零分配）→ 唤 faulter。出口
/// **恒唤** faulter：成功 = eret 命中新叶；失败 = 重试再 fault 再武装
/// （pager 重调 0x21 自愈）；pager 不再调 = faulter 永久阻塞（§5-⑤ 裁决
/// ——PS 可见/不传染）。
///
/// 多 faulter 面（G4-3 B 刀键化后）：K 槽池认领（B 刀 8 → X-5 扩 32）+
/// 每槽定向唤，错醒仅存池满溢出 convoy（N>K 时 ≤N-K 有界，N≤K 恒空；
/// C-lite 时代「错醒由重 fault 自愈」的广播面退役）。X-5 起收货 = **属主
/// 过滤取**（`el0_take_for(vd)` 只取属主匹配本 vmo 的武装槽）：有武装槽但
/// 无一属主匹配 → **-EINVAL 让位**（不消费不唤醒——老形「全局取最低位 +
/// 身份验 -EINVAL + 恒唤重臂」的错配消耗舞收敛纯靠时序运气，此根因拔除，
/// pager 换窗/换轮再收。rc 与老形冒充臂同码 -EINVAL：svcmgr dpgn ④ 冒充
/// 断言零漂移、X-5 仪器 rc 词表稳定）；供给体身份验保留（同窗不变量
/// 防御——属主过滤后 take 到的槽已证属 vd，残余 TOCTOU 路收窄为 belt-
/// and-suspenders）。
extern "C" fn sys_pager_supply_pages(tf: &mut TrapFrame) -> i64 {
    let (vh, sh, soff) = (tf.regs[1] as u32, tf.regs[2] as u32, tf.regs[3] as usize);
    if soff % PAGE_SIZE != 0 {
        return ERR_INVALID_ARG as i64;
    }
    pager_supply_common(tf, vh, sh, SrcOff::Fixed(soff))
}

/// `pager_supply_mapped(vmo, src, delta)`（0x23，G4-4 X-3，门 C 裁 A
/// add-only）：0x21 的恒等选源变体——门控/阻塞收货/供给/唤醒链全同 0x21
///（见上），唯源页偏不由调用方钉死、按**请求页现算**：`src_off = 目标页偏
/// + delta`。exec 装载 demand 化供给面：pager 无法预知下一缺页页位（0x21
/// impasse 根因——收货前钉死 src_off），本号以常量 delta 换「任意页请求
/// 一次供给」（congruent ELF：段内 file_off − vaddr 偏置为常量；i64 可负
/// ——典型 ELF vaddr 高于文件偏，偏置为负）。
extern "C" fn sys_pager_supply_mapped(tf: &mut TrapFrame) -> i64 {
    let (vh, sh, delta) = (tf.regs[1] as u32, tf.regs[2] as u32, tf.regs[3] as i64);
    if delta % PAGE_SIZE as i64 != 0 {
        return ERR_INVALID_ARG as i64;
    }
    pager_supply_common(tf, vh, sh, SrcOff::Delta(delta))
}

/// 0x24 表批窗（X-6 刀① 冻结面——`X6批供提速设计-v1.0.md` §2.1：结构体
/// 布局一次定全即冻，§7 主裁③）：`dv` = 目标 demand VMO 句柄（门控
/// Vmo+RIGHT_SET_POLICY，同 0x23）；`sv` = 源 VMO 句柄（0 = 零页臂）；
/// `delta` = 恒等选源常数（页对齐 i64 可负——同 0x23）。repr(C) 16B：
/// u32+u32+i64 自然布局 i64 恰落 offset 8 无填充、对齐 8。内核侧从用户
/// 字节逐项小端拼装（`policy_tbl_parse`/`wait_many` 先例——不把用户字节
/// 直 cast 成结构体指针）；`user/berry-sys` 手写双镜像（§8.3 原型层）。
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PagerWindow {
    pub dv: u32,
    pub sv: u32,
    pub delta: i64,
}
const _: () = assert!(core::mem::size_of::<PagerWindow>() == 16);
const _: () = assert!(core::mem::align_of::<PagerWindow>() == 8);

/// 0x24 表帽（X-6 冻结 N=32——四源镜像：本帽 / `dpg::EREQ_SLOTS` /
/// svcmgr `PAGER_SLOTS` / dpgface `tblwin::PAGER_TABLE_MAX`；抬帽走
/// len 兼容（调用方传 n），不破冻）。
const PAGER_TABLE_MAX: usize = 32;
const _: () = assert!(PAGER_TABLE_MAX == berry_dpgface::tblwin::PAGER_TABLE_MAX);

/// `pager_supply_mapped_table(tbl, n)`（0x24，X-6 刀① 批供表批，冻结裁决
/// 2026-11-08）：0x23 的多窗一次收货形——`tbl` = 调用方 PagerWindow 数组、
/// `n` ∈ [1,32]，整表拷入（栈 512B staging——`policy_tbl_parse` 2KB 栈先例；
/// ADR #9 零分配「内核静态/栈」两路皆合，取栈路免新 static）。rc =
/// **count-wins**：>0 = 供给成功页数（部分成功部分计、不回滚；失败槽
/// faulter 恒唤重臂自愈——与 0x23 同义）；**0 供才报 errno**（-EINVAL =
/// n 越界 / delta 非页对齐 / 聚合 NoneForVd；供给执行体错误（voe/vme 面）
/// 0 供时以其本码报；-EBADF = 门败）。
///
/// 语义裁定（实施刀冻结，X-6 设计 §2.1/§2.2 原文口径）：
/// - **整批门控先行**：全 n 窗双门（dv Vmo+SET_POLICY / sv Vmo+READ，同
///   0x23 门面）+ 全 retain（2N ObjRefs 单一出口统一释放）后才进收货环
///   ——懒门控会造半供给 + 晚 -EBADF 与 count-wins 打架；boot 探测
///   0x24(dv:0,n:1) 依赖「门败早返 -EBADF」区分新核（在册）与旧核
///   （未注册 -ENOTSUP）。混合失败局由此消解：门败恒在收货前整批拒。
/// - **逐 dv continue-take**：`el0_take_for` 签式不动（E3d/e/f 零耦合，
///   deliberate de-risk）——每窗内环抽干：Supplied → `pager_supply_one`
///   原样复用 + 恒唤本槽 faulter + 计数，续取；NoneForVd → 跳次窗（零
///   消费零唤醒）；同 dv 多窗各段独立（前窗已抽干，后窗即刻空跳）。
/// - **阻塞二分**：池空且已供>0 → 即返计数不阻塞（调用方有进展）；池空
///   且 0 供 → 泊 EL0 pager 键（同 0x23 阻塞收货——`block_current` 承担
///   SVC 入口类持锁的放/取舞蹈），醒后**重扫整表**（虚醒由重扫吸收）。
/// - KL 窗策略：整批 ≤32 页一窗（v1 单核简形；SMP 前强制复审——喘息点
///   设计在案不启用，挂 G4 §8 邻位账本）。SUPPLY_BUF 逐页（KL 持有语义
///   同 `dpg.rs`——单 handler 调用内顺序借还零重叠）。
extern "C" fn sys_pager_supply_mapped_table(tf: &mut TrapFrame) -> i64 {
    let (tbl_p, n) = (tf.regs[1], tf.regs[2] as usize);
    // W3-Q73：n 门 + 16B 小端拼装 + delta 页对齐预检的判定律单源
    // （dpgface tblwin——host 钉板；错误码映射在此：两态门/预检皆
    // -EINVAL。n 越界 = 全宽读——x2 高 32 位脏值同拒，不做 u32 截断
    // 受纳）。
    if let Err(_e) = berry_dpgface::tblwin::n_gate(n) {
        return ERR_INVALID_ARG as i64;
    }
    let el1 = caller_is_el1(tf);
    let mut raw = [0u8; PAGER_TABLE_MAX * core::mem::size_of::<PagerWindow>()];
    if let Err(e) = copy_from_user(el1, tbl_p, &mut raw[..n * 16]) {
        return e as i64; // -EFAULT 透传（policy_tbl_parse 同口径）
    }
    // 整表装配单源（拼装 + 预检首错即停——判序 = EFAULT → delta 拒 →
    // 门控；fail-fast 先于门控，参数面整批拒——0x23 先例）。
    let (wins_tbl, _) = match berry_dpgface::tblwin::decode_table(&raw, n) {
        Ok(v) => v,
        Err(_) => return ERR_INVALID_ARG as i64,
    };
    let wins: [PagerWindow; PAGER_TABLE_MAX] = {
        let mut w = [PagerWindow { dv: 0, sv: 0, delta: 0 }; PAGER_TABLE_MAX];
        for i in 0..n {
            w[i] = PagerWindow {
                dv: wins_tbl[i].dv,
                sv: wins_tbl[i].sv,
                delta: wins_tbl[i].delta,
            };
        }
        w
    };
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    // 整批门控先行——错柄 fail fast 不入睡（0x23 门控纪律的整批版；
    // get_raw 不 retain，此段零回滚面）。
    let mut vds: [Option<NonNull<ObjRef>>; PAGER_TABLE_MAX] = [None; PAGER_TABLE_MAX];
    let mut svs: [Option<NonNull<ObjRef>>; PAGER_TABLE_MAX] = [None; PAGER_TABLE_MAX];
    for i in 0..n {
        match t.get_raw(wins[i].dv, ObjType::Vmo, RIGHT_SET_POLICY) {
            Ok((o, _)) => vds[i] = Some(o),
            // 门败（坏句柄 -EBADF / 权利缺 -EACCES）——boot 探测判据形。
            Err(e) => return e as i64,
        }
        if wins[i].sv != 0 {
            match t.get_raw(wins[i].sv, ObjType::Vmo, RIGHT_READ) {
                Ok((o, _)) => svs[i] = Some(o),
                Err(e) => return e as i64,
            }
        }
    }
    // 跨睡眠钉活（0x23 单对 retain 纪律推广到 2N——睡眠期本表可变，retain
    // 钉对象活性）；表内重复 handle 合法（各项独立 retain/release 守恒，
    // `wait_many` 同款）。
    for i in 0..n {
        // SAFETY: get_raw 门控后对象在表；release 与此成对（唯一出口）。
        unsafe { (*vds[i].unwrap().as_ptr()).retain() };
        if let Some(s) = svs[i] {
            unsafe { (*s.as_ptr()).retain() };
        }
    }
    // dv 本体指针整批预解析一次（retain 钉活 → 跨 block 睡眠有效；供给体
    // 身份验同源口径——get_raw 已门控 ObjType::Vmo，None 臂防御复检）。
    let mut dv_ptrs = [core::ptr::null_mut::<Vmo>(); PAGER_TABLE_MAX];
    for i in 0..n {
        // SAFETY: get_raw 门控后取本体（只取地址；retain 钉活跨睡眠）。
        dv_ptrs[i] = match unsafe { (*vds[i].unwrap().as_ptr()).obj.as_vmo_mut() } {
            Some(v) => v as *mut Vmo,
            None => {
                // SAFETY: 与 retain 段成对（防御臂收尾，同唯一出口）。
                unsafe { release_wins(&vds, &svs) };
                return ERR_BAD_HANDLE as i64;
            }
        };
    }
    // 收货主环：逐 dv 内环 continue-take 抽干；全表扫毕无池空 → 聚合出环。
    let mut count: usize = 0;
    let mut last_err: Option<i64> = None; // 供给执行体错误（0 供时 errno 候选）
    loop {
        let mut park = false;
        'wins: for i in 0..n {
            loop {
                match crate::dpg::el0_take_for(dv_ptrs[i]) {
                    crate::dpg::El0Take::Supplied { far, asid, slot } => {
                        let sel = SrcOff::Delta(wins[i].delta);
                        let r = pager_supply_one(vds[i].unwrap(), svs[i], &sel, far, asid);
                        // 恒唤**本槽** faulter（0x23 同义：成功 = eret 命中
                        // 新叶；失败 = 重试再 fault 再认领自愈）；count-wins
                        // 只计成功供给（失败不计数不回滚）。
                        crate::dpg::el0_wake_faulter(slot);
                        // P0-1 线1d fastpath 直切（同 pager_supply_common
                        // 唤醒点，SVC 窗条件/舞蹈/同层 FIFO 论证同彼注）：
                        // 批内每页供给毕即探峰、命中即切——批量吞吐与单页
                        // 延迟兼得。跨切点池态可变由 continue-take 每
                        // el0_take_for 全池重扫吸收（armed swap 消费幂等，
                        // 「整批一窗」的原子性本就只依托 KL 串行跨核写者
                        // ——切点放锁后与本核他者交错同 SMP 语义）；
                        // SUPPLY_BUF 借用在 pager_supply_one 内已收口，无
                        // 跨切点存活借用。
                        berry_sched::try_preempt();
                        if r == 0 {
                            count += 1;
                        } else {
                            last_err = Some(r);
                        }
                        continue; // continue-take：本 dv 抽干续取
                    }
                    // 本窗无武装臂——跳次窗（零消费零唤醒，让位面同 0x23）。
                    crate::dpg::El0Take::NoneForVd => break,
                    crate::dpg::El0Take::PoolEmpty => {
                        if count > 0 {
                            break 'wins; // 已供>0：即返计数不阻塞（调用方有进展）
                        }
                        park = true;
                        break 'wins; // 0 供池空——出环泊键
                    }
                }
            }
        }
        if !park {
            break; // 全表扫毕（各窗皆 NoneForVd 出内环）——聚合出环
        }
        // 池空且 0 供：睡 EL0 pager 键（同 0x23 阻塞收货——SVC 入口类持锁
        // 的放/取舞蹈由 block_current 承担；KL 全谱锁窗内查位+入队 = 无丢
        // 唤醒）；醒后重扫整表（虚醒由重扫吸收）。
        let _ = berry_sched::block_current(
            crate::dpg::el0_pager_key(),
            crate::dpg::DPG_WAKE_BIT,
            u64::MAX,
        );
    }
    let ret = if count > 0 {
        count as i64
    } else {
        // 0 供才报 errno：聚合 NoneForVd → -EINVAL；供给执行体错误 → 本码
        //（0x23 单发同码面的批形收口）。
        last_err.unwrap_or(ERR_INVALID_ARG as i64)
    };
    // SAFETY: 与 retain 段成对（唯一出口）。
    unsafe { release_wins(&vds, &svs) };
    ret
}

/// 0x24 表批 2N ObjRefs 统一释放（单一出口，与整批 retain 成对）。
///
/// # SAFETY
/// 各 Some 指针须均经 `get_raw` 门控 + `retain`（与调用方成对，恰释放一次）。
unsafe fn release_wins(
    vds: &[Option<NonNull<ObjRef>>; PAGER_TABLE_MAX],
    svs: &[Option<NonNull<ObjRef>>; PAGER_TABLE_MAX],
) {
    for v in vds.iter().flatten() {
        // SAFETY: 与整批 retain 成对（恰释放一次）。
        unsafe { ObjRef::release(*v) };
    }
    for s in svs.iter().flatten() {
        // SAFETY: 与整批 retain 成对（恰释放一次）。
        unsafe { ObjRef::release(*s) };
    }
}

/// 0x21/0x23 源偏选择：Fixed = 调用方钉死单偏（0x21）；Delta = 目标页偏 +
/// 常量偏置按请求页现算（0x23）。
enum SrcOff {
    Fixed(usize),
    Delta(i64),
}

/// 0x21/0x23 共体（阻塞收货主环）：门控先行（错柄 fail fast 不入睡）→
/// retain 钉活 → poll-first 收货环（无武装请求睡 EL0 pager 键；有武装但
/// 属主无一匹配 → -EINVAL 让位不入睡，X-5）→ 出口成对 release。两号仅
/// 源偏选择不同（SrcOff）。
fn pager_supply_common(tf: &mut TrapFrame, vh: u32, sh: u32, sel: SrcOff) -> i64 {
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    // 目标/源门控先行——错柄 fail fast 不入睡（阻塞收货前收口校验面）。
    let vd = match t.get_raw(vh, ObjType::Vmo, RIGHT_SET_POLICY) {
        Ok((o, _)) => o,
        Err(e) => return e as i64,
    };
    let sv = if sh != 0 {
        match t.get_raw(sh, ObjType::Vmo, RIGHT_READ) {
            Ok((o, _)) => Some(o),
            Err(e) => return e as i64,
        }
    } else {
        None
    };
    // 跨睡眠钉住（wait_one 同款：睡眠期本表可变，retain 钉对象活性；唯一
    // 出口成对 release）。
    // SAFETY: get_raw 门控后对象在表；release 与此成对（唯一出口）。
    unsafe { (*vd.as_ptr()).retain() };
    if let Some(s) = sv {
        unsafe { (*s.as_ptr()).retain() };
    }
    // X-5 过滤取输入：dv 本体指针先行解析一次（retain 已钉活 → 跨 block
    // 睡眠有效；供给体 pager_supply_one 身份验同源口径——get_raw 已门控
    // ObjType::Vmo，None 臂防御复检同风格）。
    // SAFETY: get_raw 门控后取本体（只取地址；retain 钉活跨睡眠）。
    let dv_ptr = match unsafe { (*vd.as_ptr()).obj.as_vmo_mut() } {
        Some(v) => v as *mut Vmo,
        None => {
            // SAFETY: 与入口 retain 成对（防御臂收尾，同唯一出口）。
            unsafe { ObjRef::release(vd) };
            if let Some(s) = sv {
                unsafe { ObjRef::release(s) };
            }
            return ERR_BAD_HANDLE as i64;
        }
    };
    let ret = loop {
        match crate::dpg::el0_take_for(dv_ptr) {
            crate::dpg::El0Take::Supplied { far, asid, slot } => {
                let r = pager_supply_one(vd, sv, &sel, far, asid);
                // 恒唤**本槽** faulter（B 刀键化定向唤——自愈语义不变：
                // 成功 = eret 命中新叶；失败 = 重试再 fault 再认领，pager
                // 重调 0x21/0x23 自愈；供给窗收口后出环）。
                crate::dpg::el0_wake_faulter(slot);
                // P0-1 线1d fastpath 直切（sys_channel_send fastpath 同款
                // 参照）：唤醒只入环不抢占——被唤 faulter eff 严格更高时
                // 立即直切，供给毕即回缺页者，免整调度片等待。SVC 窗调用
                // 条件天然满足：IRQ 屏蔽 + 本号类位集含 SQ/RQ（§A126——
                // try_preempt 的 rq_lock 退化形）；切走经舞蹈按 held 位放
                // 锁、切回重取（位保持置位，svc_exit_class 出口断言恒
                // 成立）；供给锁窗口不受扰——supply_page + 源拷贝已在上
                // 行 pager_supply_one 内收口，直切点在临界段外；retain
                // 钉活跨切换有效。同层 FIFO 不插队（严格更高才切，§A43）。
                berry_sched::try_preempt();
                break r;
            }
            crate::dpg::El0Take::NoneForVd => {
                // 有武装请求但无一属主匹配本 dv（X-5 错配消耗舞修刀）：
                // 不消费不唤醒直接出——-EINVAL 让 pager 出让收货窗（svcmgr
                // 恒扫环换下一 dv 窗/下一轮再收；chan 非阻塞空取同语义）。
                // 老形此局取走他槽 → 身份验 -EINVAL + 槽已耗 + faulter 恒唤
                // 重臂 → 舞不收敛（收敛纯靠时序运气）。rc 取 -EINVAL 非
                // -EAGAIN：与老形冒充臂同码——dpgn ④ 冒充断言
                // （`supply_pages(dvb,0,0) != -EINVAL` 即早退）零漂移、四
                // smoke 面不破；`let _ =` 盲进恒扫环消费 rc 无面。
                break ERR_INVALID_ARG as i64;
            }
            crate::dpg::El0Take::PoolEmpty => {
                // 无武装请求：睡 EL0 pager 键（0x21/0x23 = 阻塞收货）。SVC 入口
                // 类持锁的放/取舞蹈由 block_current 承担（wait_one 先例）；
                // KL 全谱锁窗内查位+入队 = 无丢唤醒（跨核供给者/探针写者
                // 被 KL 串行，SMP 同验——dpg_pager 同构；G4-2 修批 C3 口径）。
                let _ = berry_sched::block_current(
                    crate::dpg::el0_pager_key(),
                    crate::dpg::DPG_WAKE_BIT,
                    u64::MAX,
                );
                // 醒后重取锁——续环再 poll（虚醒由下轮 take 吸收）。
            }
        }
    };
    // SAFETY: 与入口 retain 成对（唯一出口）。
    unsafe { ObjRef::release(vd) };
    if let Some(s) = sv {
        unsafe { ObjRef::release(s) };
    }
    ret
}

/// 0x21/0x23 供给执行体（一次武装请求的收货窗；KL 在身——SVC 入口类
/// TABLE+SQ+RQ）。源页偏由 `sel` 选：Fixed 调用方钉死（0x21）/ Delta 按
/// 请求页现算（0x23）。
fn pager_supply_one(
    vd: NonNull<ObjRef>,
    sv: Option<NonNull<ObjRef>>,
    sel: &SrcOff,
    far: usize,
    asid: u16,
) -> i64 {
    let Some(root) = crate::vmar::demand_root(asid) else {
        return ERR_INVALID_ARG as i64;
    };
    // SAFETY: root 指针 = 本 KL 窗内 fresh 查得（本函数 KL 在身——SVC 入口
    // 类 TABLE+SQ+RQ）；注销（`Vmar::Drop`）与对象释放都发生在 KL 窗内
    // ——窗内查得即窗内活，他核注销须先取同把 KL（G4-2 修批 P0-1 面 b
    // 改口——「只进不出先形」论证退役）。
    let Some((rvmo, vmo_off, base)) = (unsafe { (*root).demand_region_at(far) }) else {
        return ERR_INVALID_ARG as i64;
    };
    // 目标匹配验（§5-③）：请求区间所属 Vmo 须即调用方 vmo 句柄本体——冒充
    //（异 VMO 供给他人区间）/跨进程请求拒。只取身份不持借（供给窗内
    // supply_page 经 Region 指针另有 &mut——KL 串行下的既有点名纪律）。
    // 地址复用混票面已闭（G4-2 修批）：rvmo 经 demand 钉活（销毁拒面）、
    // dv 经调用方 retain 活——双活对象同址即同体，`!=` 恒判真异体。
    // SAFETY: get_raw 门控后取本体（只取地址）。
    let dv_ptr = match unsafe { (*vd.as_ptr()).obj.as_vmo_mut() } {
        Some(v) => v as *mut Vmo,
        None => return ERR_BAD_HANDLE as i64,
    };
    if rvmo != dv_ptr {
        return ERR_INVALID_ARG as i64;
    }
    let va = far & !(PAGE_SIZE - 1);
    // 供给窗（dpg_pager 同链搬进 SVC 面：pt_lock 嵌 KL 正序 + 幂等门 +
    // 填叶 + dsb/tlbi/broadcast + track_map）。
    // SAFETY: root 常驻论证同上；dv_ptr 指针身份已验。
    if let Err(e) = unsafe { (*root).supply_page(crate::mm::frames(), va) } {
        return vme(e) as i64;
    }
    // P1-3 ZFOD 学习点：零臂（sv=None——0x21/0x23/0x24 的 src=0）供给
    // 成功 = pager 亲证「本 VMO 无内容后备」（零臂语义 = 供给页即清零
    // 页）→ 置标志，此后本 VMO 的缺页走 `probe_el0` 内核直供快路径
    // （供给页与零臂产物逐字节等价——`Vmo::zfod` 字段注「一 VMO 一种类」
    // 契约）。有源臂（sv=Some）不置位——含源读失败形（DPGN ③
    // -EBADSTATE：子恒唤收零页 exit 9，后续缺页仍走 pager 链零漂移）。
    // SAFETY: dv_ptr 身份已验（Region 同体）；本函数 KL 契约窗内（与
    // 探针读点同锁域串行）。
    if sv.is_none() {
        unsafe { &mut *dv_ptr }.mark_zfod();
    }
    if let Some(s) = sv {
        // 源拷贝（防线 4：源页未提交 → read 报错拒——supply 链零缺页）。
        // SAFETY: get_raw 门控；KL 全谱锁窗串行（0x21 SVC 入口类 KL 在身
        // ——跨核供给者互斥；「单核屏蔽」论证退役，C3 口径同 :852 先例）。
        let Some(src) = (unsafe { (*s.as_ptr()).obj.as_vmo_mut() }) else {
            return ERR_BAD_HANDLE as i64;
        };
        let toff = vmo_off + (va - base);
        // 源偏选择：0x21 调用方钉死；0x23 恒等 delta 按请求页现算
        //（下溢/溢出 checked 拒——页对齐 delta + 页对齐 toff ⇒ 页对齐 soff）。
        let soff = match sel {
            SrcOff::Fixed(s) => *s,
            SrcOff::Delta(d) => match toff.checked_add_signed(*d as isize) {
                Some(s) => s,
                None => return ERR_INVALID_ARG as i64,
            },
        };
        let buf = crate::dpg::supply_buf();
        if let Err(e) = src.read(soff, buf) {
            return voe(e) as i64;
        }
        // SAFETY: dv_ptr 身份已验（Region 同体）；供给窗已提交该页。
        if let Err(e) = unsafe { &mut *dv_ptr }.write(toff, buf) {
            return voe(e) as i64;
        }
    }
    0
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
    if let Err(e) = copy_out_u32(el1, child_out, 0) {
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
            let r2 = copy_out_u32(el1, child_out, ch);
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
/// 出权：process = MANAGE|DUPLICATE|TRANSFER，root Vmar =
/// MAP|MANAGE|DUPLICATE|TRANSFER（boot root 同面 + TRANSFER——Zircon 新
/// handle 默认携位模型；berry-exec env 块把 self-proc/self-vmar dup 随
/// channel_send 迁交子进程，去位即断赠链）。v1 参数面：`job` 恒 0（Job
/// 对象 v2——**创建权未设防的已知豁口**，落地记录见能力模型）、`options`
/// 恒 0。
extern "C" fn sys_process_create(tf: &mut TrapFrame) -> i64 {
    let (job, options, p_out, v_out) = (tf.regs[1], tf.regs[2] as u32, tf.regs[3], tf.regs[4]);
    if job != 0 || options != 0 {
        return ERR_INVALID_ARG as i64;
    }
    let el1 = caller_is_el1(tf);
    if let Err(e) = copy_out_u32(el1, p_out, 0) {
        return e as i64;
    }
    if let Err(e) = copy_out_u32(el1, v_out, 0) {
        return e as i64;
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    // P3-3：per-parent spawn 配额——防进程炸弹（全局 64 帽前置的单父分账：
    // 无它则单 EL0 循环 create 即占满全局表、阻止其他服务拉起）。记账 =
    // 本表在册 Process handle 数（`count_type` 扫描——VMO/Channel 帽同款，
    // 随 close/take/drain/迁移天然配平、无退出递减面；sidecar 否决理由与
    // 帽值余量论证见 `process.rs` `MAX_CHILDREN_PER_PROC` 注）。
    if t.count_type(ObjType::Process) >= crate::process::MAX_CHILDREN_PER_PROC {
        return ERR_NO_MEMORY as i64; // 与全局帽同错误面
    }
    // 活进程帽（复盘雷 4）：无退出路径 + 创建权未设防 → 循环 create 的
    // 帧/Box 永不释放——超帽拒 -ENOMEM（`PROCESS_MAX` 见 process.rs）。
    if crate::process::process_live() >= crate::process::PROCESS_MAX {
        return ERR_NO_MEMORY as i64;
    }
    // 帧耗尽折 -ENOMEM（3b3 H1：new_root 曾内嵌 expect——EL0 一次 process_create
    // 即可让内核 panic 停机）。
    // F0-2：ASID 耗尽诚实返 -ENOMEM（绝不撞号）；建表失败时立即归还刚取出的 ASID。
    let Some(new_asid) = crate::aspace::try_alloc_asid() else {
        return ERR_NO_MEMORY as i64;
    };
    let mut vmar = match Vmar::new_root(crate::mm::frames(), new_asid) {
        Ok(v) => v,
        Err(e) => {
            crate::aspace::free_asid(new_asid);
            return vme(e) as i64;
        }
    };
    // W3-Q106：vDSO 数据页映入生产 AS（失败同 new_root 折 -ENOMEM 回滚——
    // vmar 正常 drop 放表树帧，vDSO VMO 永生 static 不受影响）。
    if let Err(e) = crate::vdso::map_into(crate::mm::frames(), &mut vmar) {
        return vme(e) as i64;
    }
    let (ttbr0, asid) = (vmar.ttbr0_pa(), vmar.asid());
    let hv = match t.insert(
        KernelObj::Vmar(vmar),
        RIGHT_MAP | RIGHT_MANAGE | RIGHT_DUPLICATE | RIGHT_TRANSFER,
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
    match t.insert(KernelObj::Process(proc), RIGHT_MANAGE | RIGHT_DUPLICATE | RIGHT_TRANSFER) {
        Ok(hp) => {
            let r1 = copy_out_u32(el1, p_out, hp);
            let r2 = copy_out_u32(el1, v_out, hv);
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
    if let Err(e) = copy_out_u32(el1, out, 0) {
        return e as i64;
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let Ok((pobj, _)) = t.get_raw(ph, ObjType::Process, RIGHT_MANAGE) else {
        return ERR_BAD_HANDLE as i64;
    };
    // SAFETY: get_raw 已验在表且类型 Process；Thread 构造即钉住它。
    let thrd = unsafe { crate::process::Thread::new(pobj) };
    // 1.1/S1/S3-b：创建者初始授 SET_POLICY（attr/FREEZE_READY 声明权）与
    // INSPECT（effective 查询）。仍可通过 duplicate 收紧/委托，不新增提权口。
    match t.insert(
        KernelObj::Thread(thrd),
        RIGHT_MANAGE | RIGHT_DUPLICATE | RIGHT_SET_POLICY | RIGHT_INSPECT,
    ) {
        Ok(h) => match copy_out_u32(el1, out, h) {
            Ok(()) => 0,
            Err(e) => e as i64,
        },
        Err(e) => e as i64, // Thread 入表失败：对象 drop 时 release 钉住计数，无泄漏
    }
}

/// `thread_start(thread, entry, sp, arg)`（0x35，T-1 多线程首刀）：投入运行
/// **非首线程**（首线程随 [`sys_process_start`]——本号只管进程内造线程，
/// Zircon zx_thread_start 同形四参）。门控：thread **MANAGE**。`arg` 落新线程
/// x0（与首线程 bootstrap 迁移值对称——pthread 层递 (fn, arg) 的载体）。
///
/// 校验序（镜像 [`sys_process_start`]）：`entry`==0 / `sp` 非 16 对齐 →
/// -EINVAL；坏值/类型不符 → -EBADF、缺 MANAGE → -EACCES；thread 已
/// started/exited → -EBADSTATE；所属 Process 未 start 或非 Alive →
/// -EBADSTATE（首线程必须经 process_start）。全过后与首线程同一
/// [`berry_sched::try_spawn_user`] 入口（无 bootstrap 迁移——失败零副作用，
/// try 化在一切不可逆变更之前，None → -ENOMEM）。
///
/// 成功尾四连写：`thrd.started=true`；`proc.live_threads+=1`；Process
/// retain（运行钉住，同首线程对）；Thread retain（线程钉住——退出钩子
/// Thread 簿记/join 唤醒的键）。pinned/thread_key 字段值经 try_spawn_user
/// 参数前移写入（F-E1，enqueue 前）。
/// **per-Process 线程帽**（Medium 缺口收口）：`live_threads ≥
/// MAX_THREADS_PER_PROC` → -ENOMEM（16KiB 栈 + TCB 随 REAPQ 回收，但运行期
/// 占堆——无帽可循环铺线程耗尽共享堆；查先于一切不可逆变更）。
/// sp 不做映射校验
/// （process_start 同政策：错 sp = 首切 fault 杀兜底）；线程入口不得返回
/// （返回 = fault 杀兜底）。
/// 1.1/S5-a：把 effective `SchedAttr` 消费到刚 spawn 的 Task。
///
/// - `intent`：声明/有效档双写（flag 未置 = 继承调度器默认 0）。
/// - `policy=3`：`prio_base/effective = PRIO_DEADLINE`，并把相对 deadline
///  （缺省 = period）换算成绝对 CNTVCT 写 `vdeadline`——Fair 带序 + 带内 VD
///   即 EDF；deadline 带的 enqueue/tick 不覆盖 VD。
/// - `weight` 消费挂 S2b（TCB 不扩字段，映射方案随纯 EEVDF 刀定）。
fn apply_sched_attr_to_task(task_ptr: usize, attr: crate::process::SchedAttr) {
    let tp = task_ptr as *mut berry_sched::Task;
    if attr.flags & crate::process::SA_FLAG_INTENT != 0 {
        // SAFETY: task 刚由 try_spawn_user 返回且尚未运行（单核 IRQ 屏蔽窗）。
        unsafe {
            (*tp).intent_declared = attr.intent;
            (*tp).intent_effective = attr.intent;
        }
    }
    if attr.flags & crate::process::SA_FLAG_POLICY != 0
        && attr.policy == 3
        && attr.flags & crate::process::SA_FLAG_BUDGET != 0
    {
        let rel_ns = if attr.flags & crate::process::SA_FLAG_DEADLINE != 0 {
            attr.deadline_ns
        } else {
            attr.period_ns
        };
        let abs_ns = vct_ns().saturating_add(rel_ns);
        if let Some(vct) = ns_to_vct(abs_ns) {
            // SAFETY: 同上；spawn 后、eret 前无并发执行。
            unsafe {
                (*tp).prio_base = berry_sched::PRIO_DEADLINE;
                (*tp).prio_effective = berry_sched::PRIO_DEADLINE;
                (*tp).vdeadline = vct;
                // S5-c 预算执法：周期存 duration_hint_ns（lazy unthrottle 重臂
                // vdeadline 消费）；预算存 preempt_threshold（tick 面切预算
                // 尺寸 slice 消费）。两字段此前零消费——零 Task 布局变化。
                if let Some(period_vct) = ns_to_vct(attr.period_ns) {
                    (*tp).duration_hint_ns = period_vct;
                }
                if let Some(budget_vct) = ns_to_vct(attr.budget_ns) {
                    let b = budget_vct.min(i32::MAX as u64) as i32;
                    (*tp).preempt_threshold = b;
                    (*tp).slice_left = b as u64; // 首期预算 = slice 尺寸
                }
            }
        }
    }
}

extern "C" fn sys_thread_start(tf: &mut TrapFrame) -> i64 {
    let (th, entry, sp, arg) = (tf.regs[1] as u32, tf.regs[2], tf.regs[3], tf.regs[4]);
    if entry == 0 || sp == 0 || sp % 16 != 0 {
        return ERR_INVALID_ARG as i64;
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let vt = match t.get_raw(th, ObjType::Thread, RIGHT_MANAGE) {
        Ok((o, _)) => o,
        Err(e) => return e as i64, // 坏值/类型不符 -EBADF；缺 MANAGE -EACCES
    };
    // SAFETY: get_raw 已三门控（在表、类型 Thread、权利足）；单核 IRQ 屏蔽。
    let thrd = unsafe { (*vt.as_ptr()).obj.as_thread_mut() };
    let Some(thrd) = thrd else {
        return ERR_BAD_HANDLE as i64; // 类型门控后不可达；防御面
    };
    // SAFETY: owner() 来自 Thread 构造期 retain 的钉住——对象必活且类型互异
    // 无别名（论证同 process_start 双对象同借；T-1 定案序：owner 取 Process
    // 先于 thrd.started 检查——F3b「未 start 进程」-37 落在下方两 -EBADSTATE
    // 臂，语义与检查次序无观测差，仍按定案序书写）。
    let proc_ref = thrd.owner();
    let proc = unsafe { (*proc_ref.as_ptr()).obj.as_process_mut() };
    let Some(proc) = proc else {
        return ERR_BAD_HANDLE as i64; // 防御面（Thread 钉住的对象必为 Process）
    };
    if thrd.started || thrd.exited {
        return ERR_BAD_STATE as i64; // 单次启动执法（同 process_start 口径）
    }
    if proc.state != ProcState::Alive || !proc.started {
        return ERR_BAD_STATE as i64; // 首线程必须经 process_start
    }
    // per-Process 线程帽（Medium 缺口收口，`process.rs` 帽常量注）：目标进程
    // 已启动线程数（`live_threads`——增点三/减点唯一 = 退出钩子，单核 IRQ
    // 屏蔽串行）≥ 帽先拒 -ENOMEM——每线 16KiB 内核栈 + TCB 运行期占堆，无帽
    // 单进程循环 thread_start 可耗尽共享堆。查在一切不可逆变更之前（admission
    // 只查不写，沿革 #109 纪律）；首线程走 process_start（此刻 live_threads
    // 恒 0，帽结构性不可达）。
    if proc.live_threads >= crate::process::MAX_THREADS_PER_PROC {
        return ERR_NO_MEMORY as i64;
    }
    let ht_ptr = &raw mut proc.ht as *mut HandleTable as usize;
    // 任务装配 try 化 + 前置于一切不可逆变更（process_start H2 纪律；本号无
    // bootstrap 迁移——失败零回滚面）：栈/TCB OOM → -ENOMEM。spread=true
    // （b-1 判谳）：同进程计算线程 = 铺核面（thread_start 专属——process 根
    // 恒家核，详 sched::place ④）。
    // S-6 SMP 竞态收口：一切可被新任务退出钩子观察的簿记必须在 enqueue
    // 前完成。旧序“先入环、后 started/live_threads/retain”，在跨核偷取下
    // 新线程可能在父 syscall 继续前退出：live_threads 先减会使 Process 被
    // 误判末线程并 drain 活主线程的表（SMP run7 DF/du 实录）。
    thrd.started = true;
    proc.live_threads += 1;
    // SAFETY: 双 retain 与退出钩子 release 成对；对象均在表且类型已验。
    unsafe { (*proc_ref.as_ptr()).retain() };
    unsafe { (*vt.as_ptr()).retain() };
    let eff = crate::process::SchedAttr::merge(proc.sched_attr, thrd.sched_attr);
    let task_ptr = match berry_sched::try_prepare_user(
        entry as usize,
        arg as usize,
        0, // x1 env 旗标：EL0 拉起的线程无 boot env 面（spawn_user_env 专属）
        sp as usize,
        b"user_thread",
        proc.ttbr0_pa,
        proc.asid,
        ht_ptr,
        proc_ref.as_ptr() as usize,
        vt.as_ptr() as usize,
        true,
    ) {
        Some(p) => p,
        None => {
            thrd.started = false;
            proc.live_threads -= 1;
            // SAFETY: 与上方预登记 retain 成对回滚。
            unsafe { ObjRef::release(proc_ref) };
            unsafe { ObjRef::release(vt) };
            return ERR_NO_MEMORY as i64;
        }
    };
    apply_sched_attr_to_task(task_ptr, eff);
    // SAFETY: prepare 后、簿记/属性已毕；本函数全程 IRQ 屏蔽。
    unsafe { berry_sched::enqueue_prepared_user(task_ptr) };
    0
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
/// `started=true`、**替运行中的任务再钉一次 Process 与 Thread**（T-1：Thread
/// 钉住让首线程 join 面成立——退出钩子① 按 `thread_key` 置 `exited`/唤醒/
/// release；v1 任务不退出永不释放——防 handle 全关后对象被销毁），
/// `spawn_user` 入队（tag = 子进程表指针——子的 EL0 syscall 从此派发到自己
/// 的表）。
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
    // 对抗审查修（Plan B）：per-Process 单次执法——只查 per-Thread 时
    // thread_create×2 + start×2 可造共享同一 ht 的双任务（首任务退出即
    // drain 活兄弟的表）；`state != Alive` 同时封「对已退出进程再 start」。
    // Zircon zx_process_start 的 ZX_ERR_BAD_STATE 同形。
    if thrd.started || proc.started || proc.state != ProcState::Alive {
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
    // 任务在 eret 前不会运行，下面字段补写无竞态。spread=false（b-1 判谳）：
    // process 根 = 服务/IPC 公民恒家核（详 sched::place ④）。
    // S-6 SMP：与 thread_start 同款“簿记先于入环”。首线程可在对核立刻运行，
    // Process.started/live_threads 与运行 retain/Thread retain 不得后补。
    thrd.started = true;
    proc.started = true;
    proc.live_threads = 1;
    // SAFETY: 双 retain 与退出钩子 release 成对；对象均在表且归属已验。
    unsafe { (*vr.as_ptr()).retain() };
    unsafe { (*vt.as_ptr()).retain() };
    let eff = crate::process::SchedAttr::merge(proc.sched_attr, thrd.sched_attr);
    let task_ptr = match berry_sched::try_prepare_user(
        entry as usize,
        child_x0,
        0, // x1 env 旗标：EL0 拉起的进程无 boot env 面（spawn_user_env 专属）
        sp as usize,
        b"user_child",
        proc.ttbr0_pa,
        proc.asid,
        ht_ptr,
        vr.as_ptr() as usize,
        vt.as_ptr() as usize,
        false,
    ) {
        Some(p) => p,
        None => {
            thrd.started = false;
            proc.started = false;
            proc.live_threads = 0;
            // SAFETY: 与上方预登记 retain 成对回滚。
            unsafe { ObjRef::release(vr) };
            unsafe { ObjRef::release(vt) };
            if bootstrap != 0 {
                // 迁移回滚：把子表令牌搬回调用者表（值可变，语义存活）。
                if let Ok((obj, rights)) = proc.ht.take(child_x0 as u32) {
                    let _ = t.insert_ref(obj, rights);
                }
            }
            return ERR_NO_MEMORY as i64;
        }
    };
    apply_sched_attr_to_task(task_ptr, eff);
    // SAFETY: prepare 后、Process/Thread 簿记与 effective 属性已毕。
    unsafe { berry_sched::enqueue_prepared_user(task_ptr) };
    0
}

// ---- process/thread 退出（Plan B v0.5：0x33/0x36 真实现；T-1 多线程重梳）----

/// 任务退出钩子（sched `switch_away_forever_with` 在切走前调；`set_exit_hook`
/// 注册于 [`install`]）。T-1 分序版（每任务一套，语义序 = 安全序——**顺序即
/// 正确性，颠倒即 UAF/漏醒/drain 双跑**）：
///
/// - `pinned == 0`（内核任务 / boot 手搭裸表用户进程）直通——**绝不 drain
///   私有表**：ai bench ping/pong 共享一张 tag 表，退出 drain 会扬活任务的
///   句柄（4-6 共享表粒度坑的退出版）。
/// - ①`thread_key != 0`（`thread_start` 造的非首线程 + `process_start` 造的
///   首线程——T-1 修复后**两类任务都钉 Thread**）：Thread 簿记——置
///   `exited`（join 面永真位）→ `wake_matching(thread_key, TERMINATED)` 唤醒
///   join 等待者 → release 对应 start 的 Thread 钉住（此后 Thread 生死
///   交还 handle 引用计数）。`thread_key == 0` 仅剩防御面（pinned!=0 的任务
///   现恒有 thread_key；修复前首线程不钉 = join 结构性死等）。
/// - ②`live_threads -= 1`：**减到 0 = 末线程**才做 Process 收口 = 前两步
///   同步（`mark_exited` 首因保码 → `wake_matching(pinned, TERMINATED)`）
///   + 第三步 **drain 异步移交**（P2-2：钉住引用 push 进 HT_DRAINQ——
///   O(表) 清理不在 IRQ 屏蔽的切换窗内做，由 REAP 排水点
///   [`crate::process::ht_drain_flush`] 完成）；**非末线程绝不
///   `mark_exited`、绝不 drain/移交**（K1 教训线程版——共享表上活兄弟的
///   handle 不能被先退线程扬掉）。
/// - ③Process 运行钉住引用**所有权移交 HT_DRAINQ**（队列独占持有——
///   flush 侧 `drain → release` 序收口，drain 先于 Process Drop 的安全序
///   不变、只是迟到；此后本钩子不再触 obj）。
///
/// plain `-= 1` 无下溢检查的不变量：`pinned != 0` ⟺ 该任务入过账（
/// `process_start`/`thread_start` 成功尾与 retain/pinned 写同点成对）——
/// 单核 IRQ 屏蔽路径专用（同 `started`/`state`）。
unsafe extern "C" fn task_exit_hook(pinned: usize, thread_key: usize, code: i32) {
    if pinned == 0 {
        return;
    }
    let obj = pinned as *mut ObjRef;
    // SAFETY: pinned 来自 process_start/thread_start 的 retain（运行钉住 ≥1
    // ——本钩子期间对象必活）；IRQ 屏蔽窗内（钩子调用点契约）。
    unsafe {
        if thread_key != 0 {
            let tobj = thread_key as *mut ObjRef;
            let t = &mut *tobj;
            let Some(thrd) = t.obj.as_thread_mut() else { return }; // 防御面（不可达）
            thrd.exited = true; // join 面：永不再跑点置位，单调永不清
            berry_sched::wake_matching(thread_key, SIGNAL_TERMINATED);
            // SAFETY: 与 thread_start 的 retain 成对；此后不再触 tobj。
            ObjRef::release(NonNull::new_unchecked(tobj));
        }
        let r = &mut *obj;
        let Some(proc) = r.obj.as_process_mut() else { return }; // 防御面（不可达）
        proc.live_threads -= 1;
        if proc.live_threads == 0 {
            // 末线程：Process 收口（首因保码 + 唤醒——两步同步保留：join/
            // TERMINATED 等待者不因异步化延迟醒来）。
            if proc.mark_exited() {
                proc.exit_code = code;
                // 唤醒等本 Process TERMINATED 的睡眠者（匹配键 = ObjRef 地址，
                // 同 Event/Channel 面；SLEEPQ 钩子自动触发 PI 重推）。
                berry_sched::wake_matching(pinned, SIGNAL_TERMINATED);
            }
            // P2-2 退出异步化：drain 不在切换窗内做——运行钉住引用移交
            // HT_DRAINQ（O(1)），O(表) drain + release 由 REAP 排水点异步
            // 收口（`process.rs` HT_DRAINQ 块；满队同步回退 = 旧路径原样）。
            // 安全序不变：drain 先于 Process Drop（队列持引用）；唤醒已在
            // 上方同步完成。
            // SAFETY: pinned 引用所有权移交（flush 侧 drain 后 release 成
            // 对）；此后不再触 obj。
            crate::process::defer_ht_drain(NonNull::new_unchecked(obj));
        } else {
            // SAFETY: 与 process_start 的 retain 成对；此后不再触 obj。
            ObjRef::release(NonNull::new_unchecked(obj));
        }
    }
}

/// `process_exit(code) -> !`（0x33，Plan B v0.5；T-1 重梳全进程语义）：退出
/// **整个进程**——先静默杀光兄弟线程（`kill_all_tasks_by_pinned(p, 0)`——
/// 兄弟 code 传 0（诚实语义：非主动退出无自有码，同 fault 杀口径），打印
/// 上移调用方、process_exit 路静默），再 `exit_current_with(code)` 退自己
/// （自己走钩子 = 末线程收口：mark_exited + 码 + TERMINATED 同步 +
/// HT_DRAINQ 移交——drain/release 由 REAP 排水点异步，P2-2）。当前任务非 Process 任务（`current_pinned()==0`——内核任务/
/// boot 裸表用户进程，ktest 禁测 0x33 会杀 ktest）= 只退任务自身，合法面
/// （Zircon thread_exit 同义兜底）。**ABI `-> !`**：实现永不返回（切走即不
/// 再回来）——HAL 派发回写 x0 的路径天然不可达（`exit_current_with` 内部
/// `context_switch` 不归）。
extern "C" fn sys_process_exit(tf: &mut TrapFrame) -> i64 {
    let code = tf.regs[1] as i32;
    let p = berry_sched::current_pinned();
    if p != 0 {
        // 静默杀兄弟（外层重扫循环消化「钩子内 wake_matching 搬 SLEEPQ」的
        // 队形变化；RUNQUEUE 扫描跳过 CURRENT——自己走 exit_current_with）。
        unsafe { berry_sched::kill_all_tasks_by_pinned(p, 0) };
    }
    berry_sched::exit_current_with(code)
}

/// `thread_exit() -> !`（0x36，Plan B v0.5；T-1 重梳）：仅退**当前线程**——
/// `exit_current_with(0)`（Zircon zx_thread_exit 同形无参；码 0 = 线程退出无
/// 自有退出码面）。末线程与否由钩子 `live_threads` 计数裁决：是末线程则
/// Process 收口（同 process_exit）；不是则只做 Thread 簿记 + join 唤醒，
/// 进程续活。
extern "C" fn sys_thread_exit(_tf: &mut TrapFrame) -> i64 {
    berry_sched::exit_current_with(0)
}

/// `process_get_exit_code(process, *code)`（0x3A，P5 shell v0 收官——`&&` 的
/// 真退出码源；**ABI 单源管线首次真实扩号**）。门控仅类型不查权利位（need=0，
/// `vmo_get_size`「对象几何非内容」惯例——退出码是生命周期事实非受保护内容，
/// 12 位 rights 无对应位）。未退出 → **-EAGAIN（poll 语义）**：调用方先
/// `object_wait_one(SIGNAL_TERMINATED)` 再查——与 wait 面 poll-first 哲学一致，
/// 内核不做第二个阻塞位。`*code` = i32（status 宽 4B，`copy_out_u32`）。
extern "C" fn sys_process_get_exit_code(tf: &mut TrapFrame) -> i64 {
    let (h, out) = (tf.regs[1] as u32, tf.regs[2]);
    let el1 = caller_is_el1(tf);
    // 出参预清 + 界检（fail-fast，同 vmo_get_size；code 是 i32 = status 宽 4B）。
    if let Err(e) = copy_out_u32(el1, out, 0) {
        return e as i64;
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let Ok((obj, _)) = t.get(h, ObjType::Process, 0) else {
        return ERR_BAD_HANDLE as i64;
    };
    let Some(p) = obj.as_process() else {
        return ERR_BAD_HANDLE as i64; // 类型门控后不可达；防御面
    };
    if p.state != crate::process::ProcState::Exited {
        return ERR_AGAIN as i64; // poll 语义：先 wait TERMINATED 再查
    }
    match copy_out_u32(el1, out, p.exit_code as u32) {
        Ok(()) => 0,
        Err(e) => e as i64,
    }
}

/// `cbox_drain(buf, cap, *out_len)`（0x42，cbox 刀 1）：黑匣子崩溃记录表全量
/// 快照 copy-out——12B 头 {ver,total,stored} + 存量 104B 记录按 seq 升序。
/// cap==0 = 尺寸查询（`*out_len` = 所需字节，返 0）；cap 不足 → -ENOMEM 不
/// 部分写；cap 高 32 位非零 = -EINVAL（u64 截断成假小帽会绕尺寸契约）；出参
/// 预清 fail-fast（`process_get_exit_code` 同款）。无 right 门（诊断几何面
/// ——need=0 惯例）；结构面零锁但走 SVC_CLASS_TABLE 在册（svc_enter 取锁
/// 纪律同全体 syscall，`cbox.rs` 表读写自身无锁——seqlock 快照）。

/// `restricted_enter(flags)`（0x4A，G2 刀 7）：进入受限态。
/// v1 域面：PSTATE.PAN（AArch64 EL0 SAN）硬件栅栏的内核侧桩——
/// 置当前任务 restricted 标志，后续 syscall 分派按受限态过滤。
/// 返回 0=成功；-ENOSYS=硬件不支持。
extern "C" fn sys_restricted_enter(tf: &mut TrapFrame) -> i64 {
    let _flags = tf.regs[0];
    // AArch64 PAN：PSTATE.PAN=1 时 EL0 不可直接访问 EL1 映射。
    // v1 桩：置任务标志返回成功（linux_runner 刀 8 消费）。
    // TODO(G2 刀 8): 设置 task.restricted = true 并在 syscall 分派面过滤。
    0
}

/// `restricted_kick(target, reason)`（0x4B，G2 刀 7）：踢出受限态。
/// v1 域面：清目标任务 restricted 标志。
extern "C" fn sys_restricted_kick(tf: &mut TrapFrame) -> i64 {
    let _target = tf.regs[0];
    let _reason = tf.regs[1];
    // v1 桩：返回成功（G2 刀 8 linux_runner 消费——紧急逃生面）。
    0
}


/// `guest_create(ipa_base, ipa_size)`（0x77，G2 刀 10）：创建 guest PAS。
/// v1 桩：AArch64 QEMU TCG 无 EL2 → -ENOSYS（真机 EL2 面归后续刀）。
extern "C" fn sys_guest_create(tf: &mut TrapFrame) -> i64 {
    let _ipa_base = tf.regs[0];
    let _ipa_size = tf.regs[1];
    -38 // -ENOSYS
}

/// `vcpu_create(guest_handle)`（0x78，G2 刀 10）：创建 vCPU。
extern "C" fn sys_vcpu_create(tf: &mut TrapFrame) -> i64 {
    let _guest = tf.regs[0];
    -38
}

/// `vcpu_run(vcpu_handle)`（0x79，G2 刀 10）：运行 vCPU。
extern "C" fn sys_vcpu_run(tf: &mut TrapFrame) -> i64 {
    let _vcpu = tf.regs[0];
    -38
}

extern "C" fn sys_cbox_drain(tf: &mut TrapFrame) -> i64 {
    let (buf, cap_raw, out) = (tf.regs[1], tf.regs[2], tf.regs[3]);
    let Ok(cap) = u32::try_from(cap_raw) else {
        return ERR_INVALID_ARG as i64;
    };
    let el1 = caller_is_el1(tf);
    if let Err(e) = copy_out_u32(el1, out, 0) {
        return e as i64;
    }
    // 两遍快照（先尺寸后拷贝）共一锚：drain_total 取一次贯穿两遍——遍间新
    // 崩溃不进窗口，两遍等长；拷贝回的越界分支纯防御（同锚下结构性不可达）。
    let total = crate::cbox::drain_total();
    let mut need = 0usize;
    crate::cbox::drain_with(total, &mut |s| need += s.len());
    if cap == 0 {
        return match copy_out_u32(el1, out, need as u32) {
            Ok(()) => 0,
            Err(e) => e as i64,
        };
    }
    if (cap as usize) < need {
        return ERR_NO_MEMORY as i64;
    }
    let mut off = 0usize;
    let mut err = 0i32;
    crate::cbox::drain_with(total, &mut |s| {
        if err != 0 || off + s.len() > cap as usize {
            err = ERR_NO_MEMORY;
            return;
        }
        match copy_to_user(el1, buf + off as u64, s) {
            Ok(()) => off += s.len(),
            Err(e) => err = e,
        }
    });
    if err != 0 {
        return err as i64;
    }
    match copy_out_u32(el1, out, off as u32) {
        Ok(()) => 0,
        Err(e) => e as i64,
    }
}

/// `process_terminate(process, code)`（0x3B，C-4b）：外部终止进程——kill /
/// Ctrl-C 的内核原语。`code` 落为该进程退出码，父侧收割面与 `process_exit`
/// 逐字同形（wait TERMINATED → `process_get_exit_code`）。
///
/// 门控序（[`sys_object_signal`] 先例）：`get_raw` 三门控（在表 / 类型
/// Process / 权利 **MANAGE**）→ 坏值/类型不符 -EBADF、缺 MANAGE -EACCES；
/// 随后两道状态判均 -EBADSTATE：①**自杀**（目标 ObjRef 地址 == 调用者任务
/// 的 pinned 键）——退自己走 `process_exit`（terminate 要摘的目标任务此刻
/// 正在它自己栈上执行本 syscall，REAPQ 回收自己在结构上不可行）；②已
/// Exited——幂等语义收口在 wait/get_exit_code，不重复杀。
///
/// 收口（**不开第二退出面**）：
/// - **已 start**：[`berry_sched::kill_all_tasks_by_pinned`] 群杀（T-1 起进程
///   可多线程——外层重扫循环消化「钩子内 `wake_matching` 搬 SLEEPQ」的队形
///   变化），逐任务走与自退出同一退出钩子（末任务钩子完成 Process 收口：
///   mark_exited + 码 + TERMINATED 唤醒同步 + HT_DRAINQ 移交——drain/
///   release 由 REAP 排水点异步收口，P2-2）
///   + 摘队 + REAPQ；观测打一行 `TERM: killed '<名>'`（n>1 追加 ` x<n>`
///   ——非 smoke 标记）。**terminate 不再做 started 路径的 Process 侧收口**
///   （末任务钩子已做——重做 = drain 双跑/refs 打穿）。
/// - **未 start**：Process 侧收口 = 钩子语义**减 release**（`process_start`
///   的运行钉住从未发生，release 会把 refs 打穿 0、在调用方 handle 仍指它时
///   销毁对象）；ht 出生为空（bootstrap 迁移在 start 才发生），drain 防御
///   保留。ktest 可终止未 start 进程再断言 -EBADSTATE/退出码——无需 spawn。
///
/// 借用纪律：`kill_all_tasks_by_pinned` 返回 `Some` 即钩子内已逐任务
/// `ObjRef::release`——此后不再解引用 `obj`（调用方表槽持有 ≥1 引用，对象
/// 活到调用方 close）。
extern "C" fn sys_process_terminate(tf: &mut TrapFrame) -> i64 {
    let (h, code) = (tf.regs[1] as u32, tf.regs[2] as i32);
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let obj = match t.get_raw(h, ObjType::Process, RIGHT_MANAGE) {
        Ok((o, _)) => o,
        Err(e) => return e as i64, // 坏值/类型不符 -EBADF；缺 MANAGE -EACCES
    };
    if obj.as_ptr() as usize == berry_sched::current_pinned() {
        return ERR_BAD_STATE as i64; // 自杀：退自己走 process_exit
    }
    // SAFETY: get_raw 已三门控（在表、类型 Process、权利足）；单核 IRQ 屏蔽。
    unsafe {
        let r = &mut *obj.as_ptr();
        let Some(p) = r.obj.as_process_mut() else {
            return ERR_BAD_HANDLE as i64; // 不可达防御面（类型刚验过）
        };
        if p.state == ProcState::Exited {
            return ERR_BAD_STATE as i64; // 已终止：幂等面在 wait/get_exit_code
        }
        if p.started {
            // 已 start：目标任务在场（Alive ⟹ 未退出 ⟹ Ready/Blocked；兄弟
            // 线程任意状态——SLEEPQ/Runqueue 双扫 + 外层重扫循环）。Some 分支
            // 钩子内已逐任务 release——立即返回，不再触 obj；末任务钩子完成
            // Process 收口（本函数不再做 started 路径的 Process 侧收口）。
            if let Some((name, n)) =
                berry_sched::kill_all_tasks_by_pinned(obj.as_ptr() as usize, code)
            {
                // G43/G44 拼行收口：TERM 行五段（含条件群杀计数）在 PRINT_LOCK
                // 内一次拼完——SVC 锁窗内运行，KERN→PRINT 单向嵌套合法、
                // with_print_lock 自适应不复原解蔽。
                berry_hal_aarch64::kern_lock::with_print_lock(|| {
                    berry_hal_aarch64::uart_puts(b"\nTERM: killed '");
                    berry_hal_aarch64::uart_puts(name);
                    berry_hal_aarch64::uart_puts(b"'");
                    if n > 1 {
                        // 群杀计数（手写十进制——no_std 零依赖；任务数 4 槽含哨兵足
                        // 够，n 帽 = 表容量量级 < 10^3）。
                        let mut buf = [0u8; 4];
                        let mut i = buf.len();
                        let mut v = n;
                        while v > 0 {
                            i -= 1;
                            buf[i] = b'0' + (v % 10) as u8;
                            v /= 10;
                        }
                        berry_hal_aarch64::uart_puts(b" x");
                        berry_hal_aarch64::uart_puts(&buf[i..]);
                    }
                    berry_hal_aarch64::uart_puts(b"\n");
                });
                return 0;
            }
            // None + started = 不可达防御面（任务必在场）——落未 start 收口向。
        }
        // 未 start（或上述防御面）：Process 侧收口，无 release（无运行钉住）。
        if p.mark_exited() {
            p.exit_code = code;
            // 唤醒等本 Process TERMINATED 的睡眠者（ktest 期 SLEEPQ 恒空——
            // wake_matching 空队安全返回；SLEEPQ 钩子自动触发 PI 重推）。
            berry_sched::wake_matching(obj.as_ptr() as usize, SIGNAL_TERMINATED);
        }
        p.ht.drain();
    }
    0
}

/// `berry_process_suspend(target, options, *token)`（0x3E，1.1/S3）：
///
/// - `options=0`：目标必须为 Process，冻结该进程全部任务。
/// - `options bit0=SINGLE_THREAD`：目标必须为 Thread，且目标已以
///   `sched_setattr(FREEZE_READY)` 自证用户锁到达安全点；无自证 → `-EBUSY`。
/// - `options bits1+` 保留 → `-ENOTSUP`。
///
/// 解冻 = `handle_close(token)`；多 token 以 depth 计数，最后一个 close 才
/// thaw。单线程最后一次 thaw 会自动清 `FREEZE_READY`。1.1 拒绝自冻与
/// 「进程全冻 × 单线程冻」重叠（单 `HELD_FROZEN` 位不能表达交叉持有）。
extern "C" fn sys_process_suspend(tf: &mut TrapFrame) -> i64 {
    use crate::process::SA_FLAG_FREEZE_READY;
    const SUSPEND_OPT_SINGLE_THREAD: u32 = 1 << 0;

    let (h, options) = (tf.regs[1] as u32, tf.regs[2] as u32);
    let token_out = tf.regs[3] as *mut u32;
    if options & !SUSPEND_OPT_SINGLE_THREAD != 0 {
        return ERR_NOT_SUPPORTED as i64; // INCLUDE_CHILDREN 等 v2
    }
    let single = options & SUSPEND_OPT_SINGLE_THREAD != 0;
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };

    if !single {
        // ---- 进程全冻（S3-a；depth 支持嵌套 token）----
        // S5.12 防 freeze DoS：每 caller 活跃 SuspendToken 上限 64——超限
        // -EBUSY（安全审计 F1 修：原在 sys_process_terminate 错位——suspend
        // 创建路径无帽致 am 持 ≥64 token 后 terminate 恒 -EBUSY 监督 kill 链
        // 瘫痪；检查移到 token 生产者〔本处+thread 臂〕而非消费者）。
        // 帽值单源在 berry-susface（W3-Q38）；判位保 base 形（get_raw 前——
        // token 帽先于 handle 错误码报）。
        if t.count_type(ObjType::SuspendToken) >= berry_susface::SUSPEND_TOKEN_CAP as usize {
            crate::telemetry::freeze_busy();
            return ERR_BUSY as i64;
        }
        let obj = match t.get_raw(h, ObjType::Process, RIGHT_MANAGE) {
            Ok((o, _)) => o,
            // options=0 只收 Process：活 handle 但类型不符 → EINVAL（§5.4）。
            Err(ERR_BAD_HANDLE) if t.alive(h) => return ERR_INVALID_ARG as i64,
            Err(e) => return e as i64,
        };
        if obj.as_ptr() as usize == berry_sched::current_pinned() {
            return ERR_INVALID_ARG as i64; // 自冻拒绝（单核永久停摆）
        }
        let pinned = obj.as_ptr() as usize;
        let first_process_token;
        unsafe {
            let r = &mut *obj.as_ptr();
            let Some(p) = r.obj.as_process_mut() else {
                return ERR_BAD_HANDLE as i64;
            };
            first_process_token = p.freeze_depth == 0;
            // 六门判序（Exited → pager → 粒度互斥 → depth 帽 → precheck）
            // 单源在 berry-susface（W3-Q38 host 钉板）——token 帽已在上方
            // get_raw 前保 base 判位先报，此处装包恒绿位（0）只走后五门。
            let gate = berry_susface::proc_admission(&berry_susface::ProcGateInput {
                caller_tokens: 0,
                state: if p.state == ProcState::Exited {
                    berry_susface::ProcStateP::Exited
                } else {
                    berry_susface::ProcStateP::Running
                },
                pager_load: p.ht.pager_load() as u32,
                freeze_depth: p.freeze_depth,
                thread_freezes: p.thread_freezes,
                precheck: berry_sched::freeze_precheck_by_pinned(pinned),
            });
            match gate {
                berry_susface::Admission::Ok => {}
                berry_susface::Admission::Busy => return ERR_BUSY as i64,
                berry_susface::Admission::BadState => return ERR_BAD_STATE as i64,
            }
        }
        unsafe {
            let r = &mut *obj.as_ptr();
            let Some(p) = r.obj.as_process_mut() else {
                return ERR_BAD_HANDLE as i64;
            };
            p.freeze_depth += 1;
            r.retain();
        }
        // 已冻任务也计匹配数：嵌套第二 token 不要求找到新的未冻任务。
        if berry_sched::freeze_by_pinned(pinned) == 0 {
            unsafe {
                let r = &mut *obj.as_ptr();
                if let Some(p) = r.obj.as_process_mut() {
                    p.freeze_depth -= 1;
                }
                ObjRef::release(obj);
            }
            return ERR_BAD_STATE as i64;
        }
        // AM-1.5 wake-pending：首枚 token 冻结生效 → 枚举目标表给全部
        // Channel 端点打冻结属主标（嵌套层不重复——标已在席）；send 命中
        // 冻端点即触发通知（send 尾分支）。冻进程任务全冻 ⇒ 目标表此后
        // 不可变 ⇒ 标集稳定到 thaw。
        if first_process_token {
            // SAFETY: obj 已 get_raw 类型门控（Process）+ 上文 retain；SVC
            // 屏蔽串行。
            unsafe {
                if let Some(p) = (*obj.as_ptr()).obj.as_process_mut() {
                    p.ht.freeze_channel_ends(obj);
                }
            }
        }
        let tok = crate::suspend::SuspendTok {
            target: crate::suspend::SuspendTarget::Process(obj),
            // thaw 键装载律单源（W3-Q38）：Process = pinned。
            key: berry_susface::thaw_route(berry_susface::TargetKind::Process, pinned, 0),
            since_vct: berry_hal_aarch64::timer::cntvct(),
        };
        let ret = insert_suspend_token(tf, t, tok, token_out);
        if ret == ERR_NO_MEMORY as i64 {
            // insert 失败时 token 未入表，手动撤销本层 depth/retain；仅首枚
            // token 需撤冻面（嵌套失败时既有 token 仍持有冻结义务）。
            if first_process_token {
                berry_sched::thaw_by_pinned(pinned);
                // AM-1.5：撤端点冻标（配平上文的 freeze_channel_ends——
                // 本函数早前的 retain 此刻仍在身 ⇒ 枚举中属主恒活）。此窗
                // 单核屏蔽无他任务可跑 ⇒ wake_pending 必未置位，无需清。
                // SAFETY: 同上（obj 门控过 + retain 在身）。
                unsafe {
                    if let Some(p) = (*obj.as_ptr()).obj.as_process_mut() {
                        p.ht.thaw_channel_ends(obj);
                    }
                }
            }
            unsafe {
                let r = &mut *obj.as_ptr();
                if let Some(p) = r.obj.as_process_mut() {
                    if p.freeze_depth > 0 {
                        p.freeze_depth -= 1;
                    }
                }
                ObjRef::release(obj);
            }
        }
        if ret == 0 {
            // sim-S2 A2：freeze 成功遥测（S5.11 既有 doc 面、调用点本刀补齐——
            // ktest freeze 循环「轮际 token 计数严格递增」断言的载体）。
            crate::telemetry::freeze_ok();
        }
        return ret;
    }

    // ---- 单线程冻结（S3-b SINGLE_THREAD）----
    let obj = match t.get_raw(h, ObjType::Thread, RIGHT_MANAGE) {
        Ok((o, _)) => o,
        // SINGLE_THREAD 只收 Thread：活 handle 但类型不符 → EINVAL（§5.4）。
        Err(ERR_BAD_HANDLE) if t.alive(h) => return ERR_INVALID_ARG as i64,
        Err(e) => return e as i64,
    };
    let thread_key = obj.as_ptr() as usize;
    if thread_key == berry_sched::current_thread_key() {
        return ERR_INVALID_ARG as i64; // 当前线程自冻拒绝
    }
    unsafe {
        let r = &mut *obj.as_ptr();
        let Some(thrd) = r.obj.as_thread_mut() else {
            return ERR_BAD_HANDLE as i64;
        };
        // 目标所属 Process 必须活着且已启动；pager 义务同进程全冻口径保守拒绝。
        // owner 缺席面保 base 形（EBAD_STATE 拒，不 panic）；七门判序单源在
        // berry-susface（W3-Q38 host 钉板）——base 形本臂无 token 帽（F1 帽
        // 只落进程生产者臂），装包恒绿位（0）不引入新拒面。
        let owner = thrd.owner();
        let owner_gate;
        {
            let Some(p) = (*owner.as_ptr()).obj.as_process() else {
                return ERR_BAD_STATE as i64;
            };
            let owner_state = if p.state == ProcState::Exited {
                berry_susface::ProcStateP::Exited
            } else {
                berry_susface::ProcStateP::Running
            };
            owner_gate = berry_susface::ThreadOwnerGate {
                owner_state,
                owner_started: p.started,
                pager_load: p.ht.pager_load() as u32,
                owner_freeze_depth: p.freeze_depth,
            };
        }
        let gate = berry_susface::thread_admission(&berry_susface::ThreadGateInput {
            caller_tokens: 0,
            started: thrd.started,
            exited: thrd.exited,
            owner: owner_gate,
            freeze_ready: thrd.sched_attr.flags & SA_FLAG_FREEZE_READY != 0,
            freeze_depth: thrd.freeze_depth,
            precheck: berry_sched::freeze_precheck_by_thread(thread_key),
        });
        match gate {
            berry_susface::Admission::Ok => {}
            berry_susface::Admission::Busy => return ERR_BUSY as i64,
            berry_susface::Admission::BadState => return ERR_BAD_STATE as i64,
        }
    }
    let first_thread_token;
    unsafe {
        let r = &mut *obj.as_ptr();
        let Some(thrd) = r.obj.as_thread_mut() else {
            return ERR_BAD_HANDLE as i64;
        };
        first_thread_token = thrd.freeze_depth == 0;
        thrd.freeze_depth += 1;
        if first_thread_token {
            let owner = thrd.owner();
            if let Some(p) = (*owner.as_ptr()).obj.as_process_mut() {
                p.thread_freezes += 1;
            }
        }
        r.retain(); // token 持有 Thread；Thread 自身 retain 所属 Process
    }
    if berry_sched::freeze_by_thread(thread_key) != 1 {
        // 防御面：started 任务必在 SLEEPQ/RQ；失败全量回滚，不留半冻。
        unsafe {
            let r = &mut *obj.as_ptr();
            if let Some(thrd) = r.obj.as_thread_mut() {
                thrd.freeze_depth -= 1;
                if first_thread_token {
                    let owner = thrd.owner();
                    if let Some(p) = (*owner.as_ptr()).obj.as_process_mut() {
                        p.thread_freezes -= 1;
                    }
                }
            }
            ObjRef::release(obj);
        }
        return ERR_BAD_STATE as i64;
    }
    let tok = crate::suspend::SuspendTok {
        target: crate::suspend::SuspendTarget::Thread(obj),
        // thaw 键装载律单源（W3-Q38）：Thread = Thread ObjRef 地址。
        key: berry_susface::thaw_route(berry_susface::TargetKind::Thread, 0, thread_key),
        since_vct: berry_hal_aarch64::timer::cntvct(),
    };
    let ret = insert_suspend_token(tf, t, tok, token_out);
    if ret == ERR_NO_MEMORY as i64 {
        // insert 失败：token 未入表，手动撤销本层 depth/retain。仅首枚
        // token 撤冻面/READY/Process 占位；嵌套失败时既有 token 仍持冻。
        unsafe {
            let r = &mut *obj.as_ptr();
            if let Some(thrd) = r.obj.as_thread_mut() {
                if thrd.freeze_depth > 0 {
                    thrd.freeze_depth -= 1;
                }
                if first_thread_token {
                    thrd.sched_attr.flags &= !SA_FLAG_FREEZE_READY;
                    let owner = thrd.owner();
                    if let Some(p) = (*owner.as_ptr()).obj.as_process_mut() {
                        p.thread_freezes -= 1;
                    }
                    berry_sched::thaw_by_thread(thread_key);
                }
            }
            ObjRef::release(obj);
        }
    }
    if ret == 0 {
        // sim-S2 A2：单线程冻结成功遥测（process 臂同款——计数器族）。
        crate::telemetry::freeze_ok();
    }
    ret
}

/// sim-S3（PM-16 §4.4 步3）：深睡请求执行体（ktest 直调，零 syscall 号——
/// EL0 深睡入口随 PM-03 挂起原语刀另裁，本刀只立机制面）。
///
/// PSCI SYSTEM_SUSPEND 可达（probe 缓存 `susp=1`）→ 完整 HVC 形（fnid
/// 0x8400000C，x0 inout，x1=0 = NULL re-entry：规范要求 INVALID_PARAMETERS，真臂只归独立 spike）；SMCCC 语义成功 = 下电不归，**返回即失败**
/// → 落降级兜底（TCG 不可达面：probe 后 `suspend_supported()`=false 恒门住）。
///
/// 降级分支原语级复用 freeze 机器：`freeze_precheck_by_pinned` +
/// `freeze_by_pinned` + 成功尾 `freeze_ok` 既有写点照走 + 深睡降级记账
/// `note_degrade_freeze` + token 立即 Drop 语义（零持窗——ktest 前窗口无
/// 定时器、TCG 无唤醒事件，冻结即 thaw）：`auto_thaw` + `thaw_by_pinned`，
/// 与 SuspendTok Drop 在 depth 归零时的效果逐位同（无 ObjRef 面的原语级）。
///
/// ADR #67：slept_ns/suspend_count 记账待 KVM spike 回填三档表后裁，本刀
/// 只留钩。打印零（标记归 ktest 断言层——PWRKEY 先例：标记行读台账）。
///
/// 调用窗契约：当前 ktest 直调在 sched::init/timer::init 前（DAIF 屏蔽、
/// CURRENT=null）；PM-03 接 EL0 时必须持有 KL 并补自冻/交叉 token 门。
///
/// 与 sys_process_suspend 的语义差：**无自冻拒绝门**（彼 :1652 有
/// `current_pinned()` 拒绝——单核永久停摆防线）——本面调用面唯 ktest 直调
/// + 降级形零持窗返回前自愈（thaw 内联完成），不构成停摆；PM-03 EL0 入口
/// 接机制面时补同款门。
pub(crate) fn request_deep_sleep(pinned: usize) -> i64 {
    if berry_hal_aarch64::psci::suspend_supported() {
        // 真 SYSTEM_SUSPEND 面：成功 = 下电不归（无返回）；任何返回 = 失败，
        // 落降级兜底。TCG 路结构性不可达。
        let _ = berry_hal_aarch64::psci::system_suspend_hvc();
    }
    // 失败路径照 sys_process_suspend 的错误返回形（precheck 不过 -EBUSY /
    // 无匹配任务 -EBADSTATE）。
    if !berry_sched::freeze_precheck_by_pinned(pinned) {
        return ERR_BUSY as i64;
    }
    if berry_sched::freeze_by_pinned(pinned) == 0 {
        return ERR_BAD_STATE as i64;
    }
    // 成功尾 freeze_ok 既有写点照走（sys_process_suspend 成功尾同款）。
    crate::telemetry::freeze_ok();
    // 深睡降级记账（sim-S3 PWRDEG 判据载体）。
    crate::telemetry::note_degrade_freeze();
    // token 立即 Drop（零持窗）→ auto_thaw + thaw_by_pinned。
    crate::telemetry::auto_thaw();
    berry_sched::thaw_by_pinned(pinned);
    0
}

/// SuspendToken 入调用方表 + 出参 copy_out。0 = 成功；`insert` 失败回
/// `-ENOMEM`；出参坏指针回 `-EFAULT`（此时 close 刚入表 token，由 Drop 完成
/// depth/thaw/release 全回滚）。
fn insert_suspend_token(
    tf: &TrapFrame,
    t: &mut HandleTable,
    tok: crate::suspend::SuspendTok,
    token_out: *mut u32,
) -> i64 {
    use crate::handle::KernelObj;
    let Ok(token_h) = t.insert(
        KernelObj::SuspendToken(tok),
        RIGHT_DUPLICATE | RIGHT_TRANSFER | RIGHT_INSPECT,
    ) else {
        return ERR_NO_MEMORY as i64;
    };
    if copy_out_u32(caller_is_el1(tf), token_out as u64, token_h).is_err() {
        let _ = t.close(token_h); // Drop → thaw/depth/release
        return ERR_FAULT as i64;
    }
    0
}

// ---- 1.1/S1：sched_setattr / sched_getattr（0x3C / 0x3D）----

// W3-Q22：常量族单源迁 `berry-safeck`（sa_check 纯判定面持有——SA_F_* 位
// 名对齐 berry-polface SA_FLAG_* 词汇表）；此处 use 消费（getattr 回写臂
// 用 SCHED_ATTR_SIZE/MAX、admission 记账面用 SA_F_BUDGET/FREEZE_READY，
// 既有名零改动）。
use berry_safeck::{SA_F_BUDGET, SA_F_FREEZE_READY, SCHED_ATTR_MAX, SCHED_ATTR_SIZE};

/// 读入并校验一个 `berry_sched_attr`（薄壳——W3-Q22 结构解码+校验纯面单源
/// 在 `berry-safeck::sa_check`）。返回 `(属性, 调用方声明 size)`——
/// `getattr` 需要原 size 决定回写尾部长度。
pub(crate) fn sa_parse(el1: bool, ptr: u64) -> Result<(crate::process::SchedAttr, usize), i32> {
    let mut buf = [0u8; SCHED_ATTR_MAX];
    if let Err(e) = copy_from_user(el1, ptr, &mut buf[..SCHED_ATTR_SIZE]) {
        return Err(e);
    }
    let size = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if size < SCHED_ATTR_SIZE || size > SCHED_ATTR_MAX {
        return Err(ERR_INVALID_ARG);
    }
    // 前向兼容：size > 内核布局时尾部必须全零（Linux sched_attr 同款）。
    if size > SCHED_ATTR_SIZE {
        if let Err(e) = copy_from_user(el1, ptr + SCHED_ATTR_SIZE as u64, &mut buf[SCHED_ATTR_SIZE..size]) {
            return Err(e);
        }
    }
    // 纯判定面单源调用：cur_u = admission 全局原子快照（Q12）；SaError
    // 映射回 ERR_* 常量。判定律（size 尾零/version/域门/交叉门/admission
    // 3891 帽）全部钉板在 berry-safeck host 测试。
    let cur_u = DL_TOTAL_UTIL_Q12.load(Ordering::Relaxed);
    berry_safeck::sa_check(&buf[..size], cur_u).map_err(|e| match e {
        berry_safeck::SaError::InvalidArg => ERR_INVALID_ARG,
        berry_safeck::SaError::E2big => ERR_E2BIG,
        berry_safeck::SaError::NotSupported => ERR_NOT_SUPPORTED,
        berry_safeck::SaError::Busy => ERR_BUSY,
    })
}

/// S5 admission：deadline 任务总利用率（Q12 定点——1.0 = 4096）。
/// sched_setattr budget 置位成功 +、清位/覆盖 −（budget_ns=0 覆盖回退）。
static DL_TOTAL_UTIL_Q12: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Thread effective 属性：Process 默认 ∪ Thread 覆盖。位不设 = 继承；位设
/// 后字段取 Thread 值（含 0=UNSPECIFIED 的显式语义）。
fn sched_effective(th: &crate::process::Thread) -> crate::process::SchedAttr {
    // SAFETY: Thread 持 owner ObjRef retain；SVC 单核窗内对象存活。
    let owner = unsafe { &*th.owner().as_ptr() };
    let base = owner.obj.as_process().map(|p| p.sched_attr).unwrap_or_default();
    crate::process::SchedAttr::merge(base, th.sched_attr)
}

/// `berry_sched_setattr(handle, attr_ptr)`（0x3C）。目标 Process = 写该进程
/// 线程默认；目标 Thread = 写覆盖（flags=0 = 清覆盖回默认）。S1 只存；
/// S2/S5 消费 weight/intent/policy/deadline。
extern "C" fn sys_sched_setattr(tf: &mut TrapFrame) -> i64 {
    let (h, attr_ptr) = (tf.regs[1] as u32, tf.regs[2] as u64);
    let el1 = caller_is_el1(tf);
    let (attr, _) = match sa_parse(el1, attr_ptr) {
        Ok(v) => v,
        Err(e) => return e as i64,
    };
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    // 精确类型门控：先 Thread（覆盖），再 Process（默认）。两者都需 SET_POLICY。
    let obj = match t.get_raw(h, ObjType::Thread, RIGHT_SET_POLICY) {
        Ok((o, _)) => o,
        Err(e) => match t.get_raw(h, ObjType::Process, RIGHT_SET_POLICY) {
            Ok((o, _)) => o,
            Err(_) => return e as i64,
        },
    };
    // SAFETY: get_raw 已三门控；SVC IRQ 屏蔽窗内单核访问。
    unsafe {
        let r = &mut *obj.as_ptr();
        match &mut r.obj {
            KernelObj::Thread(th) => {
                if attr.flags & SA_F_FREEZE_READY != 0 {
                }
                // S5 admission 扣减/累加面：旧 budget 退回 + 新 budget 累加
                //（原子对——sa_parse 只校验不写，成功尾一次记账防泄漏）。
                let old = th.sched_attr;
                if old.flags & SA_F_BUDGET != 0 && old.period_ns > 0 {
                    let old_u = old.budget_ns as u128 * 4096 / old.period_ns as u128;
                    DL_TOTAL_UTIL_Q12.fetch_sub(old_u as u64, Ordering::Relaxed);
                }
                if attr.flags & SA_F_BUDGET != 0 && attr.period_ns > 0 {
                    let new_u = attr.budget_ns as u128 * 4096 / attr.period_ns as u128;
                    DL_TOTAL_UTIL_Q12.fetch_add(new_u as u64, Ordering::Relaxed);
                }
                th.sched_attr = attr;
            }
            KernelObj::Process(p) => {
                // FREEZE_READY 是 Thread 自证位；对 Process 默认声明非法。
                if attr.flags & SA_F_FREEZE_READY != 0 {
                    return ERR_INVALID_ARG as i64;
                }
                p.sched_attr = attr;
            }
            _ => return ERR_BAD_HANDLE as i64,
        }
    }
    0
}

/// `berry_sched_getattr(handle, attr_ptr)`（0x3D）。返回 effective 属性；
/// 调用方 size > 内核布局时尾部写零（前向兼容写侧对称）。
extern "C" fn sys_sched_getattr(tf: &mut TrapFrame) -> i64 {
    let (h, attr_ptr) = (tf.regs[1] as u32, tf.regs[2] as u64);
    let el1 = caller_is_el1(tf);
    // 先读调用方声明 size（<56 / >256 直接 EINVAL）。
    let mut head = [0u8; 4];
    if let Err(e) = copy_from_user(el1, attr_ptr, &mut head) {
        return e as i64;
    }
    let out_size = u32::from_le_bytes(head) as usize;
    if out_size < SCHED_ATTR_SIZE || out_size > SCHED_ATTR_MAX {
        return ERR_INVALID_ARG as i64;
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let obj = match t.get_raw(h, ObjType::Thread, RIGHT_INSPECT) {
        Ok((o, _)) => o,
        Err(e) => match t.get_raw(h, ObjType::Process, RIGHT_INSPECT) {
            Ok((o, _)) => o,
            Err(_) => return e as i64,
        },
    };
    // SAFETY: get_raw 已门控；只读组合。
    let attr = unsafe {
        let r = &*obj.as_ptr();
        match &r.obj {
            KernelObj::Thread(th) => sched_effective(th),
            KernelObj::Process(p) => p.sched_attr,
            _ => return ERR_BAD_HANDLE as i64,
        }
    };
    let mut raw = [0u8; SCHED_ATTR_SIZE];
    raw[0..4].copy_from_slice(&(SCHED_ATTR_SIZE as u32).to_le_bytes());
    raw[4..6].copy_from_slice(&1u16.to_le_bytes());
    raw[6..8].copy_from_slice(&attr.flags.to_le_bytes());
    raw[8..12].copy_from_slice(&attr.weight.to_le_bytes());
    raw[12] = attr.policy;
    raw[13] = attr.intent;
    raw[14] = attr.freeze_clock;
    raw[16..24].copy_from_slice(&attr.min_ns.to_le_bytes());
    raw[24..32].copy_from_slice(&attr.max_ns.to_le_bytes());
    raw[32..40].copy_from_slice(&attr.deadline_ns.to_le_bytes());
    raw[40..48].copy_from_slice(&attr.budget_ns.to_le_bytes());
    raw[48..56].copy_from_slice(&attr.period_ns.to_le_bytes());
    // v1.2：**运行时态注入**——自查（queried Thread = current task）时，
    // deadline_ns/budget_ns 覆写为实际 Task 状态（绝对截止期 + 剩余预算，
    // CNTVCT→ns 换算）。非自查（查别的线程）保持存值。
    if obj.as_ptr() as usize == berry_sched::current_thread_key() {
        let (vd, sl) = berry_sched::current_deadline_state();
        if vd > 0 {
            let ns = vd.saturating_mul(1_000_000_000) / berry_hal_aarch64::timer::cntfrq();
            raw[32..40].copy_from_slice(&ns.to_le_bytes());
        }
        if sl > 0 {
            let ns = sl.saturating_mul(1_000_000_000) / berry_hal_aarch64::timer::cntfrq();
            raw[40..48].copy_from_slice(&ns.to_le_bytes());
        }
    }
    let mut out = [0u8; SCHED_ATTR_MAX];
    out[..SCHED_ATTR_SIZE].copy_from_slice(&raw);
    match copy_to_user(el1, attr_ptr, &out[..out_size]) {
        Ok(()) => 0,
        Err(e) => e as i64,
    }
}

/// `berry_thread_yield()`（0x3F）：主动让出当前片。无 handle、无参数；
/// 无其他可跑任务时立即返回 0。等价内核 `yield_now`（含切前放锁/切回重取
/// 舞蹈——SVC 入口类位集被完整继承）。
extern "C" fn sys_thread_yield(_tf: &mut TrapFrame) -> i64 {
    berry_sched::yield_now_keep_irq();
    0
}

/// `berry_sched_util_snapshot(*util, count)`（0x40，CP-08 util 通道首环——
/// OQ-8 通道形态首裁快照 pull 先行）：per-CPU util EWMA 快照读 = 生产侧
/// 唯一读出口 `berry_sched::cpu_util` 的用户态面（0..=1024，与
/// `Topo::capacity`/EM capacity 同尺度；PM 服务 schedutil 同构
/// `f ≈ util × fmax / capacity` 的输入源——**频率裁决单点归 PM**，本号只读
/// 不写、调度路径零频率写点）。生产侧零新累积：`Fair::tick` 记账在役
/// （`class.rs` 的 `util_accumulate`/`util_tick_window`）。
///
/// 契约：`count < UTIL_NR_CPU` → -EINVAL；≥ 则只写实数槽（余槽不动），
/// 返回 >0 = 实写槽位数（恰 = 内核 `UTIL_NR_CPU`——调用方可以此探核数，
/// count-wins 口径同 0x24 批供表）；copy_out 失败 → -EFAULT 单发整段
/// 拷贝无半写面。零 handle 零权利位（系统级遥测读面——统计信号非对象
/// 内容，gen_abi `NO_GATE_SYSCALLS` 在册豁免）。
///
/// 锁类 TABLE（仅 KL）：EWMA 槽 Relaxed 原子读（统计信号无序角色，同
/// `sched.rs::LB_TICKS` 先例——`IRQ_TICKS` 2026-11-17 机会三退役）零共享串行面；copy_out 链走查调用方页表
/// = §A113 读者面（同 0x61 clock_read/0x87 signal_take 升档论证）。
extern "C" fn sys_sched_util_snapshot(tf: &mut TrapFrame) -> i64 {
    let (ptr, count) = (tf.regs[1], tf.regs[2]);
    if count < berry_sched::UTIL_NR_CPU as u64 {
        return ERR_INVALID_ARG as i64;
    }
    let el1 = caller_is_el1(tf);
    let mut raw = [0u8; berry_sched::UTIL_NR_CPU * 4];
    for (c, slot) in raw.chunks_exact_mut(4).enumerate() {
        slot.copy_from_slice(&berry_sched::cpu_util(c).to_le_bytes());
    }
    match copy_to_user(el1, ptr, &raw) {
        Ok(()) => berry_sched::UTIL_NR_CPU as i64,
        Err(e) => e as i64,
    }
}

/// `berry_signal_send(h_process, sig)`（0x86，A7-4 段②）：向进程 pending
/// 集位置位（幂等 = POSIX 标准信号合并语义）。`h=0` → 当前进程（raise 面，
/// 零 handle 需求）；`h≠0` 门控 Process **MANAGE**（同 terminate 权利面）。
/// sig ∉ 1..=64 → -EINVAL；目标非 Alive → -EBADSTATE。**不唤醒目标、不解蔽
/// IRQ、零分配**（协同式用户态投递——投递点在 libc，A7-4 §2 方案 B；免锁
/// 根基按构造不撞）。与 terminate 的语义边界：terminate = 异步死亡（收到即
/// 死）；signal = 可捕获投递（默认动作表在用户态）——两原语并存。
extern "C" fn sys_signal_send(tf: &mut TrapFrame) -> i64 {
    let (h, sig) = (tf.regs[1] as u32, tf.regs[2] as u32);
    if sig == 0 || sig > 64 {
        return ERR_INVALID_ARG as i64; // POSIX：无 0 号信号；位图界 1..=64
    }
    let r: &mut ObjRef = if h == 0 {
        let p = berry_sched::current_pinned();
        if p == 0 {
            return ERR_BAD_STATE as i64; // 当前任务无 Process（内核任务/ktest 期）
        }
        // SAFETY: current_pinned = 调用者任务运行钉住的 Process ObjRef 地址；
        // 单核 IRQ 屏蔽（syscall 主体不可抢占）+ 任务存活 ⇒ 指针有效。
        unsafe { &mut *(p as *mut ObjRef) }
    } else {
        let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
        match t.get_raw(h, ObjType::Process, RIGHT_MANAGE) {
            // SAFETY: get_raw 已三门控（在表、类型 Process、权利 MANAGE）。
            Ok((o, _)) => unsafe { &mut *o.as_ptr() },
            Err(e) => return e as i64, // 坏值/类型不符 -EBADF；缺 MANAGE -EACCES
        }
    };
    // SAFETY: r 借用窗内单核 IRQ 屏蔽；h=0 路指针源本就是 Process 任务。
    let Some(p) = r.obj.as_process_mut() else {
        return ERR_BAD_HANDLE as i64; // 不可达防御面（类型刚验过/钉住源）
    };
    if p.state != ProcState::Alive {
        return ERR_BAD_STATE as i64; // 已终止进程不收新信号
    }
    // S-3c C21：原子置位（fetch_or——位叠加不丢；序归锁窗）。
    p.pending_signals.fetch_or(1 << (sig - 1), core::sync::atomic::Ordering::Relaxed);
    0
}

/// `berry_signal_take(*pending_out)`（0x87，A7-4 段②）：取走当前进程
/// pending 集（swap(0) 语义——取后内核位清零，投递中新到位置回内核
/// pending 留下轮 take）。`*pending` = u64 出参 **8 字节**（非 handle 出参
/// 宽度跟 ABI 类型走）；零 handle 面零权利位（「对象几何非内容」惯例）。
/// 出参写失败不消费（先写后清——单核 IRQ 屏蔽下两步间无并发 send）。
extern "C" fn sys_signal_take(tf: &mut TrapFrame) -> i64 {
    let out = tf.regs[1];
    let p = berry_sched::current_pinned();
    if p == 0 {
        return ERR_BAD_STATE as i64; // 无 Process 面（内核任务/ktest 期）零意义
    }
    // SAFETY: current_pinned 指针有效（同 send）；借用窗内 IRQ 屏蔽。
    // S-3c C21 取走协议（swap 先行 + 失败回滚）：原子 swap(0) 取走当前集，
    // copy_out 失败则 fetch_or(v) 回滚——回滚只补 v 的原位，窗口期新到位
    // （若有）保留在 pending 留下轮 take，语义 = 文档「投递中新到位置回
    // 内核 pending」的原子版；KL 纪元两序（先读后清 vs swap 先行）观察等价。
    let v = {
        let Some(proc) = (unsafe { (*(p as *mut ObjRef)).obj.as_process_mut() }) else {
            return ERR_BAD_HANDLE as i64; // 不可达防御面（钉住源必为 Process）
        };
        proc.pending_signals.swap(0, core::sync::atomic::Ordering::Relaxed)
    };
    match copy_out_u64(caller_is_el1(tf), out, v) {
        Ok(()) => 0,
        Err(e) => {
            // 写失败不消费：原集回滚进 pending（叠加位不丢，调用方重试拿全）。
            // SAFETY: 同上。
            if let Some(proc) = unsafe { (*(p as *mut ObjRef)).obj.as_process_mut() } {
                proc.pending_signals
                    .fetch_or(v, core::sync::atomic::Ordering::Relaxed);
            }
            e as i64
        }
    }
}

// ---- 策略注入（S6-b 最小版：0x83 policy_bind——COMMIT 面）----

/// `berry_policy_bind` 用户态表条目（16B，#[repr(C)]——与内核 `policy::SnapSlot`
/// 布局兼容：key 落偏移 0，可按位审计同源）。
#[repr(C)]
struct PolicyEntryABI {
    key: u64,
    weight: i32,
    placement: u8,
    epp_flags: u8,
    _rsv: u16,
}
const _: () = assert!(core::mem::size_of::<PolicyEntryABI>() == 16);

/// `PolicyEntryABI::epp_flags` 位定义：bit0 = 带 EPP 覆盖，bit1 = EPP 取值
/// （0 = Efficiency / 1 = Perf）——镜像 `sched/policy.rs` 私有 EPP_HAS/EPP_VAL；
/// bit2.. 未定义（非 0 尾 → -EINVAL，options 纪律）。
const PE_EPP_HAS: u8 = 1 << 0;
const PE_EPP_VAL: u8 = 1 << 1;

/// 单次收表上限（限流面）：镜像 `sched/policy.rs` 私有 `TBL_MAX = 128`（桶 256
/// 装载因子 ≤50% 上界）。policy 侧 build 时自会截尾，但那是**内核侧写者的宽容
/// 面**——用户态超限在此提前诚实拒收（-E2BIG），不做静默截断。
const POLICY_TBL_MAX: usize = 128;

/// 空条目（`entries` 栈数组 const 初始化基底）。
const PE_ZERO: berry_sched::policy::Entry = berry_sched::policy::Entry {
    task_key: 0,
    directive: berry_sched::policy::Directive {
        placement: 0,
        weight: 0,
        epp: None,
    },
};

/// `berry_policy_bind(auth, op, p1, p2, p3, p4)`（0x83，S6-b COMMIT 面 +
/// S6-c shadow 谱系 op 词汇扩展）。
///
/// op=1 (COMMIT)：p1 = 表指针（用户态 `&[PolicyEntryABI]`），p2 = 条目数
/// （0 = 空表提交，「接口在位、表空」合法态），p3 = gen（世代数），p4 = 保留 0。
/// → [`policy_tbl_parse`] 整表拷入转换（weight 夹紧 ±64——handler 第一道，
/// policy build/消费点再夹，三点执法；0 键/重复/溢出由 `bind_gen` 的 build_snap
/// 语义丢弃——first-wins）→ `policy::bind_gen(tbl, gen)`：
/// true = 0 / false = **-EAGAIN**（陈旧拒收——gen ≤ 活跃世代，写者重放/乱序
/// 到达的旧表不得覆盖新表，世代单调性在 policy 侧收口；活跃快照未动，调用方
/// 对账 gen 后携新 gen 重试）。
///
/// op=2 (STATUS)：p1 = `*out_packed`（u64 出参）——bit 0-31 = active gen
/// （`policy::gen()` 截低 32 位），bit 32-63 = shadow gen
/// （`policy::shadow_gen()` 截低 32 位）——双谱系一次读全（写者对账/遥测）。
/// 打包算式下放 [`policy_status_packed`]（ktest SHD-K SH-6 直测面）。
/// p2 = `*out_self_key`（u64 出参，**可选**——0 = NULL 指针跳过）= 调用者
/// 自身 proc_tag/policy key：v1 key 载体即 `Task::proc_tag`（见
/// `sched/policy.rs`——apply/weight_of 消费点读 `task.proc_tag`），handler 经
/// `berry_sched::current_tag()` 同源读取——用户态据此发现「我现在是几号键」
/// （表里给别的进程加权前先对账自己）。内核任务/boot 期 = 0（哨兵键——0 键
/// 条目表侧本就丢弃，天然诚实）。
/// p3 = `*out_divergence`（u64 出参，**可选**——0 = NULL 指针跳过）= 对账
/// 遥测双计数打包：bit 0-31 = `policy::divergence_count()`（截低 32 位）、
/// bit 32-63 = `policy::shadow_eval_count()`（截低 32 位）——A/B 评估消费面
/// （divergence 率 = 分子/分母 → PROMOTE/DEMOTE 判据，宿主侧
/// `tools/policy_ab_eval.py`；两计数器经 promote/demote 轨道切换自动清零，
/// 读数即当前对账期）。打包算式下放 [`policy_divergence_packed`]（ktest
/// DV-K DV-5 直测面）。
/// → 0 + copy_out_u64 ×3（任一写失败 → -EFAULT 透传；跳过面不计）。
///
/// op=3 (PROMOTE)：`policy::promote()`——shadow → enforced 原子转正：
/// true = 0 / false = **-EBADSTATE**（shadow 空——gen == 0 从未
/// `bind_gen_shadow` 受理，无表可转正，ACTIVE 不动）。
///
/// op=4 (DEMOTE)：`policy::demote()`——enforced → shadow 降级（ACTIVE 回
/// 哨兵空表、shadow 表保留可复转正）→ 恒 0。
///
/// op=5 (UNBIND)：`policy::unbind()`——解绑回「接口缺席」哨兵态 → 恒 0。
///
/// op=6 (SHADOW_COMMIT)：表形同 COMMIT（p1/p2/p3 同参、经同一
/// [`policy_tbl_parse`]）→ `policy::bind_gen_shadow(tbl, gen)`：true = 0 /
/// false = **-EAGAIN**（陈旧拒收——gen ≤ shadow 侧世代，**比照对象换 shadow
/// 谱系**、与 active 侧 gen 无关；shadow 未动）。
///
/// 其他 op → -ENOTSUP。全 op 面 p4 保留 0（非 0 → -EINVAL，options 纪律）。
///
/// 门控（arch-r2 High 收口）：`auth` = Process handle，须过 `get_raw` 三门控
/// （在表、类型 Process、权利 **MANAGE**——同 terminate/signal_send 权利面）。
/// 策略注入是全局旁路（COMMIT/PROMOTE 可给任意 proc_tag 加权 ±64 自肥抢占、
/// UNBIND 可拆其他服务设置的策略），按「机制在内核、策略注入须过能力门控」
/// 纲领收口：只有持某 Process MANAGE 权的调用者（通常是 svcmgr/初始化服务）
/// 可操作全局策略表；对象本身不消费（纯资格面——SET_POLICY 细粒度权随 S6-c
/// Job 对象落地再迁）。拒收面：坏值/类型不符 → -EBADF、缺 MANAGE → -EACCES、
/// 无表调用者 → -EBADF（caller_table 既有口径）。锁类 TABLE：copy 链页表走查
/// （§A113 读者面，同 clock_read）+ `SNAPS` 全局单写者跨核 KL 串行；SVC 主体
/// IRQ 屏蔽（单核）下无读者竞态窗。
extern "C" fn sys_policy_bind(tf: &mut TrapFrame) -> i64 {
    let (auth, op, p1, p2, p3, rsv) =
        (tf.regs[1], tf.regs[2], tf.regs[3], tf.regs[4], tf.regs[5], tf.regs[6]);
    // 能力门（arch-r2）：auth = Process handle，三门控（在表/类型/权利）任一
    // 不过即拒——对象不消费（纯资格面）。EL0 裸调（auth=0 或无 MANAGE 权
    // handle）在此全 op 面统一拒，不再能注入/转正/拆绑全局策略。
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    if let Err(e) = t.get_raw(auth as u32, ObjType::Process, RIGHT_MANAGE) {
        return e as i64; // 坏值/类型不符 -EBADF；缺 MANAGE -EACCES
    }
    if rsv != 0 {
        return ERR_INVALID_ARG as i64; // p4 保留 0（options 纪律，全 op 面）
    }
    let el1 = caller_is_el1(tf);
    match op {
        // COMMIT：p1 = 表指针、p2 = 条目数、p3 = gen → active 谱系换表。
        1 => {
            let (entries, n) = match policy_tbl_parse(el1, p1, p2) {
                Ok(v) => v,
                Err(e) => return e as i64,
            };
            if berry_sched::policy::bind_gen(&entries[..n], p3) {
                0
            } else {
                ERR_AGAIN as i64 // 陈旧拒收：gen ≤ 活跃世代，活跃快照未动
            }
        }
        // STATUS：p1 = *out_packed（gen 双谱系）、p2 = *out_self_key（调用者
        // proc_tag——可选出参，0 = NULL 跳过）、p3 = *out_divergence（对账
        // 双计数——可选出参，0 = NULL 跳过）。
        2 => {
            let packed = policy_status_packed();
            if let Err(e) = copy_out_u64(el1, p1, packed) {
                return e as i64;
            }
            if p2 != 0 {
                // 自身策略键：与 apply/weight_of 消费点同源（v1 key =
                // Task::proc_tag，current_tag 只读透传；单核屏蔽窗内值稳定）。
                // 内核任务/ktest 期 = 0——哨兵键语义诚实透出，不伪装。
                let self_key = berry_sched::current_tag() as u64;
                if let Err(e) = copy_out_u64(el1, p2, self_key) {
                    return e as i64;
                }
            }
            if p3 != 0 {
                // 对账遥测双计数（分子 divergence / 分母 eval 各截低 32 打包）
                //——A/B 评估面（tools/policy_ab_eval.py），读窗纪律同 p1 面。
                let divergence = policy_divergence_packed();
                if let Err(e) = copy_out_u64(el1, p3, divergence) {
                    return e as i64;
                }
            }
            0
        }
        // PROMOTE：shadow → enforced 转正；shadow 空 → -EBADSTATE。
        3 => {
            if berry_sched::policy::promote() {
                0
            } else {
                ERR_BAD_STATE as i64 // shadow 从未绑定（gen==0）：无表可转正
            }
        }
        // DEMOTE：enforced → shadow 降级（幂等面 → 恒 0）。
        4 => {
            berry_sched::policy::demote();
            0
        }
        // UNBIND：回「接口缺席」哨兵态（幂等面 → 恒 0）。
        5 => {
            berry_sched::policy::unbind();
            0
        }
        // SHADOW_COMMIT：表形同 COMMIT → shadow 谱系换表（陈旧比照 SHD gen）。
        6 => {
            let (entries, n) = match policy_tbl_parse(el1, p1, p2) {
                Ok(v) => v,
                Err(e) => return e as i64,
            };
            if berry_sched::policy::bind_gen_shadow(&entries[..n], p3) {
                0
            } else {
                ERR_AGAIN as i64 // 陈旧拒收：gen ≤ shadow 世代，shadow 谱系未动
            }
        }
        _ => ERR_NOT_SUPPORTED as i64, // 未定义 op（options 纪律）
    }
}

/// STATUS（op=2）**p1 面**打包算式：bit 0-31 = active gen（`policy::gen()` 截低
/// 32 位）、bit 32-63 = shadow gen（`policy::shadow_gen()` 截低 32 位）——双谱系
/// 一次读全。（op=2 的 p2 面 = 调用者自身 proc_tag/policy key，handler 直读
/// `berry_sched::current_tag()` 整值写出——无位布局组合，不经本函数、无需
/// 下放直测面。）抽成 pub(crate) 的动因：op=1/3/4/6 的 bool→errno 是字面映射
/// （底座 ktest 全覆盖后映射面零风险），但本算式是 **ABI 可见的位布局组合**
/// ——半区对调/位移宽度/gen 侧截断缺失都静默错（写者对账读反双谱系），且原本
/// 只存在于 handler 内（TrapFrame + copy 链挡住 ktest 直调）。按 [`sa_parse`]
/// 直测先例下放为可测单元，由 ktest SHD-K SH-6 真数据源驱动直测（连「哪个
/// 谱系进哪个半区」的绑定序一并收口）。读窗纪律同 policy 读接口（单核
/// 屏蔽窗——两读间无写者，无双谱系撕裂）。
pub(crate) fn policy_status_packed() -> u64 {
    ((berry_sched::policy::shadow_gen() & 0xFFFF_FFFF) << 32)
        | (berry_sched::policy::gen() & 0xFFFF_FFFF)
}

/// STATUS（op=2）**p3 面**打包算式：bit 0-31 = divergence_count（截低 32
/// 位）、bit 32-63 = shadow_eval_count（截低 32 位）——对账遥测双计数一次
/// 读全（A/B 评估面：divergence 率 = 分子/分母，宿主侧
/// `tools/policy_ab_eval.py` 消费）。同 [`policy_status_packed`] 的下放动因：
/// 本算式是 **ABI 可见的位布局组合**——半区对调/位移宽度/截断缺失都静默错
/// （对调 = 分子分母互换 → 率倒置，PROMOTE/DEMOTE 判据全反），且 handler 内
/// TrapFrame + copy 链挡住 ktest 直调。ktest DV-K DV-5 真数据源驱动直测
/// （分母 == ROUNDS、分子开区间 (0, ROUNDS)——两半非对称，对调必炸）。读窗
/// 纪律同 p1 面：SVC 主体 IRQ 屏蔽（单核）下两读间无写者，双计数器无撕裂
/// （Relaxed 纯统计读，非通信值）。
pub(crate) fn policy_divergence_packed() -> u64 {
    ((berry_sched::policy::shadow_eval_count() & 0xFFFF_FFFF) << 32)
        | (berry_sched::policy::divergence_count() & 0xFFFF_FFFF)
}

/// COMMIT（op=1）/SHADOW_COMMIT（op=6）公共表解析：`tbl_p` = 表指针（用户态
/// `&[PolicyEntryABI]`）、`n` = 条目数（0 = 空表合法）→ copy_from_user 整表
/// 拷入 → 逐条转换 `policy::Entry`。拒收面全在 handler 侧收口：超限 →
/// **-E2BIG**（单次表 ≤128 条 TBL_MAX 限流——policy 侧 build 会截尾，但那是
/// 内核写者的宽容面，用户态超限提前诚实拒收、不静默截断）、拷页失败 →
/// -EFAULT 透传、保留位/未定义 flags 非 0 → **-EINVAL**（调用方可清零修复，
/// 同 sa_parse reserved0 口径——不是 NOT_SUPPORTED）。返回 `(entries, n)`：
/// 前 n 槽就绪（0 键/重复/溢出留给 policy 侧 build_snap 语义丢弃——
/// first-wins，此处不预审）。
fn policy_tbl_parse(
    el1: bool,
    tbl_p: u64,
    n: u64,
) -> Result<([berry_sched::policy::Entry; POLICY_TBL_MAX], usize), i32> {
    use berry_sched::policy::{Directive, Entry, WEIGHT_MAX};
    if n > POLICY_TBL_MAX as u64 {
        return Err(ERR_E2BIG); // 限流：单次表 ≤128 条（TBL_MAX）
    }
    let n = n as usize;
    let mut raw = [0u8; POLICY_TBL_MAX * core::mem::size_of::<PolicyEntryABI>()];
    if let Err(e) = copy_from_user(el1, tbl_p, &mut raw[..n * 16]) {
        return Err(e);
    }
    let mut entries = [PE_ZERO; POLICY_TBL_MAX];
    for i in 0..n {
        let o = i * 16;
        let e = PolicyEntryABI {
            key: u64::from_le_bytes(raw[o..o + 8].try_into().unwrap()),
            weight: i32::from_le_bytes(raw[o + 8..o + 12].try_into().unwrap()),
            placement: raw[o + 12],
            epp_flags: raw[o + 13],
            _rsv: u16::from_le_bytes([raw[o + 14], raw[o + 15]]),
        };
        // 保留位/未定义 flags 非零 → -EINVAL（调用方可清零修复，同 sa_parse
        // reserved0 口径——不是 NOT_SUPPORTED）。
        if e._rsv != 0 || e.epp_flags & !(PE_EPP_HAS | PE_EPP_VAL) != 0 {
            return Err(ERR_INVALID_ARG);
        }
        entries[i] = Entry {
            task_key: e.key,
            directive: Directive {
                placement: e.placement,
                // 夹紧 ±WEIGHT_MAX（±64）：包络是内核执法，不信任注入方自律。
                weight: e.weight.clamp(-WEIGHT_MAX, WEIGHT_MAX),
                epp: if e.epp_flags & PE_EPP_HAS != 0 {
                    Some(if e.epp_flags & PE_EPP_VAL != 0 {
                        berry_sched::EppHint::Perf
                    } else {
                        berry_sched::EppHint::Efficiency
                    })
                } else {
                    None
                },
            },
        };
    }
    Ok((entries, n))
}

// ---- channel（3-4b；对象 = `channel.rs`，端点互钉与环泄漏政策见其模块注）----

/// `channel_create(options, *out0, *out1)`（4-3c 起 options 首参，Zircon/
/// `event_create` 同形）：一次建一对端点（`channel.rs` 两端互指互钉——一端
/// 活则对端对象必活）。两端均授 **SEND|RECV|DUPLICATE|TRANSFER**（对称
/// 权利；TRANSFER 随 send 随行预检执法——Zircon 新 handle 默认携位模型）。
/// options：bit0 =
/// [`CHAN_OPT_NO_INTENT`]（intent opt-out，两端同置——通道级不传播）；其余位
/// → -EINVAL（options 纪律）。
extern "C" fn sys_channel_create(tf: &mut TrapFrame) -> i64 {
    let (options, out0, out1) = (tf.regs[1], tf.regs[2], tf.regs[3]);
    if options & !CHAN_OPT_NO_INTENT != 0 {
        return ERR_INVALID_ARG as i64;
    }
    let no_intent = options & CHAN_OPT_NO_INTENT != 0;
    let el1 = caller_is_el1(tf);
    if let Err(e) = copy_out_u32(el1, out0, 0) {
        return e as i64;
    }
    if let Err(e) = copy_out_u32(el1, out1, 0) {
        return e as i64;
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    // per-Process channel 配额帽（Medium 缺口收口，`process.rs` 帽常量注）：
    // 本表在册 channel 端点数 ≥ 帽先拒 -ENOMEM——fail-fast 先于两端
    // CHAN_DEPTH×Message ≈ 2.7KB/端的队列预储分配（计数单位 = 端点，
    // `channel_create` 一次入 2；`count_type` 表扫描口径同 sys_vmo_create）。
    if t.count_type(ObjType::Channel) >= crate::process::MAX_CHANNEL_PER_PROC {
        return ERR_NO_MEMORY as i64;
    }
    let rights = RIGHT_SEND | RIGHT_RECV | RIGHT_DUPLICATE | RIGHT_TRANSFER;
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
                    let r0 = copy_out_u32(el1, out0, h0);
                    let r1 = copy_out_u32(el1, out1, h1);
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

/// G3-1c-2 管道面原子段禁切守卫（send 满检→push / recv pop→回滚完结）：
/// new 取窗（计数 +1），Drop 放窗——段内任何早退（`return`/`?`）自动配平，
/// 配平违约结构性消除（对照：裸 preempt_disable/enable 对 = 20 个早退点
/// （send 10 / recv 10）手工
/// 配平，漏一 = 本核静默永禁切）。错误路径跳过段出口检查点 → g3-1b 落点①
/// （SVC 派发尾，零锁）承接拷贝窗内累积的切换诉求。
struct PreemptSpan;

impl PreemptSpan {
    #[inline]
    fn new() -> Self {
        berry_sched::preempt_disable();
        PreemptSpan
    }
}

impl Drop for PreemptSpan {
    #[inline]
    fn drop(&mut self) {
        berry_sched::preempt_enable();
    }
}

/// G3-1c-2 拷贝段局部解蔽窗（仅限 send/recv 原子段内调用）：save→unmask→
/// f()→restore 闭式（f 内禁早退——配平结构性成立）。窗内可达 IRQ 四源：tick、
/// SGI-resched、PL011 RX（SPI33——每次键入都进，wake_matching 是窗内 SLEEPQ
/// 手术最频繁者）、SGI-TLB——全部经退化锁安全（g3-1c-2 三镜 verify 核：探峰
/// RQ 位 / tick SQ 位在集即退化，本域 0x2E 恒真，§A126 推论正用），禁切由段
/// preempt_disable（计数 >0）+ g3-1c-1 双门（tick go 支/on_sgi_resched 闭门
/// 早退）保证；§A113 PT 锁窗禁切换由段计数保。**窗内新增可达 IRQ 源 = 先过
/// 退化锁审计再放行**。**窗在调用点不在 copy_from_user 内部**——其余号的
/// 调用面仍全屏蔽且计数 0，窗若内置 = 那些号拷贝中可切（§A113 违约）。
fn irq_window<R>(f: impl FnOnce() -> R) -> R {
    let daif = berry_hal_aarch64::irq_save();
    berry_hal_aarch64::enable_irq();
    let r = f();
    berry_hal_aarch64::irq_restore(daif);
    r
}

/// `channel_send(ch, bytes, nbytes, handles, nhandles)`：消息压**对端**队列
/// （非阻塞：满 → -EBUSY）。门控：本端 **SEND**。随行 handle 逐个 `take` 出
/// 调用者表（发送方失去原值）、令牌随消息挂起，接收方 recv 时 `insert_ref`
/// 落进**它的**表——能力随消息跨表迁移（能力模型 §4）。
///
/// 原子序（任一步失败 = 调用者表零变化）：参数上限（no-panic 前置）→ 门控 →
/// 对端满检 → 随行值全量预检（活值 + RIGHT_TRANSFER 迁移资格 + 数组内无
/// 重复——重复值先 take 会作废后值，回滚也换值）→ 载荷拷入 → 全量 take →
/// 压队列。take 失败仅剩不可达防御面
/// （预检后单核无插队），回滚重插（值或变、语义活）。
///
/// **4-4 fastpath/slowpath 分层**（`IPC设计.md` §7 检查链）：共享前缀（上列
/// 全部错误面，含随行 handle 预检 + 全量 take + 内联进消息——迁移面全链
/// 非阻塞：查表纯槽操作无锁、refs 不动、无 drop、take 路零分配，fork 之后
/// 两路均不再触表）之后按检查链 fork——「对端队列空 + 恰一个非哨兵
/// READABLE 等待者且无哨兵在场」全过 → fastpath
/// 定点唤醒（`fast_wake_locked`，与 slowpath 共用同一内部唤醒序列）+ 直切判定
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
    if !berry_chanface::send_args_legal(nbytes, nhandles) {
        return ERR_INVALID_ARG as i64;
    }
    let el1 = caller_is_el1(tf);
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let self_ptr = match t.get_raw(h, ObjType::Channel, RIGHT_SEND) {
        Ok((o, _)) => o,
        Err(e) => return e as i64,
    };
    // g3-1c-2 原子段（满检→push，草案 §8.B 管道域）：段禁切守卫承接今日
    // 「单核 IRQ 屏蔽无并发」的插队排除论证——单核禁切 = 无他任务插队
    // （双核 handler 整体由 SVC 入口结构锁窗串行）；段内拷贝位点局部解蔽
    // （`irq_window`），满检/空快照链项的「此后不变」由禁切保。
    // 对端指针 + 满检 + 队列空快照（4-4 检查链项——对端与本端是不同对象，
    // pair 互指保证，&mut 无别名；满检过且此刻空 = 「队列空」链项成立）。
    // SAFETY: get_raw 已门控；对端由互钉保活（本端活 ⇒ 对端对象必在堆）；
    // 单核禁切无插队（g3-1c-2 段守卫；双核结构锁窗）。
    let _span = PreemptSpan::new();
    let (peer_ptr, peer_empty, peer_frozen) = unsafe {
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
        // AM-1.5：冻结属主标快照（freeze 时枚举属主表预打——send 热路径免
        // 反查属主；非冻结态恒 None ⇒ 本快照即热路径唯一新增成本，一次
        // Option 判空）。
        (peer, !peer_end.has_message(), peer_end.frozen_owner())
    };
    // 随行 handle 值拷入 + 全量预检（无副作用——失败 = 表零变化）。拷贝窗
    // （g3-1c-2）：走查 + 拷入同窗解蔽，段计数保 §A113。
    // W3-Q92（02 号 IPC 审计机会一）：send 双拷窗并锁——hbuf/bbuf 两拷原各
    // 带一次 PT 锁取放，并成**单一 PT 锁窗**贯穿（语句序/错误码/先序逐位
    // 保持——bbuf 拷仍在 probe/distinct 之后；基建 = vmo 族 S-4 4-2 判例：
    // `copy_from_user_locked` + `last_page` 同页缓存。send 全 read 相——
    // 单缓存无读写相混面）。窗内 probe/distinct/take 均纯表操作零锁面，
    // 不新增锁序边（KL 在身、PT 恒 KL 窗内取放——既有不变式）。
    let _pt = if el1 { None } else { Some(crate::vmar::pt_lock_current()) };
    let mut last_page = SAME_PAGE_NONE;
    let mut hbuf = [0u8; CHAN_HND_MAX * 4];
    if let Err(e) = irq_window(|| copy_from_user_locked(el1, handles, &mut hbuf[..nhandles * 4], &mut last_page)) {
        return e as i64;
    }
    let mut hvals = [0u32; CHAN_HND_MAX];
    for i in 0..nhandles {
        hvals[i] = u32::from_le_bytes([hbuf[i * 4], hbuf[i * 4 + 1], hbuf[i * 4 + 2], hbuf[i * 4 + 3]]);
    }
    // 随行 handle 预检（每枚两查）：活值 + **RIGHT_TRANSFER 迁移资格**——
    // 安全轮 2 Medium 缺口关闭（`docs/阶段沿革.md`「RIGHT_TRANSFER 零执法」）：
    // 迁移即能力委托，授予方去 TRANSFER 位的 handle 不得随消息转发第三方
    //（Zircon `zx_channel_write` 对随行 handle 的 ZX_RIGHT_TRANSFER 同款门）。
    // 死值 -EBADF（探活语义不变）、活值缺位 -EACCES。rights 附着槽上随
    // handle 走——W3-Q91 起经类型无关探针 [`HandleTable::rights_of`]（查表
    // +代号校验即出槽 rights 全集，探活内联）**单查折叠**：旧形态按在册
    // 类型逐一 `get_raw(need=0)` 探测（7 类型 × CHAN_HND_MAX=8 最坏 56 次查
    // 表），两路径语义逐位等价（死值同 -EBADF、活值同 rights——判定律单源
    // `berry_hdlface::probe::probe_transfer_ok`，host 钉板对拍在册）；旧注释
    // 「HandleTable 无类型无关查询面」与 rights_of 在册事实相抵，随本刀
    // 翻正（02 号 IPC 审计机会四）。
    for i in 0..nhandles {
        let probe = t.rights_of(hvals[i]).map(|r| r as u32);
        if let Some(e) = berry_hdlface::probe::probe_transfer_ok(probe) {
            return e as i64;
        }
    }
    // 数组内重复预检（W3-Q9 单源判据——循环体见函数注释原子序论证）。
    if !berry_chanface::handles_distinct(&hvals[..nhandles]) {
        return ERR_INVALID_ARG as i64;
    }
    // W3-Q98（02 号机会三刀 b）：载荷拷入**直达尾槽**——满检已过，尾槽此刻
    // 必空且单核禁切窗内无插队（g3-1c-2 段守卫在持）；user→槽位 bytes 单拷贝
    // 直达，bbuf 中转拷贝（copy_from_slice 第二跳）消除。语义逐位保持：
    // 错误面同（源 fault = -EFAULT 早退，槽残句/qlen 不动 = 队列零变化——
    // 残句仅本端点内部可见，用户无观察面：旧形 bbuf 是栈变量同样残句、
    // 满检过后此槽必不被 recv 触达）；语句序不动（拷贝仍在 probe/distinct
    // 之后、take 之前——原子序注全链保持）。W3-Q92 单 PT 锁窗纪律不动：
    // 拷贝窗走查+直达拷出同窗解蔽。
    // SAFETY: peer 互钉在活；满检（queue_full 拒）+ 禁切窗保尾槽空且稳定。
    let slot = unsafe {
        let Some(peer_end) = (*peer_ptr.as_ptr()).obj.as_channel_mut() else {
            return ERR_BAD_HANDLE as i64; // 上面刚解包过——不可达防御面
        };
        peer_end.tail_slot()
    };
    if let Err(e) = irq_window(|| copy_from_user_locked(el1, bytes, &mut slot.bytes[..nbytes], &mut last_page)) {
        return e as i64;
    }
    drop(_pt); // W3-Q92 收窗：此后零用户内存触点——PT 锁不带进 take/push/唤醒链（锁序零新边）
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
    // W3-Q98（机会三刀 b 续）：消息构造**就地槽位**——nbytes/toks/meta/billing
    // 直写预占尾槽（bytes 已在上方单拷贝直达；3b3 H2 零分配纪律不动——
    // 槽位属队列预储容量）。旧形 Message::new + 双 copy_from_slice（bbuf→msg
    // 中转 + msg→push 槽位 440B struct move 第三跳）全消。
    slot.nbytes = nbytes;
    slot.handles[..nhandles].copy_from_slice(&toks[..nhandles]);
    slot.nhandles = nhandles;
    // 4-3c：消息头快照发送者 intent_effective（ADR #42 五件套——fastpath 增量
    // = 一次结构体填充；记账/帽判定全在 recv 结算 slowpath，「fastpath 零记账」）。
    // CURRENT 无效（ktest/init 前语境）→ 零值头（UNSPECIFIED 首手）照常发送。
    if let Some((meta, snap)) = berry_sched::intent_snapshot_current(vct_ns()) {
        slot.meta = meta;
        if snap == berry_sched::SnapOut::Capped {
            // 每跳预算帽：降级为不传播（消息头已装自己的 declared）+ 计数 + trace。
            crate::telemetry::intent_budget_capped();
            berry_hal_aarch64::uart_puts(b"intent: hop budget cap (send)\n");
        }
    }
    // 1.1/S4：受益者付费——send 侧快照发送者域 ID（= Task.pinned）。
    // 零分配（纯填字段）；recv 侧可读（S4 只记不罚——telemetry 计数）。
    slot.billing.domain_id = berry_sched::current_pinned() as u64;
    // 1.1/S4-b 区间计费终末：本任务是 server（此前 recv 设过区间起点）→
    // 算差 → 记 client 域账户 → 清起点。Wall-clock 近似（穿插任务混入
    // ——精确 per-task CPU 需 TCB 扩字段，S5-c 后再议）。
    {
        use core::sync::atomic::Ordering;
        let slot = crate::telemetry::billing_slot();
        let d = crate::telemetry::BILLING_RECV_DOMAIN[slot].load(Ordering::Relaxed);
        if d != 0 {
            let since = crate::telemetry::BILLING_RECV_VCT[slot].load(Ordering::Relaxed);
            let now = berry_hal_aarch64::timer::cntvct();
            if now > since {
                let cycles = now - since;
                // CNTVCT→ns：timer::ticks_to_ns（init 预计算倒数乘-移对——
                // 除法消解同 vct_ns，顺带消掉旧 `cycles*1e9` 在超长区间
                //（>288s @62.5MHz）上的潜在 u64 溢出）。频率不可读 → 0ns →
                // billing_charge 零时长拒收不建槽，与旧 freq!=0 守卫同效。
                let ns = berry_hal_aarch64::timer::ticks_to_ns(cycles);
                crate::telemetry::billing_charge(d, ns);
            }
            // 清起点（下一个 recv 重设）。
            crate::telemetry::BILLING_RECV_DOMAIN[slot].store(0, Ordering::Relaxed);
        }
    }
    // 4-4 检查链判定（push 前取条件——「对端队列空」链项此后不变：单核禁切
    // 无插队，g3-1c-2 段守卫在持）：队列空 + 恰一个非哨兵 READABLE 等待者
    // 且无哨兵在场 → fastpath；否则 slowpath（检查链其余项 v1 结构性成立，
    // 见函数注释）。随行 handle 不再挡快路（P0 拓宽，原「迁移面留 slowpath」
    // 为历史保守切分）：take 迁移面全链非阻塞——alive/take 纯槽操作（无锁、
    // refs 不动、无 drop、take 路零分配），且已在共享前缀完成并内联进消息；
    // fork 之后快慢两路均不再触调用者表 → 有 handle 消息同样直切。
    // S-3c c-2a 义务④窗：find→push→fast_wake 同一 SLEEPQ 锁窗（终态形
    // 「查位+写位+唤醒不可分割」；过渡态 KL 全量在场 ⇒ 零争用，窗形先行
    // ——SLEEPQ_LOCK 注释的逆序标注在此同样适用）。未命中即闭窗——
    // slowpath 的 wake_matching 自带窗。
    // c-2c slice 2a：退化形窗——phys 随锁返回（位已置 = 外层逻辑持有，
    // 物理闭窗跳过）；miss/防御面/hit 三闭窗位互斥。
    let mut sq_phys = false;
    let waiter = if peer_empty {
        sq_phys = berry_sched::sq_lock();
        let w =
            berry_sched::find_single_waiter_locked(peer_ptr.as_ptr() as usize, SIGNAL_READABLE);
        if w.is_none() {
            berry_sched::sq_unlock(sq_phys);
            sq_phys = false;
        }
        w
    } else {
        None
    };
    // SAFETY: peer_ptr 互钉在活；队列有界（满检过——单核禁切无插队 g3-1c-2
    // 段守卫；双核结构锁窗）。W3-Q98：尾槽已由上方 slot 直拷构造，此处仅
    // commit（len +1）——旧形 440B struct move 的 push 全消。
    unsafe {
        let Some(peer_end) = (*peer_ptr.as_ptr()).obj.as_channel_mut() else {
            // 上面刚解包过——不可达防御面（消息泄漏，安全向）。c-2a：若义务④
            // 窗已开（waiter 命中 ⇔ SQ 在持）先闭窗再退——锁泄漏 = 全局死锁。
            if waiter.is_some() {
                berry_sched::sq_unlock(sq_phys);
            }
            return ERR_BAD_HANDLE as i64;
        };
        peer_end.push_tail_commit();
    }
    drop(_span); // g3-1c-2 段终点（push 毕）：原子段收口配平
    match waiter {
        Some(t) => {
            // fastpath：定点唤醒（同一内部唤醒序列 + PI 收尾重推）→ **闭窗** →
            // 直切判定（严格更高 eff 才切——切走后从 try_preempt 续行、IRQ 仍
            // 屏蔽；try_preempt 可切换，不入 SQ 窗 = 终态「结构锁窗内禁切换」）。
            // SAFETY: t 来自同一 SQ 窗内 find_single_waiter_locked（此刻仍在
            // SLEEPQ 上）。
            unsafe { berry_sched::fast_wake_locked(t) };
            berry_sched::sq_unlock(sq_phys);
            crate::telemetry::ipc_fast_hit();
            if berry_sched::try_preempt() {
                crate::telemetry::ipc_direct_switch();
            }
        }
        None => {
            // slowpath：4-2 唤醒点——消息落对端队列 → 对端 READABLE 置位，唤醒
            // 等「对端可读」的睡眠者（键 = 对端 ObjRef 地址；等价于 Zircon send
            // 侧的可读信号；无等待者时本遍历近零开销）。S-3b b-1 定谳：纯广播
            // ——公平/上限不进唤醒选择器（到达序内生闭环判词详
            // `berry_sched::wake_matching` 文档），容量槽公平归放置/偷取面。
            crate::telemetry::ipc_fast_miss();
            berry_sched::wake_matching(peer_ptr.as_ptr() as usize, SIGNAL_READABLE);
        }
    }
    // AM-1.5 wake-pending（IPC × 冻结，Q1 裁决「唤醒即解冻」机制面）：send
    // 命中冻结端点 → 消息已照常入队（上 push——排队不丢弃），此处补通知
    // 闭环：属主首命中置 wake_pending（**一次语义**——同进程多端点/后续
    // send 见位不再重复通知，防风暴；S5.12 token 在册上限 64 已是 DoS 帽）
    // + 以进程钉键广播唤醒等其 SuspendToken 的持有者（wait_one 的 token
    // 分支同键挂队——同进程全部在册 token 一跳全收）；策略在用户态——
    // 持有者收通知后自行 close token 解冻。仅成功路径：-EBUSY 满拒未入
    // 队、无消息可言，不通知（调用方 wait WRITABLE 后重试即再触发）。
    if let Some(owner) = peer_frozen {
        // SAFETY: 冻标由 freeze 路径 retain 属主（配平在 thaw 清面）；SVC
        // 屏蔽串行语境无并发写竞；owner 必为 KernelObj::Process（打标侧
        // 类型门控过），as_process_mut 恒 Some（else = 不可达防御面）。
        unsafe {
            if let Some(p) = (*owner.as_ptr()).obj.as_process_mut() {
                if !p.wake_pending {
                    p.wake_pending = true;
                    berry_sched::wake_matching(
                        owner.as_ptr() as usize,
                        crate::handle::SIGNAL_WAKE_PENDING,
                    );
                }
            }
        }
    }
    // 段出口检查点（g3-1c-2，f4 裁决 = wake 链尾非 push 直后）：拷贝窗内被
    // 双门（tick go 支/SGI）挡下的切换诉求在唤醒链全毕后兑现——①push 直后
    // 切走会打断 S-3c 义务④「find→push→fast_wake 查位+写位+唤醒不可分割」
    // 窗（物理锁舞蹈能保、语义不可分割不能）；②wake 后才切不劣于今日
    //（今日 = wake 后紧跟 fastpath try_preempt 探峰）；③与 fastpath 探峰分工
    //——fast 命中路先探、段检查点收 slow/收尾，fast 命中路的重探恒空 = 白付
    // 但中性（icount 已证）。
    // 探峰合法 ⇔ RQ 位在集（本号 0x2E ✓，try_preempt rq_lock 退化形，§A126
    // 定谳）；段内错误路径早退跳过此处，由 g3-1b 落点①（SVC 派发尾零锁）承接。
    berry_sched::preempt_checkpoint();
    0
}

/// recv 失败回滚：`inserted` 里的新 handle 值逐个 take 回（调用者表复原——
/// 令牌本就还在消息里，insert_ref 不搬走它），消息压回**队首**（FIFO 序复原）。
/// 令牌引用计数全程未动（take/insert_ref 都不碰 refs）→ 复原后系统与调用前一致。
///
/// # Safety
/// `endp` 须为本端在表对象（get_raw 门控过）；`msg` 须为从该端队列 pop 出的
/// 消息；单核禁切上下文（g3-1c-2：调用点均在 recv 原子段守卫内）。
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
/// 原子序：空队先检（空时零用户内存读 = 零 fault 面）→ pop 消费 → 容量 →
/// **出参段全量 access_ok 预扫**（bytes/handles 数组/nbytes/nhandles 槽，
/// write 走查——载荷拷出**前**收口全部 fault 面：否则载荷已落用户、后续
/// 出参写才 fault → 回滚压回 = 同一消息可被无限次窥探的缺口）→ 载荷/
/// 令牌入表/回写各步任一失败 → [`recv_rollback`]（表 + 队列全复原，最后防线）。
/// recv 尾段三步（单条消息的完整消费序，`sys_channel_recv` 与
/// `sys_channel_recv_n`〔W3-Q97 批量循环〕共用——语义单源，不长第二套）：
/// ①令牌入表 + ②同窗回写（载荷/新值/双 u64）+ ③每消息结算面
/// （intent/billing/WRITABLE 唤醒）。前置已由调用方完成：门控、pop、
/// 容量读与容量门、出参段全量预扫（SEC-C）、`_pt`/`_span` 在持。
///
/// 返回 `Err(e)` = 该条消息消费失败（调用方回滚该条并返回错误码——
/// 批量面 = 已成功的 i-1 条**已消费不回滚**（intent 已结算、腾槽唤醒收尾照发，
/// 语义与逐条 recv 到第 i 条失败完全一致）；`Ok(())` = 该条消费完成。
///
/// `slot` = 本条消息的出参窗基址：单发 recv = 三出参独立地址
/// （bytes/handles/nbytesp/nhandlesp 各自的 ABI 地址，容量面已验）；
/// 批量 recv_n = 槽基址（头 16B = nbytes/nhandles 出参、其后载荷区、
/// 再后 handle 值区——槽距 `RECV_N_SLOT_STRIDE`，窗内已 write 预扫）。
/// 两种形态都只含「写已预扫过的窗」这一动作，无双轨。
#[allow(clippy::too_many_arguments)]
fn recv_consume_one(
    t: &mut HandleTable,
    self_ptr: NonNull<ObjRef>,
    mut msg: Message,
    el1: bool,
    lp_write: &mut u64,
    bytes: u64,
    handles: u64,
    nbytesp: u64,
    nhandlesp: u64,
) -> Result<(), i32> {
    // 令牌入调用者表（新值收集；任一失败全量回滚——表里已入的 take 回）。
    let mut newvals = [0u32; CHAN_HND_MAX];
    for i in 0..msg.nhandles {
        match t.insert_ref(msg.handles[i].0, msg.handles[i].1) {
            Ok(nh) => newvals[i] = nh,
            Err(e) => {
                // SAFETY: 同上（msg 内令牌未动，回滚只需压回）。
                unsafe { recv_rollback(t, self_ptr, msg, &[]) };
                return Err(e);
            }
        }
    }
    let n = msg.nhandles;
    let mut obuf = [0u8; CHAN_HND_MAX * 4];
    for i in 0..n {
        obuf[i * 4..i * 4 + 4].copy_from_slice(&newvals[i].to_le_bytes());
    }
    // W3-Q92 尾段：载荷/新值/双 u64 回写全走同窗 `_locked` 族（窗 = 调用方
    // 持的单一 PT 锁窗；错误码与回滚面逐位保持）。
    let tail = irq_window(|| -> Result<(), i32> {
        copy_to_user_locked(el1, bytes, &msg.bytes[..msg.nbytes], lp_write)?;
        copy_to_user_locked(el1, handles, &obuf[..n * 4], lp_write)?;
        copy_out_u64_locked(el1, nbytesp, msg.nbytes as u64, lp_write)?;
        copy_out_u64_locked(el1, nhandlesp, n as u64, lp_write)?;
        Ok(())
    });
    if let Err(e) = tail {
        // SAFETY: 回滚含 take 回已入表的 n 个令牌（位置号即条数）。
        unsafe { recv_rollback(t, self_ptr, msg, &newvals[..n]) };
        return Err(e);
    }
    // SAFETY: 本端门控过；单核禁切（g3-1c-2 段守卫在持）。
    // W3-Q97 复审修（A 案摊销）：WRITABLE 唤醒自本结算面剥出——单发/
    // 批量同构：消费窗内恒免唤醒，调用方尾段 PT 锁窗外收尾一次
    // `wake_matching`（单发 = 成功路径无条件；批量 = `recv_n_wake_needed`
    // (got)` 门）。语义等价律：S-3b b-1 纯广播定谳 + PreemptSpan 全窗
    // 禁切 ⇒ 窗内 k 次与 1 次终态等价（窗内无并发观察者）。
    unsafe { recv_settle_one(self_ptr, &mut msg) };
    Ok(())
}

/// recv 每消息结算面（billing + intent；W3-Q97 自 `sys_channel_recv`
/// 尾段抽出——单发/批量共用，语义单源；**不含 WRITABLE 唤醒**——复审修
/// A 案：唤醒摊到调用方收尾一次，本面只留每消息必结算项）：
/// - 4-3c slowpath 记账：dequeue 即结算 intent（每消息重导——借入/
///   opt-out/超帽的语义与遥测见 `berry_sched::intent`；CURRENT 无效
///   则跳过不记）。
/// - 1.1/S4-b：recv 侧——记跨域计数 + 设区间计费起点（domain +
///   CNTVCT 戳；send 侧〔server 回复时〕算差 → 记 client 域）。
/// - 4-2 唤醒律（消费方尾段统一执行，非本面）：本端弹出一条 → 对端
///   队列腾出一个槽 → 对端 WRITABLE 置位，唤醒等「对端未满」的睡眠者
///   （A 的可写性由 B 的队列决定——键 = 对端 ObjRef 地址；仅成功路径
///   唤醒：回滚路径状态全复原，唤醒即虚假）。S-3b b-1 定谳：纯广播
///   ——公平/上限不进唤醒选择器（判词详 `berry_sched::wake_matching`
///   文档）。
///
/// SAFETY: `self_ptr` 须为本端在表对象（get_raw 门控过）；单核禁切
/// 上下文（g3-1c-2：调用点均在 recv 原子段守卫内）。
unsafe fn recv_settle_one(self_ptr: NonNull<ObjRef>, msg: &mut Message) {
    // 4-3c slowpath 记账。SAFETY: 本端门控过；只读 no_intent 标志。
    let opt_out = (*self_ptr.as_ptr()).obj.as_channel().map_or(false, |e| e.no_intent());
    // 1.1/S4-b：recv 侧——记跨域计数 + 设区间计费起点。
    // v2 协议标签过滤：reply(msg_type=2) 不设计费区间——client recv
    // server 回复不误归 server 域（v1 全部 msg_type=0 照设——已知局限）。
    if msg.billing.domain_id != 0 {
        crate::telemetry::billing_cross_domain();
        // S4-b 区间起点：per-CPU 槽（v1 单核 0 号；SMP 需 per-CPU 数组）。
        if msg.billing.msg_type != crate::channel::MSG_BILLING_REPLY {
            let slot = crate::telemetry::billing_slot();
            crate::telemetry::BILLING_RECV_DOMAIN[slot].store(msg.billing.domain_id, core::sync::atomic::Ordering::Relaxed);
            crate::telemetry::BILLING_RECV_VCT[slot].store(berry_hal_aarch64::timer::cntvct(), core::sync::atomic::Ordering::Relaxed);
        }
    }
    // W3-Q6：档位直方/通胀计数（Own/Borrowed 皆记——effective 档分布 +
    // 借入档差；三帽计数器族同窗同纪律）。
    if let Some((out, declared, effective)) =
        berry_sched::intent_settle_current_ext(msg.meta, opt_out, vct_ns())
    {
        crate::telemetry::intent_settle_note(declared, effective);
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
}

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
    // g3-1c-2 原子段（pop→回滚完结，草案 §8.B 管道域）：段禁切守卫承接今日
    // 「单核 IRQ 屏蔽」的插队排除论证——rollback push_front 与并发 send 的
    // FIFO 序破面，单核禁切即封（双核 handler 整体由 SVC 入口结构锁窗串行）；
    // 段内拷贝位点局部解蔽（`irq_window`）。
    // SAFETY: get_raw 已门控；单核禁切（g3-1c-2 段守卫在持）。pop 消费，失败
    // 路径经回滚压回队首。
    let _span = PreemptSpan::new();
    let msg = unsafe {
        let Some(end) = (*self_ptr.as_ptr()).obj.as_channel_mut() else {
            return ERR_BAD_HANDLE as i64;
        };
        match end.front() {
            Some(_) => end.pop(),
            None => return ERR_AGAIN as i64, // 非阻塞：空即返（阻塞等消息 = wait_one READABLE，4-2）
        }
    };
    // W3-Q92（02 号 IPC 审计机会一）：recv 用户参数 I/O 合并单窗——容量
    // 双读 + 出参段全量预扫 + 载荷/新值/双 u64 回写原 10 次 walk_leaf、
    // 6 次 PT 锁取放、6 次 irq_window 开包（copy_in_u64×2/access_ok×4/
    // copy_to_user×3/copy_out_u64×2 各自带窗），并成**单一 PT 锁窗 + 单一
    // 解蔽窗**贯穿收尾（基建 = vmo 族 S-4 4-2 判例：`access_ok_window` 同页
    // 缓存 + `copy_*_locked` 族——10 次走查里 8 次本就是重复验同页）。
    // 语义逐位保持：容量读错/容量门拒/预扫拒/拷出/回写各步的**错误码与
    // 回滚面不变**（回滚臂 `recv_rollback` 语义契约零触碰）——拷贝窗物理
    // 并窗，段守卫（g3-1c-2）与 §A113 walk+store 同窗铁律原样成立（窗更
    // 大、窗数更少）。中段表操作（insert_ref 令牌入表）零用户内存触点，
    // 留窗外（表无锁面不进 PT 窗——锁序不变式：窗内可达面不新增锁类）。
    let _pt = if el1 { None } else { Some(crate::vmar::pt_lock_current()) };
    // 权限相分离（SEC-C 保持）：`last_page` 同页缓存**不分读写权限**——读相
    // （容量双读）与写相（预扫/拷出/回写）须各持独立缓存：若共用，读走查
    // 缓存页会使写走查被跳过（RO 页读得过、写不过——正是预扫要抓的缺口
    // 形状；写被跳过 → 原始 store 落 RO 页 = EL1 权限 fault）。vmo 族先例
    // 单窗单权限相（read/write 各一窗）无此面；本号读写双相 → 双缓存。
    let mut lp_read = SAME_PAGE_NONE; // 读相：容量双读（nbytesp/nhandlesp 同页时第二读免走查）
    let mut lp_write = SAME_PAGE_NONE; // 写相：预扫四段 + 尾段三写（同页免重复走查——全部 write=true 相）
    // 前半（容量双读）：
    let caps = irq_window(|| -> Result<(usize, usize), i32> {
        let cap_bytes = copy_in_u64_locked(el1, nbytesp, &mut lp_read)? as usize;
        let cap_hnd = copy_in_u64_locked(el1, nhandlesp, &mut lp_read)? as usize;
        if !berry_chanface::recv_capacity_legal(msg.nbytes, msg.nhandles, cap_bytes, cap_hnd) {
            return Err(ERR_INVALID_ARG);
        }
        Ok((cap_bytes, cap_hnd))
    });
    if let Err(e) = caps {
        // SAFETY: 同上（容量读/容量门失败——无令牌入表）。
        unsafe { recv_rollback(t, self_ptr, msg, &[]) };
        return e as i64;
    }
    // 预扫面（同窗）：四段 write 走查（bytes/handles/nbytesp/nhandlesp）——
    // 原 4 次独立 access_ok 并成同窗 4 段 span_pages 流。
    // SEC-C（recv 窥探缺口，安全修复语义保持）：载荷拷出**前**预扫全部出参
    // 段——否则载荷已落用户、后续出参写才 fault → 回滚压回 = 同一消息可被
    // 无限次窥探 + 队列槽钉死（DoS）。nbytesp/nhandlesp 槽的 write 预扫
    // 不可省：上方容量读只验过**读**（RO 页读得过、写不过——正是原缺口
    // 形状）。预扫任一失败 → 载荷零拷出即 -EFAULT（消息压回 = 未消费原样）。
    let prescan_ok = irq_window(|| {
        access_ok_window(el1, bytes, msg.nbytes as u64, true, &mut lp_write)
            && access_ok_window(el1, handles, (msg.nhandles * 4) as u64, true, &mut lp_write)
            && access_ok_window(el1, nbytesp, 8, true, &mut lp_write)
            && access_ok_window(el1, nhandlesp, 8, true, &mut lp_write)
    });
    if !prescan_ok {
        // SAFETY: 同上（无令牌入表；载荷未拷出——用户侧零可见副作用）。
        unsafe { recv_rollback(t, self_ptr, msg, &[]) };
        return ERR_FAULT as i64;
    }
    // 载荷拷出随 W3-Q92 移入尾段同窗（原独立拷出窗退役）。
    // W3-Q97：入表→回写→结算收进 `recv_consume_one`（与批量 recv_n 共用，
    // 语义单源；错误码与回滚面逐位保持——Err 路径的回滚在函数内完成）。
    if let Err(e) = recv_consume_one(t, self_ptr, msg, el1, &mut lp_write, bytes, handles, nbytesp, nhandlesp) {
        return e as i64;
    }
    // W3-Q97：msg 已随消费序移入 recv_consume_one（按值）——原显式
    // drop(msg) 退役（Message 定长内联，自然 drop 同效：无堆动作不碰 refs）。
    drop(_pt); // W3-Q92 收窗：此后零用户内存触点——PT 锁不带进唤醒链（SQ/RQ 锁序零新边）
    drop(_span); // g3-1c-2 段终点（回滚链毕）：原子段收口配平
    // W3-Q97 复审修（A 案摊销）：4-2 WRITABLE 唤醒收尾一次（消费窗内免
    // 唤醒——唤醒自 recv_settle_one 剥出；成功路径无条件，时序/锁序与
    // 剥出前基线逐位一致：PT 锁窗外、段收口后）。SAFETY: 本端门控过、
    // 互钉在活；只读对端指针。
    if let Some(peer) = unsafe { (*self_ptr.as_ptr()).obj.as_channel() }.and_then(|e| e.peer()) {
        berry_sched::wake_matching(peer.as_ptr() as usize, SIGNAL_WRITABLE);
    }
    // 段出口检查点（g3-1c-2，f4 裁决对称 = WRITABLE wake 链后、返回前）：
    // 拷贝窗内被双门挡下的切换诉求在唤醒链全毕后兑现（论据同 send 尾注）；
    // 探峰合法 ⇔ RQ 位在集（0x2E ✓，§A126 定谳）；段内错误路径早退由
    // g3-1b 落点①承接。
    berry_sched::preempt_checkpoint();
    0
}

/// `channel_recv_n(ch, msgs, k_max, *n_recv)`：**排空式批量收**（W3-Q97，
/// 02 号 IPC 审计机会五——add-only 新号，先例 0x21/0x23/0x24/0x83/0x84）。
/// 门控：本端 **RECV**（与 0x53 同门同序）。`msgs` = 定长槽位数组
/// （槽距 [`RECV_N_SLOT_STRIDE`] = 304B：头 16B〔`nbytes:u64`/
/// `nhandles:u64` **出参**〕+ 载荷 256B + handle 值 32B〔新值区〕）。
/// 返回 0 = 成功（实收数走 `*n_recv`：0 = 队空 poll 语义同形，1..=k =
/// 实收条数——每槽头回写该条的实际 `nbytes`/`nhandles`）。
///
/// **每消息语义逐条保持**（机会五裁案）：批量循环体 = `recv_consume_one`
/// （与单发 recv 完全同函数——intent 结算/billing/insert_ref 入表
/// per-message），仅 SVC 入口/caller_table/get_raw 门控/PT 锁获取/
/// WRITABLE 唤醒摊到 k 条（复审修 A 案：唤醒自 `recv_settle_one` 剥出，
/// 批量收尾由 [`recv_n_wake_needed`]`(got)` 门控单发一次——纯广播一次
/// 覆盖全部腾槽，S-3b b-1 定谳）。容量语义 = 槽内**满形预储**（每槽按
/// `CHAN_MSG_MAX`/`CHAN_HND_MAX` 满形预扫——不做逐条 in-out 容量读：
/// 批量面的容量契约简化为「槽距即容量」，单发 recv 的 in-out 容量语义
/// （不足 -EINVAL）不进批量面——满形槽恒容得下任何合法消息，该错误
/// 面在批量 ABI 上结构性消失）。
///
/// 原子序（g3-1c-2：一个 PreemptSpan 覆盖整个批量窗）：门控 → 全窗
/// write 预扫（k 槽 × 304B + `*n_recv` 槽——SEC-C：载荷拷出**前**收口
/// 全部 fault 面）→ 循环 k 次 { pop（空即断）→ 令牌入表 → 槽内回写 →
/// 结算 } → `*n_recv` 回写 → 收尾一次 WRITABLE 唤醒（`recv_n_wake_needed` 门）。
/// **中途失败语义**：第 i 条
/// `recv_consume_one` Err = 该条回滚压回（消息原样），前 i-1 条已消费
/// **不回滚**（intent 已结算、腾槽唤醒收尾照发——与逐条 recv 到第 i 条失败完全
/// 一致），`*n_recv` = i-1，返回错误码（调用方按需重试第 i 条）。
extern "C" fn sys_channel_recv_n(tf: &mut TrapFrame) -> i64 {
    use berry_chanface::{recv_n_k_legal, recv_n_slot_legal, recv_n_wake_needed, RECV_N_SLOT_STRIDE};
    let (h, msgs, k_max, n_recvp) = (
        tf.regs[1] as u32,
        tf.regs[2],
        tf.regs[3] as usize,
        tf.regs[4],
    );
    // no-panic 前置（判据单源 = chanface recvn）：k 域 + 出参槽地址。
    if !recv_n_k_legal(k_max) {
        return ERR_INVALID_ARG as i64;
    }
    if !recv_n_slot_legal(n_recvp as usize) {
        return ERR_INVALID_ARG as i64;
    }
    let el1 = caller_is_el1(tf);
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let self_ptr = match t.get_raw(h, ObjType::Channel, RIGHT_RECV) {
        Ok((o, _)) => o,
        Err(e) => return e as i64,
    };
    // g3-1c-2 原子段：一个 PreemptSpan 覆盖整个批量窗（pop→回滚完结 × k）。
    // SAFETY: get_raw 已门控；段守卫在持（与单发 recv 同形，窗更长——批量
    // 面 k ≤ 16，窗内拷贝位点局部解蔽同单发）。
    let _span = PreemptSpan::new();
    // W3-Q92 同款单 PT 锁窗贯穿全批量窗（写相缓存共享——全部 write=true 相）。
    let _pt = if el1 { None } else { Some(crate::vmar::pt_lock_current()) };
    let mut lp_write = SAME_PAGE_NONE;
    // SEC-C 全窗预扫：k 槽 × 304B + n_recv 槽——载荷拷出前收口全部 fault 面
    // （任一失败 → 零拷出 -EFAULT，队列原样）。同窗 span_pages 流（W3-Q92
    // 预扫面同构，窗从 4 段扩为 k×304+8 连续段）。
    let window = k_max as u64 * RECV_N_SLOT_STRIDE as u64;
    let prescan_ok = irq_window(|| {
        access_ok_window(el1, msgs, window, true, &mut lp_write)
            && access_ok_window(el1, n_recvp, 8, true, &mut lp_write)
    });
    if !prescan_ok {
        return ERR_FAULT as i64; // 零消费零拷出——队列原样，无回滚面
    }
    // 批量循环：每条 = recv_consume_one（单发 recv 同函数——语义单源）。
    let mut got: usize = 0;
    let mut ret: i64 = 0;
    for i in 0..k_max {
        // SAFETY: get_raw 已门控；段守卫在持；pop 消费，失败路径回滚压回。
        let msg = unsafe {
            let Some(end) = (*self_ptr.as_ptr()).obj.as_channel_mut() else {
                ret = ERR_BAD_HANDLE as i64; // 不可达防御面（get_raw 过）
                break;
            };
            match end.front() {
                Some(_) => end.pop(),
                None => break, // 队空即断（poll 语义：实收 got 条）
            }
        };
        let slot = msgs + (i as u64) * (RECV_N_SLOT_STRIDE as u64);
        let (sbytes, shandles, snbytes, snhandles) = (
            slot + 16,             // 载荷区基址（头 16B 后）
            slot + 16 + CHAN_MSG_MAX as u64, // handle 值区基址
            slot,                  // nbytes 出参（头 8B）
            slot + 8,              // nhandles 出参（头次 8B）
        );
        if let Err(e) = recv_consume_one(
            t, self_ptr, msg, el1, &mut lp_write, sbytes, shandles, snbytes, snhandles,
        ) {
            ret = e as i64; // 第 i 条回滚压回（函数内完成）；前 i-1 条已消费
            break;
        }
        got += 1;
    }
    // 出参回写（预扫过）：实收数。失败路径也回写 got（部分成功诚实上报——
    // 调用方拿得到已消费条数，错误码指向第 got+1 条）。
    let tail = irq_window(|| -> Result<(), i32> {
        copy_out_u64_locked(el1, n_recvp, got as u64, &mut lp_write)?;
        Ok(())
    });
    if let Err(e) = tail {
        ret = e as i64; // 出参写失败：消息已消费——调用方重试态自理（同 event_create 政策）
    }
    drop(_pt);
    drop(_span); // g3-1c-2 段终点（回滚链毕）：原子段收口配平（与单发同构）
    // W3-Q97 复审修（A 案摊销）：4-2 WRITABLE 唤醒收尾一次——批量窗内
    // recv_consume_one 恒免唤醒（唤醒自 recv_settle_one 剥出），此处由
    // `recv_n_wake_needed(got)` 门控单发一次（弹 >0 条 → 对端腾 got 槽 →
    // 纯广播一次覆盖全部腾槽——S-3b b-1 定谳；弹 0 条〔队空即返〕不唤醒
    // ——无槽腾出，唤醒即虚假，单发 recv 空队早退同律）。等价律：PreemptSpan
    // 全窗禁切 ⇒ 窗内 k 次与 1 次终态等价（窗内无并发观察者）；失败路径
    // got>0 也照发（前 got 条已消费腾槽，唤醒真实非虚假）。锁序 = 单发
    // 同构：PT 锁窗外、段收口后。SAFETY: 本端门控过、互钉在活；只读对端指针。
    if recv_n_wake_needed(got) {
        if let Some(peer) = unsafe { (*self_ptr.as_ptr()).obj.as_channel() }.and_then(|e| e.peer()) {
            berry_sched::wake_matching(peer.as_ptr() as usize, SIGNAL_WRITABLE);
        }
    }
    // 段出口检查点（g3-1c-2，f4 裁决同单发 recv：WRITABLE wake 链后、返回前）；
    // 探峰合法 ⇔ RQ 位在集（0x2E ✓，§A126 定谳）。
    berry_sched::preempt_checkpoint();
    ret
}

// ---- event_create（3-4c；对象 = `event.rs` 纯用户位掩码）----

/// `event_create(options, *out)`：建一个 Event（出生信号全零）。`options` v1
/// 恒 0（非 0 → -EINVAL，ABI §8.3 注记）；授 **SIGNAL|DUPLICATE|TRANSFER**
/// ——持有者可置/清位、可复制授权给同伴（等的一方 dup 到只读即可，wait 不
/// 需权利位）；TRANSFER = send 随行迁移资格（Zircon 默认携位模型）。
extern "C" fn sys_event_create(tf: &mut TrapFrame) -> i64 {
    let (options, out) = (tf.regs[1], tf.regs[2]);
    if options != 0 {
        return ERR_INVALID_ARG as i64;
    }
    let el1 = caller_is_el1(tf);
    if let Err(e) = copy_out_u32(el1, out, 0) {
        return e as i64;
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    let h = match t.insert(KernelObj::Event(Event::new()), RIGHT_SIGNAL | RIGHT_DUPLICATE | RIGHT_TRANSFER) {
        Ok(h) => h,
        Err(e) => return e as i64, // 表满/gen 尽/装箱 OOM——对象未入表，零回滚面
    };
    match copy_out_u32(el1, out, h) {
        Ok(()) => 0,
        Err(e) => e as i64, // 对象已入表——调用方重试态自理（同 channel_create 政策）
    }
}

// ---- object_wait/signal 面（3-4c 事件三号 → 4-1 阻塞化 → 4-2 扩 Channel）----

/// 对象当前信号状态（4-2 wait 面统一查询点）：
/// - Event → 用户位掩码原样；
/// - Channel → 内核派生：`READABLE`（**本端**队列非空）| `WRITABLE`（**对端**
///   队列未满；未配对端点对端恒「满」——运行期不可达，create 即配对）；
/// - SuspendToken → 内核派生：`WAKE_PENDING`（目标进程冻结期间有 send 命中
///   其端点——AM-1.5；零 per-token 状态，读目标 `Process.wake_pending` 派生）；
/// - 其余类型 → `None`（wait 面只收 Event|Channel|Process|Thread|SuspendToken）。
///
/// # Safety
/// `obj` 须为在表/钉住的活对象；Channel 分支解引用互钉对端（互钉保活）、
/// SuspendToken 分支解引用 token retain 的目标（Drop 链保活）。
/// 只读；调用点均在 IRQ 屏蔽串行语境。
unsafe fn signal_state(obj: NonNull<ObjRef>) -> Option<u64> {
    unsafe {
        match &(*obj.as_ptr()).obj {
            KernelObj::Event(e) => Some(e.load()),
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
            // Plan B v0.5：TERMINATED 单调置位（Exited 后永不清——同 Zircon
            // TASK_TERMINATED；等其它位（READABLE 等）= 永不命中，同 Channel
            // 用户位的不设门口径）。
            KernelObj::Process(p) => {
                Some(if p.state == ProcState::Exited { SIGNAL_TERMINATED } else { 0 })
            }
            // T-1 join 面：Thread 同口径（退出钩子在「永不再跑点」置 exited
            // ——单调永不清；等非 TERMINATED 位 = 永不命中，不设门同上）。
            KernelObj::Thread(th) => {
                Some(if th.exited { SIGNAL_TERMINATED } else { 0 })
            }
            // AM-1.5：SuspendToken 信号面 = 目标进程 wake_pending 派生（同刻
            // 全部在册 token 同视；置位点 = send 命中冻端点、清位点 = 全解冻，
            // 均不在本函数）。Thread 目标不参与——端点冻标只在进程全冻时打
            //（单线程冻不在本面），且进程冻 × 线程冻互斥 ⇒ 线程 token 恒观 0。
            // 等非 WAKE_PENDING 位 = 永不命中（不设门，同上两口径）。
            KernelObj::SuspendToken(tok) => Some(match tok.target {
                crate::suspend::SuspendTarget::Process(p) => {
                    // SAFETY: token retain 过目标（Drop 链保活）；只读一位。
                    let wp = (*p.as_ptr()).obj.as_process().map_or(false, |pr| pr.wake_pending);
                    if wp { crate::handle::SIGNAL_WAKE_PENDING } else { 0 }
                }
                crate::suspend::SuspendTarget::Thread(_) => 0,
            }),
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
    if !berry_evface::signal_masks_legal(clear, set) {
        return ERR_INVALID_ARG as i64; // 低 32 位内核派生保留（4-2 位空间；
        // 判定律单源 berry-evface，host 钉板——W3-Q7）
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

/// wait 面取对象（wait_one/wait_many 共用，Plan B 起三类型链、T-1 起
/// 四类型链、AM-1.5 起五类型链）：Event → Channel → Process → Thread →
/// SuspendToken（wait 不需权利位，need=0）。全拒回**首个**（Event 的）
/// 错误——与 4-2 的两类型链语义逐字一致（-EBADF/-EACCES 原样透传）。
/// W3-Q91 起折叠为**类型无关单查**：`rights_of`（查表+代号校验）出对象
/// 槽后一次 `obj_type()` 判 waitable（词表单源 `berry_hdlface::probe`，
/// host 钉板）——旧五链每枚 2-5 次完整查表（含 ObjRef 解引用×类型比对
/// ）压为 1 次；词表外类型同码拒（-EBADF），行为逐位一致（02 号 IPC
/// 审计机会四）。
fn wait_get(t: &HandleTable, h: crate::handle::Handle) -> Result<NonNull<ObjRef>, i32> {
    t.get_waitable(h)
}

/// `object_wait_one(h, signals, deadline, *observed)`（4-1 阻塞化；4-2 扩
/// Channel）：等待 `current & signals != 0`。命中 → `*observed = current`
/// （**全满足集**，Zircon 同形）返 0；poll（`deadline==0`）未命中 → -EAGAIN；
/// `deadline≠0`（MONOTONIC 绝对时刻）未命中 → 挂 [`berry_sched::block_current`]
/// 睡到点。
///
/// **wait 面对象 = Event | Channel | Process | Thread | SuspendToken**
///（4-2 / Plan B / T-1 / AM-1.5）：Event 状态 = 用户位掩码；Channel 状态 =
/// 内核派生 READABLE/WRITABLE；Process 状态 = 内核派生 TERMINATED（Exited
/// 单调置位——父进程 wait 后 unmap/close 回收，[`berry_exec`] 的 reap 链）；
/// Thread 状态 = 内核派生 TERMINATED（T-1 join 面——`thread_start` 后
/// `object_wait_one(th, TERMINATED, …)` 即内核侧 join，裁决二）；SuspendToken
/// 状态 = 内核派生 WAKE_PENDING（AM-1.5：冻结期间有 send 命中目标进程
/// 端点——持有者收通知后自行 close token 解冻，策略在用户态）。查询点
/// [`signal_state`]；等不出的位（Channel 用户位 / Process|Thread 非
/// TERMINATED 位 / SuspendToken 非 WAKE_PENDING 位）永不命中，同 Zircon
/// 不设门。
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
    // 4-2/Plan B/T-1：四类型链（Event|Channel|Process|Thread）——wait 不需
    // 权利位；全拒回首个错误（-EBADF/-EACCES 原样透传）。
    let obj = match wait_get(t, h) {
        Ok(o) => o,
        Err(e) => return e as i64,
    };
    // 统一循环：挂队前 + 每次醒来都重验（醒因不等于结果）。钉住**一次覆盖
    // 全部睡眠轮次**（清位竞态下可能再睡多轮），release 收口在唯一出口——
    // release 后不再触 obj。
    // SAFETY: 活对象（表内 handle 钉着）；睡眠期间全表 handle 被关光也不销毁
    //（3-3 钉住纪律的跨睡眠版）。
    unsafe { (*obj.as_ptr()).retain() };
    // AM-1.5：SuspendToken 等待者挂队键 = 目标钉键（`SuspendTok.key`——
    // Process 目标 = 进程 ObjRef 地址 = pinned；Thread 目标 = 线程键）而非
    // token 自身 ObjRef 地址：send 侧 wake-pending 广播以进程钉键唤该进程
    // **全部**在册 token 的等待者（嵌套多 token 一跳全收，无需逐 token 寻址）。
    // 键与 Process 本体等待者（TERMINATED）共用，但请求位集不相交
    //（WAKE_PENDING vs TERMINATED）⇒ 零互扰；偶发跨键虚假唤醒由醒来重验
    // 兜底（同 WAIT_ANY 哨兵口径）。PI 面安全：`pi::donor_edge` 只对等
    // READABLE|WRITABLE 派生位的睡眠者解引用 wait_obj（§A40 不变量），
    // WAKE_PENDING 等待者恒不入捐赠图。
    // SAFETY: 活对象（上方 retain 在身）；只读类型分支与 key 字段。
    let wait_key = match unsafe { &(*obj.as_ptr()).obj } {
        KernelObj::SuspendToken(tok) => tok.key,
        _ => obj.as_ptr() as usize,
    };
    // G3-4b SGI 路进入点钩子（位签名 + armed 双过滤——默认面恒短路，仅
    // 付一次寄存器比较；钉住后调用 = 落笔的 ObjRef 活性由本 wait 钉着）。
    // 前提〔f4 F8〕：「wait 入口位已清」是 EL0 payload 纪律而非内核执法——
    // 位已置时下方快查命中即返，ENTER 已计入而 sleeper 不睡；该违例由
    // waker 侧 n==1 断言 fail-loud 收口（G3-4b 首跑 first.log 实证形态）。
    if signals == crate::LAT_SGI_BIT
        && crate::LATSGI_ARMED.load(core::sync::atomic::Ordering::Relaxed)
    {
        crate::latsgi_wait_enter(obj.as_ptr() as usize);
    }
    let ret = loop {
        // SAFETY: 钉住在身对象必活；只读（signal/send/recv 写路径与本读点同
        // 为屏蔽串行）。
        let Some(current) = (unsafe { signal_state(obj) }) else {
            break ERR_BAD_HANDLE as i64; // 不可达防御面
        };
        // W3-Q7：命中判单源 berry-evface::wait_hit（current & signals != 0
        // 纯投影，host 钉板）。
        if berry_evface::wait_hit(current, signals) {
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
        // 匹配键 = ObjRef 地址（SuspendToken 例外 = 目标钉键，见上方
        // wait_key 注）；请求位 = signals（signal 侧以新置位集匹配）。
        // 醒因丢弃：Signaled/Timeout 都只信重验——Timeout 时位可能刚被置、
        // Signaled 时位可能又被清；未命中且未到点则再睡，循环兜底。
        let _reason = berry_sched::block_current(wait_key, signals, deadline_vct);
        // G3-4b SGI 路醒来侧钩子（run 侧读点本体——醒后首段内核执行取戳；
        // Signaled 才采样，重验循环/返程尾钩执法其余路径）。
        if signals == crate::LAT_SGI_BIT
            && crate::LATSGI_ARMED.load(core::sync::atomic::Ordering::Relaxed)
        {
            crate::latsgi_wake(_reason);
        }
    };
    // G3-4b SGI 路返程尾钩子（非 0 返回 = waker 漏醒 L0 fail-loud；钉住窗内
    // 与进入点钩子成对包络整个 wait）。
    if signals == crate::LAT_SGI_BIT
        && crate::LATSGI_ARMED.load(core::sync::atomic::Ordering::Relaxed)
    {
        crate::latsgi_ret(ret);
    }
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
/// 走查；本号全程 IRQ 屏蔽（send/recv 外多数号如此）→ 校验与解析间无
/// TOCTOU）。数组内重复 handle
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
    let mut want = 0u64; // 请求位并集（睡脸掩码——b-2 修正，见下方 block_current）
    for i in 0..n {
        let mut b = [0u8; 8];
        b.copy_from_slice(&sraw[i * 8..i * 8 + 8]);
        sbuf[i] = u64::from_le_bytes(b);
        want |= sbuf[i];
    }
    let Ok(t) = caller_table(tf) else { return ERR_BAD_HANDLE as i64 };
    // 解析全部对象（Event|Channel|Process|Thread 四类型链 [`wait_get`]，同
    // wait_one；wait 不需权利位）；任一失败 = 整体拒绝、首错透传——此时尚无
    // retain，无回滚面。
    let mut objs: [Option<NonNull<ObjRef>>; WAIT_MANY_MAX] = [None; WAIT_MANY_MAX];
    for i in 0..n {
        let o = match wait_get(t, hbuf[i]) {
            Ok(o) => o,
            Err(e) => return e as i64,
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
            // W3-Q7：命中判单源 berry-evface::wait_hit（wait_one 同款——
            // 入参序最前命中循环留在调用方）。
            if berry_evface::wait_hit(current, sbuf[i]) {
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
        // 哨兵 + **请求位并集**（b-2 修正：原 `!0` 全位掩码 = 任何对象的任何
        // 新置位都唤醒本任务——任何 wait_many 收割者常驻（如 LB lb_master 挂
        // TERMINATED 收割 90s）期间：①全系统每次 channel send/recv 都虚假唤醒
        // 它一轮重验（空转风暴）；②`!0` 与一切唤醒位相交 → find_single_waiter
        // 哨兵分支恒退 slowpath → send fastpath 直切全局关停 → 依赖「唤醒即
        // 直切」确定性的 poll-0 测试（ai_ping）结构性必断。并集掩码无漏醒
        // （任一 S_i ⊆ want，命中 S_i 的唤醒必含 want 位）且快路透明（位不相
        // 交哨兵不挡 fastpath——sched 侧对齐修正）。
        let _reason =
            berry_sched::block_current(berry_sched::WAIT_ANY, want, deadline_vct);
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
/// 阻塞化 / 4-2 wait 面扩 Channel 均为改板；4-2b 加 wait_many(0x05) = 24 号；
/// G-eco 加 vmo_get_size(0x1A) = 25 号；**Plan B v0.5 加 process_exit(0x33) /
/// thread_exit(0x36) = 27 号**；**P5 加 process_get_exit_code(0x3A) = 28 号**；
/// **P6 A-3 加 vmo_get_phys(0x1B) = 29 号**；**C-4b 加 process_terminate(0x3B)
/// = 30 号**；**T-1 加 thread_start(0x35) = 31 号；A7-4 加
/// signal_send(0x86)/signal_take(0x87) = 33 号；G4-2 加
/// pager_supply_pages(0x21，v2 号激活) = 34 号；G4-4 X-3b 加
/// pager_supply_mapped(0x23，门 C 裁 A add-only——恒等选源挂派发) = 35 号；
/// X-6 刀① 加 pager_supply_mapped_table(0x24) = 36 号已实现；桌面UX刀D 加
/// power_off(0x88) = 37 号；CP-08 加 sched_util_snapshot(0x40，util 快照读
/// ——OQ-8 首环，add-only) = 38 号已实现；v1 在册 39 号
/// （唯 0x85 `berry_attest` 预留未实现）**；
/// **S6-b 加 policy_bind(0x83，v3 号提前实现最小 COMMIT 面——v1 号面之外
/// 首个真注册的 v3 号；0x84 telemetry_open 同款 v3 提前实现（klog 快照首域，见尾部 reg 局注），ABI §4 在册同名单源）**。
/// 未列号走 HAL 默认 `-ENOTSUP`。同时注册**任务退出钩子**（Plan B：Process
/// Exited 落账 + TERMINATED 唤醒 + drain/release 异步移交；T-1 分序版：Thread
/// 簿记/join 唤醒 + `live_threads` 末线程裁决——见 [`task_exit_hook`]）与
/// **REAP 后处理钩子**（P2-2：pending ht-drain 队列在 CPU0 idle/yield 排水
/// 点排空——见 [`crate::process::ht_drain_flush`]）。
pub fn install() {
    use berry_hal_aarch64::syscall::install as reg;
    // S-3c c-2c slice 2b：第三参 = SVC 入口锁类位集（`kern_lock::SVC_CLASS_*`，
    // 位值镜像 sched HELD_*——kernel main 编译期断言对齐）。分类审计 =
    // `docs/SMP-S3实施设计草案-v1.0.md` §:390 修正版落类表；0x37 已随 slice 3-1a
    // 降档 SQ_RQ 无表档（`nearest_deadline` KL-only 读面收口 SQ——SLEEPQ
    // 全触面纯 SQ 串行，该号免 KL；死锁论证见常量注）。
    use berry_hal_aarch64::kern_lock::{
        SVC_CLASS_ALL, SVC_CLASS_NONE, SVC_CLASS_RQ_TABLE, SVC_CLASS_SQ_RQ,
        SVC_CLASS_SQ_RQ_TABLE,
        SVC_CLASS_TABLE,
    };
    berry_sched::set_exit_hook(task_exit_hook);
    // P2-2 退出异步化：REAP 排水尾钩子——CPU0 idle/yield 点排空 pending
    // ht-drain 队列（退出钩子的异步半场；注册先于 sched::init/首个任务）。
    berry_sched::set_reap_post_hook(crate::process::ht_drain_flush);
    // cbox 刀 1：黑匣子排液（0x42）——静态表读零锁，但出参 copy_out 链经
    // walk_leaf 无锁走查本进程页表 = §A113 读者面（0x61/0x87 同链同判）——
    // 升 TABLE 持 KL 与页表写者互斥，NONE 实装号归零不变量（刀 1 初判 NONE
    // 为漏审，刀 2 回改）。
    reg(NR_CBOX_DRAIN, sys_cbox_drain, SVC_CLASS_TABLE);
    // —— G2 刀 7：受限态原语（0x4A/0x4B）——v1 桩（TABLE：abi_gen 常量面走查）。
    // v2 号位——abi_gen 常量层只产 v1/1.1 行（gen_abi.py defined 集
    // 同 0x83 先例），字面量挂。
    reg(0x4A, sys_restricted_enter, SVC_CLASS_TABLE);
    reg(0x4B, sys_restricted_kick, SVC_CLASS_TABLE);
    // —— G2 刀 10：Hypervisor MicroVM（0x77/0x78/0x79）——v1 桩 -ENOSYS
    // （QEMU TCG 无 EL2；真机 EL2 面归后续刀）。
    reg(0x77, sys_guest_create, SVC_CLASS_TABLE);
    reg(0x78, sys_vcpu_create, SVC_CLASS_TABLE);
    reg(0x79, sys_vcpu_run, SVC_CLASS_TABLE);
    // —— SQ+RQ+TABLE（0x2E）：wait/send 族（wake→enqueue 双结构窗 + 表）——
    reg(NR_HANDLE_CLOSE, sys_handle_close, SVC_CLASS_SQ_RQ_TABLE);
    reg(NR_HANDLE_DUPLICATE, sys_handle_duplicate, SVC_CLASS_TABLE);
    reg(NR_HANDLE_REPLACE, sys_handle_replace, SVC_CLASS_TABLE);
    reg(NR_OBJECT_WAIT_ONE, sys_object_wait_one, SVC_CLASS_SQ_RQ_TABLE);
    reg(NR_OBJECT_WAIT_MANY, sys_object_wait_many, SVC_CLASS_SQ_RQ_TABLE);
    reg(NR_OBJECT_SIGNAL, sys_object_signal, SVC_CLASS_SQ_RQ_TABLE);
    // —— NONE：clock_read（0x61）——W3-Q100 自 TABLE 降档（03 号审计机会
    // 二）：CNTVCT/CNTFRQ 硬件读零共享，rtc 注入面（EPOCH_S/BOOT_MONO_NS
    // Relaxed 原子）无锁需求；出参 copy_out 链 = §A113 读者面，S-4 4-2 起
    // 走查与 store 同一 per-AS PT 锁窗（跨核 unmap/protect 写者同窗互斥，
    // §A115 四问复审——KL 后继义务已偿清）。NONE 旧禁令（「copy 链禁落
    // NONE」）系 PT 锁前时代的 §A113 判例，判据已随源复审。
    reg(NR_CLOCK_READ, sys_clock_read, SVC_CLASS_NONE);
    // —— TABLE（0x20，仅 KL）：vmo/vmar 十一号——S-4 4-2 自 ALL（0x3E）降档，
    // 判例 §A115 四问逐项过：①读者面 = 本组号自身 copy 链（vmo_read/write、
    // vmar_map in/out 等）与他核页表写者——KL 保留即互斥不失，且本刀起 copy
    // 链另持 per-AS PT 锁（§A113 同窗铁律，弃 KL 后的互斥后继）；handler 区
    // 零 berry_sched 触达（结构位原就未用）；②位集收缩 = 取锁子集不添边
    // （真正的新边 PT 锁恒嵌 KL 窗内取放 = §A108 过渡态合法）；③copy 链跨核
    // 分窗 = 每次调用一窗（256B 分块粒度可续取）；④PT 锁不进 HELD 位图、不
    // 绑 CURRENT（零镜像面）。KL 留守实义务 = 帧分配器（纯 KL 串行）+
    // handle 表 + 堆 + 跨 AS 面（同 VMO 映两 AS 的 untrack_map 竞态）。——
    reg(NR_VMO_CREATE, sys_vmo_create, SVC_CLASS_TABLE);
    reg(NR_VMO_COMMIT, sys_vmo_commit, SVC_CLASS_TABLE);
    reg(NR_VMO_DECOMMIT, sys_vmo_decommit, SVC_CLASS_TABLE);
    reg(NR_VMO_CACHE_OP, sys_vmo_cache_op, SVC_CLASS_TABLE);
    reg(NR_VMO_READ, sys_vmo_read, SVC_CLASS_TABLE);
    reg(NR_VMO_WRITE, sys_vmo_write, SVC_CLASS_TABLE);
    reg(NR_VMO_GET_SIZE, sys_vmo_get_size, SVC_CLASS_TABLE);
    reg(NR_VMO_GET_PHYS, sys_vmo_get_phys, SVC_CLASS_TABLE);
    reg(NR_VMAR_MAP, sys_vmar_map, SVC_CLASS_TABLE);
    reg(NR_VMAR_UNMAP, sys_vmar_unmap, SVC_CLASS_TABLE);
    reg(NR_VMAR_PROTECT, sys_vmar_protect, SVC_CLASS_TABLE);
    reg(NR_VMAR_ALLOCATE, sys_vmar_allocate, SVC_CLASS_TABLE);
    // —— SQ+RQ+TABLE（0x2E）：0x21 pager_supply_pages（G4-2）——阻塞收货
    //（block_current SQ 面）+ 唤 faulter（wake→enqueue RQ 面）+ 双对象
    // 表门控；供给窗 = vmar 族同构 KL 面（pt_lock 嵌 KL 正序）。锁类对齐
    // wait_one（同为「表 + 可睡」handler）。0x23（X-3b）同形挂同类；0x24
    //（X-6 刀① 表批）多窗一次收货——表门控 ×N、供给/唤醒链同构，同类。
    reg(NR_PAGER_SUPPLY_PAGES, sys_pager_supply_pages, SVC_CLASS_SQ_RQ_TABLE);
    reg(NR_PAGER_SUPPLY_MAPPED, sys_pager_supply_mapped, SVC_CLASS_SQ_RQ_TABLE);
    reg(NR_PAGER_SUPPLY_MAPPED_TABLE, sys_pager_supply_mapped_table, SVC_CLASS_SQ_RQ_TABLE);
    reg(NR_PROCESS_CREATE, sys_process_create, SVC_CLASS_ALL);
    // —— RQ+TABLE（0x2C）：start 双号（enqueue 双核升序 + 表）——
    reg(NR_PROCESS_START, sys_process_start, SVC_CLASS_RQ_TABLE);
    // —— ALL：真退出（`-> !`，舞蹈收口）——
    reg(NR_PROCESS_EXIT, sys_process_exit, SVC_CLASS_ALL);
    reg(NR_THREAD_CREATE, sys_thread_create, SVC_CLASS_ALL);
    reg(NR_THREAD_START, sys_thread_start, SVC_CLASS_RQ_TABLE);
    reg(NR_THREAD_EXIT, sys_thread_exit, SVC_CLASS_ALL);
    // —— SQ+RQ（0x0E，无表无 KL）：thread_sleep 单号——slice 3-1a 降档
    //（block_current SQ+RQ 双结构面纯睡眠，表触零；入口不取 KL，舞蹈
    // 放/取步按 0x0E 位集不碰 KL——锁序论证见 SVC_CLASS_SQ_RQ 常量注）。
    reg(NR_THREAD_SLEEP, sys_thread_sleep, SVC_CLASS_SQ_RQ);
    // —— TABLE：join 读面（表 + state 平读）——
    reg(NR_PROCESS_GET_EXIT_CODE, sys_process_get_exit_code, SVC_CLASS_TABLE);
    // —— ALL：terminate kill 链（同 fault 面）——
    reg(NR_PROCESS_TERMINATE, sys_process_terminate, SVC_CLASS_ALL);
    // —— TABLE：suspend（表门控 + freeze 扫描，不触 SQ/RQ 结构面）——
    reg(NR_PROCESS_SUSPEND, sys_process_suspend, SVC_CLASS_TABLE);
    // —— TABLE：sched attr 双号（表门控 + Thread 字段写/读）——
    reg(NR_SCHED_SETATTR, sys_sched_setattr, SVC_CLASS_TABLE);
    reg(NR_SCHED_GETATTR, sys_sched_getattr, SVC_CLASS_TABLE);
    // —— RQ+TABLE：yield（pick/requeue + 切换舞蹈；表位保守对称）——
    reg(NR_THREAD_YIELD, sys_thread_yield, SVC_CLASS_RQ_TABLE);
    // —— NONE：CP-08 util 快照读（0x40——W3-Q100 自 TABLE 降档）：EWMA 槽
    //    Relaxed 原子读零共享面（统计信号无序角色）；copy_out 链 = §A113
    //    读者面，S-4 4-2 起 per-AS PT 锁同窗——§A115 四问复审落 NONE
    //    （同 0x61/0x87）。
    reg(NR_SCHED_UTIL_SNAPSHOT, sys_sched_util_snapshot, SVC_CLASS_NONE);
    reg(NR_CHANNEL_CREATE, sys_channel_create, SVC_CLASS_ALL);
    // —— SQ+RQ+TABLE：send/recv（fastpath sq_phys 外提已备，slice 2a）——
    reg(NR_CHANNEL_SEND, sys_channel_send, SVC_CLASS_SQ_RQ_TABLE);
    reg(NR_CHANNEL_RECV, sys_channel_recv, SVC_CLASS_SQ_RQ_TABLE);
    // —— SQ+RQ+TABLE（0x2E）：W3-Q97 批量 recv（机会五）——锁类与单发
    // recv 完全同面（同 caller_table/get_raw/PT 窗/wake_matching），同类
    // 挂法；批量窗更长（k ≤ 16）但锁集零新增（§A126 探峰合法同 0x2E）。
    reg(NR_CHANNEL_RECV_N, sys_channel_recv_n, SVC_CLASS_SQ_RQ_TABLE);
    reg(NR_EVENT_CREATE, sys_event_create, SVC_CLASS_ALL);
    // A7-4 段②：信号面两号（0x86/0x87——协同式用户态投递，内核只管跨进程
    // 位置位与取走；handler 本体见 sys_process_terminate 后）。0x86 = 表查 +
    // 目标 pending 原子（不唤醒目标）——TABLE 在册；0x87 = current pending
    // 自面原子，出参 copy_out 链 = §A113 读者面——S-4 4-2 起 per-AS PT 锁
    // 同窗，W3-Q100 §A115 四问复审降 NONE（同 0x61）。
    reg(NR_SIGNAL_SEND, sys_signal_send, SVC_CLASS_TABLE);
    reg(NR_SIGNAL_TAKE, sys_signal_take, SVC_CLASS_NONE);
    // —— TABLE：桌面UX刀D 关机最小闭环（0x88 power_off）。零参数零出参零
    //    表触达；TABLE（仅 KL）= 保守档：UART 诊断行 + PSCI 缓存原子读在
    //    SVC 屏蔽窗内自安全（NONE 落类须过 §A115 四问后继审计——0x88 触
    //    UART/PSCI 诊断面未过审，保守留 TABLE）；成功路不归（整机下电），
    //    失败路正常返还解锁。
    reg(NR_POWER_OFF, sys_power_off, SVC_CLASS_TABLE);
    // —— TABLE：S6-b 策略注入最小 COMMIT 面。0x83 policy_bind 在 abi.toml §4
    // 在册但 stage=v3——abi_gen 常量层只产 v1/1.1 行（gen_abi.py defined 集），
    // NR_POLICY_BIND 不存在，故字面量挂派发（v3 常量层落地后换常量名）。
    // TABLE 理由：copy_from_user 整表拷入走查本进程页表（§A113 读者面，同
    // 0x87 signal_take）+ policy SNAPS 三槽轮转区是全局单写者共享面（KL 串
    // 行跨核 SVC 写者；读者 apply/weight_of 在 RQ 屏蔽决策窗，无共同写窗）。
    reg(0x83, sys_policy_bind, SVC_CLASS_TABLE);
    // —— TABLE：阶段五第 16 步 0x84 telemetry_open 翻真（klog 快照首域）。
    // 同 0x83：v3 在册号、abi_gen 常量层不产 NR_TELEMETRY_OPEN，字面量挂
    // 派发（v3 常量层落地后换常量名）。TABLE 理由：copy_out 链走查调用方
    // 页表（§A113 读者面，同 0x61/0x87）+ 快照暂存 stage 是 KL 串行共享面
    //（见 sys_telemetry_open 注）。
    reg(0x84, sys_telemetry_open, SVC_CLASS_TABLE);
}
