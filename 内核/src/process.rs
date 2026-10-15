//! Process / Thread 最小内核对象（3-3；能力模型 §3）。
//!
//! v1 最小面：Process = **root Vmar 的壳**（地址空间 + ASID 的拥有者，自身不
//! 分配任务）；Thread = 挂在某 Process 下的可调度执行流**描述**（入口/栈在
//! `process_start` 给出——Zircon 同形的首线程启动路径），`started` 前不可运行。
//!
//! **对象间钉住（防悬空）**：Thread 存所属 Process 的 `NonNull<ObjRef>`、
//! Process 存 root Vmar 的 `NonNull<ObjRef>`，创建时 `retain()`、`Drop` 时
//! `release()`——即使持有者关光 handle，被引用对象仍存活到释放方 drop。
//! `process_start` 成功后再 `retain(Process)` 一次替**运行中的任务**钉住
//! （v1 任务不退出，永不释放——与 `UserProcess` 泄漏持有同政策）。
//!
//! v1 裁剪：无 Job（process_create 的 job 参数恒 0——创建权未设防的已知豁口，
//! Job 落地 v2 后收紧）；无 per-process handle 表（全局单表过渡，见
//! `syscall.rs` 并发说明）；进程退出/杀死路径未实现（v1 任务不退出）。

use crate::handle::ObjRef;
use core::ptr::NonNull;

/// 进程 = 地址空间容器：root Vmar（页表树 + ASID）+ 供调度器用的表根快照。
pub struct Process {
    /// 地址空间根（ObjRef 钉住——父进程的 vmar handle 关光后子空间仍存活）。
    vmar: NonNull<ObjRef>,
    /// L1 表 PA 快照（spawn 用；与 `vmar` 指向的 Vmar 一致）。
    pub ttbr0_pa: usize,
    /// ASID 快照。
    pub asid: u16,
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
        Self { vmar: vmar_ref, ttbr0_pa, asid }
    }

}

impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: 构造时 retain 过；单核顺序（对象仅在 IRQ 屏蔽路径销毁）。
        unsafe { ObjRef::release(self.vmar) };
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
