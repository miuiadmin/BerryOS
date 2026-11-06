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
/// root Vmar 的 L1 表树帧在 v1 **不回收**（Box 回收链已实：handle 三释放
/// 点 `drop(Box::from_raw)` + `Vmar::Drop` 注销 asid 反查槽/放 demand 钉
/// 〔63b7abc〕；但表树帧/ASID 号留 v2——vmar.rs Drop 自述「v1 不回收表树
/// 帧」模块政策）——无帽则单 EL0 循环 create 泄漏 L1 表树帧，可把全系统
/// 耗到 -ENOMEM 停摆（no-panic 守住不死、但一切服务停摆）；
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
    /// 活线程计数（T-1 多线程）：增点三（new 置 0 / process_start 成功尾置 1
    /// ——首线程 / thread_start 成功尾 +=1）；减点唯一 = 任务退出钩子（一切
    /// 退出路径汇经钩子）。减到 0 = 末线程 → Process 收口（mark_exited + 码 +
    /// TERMINATED + drain）；**非末线程绝不收口**（K1 教训线程版——共享表上
    /// 活兄弟的 handle 不能被先退线程扬掉）。plain usize——单核 IRQ 屏蔽路径
    /// 专用无原子（同 started/state）。
    pub live_threads: usize,
    /// 退出码（v1 面上不可读——get_info 落 v2；fault 杀路径置 0（诚实注释：
    /// 杀因观察面缺位，非语义 0）。
    pub exit_code: i32,
    /// 待投递信号位图（A7-4 段②，0x86/0x87）：u64 一位一号（信号 n = 位
    /// n-1，覆盖 1..=64）；`signal_send` 置位（幂等 = POSIX 标准信号合并）、
    /// `signal_take` 原子取走（swap(0)）。plain u64——单核 IRQ 屏蔽路径专用
    /// S-3c C21 起原子（fetch_or 置 / swap(0) 取走 + 失败 fetch_or 回滚——
    /// 0x87 syscall 侧协议）；序归锁窗（Relaxed）。投递执行全在用户态 libc
    /// （协同式，A7-4 §2 方案 B——内核不存 handler 表、不唤醒目标）。
    pub pending_signals: core::sync::atomic::AtomicU64,
    /// 1.1/S1：本进程线程的调度属性默认值（Thread 覆盖前的 base）。
    pub sched_attr: SchedAttr,
    /// 1.1/S3-b：冻结嵌套深度（`process_suspend` +1 / `SuspendTok::drop` -1）。
    /// > 0 → 冻结中；归 0 → `thaw_by_pinned` 解冻。多 token 同进程：最后一个
    /// close 才解冻。上限 8（超限 -EBUSY）。
    pub freeze_depth: u32,
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
            live_threads: 0,
            exit_code: 0,
            pending_signals: core::sync::atomic::AtomicU64::new(0),
            sched_attr: SchedAttr::default(),
            freeze_depth: 0,
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

/// 1.1/S1：调度属性声明（ABI `berry_sched_attr` 的内核侧浓缩——布局字段
/// 见 `调点调度-v1.1.md` §7.2；`flags=0` = 清除覆盖/回继承）。
///
/// `Process` 持有的是该进程线程的默认值；`Thread` 持有的是覆盖值。S2/S5
/// 消费侧（weight/intent/policy/deadline）从 **effective**（默认 ∪ 覆盖）
/// 读取；S1 只存不执法。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SchedAttr {
    /// 生效字段位（bit0 weight / bit1 intent / bit2 policy / bit3 freeze_clock /
    /// bit4 FREEZE_READY / bit5 reserve / bit6 deadline / bit7 budget）。
    pub flags: u16,
    pub weight: u32,
    pub policy: u8,
    pub intent: u8,
    pub freeze_clock: u8,
    pub min_ns: u64,
    pub max_ns: u64,
    pub deadline_ns: u64,
    pub budget_ns: u64,
    pub period_ns: u64,
}

impl SchedAttr {
    /// Process 默认 ∪ Thread 覆盖：位置位取覆盖值，未置位继承默认；
    /// FREEZE_READY 只可能来自 Thread 覆盖（Process 声明该位非法）。
    pub fn merge(base: Self, over: Self) -> Self {
        let mut out = base;
        out.flags |= over.flags;
        if over.flags & SA_FLAG_WEIGHT != 0 { out.weight = over.weight; }
        if over.flags & SA_FLAG_INTENT != 0 { out.intent = over.intent; }
        if over.flags & SA_FLAG_POLICY != 0 { out.policy = over.policy; }
        if over.flags & SA_FLAG_FREEZE_CLOCK != 0 { out.freeze_clock = over.freeze_clock; }
        if over.flags & SA_FLAG_RESERVE != 0 {
            out.min_ns = over.min_ns;
            out.max_ns = over.max_ns;
        }
        if over.flags & SA_FLAG_DEADLINE != 0 { out.deadline_ns = over.deadline_ns; }
        if over.flags & SA_FLAG_BUDGET != 0 {
            out.budget_ns = over.budget_ns;
            out.period_ns = over.period_ns;
        }
        if over.flags & SA_FLAG_FREEZE_READY == 0 {
            out.flags &= !SA_FLAG_FREEZE_READY;
        }
        out
    }
}

/// `SchedAttr.flags` 位（ABI §7.2；与 berry-sys 侧同名位语义一致）。
pub const SA_FLAG_WEIGHT: u16 = 1 << 0;
pub const SA_FLAG_INTENT: u16 = 1 << 1;
pub const SA_FLAG_POLICY: u16 = 1 << 2;
pub const SA_FLAG_FREEZE_CLOCK: u16 = 1 << 3;
pub const SA_FLAG_FREEZE_READY: u16 = 1 << 4;
pub const SA_FLAG_RESERVE: u16 = 1 << 5;
pub const SA_FLAG_DEADLINE: u16 = 1 << 6;
pub const SA_FLAG_BUDGET: u16 = 1 << 7;

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
    /// 1.1/S1：`sched_setattr` 写入的线程覆盖属性（flags=0 = 回 Process 默认）。
    pub sched_attr: SchedAttr,
    /// ——`signal_state` 映射为 Thread `SIGNAL_TERMINATED`（join 等待者由
    /// `object_wait_one(th, TERMINATED, …)` 收割，裁决二：内核侧 join 的
    /// 死透保证）。与 started 并存两 bool 不上 enum——最小扰动 process_start
    /// 既有消费点。
    pub exited: bool,
}

impl Thread {
    /// 建未启动线程（入口/栈先置 0，`process_start` 填）。构造即钉住 `proc_ref`。
    ///
    /// # Safety
    /// `proc_ref` 须为在表、类型 Process 的活对象。
    pub unsafe fn new(proc_ref: NonNull<ObjRef>) -> Self {
        (*proc_ref.as_ptr()).retain();
        Self { proc: proc_ref, entry: 0, sp: 0, started: false, exited: false,
               sched_attr: SchedAttr::default() }
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
