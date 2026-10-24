//! Process / Thread 最小内核对象（3-3；能力模型 §3）。
//!
//! v1 最小面：Process = **root Vmar 的壳**（地址空间 + ASID 的拥有者，自身不
//! 分配任务）+ **自己的 handle 表**（3-4a：per-process 表——EL0 syscall 经
//! 任务标签派发到所属进程的表；bootstrap/IPC 的 handle 迁移以此为载体）；
//! Thread = 挂在某 Process 下的可调度执行流**描述**（入口/栈在
//! `process_start` 给出——Zircon 同形的首线程启动路径），`started` 前不可运行。
//!
//! **对象间钉住（防悬空）**：Thread 存所属 Process 的 `NonNull<ObjRef>`、
//! Process 存 root Vmar 的 `NonNull<ObjRef>`，创建时 `retain()`、`Drop` 时
//! `release()`——即使持有者关光 handle，被引用对象仍存活到释放方 drop。
//! `process_start` 成功后再 `retain(Process)` 一次替**运行中的任务**钉住
//! （Plan B：Task `pinned` 字段持有——任务退出钩子 drain 子表后 release）。
//!
//! v1 裁剪：无 Job（process_create 的 job 参数恒 0——创建权未设防的已知豁口，
//! Job 落地 v2 后收紧）；boot 期手搭的用户进程（`UserProcess`）只有裸
//! `HandleTable`（leak 常驻）无 Process 对象——它们不经 syscall 创建。
//! **退出路径（Plan B v0.5）**：`process_exit`(0x33)/`thread_exit`(0x36) 经
//! sched 退出钩子把 `Process` 置 `Exited`（exit_code 落账）并唤醒
//! `SIGNAL_TERMINATED` 等待者（wait 面 = 内核派生低位，同 Channel READABLE
//! 口径）；任务体/内核栈由 REAPQ 回收；表随**最后被 drain 的引用**释放
//! （Process Drop 链：release root Vmar pin）。

use crate::handle::{HandleTable, ObjRef};
use core::ptr::NonNull;
use core::sync::atomic::{AtomicUsize, Ordering};

/// 活进程帽（`sys_process_create` 门控）：创建权未设防（`job` 恒 0）且
/// root Vmar 的 L1 帧 + Process/Vmar Box 在 v1 **不回收**（Plan B 回收链止于
/// 映射帧与任务体；Vmar 无 Drop、ASID 释放留 v2）——无帽则单 EL0 循环
/// create 可把全系统耗到 -ENOMEM 停摆（no-panic 守住不死、但一切服务停摆）；
/// 计数把无界封为有界——`vmo_create` 的 `VMO_MAX_SIZE` 帽同哲学。
/// per-caller 配额随 v2 Job 收口（届时按调用方分账）。
pub const PROCESS_MAX: usize = 64;

/// 活 Process 对象计数（`Process::new` 增、`Drop` 减）。单核顺序（对象仅在
/// IRQ 屏蔽路径创建/销毁）——Relaxed 足够，用原子只为免 unsafe static mut。
static PROCESS_LIVE: AtomicUsize = AtomicUsize::new(0);

/// 当前活进程数（帽门控与观测面）。
pub fn process_live() -> usize {
    PROCESS_LIVE.load(Ordering::Relaxed)
}

/// 进程生命周期（Plan B v0.5）：Alive → Exited 单向；Exited 后 wait 面
/// `SIGNAL_TERMINATED` 置位、句柄仍可 wait/close（对象钉住纪律保活）。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProcState {
    Alive,
    Exited,
}

/// 进程 = 地址空间容器：root Vmar（页表树 + ASID）+ 供调度器用的表根快照。
pub struct Process {
    /// 地址空间根（ObjRef 钉住——父进程的 vmar handle 关光后子空间仍存活）。
    vmar: NonNull<ObjRef>,
    /// L1 表 PA 快照（spawn 用；与 `vmar` 指向的 Vmar 一致）。
    pub ttbr0_pa: usize,
    /// ASID 快照。
    pub asid: u16,
    /// **本进程的 handle 表**（3-4a per-process 拆分；出生为空——一切能力由
    /// 父进程经 `process_start` bootstrap / channel 迁移授入，能力模型 §4/§5）。
    pub ht: HandleTable,
    /// 生命周期（Plan B：退出钩子置 Exited + 唤醒 TERMINATED 等待者）。
    pub state: ProcState,
    /// 已启动（对抗审查修，Plan B）：`process_start` **每进程至多一次**——
    /// v1 单线程契约的内核执法（Zircon `zx_process_start` 同形 -EBADSTATE）。
    /// 只查 per-Thread `started` 时 thread_create×2 + start×2 可造共享同一
    /// `ht` 的双任务：首任务退出即 drain 活兄弟的表 + 提前置 TERMINATED，
    /// 父侧 reap 把地址空间从运行中任务脚下抽走。
    pub started: bool,
    /// 退出码（v1 面上不可读——get_info 落 v2；fault 杀路径置 0（诚实注释：
    /// 杀因观察面缺位，非语义 0）。
    pub exit_code: i32,
}

impl Process {
    /// 建空进程（无任何映射；代码/栈由父进程经返回的 vmar handle 映入）。
    /// `vmar_ref` 须为刚入表的 root Vmar 对象（本构造即钉住它）。
    ///
    /// # Safety
    /// `vmar_ref` 须为在表、类型 Vmar 的活对象；调用后本 Process 存活期间
    /// 其不可被销毁（已 retain 保证）。
    pub unsafe fn new(vmar_ref: NonNull<ObjRef>, ttbr0_pa: usize, asid: u16) -> Self {
        (*vmar_ref.as_ptr()).retain();
        PROCESS_LIVE.fetch_add(1, Ordering::Relaxed);
        Self {
            vmar: vmar_ref,
            ttbr0_pa,
            asid,
            ht: HandleTable::new(),
            state: ProcState::Alive,
            started: false,
            exit_code: 0,
        }
    }

    /// 置 Exited（幂等：重复退出/双重标记不叠加）。返回是否**首次**——首次
    /// 调用方随后负责落 `exit_code` + `wake_matching(TERMINATED)`；非首次
    /// （fault 杀先到、exit 后到等）保首因。
    pub fn mark_exited(&mut self) -> bool {
        if self.state == ProcState::Exited {
            return false;
        }
        self.state = ProcState::Exited;
        true
    }

}

impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: 构造时 retain 过；单核顺序（对象仅在 IRQ 屏蔽路径销毁）。
        unsafe { ObjRef::release(self.vmar) };
        PROCESS_LIVE.fetch_sub(1, Ordering::Relaxed);
    }
}

/// 线程 = 挂在某 Process 下的执行流描述。`process_start` 前不进调度器。
pub struct Thread {
    /// 所属 Process（ObjRef 钉住——父进程的 process handle 关光后仍可 start）。
    proc: NonNull<ObjRef>,
    /// 入口 VA（process_start 时校验使用）。
    pub entry: usize,
    /// 栈顶 VA（sp0，16 对齐由 process_start 校验）。
    pub sp: usize,
    /// 已启动（true = 已 spawn，再 start → -EBADSTATE）。
    pub started: bool,
}

impl Thread {
    /// 建未启动线程（入口/栈先置 0，`process_start` 填）。构造即钉住 `proc_ref`。
    ///
    /// # Safety
    /// `proc_ref` 须为在表、类型 Process 的活对象。
    pub unsafe fn new(proc_ref: NonNull<ObjRef>) -> Self {
        (*proc_ref.as_ptr()).retain();
        Self { proc: proc_ref, entry: 0, sp: 0, started: false }
    }

    /// 所属 Process 对象标识（process_start 校验 thread↔process 归属）。
    pub fn owner(&self) -> NonNull<ObjRef> {
        self.proc
    }
}

impl Drop for Thread {
    fn drop(&mut self) {
        // SAFETY: 构造时 retain 过；单核顺序。
        unsafe { ObjRef::release(self.proc) };
    }
}
