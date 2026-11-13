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
//! **P2-2 退出异步化**：末线程钩子不再同步 `ht.drain()`（O(表) 清理 + VMO
//! Drop 帧回收链嵌在 IRQ 屏蔽的切换窗内 = 调度延迟尖峰）——钉住引用移交
//! 本模块 HT_DRAINQ（O(1)），drain 由 REAP 排水点异步完成（见该块头注）。

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

// ---- per-Process 对象配额帽（Medium 安全缺口收口：全局堆无配额）----
// 威胁面：单 EL0 进程循环 create 即可耗尽**共享堆**（slab 后备 = 全局帧池，
// 堆枯 = 全机枯）——`vmo_create(128MiB)` 每个仅记账就吃 ≈576KiB（pages +
// mapcnt 两 Vec 预储，见 `PageLedger::new_anon`）、`channel_create` 每端预储
// CHAN_DEPTH×Message ≈ 2.7KB、`thread_start` 每线 16KiB 内核栈 + TCB——其他
// 进程随后一切分配恒 -ENOMEM 停摆。计数把无界封为有界（`PROCESS_MAX` /
// `VMO_MAX_SIZE` 同哲学）；per-caller 分账随 v2 Job 收口。
//
// 记账面 = **调用方 handle 表在册数**（创建面 syscall 经 `count_type` 表扫描
// 执法——S5.12 SuspendToken 帽同款），**非** Process sidecar 计数器：sidecar
// 在 handle 跨表迁移（`channel_send` take / recv `insert_ref` / bootstrap
// 迁移）下增减结构性失配——close 面拿不到记账主（表无属主回指）、对象可
// 活过被记账的 Process（fetch_sub = UAF）；扫描派生随 close/take/drain/
// 迁移天然配平（`HandleTable::pager_load` 同口径论证）。已启动线程帽 =
// `live_threads`（增点三/减点唯一已精确维护，`thread_start` 执法）。
// EL1 内核侧表（ktest/常驻服务）同帽——合法用量远低于帽（smoke 实测面）。
//
/// 单进程（单表）在册 VMO handle 上限（含 dup 别名——按槽计）。
pub const MAX_VMO_PER_PROC: usize = 256;
/// 单进程（单表）在册 channel **端点**上限（`channel_create` 一次入 2 = 一对）。
pub const MAX_CHANNEL_PER_PROC: usize = 64;
/// 单进程**已启动**线程上限（`live_threads` 口径——首线程 + thread_start 造的
/// 非首线程；未启动 Thread 描述 ≈ 百字节级 box，被表满（65535 槽）结构性
/// 封顶，不另设帽）。
pub const MAX_THREADS_PER_PROC: usize = 16;

/// P3-3：单表（单父）在册 **Process** handle 上限 = per-parent spawn 配额
/// ——防进程炸弹（单 EL0 循环 `process_create` 占满全局 `PROCESS_MAX` 表、
/// 阻止其他服务拉起；全局 64 帽是**系统级**总账，本帽是**单父**分账）。
/// 记账面同族前帽：`count_type(Process)` 表扫描（`sys_process_create` 执法
/// ——非 Process sidecar 计数器，否决理由见上块〔迁移失配/UAF/EL1 无宿主〕
/// ——故**无退出递减面**：扫描派生随 close/take/drain/迁移天然配平）。
/// 「子」的口径 = 本表在册 Process handle：含自进程柄（berry-exec env 块
/// dup 入表）与经 channel 迁入的他人进程柄——同 VMO/Channel 帽的在册语义，
/// 换取零新增记账状态。svcmgr 正当多子：诊断实测稳态仅 ≈5-7（自柄 1 +
/// echo×2/talker 泄漏柄 3 + 瞬态；app 批 9 子串行 spawn→reap 即关柄），
/// dpgf 满配 15 子并发（dpgsoak 变体面）叠加后峰值 ≈20-22——16 会在波中
/// 途拒造，帽定 24（24 < 64 全局帽仍留 2.6× 反垄断余量：单父至多吃系统
/// 进程表的 3/8）。
pub const MAX_CHILDREN_PER_PROC: usize = 24;

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
    /// 1.1/S3-b：进程级冻结嵌套深度（最后一个 token close 才 thaw）。
    pub freeze_depth: u32,
    /// 1.1/S3-b：当前持有单线程冻结 token 的 Thread 数（非 token 数）。
    /// 用于拒绝「进程全冻 × 单线程冻」重叠：HELD_FROZEN 目前只有一位，
    /// 两种粒度交叉 close 会提前解冻。
    pub thread_freezes: u32,
    /// v1.2/S6-e：进程 band 成员号（0 = 未编带——扁平 fair 默认；1..=4 =
    /// class_fair ×4 带实例〔`调点调度-v1.2.md` §7〕）。**内核侧记账字段**
    /// ——ABI `sched_attr` 不加 band 字段（§11「intent 即带键」；本字段与
    /// ABI 零关联，35+ 号零漂移）。消费面 = `sched::band` 预算包络：spawn
    /// 时随任务带键（intent 档）映射，quota 0 全关 = 与接口缺席逐位等价。
    /// plain u8——单核 IRQ 屏蔽路径专用（同 freeze_depth）。
    #[allow(dead_code)] // S6-e band 执法刀消费（spawn 映射点 + sched::band）
    pub band_id: u8,
    /// AM-1.5 wake-pending：冻结期间有 send 命中本进程端点（Q1 裁决「唤醒
    /// 即解冻」的内核机制面）。**一次语义**——首命中置位 + 通知 SuspendTok
    /// 持有者，后续 send 见位不再重复通知（防风暴；S5.12 token 上限 64 已
    /// 是 DoS 帽）；**全解冻**（freeze_depth 归零的最后一枚 token close）才
    /// 清零——嵌套中间层不解冻不清。消费 = `SuspendTok` wait 面派生信号位
    /// `SIGNAL_WAKE_PENDING`（syscall.rs `signal_state`）。plain bool——单核
    /// IRQ 屏蔽路径专用（同 freeze_depth）。
    pub wake_pending: bool,
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
            thread_freezes: 0,
            band_id: 0,
            wake_pending: false,
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

// ---- P2-2 退出异步化：pending ht-drain 队列（REAPQ 的 Process-表姊妹面）----
//
// 问题：末线程退出钩子（IRQ 屏蔽 + KL 在身的上下文切换窗内）同步
// `ht.drain()`——O(句柄表) 清理（逐槽 release + 归零 drop Box → VMO Drop
// 还帧 / Vmar Drop 注销 demand / ChannelEnd Drop 放互钉）阻塞下一个任务
// 上 CPU；句柄表大（per-Process 型帽：VMO 256 + channel 端点 64 + …，页
// 级 VMO 的 Drop 链还帧可达千帧）时是调度延迟尖峰，churn 负载（rootfs 相
// spawn/退出风暴）放大。
//
// 形态：钩子只做 O(1) 簿记（mark_exited + 唤醒 join/TERMINATED 等待者 +
// Process 运行钉住引用**移交**本队列）；drain 移到 REAP 排水点（sched
// `reap_drain` 尾钩子 → [`ht_drain_flush`]——CPU0 idle/yield 语境，永不在
// 退出者自身上下文，与 Task Box 回收同一批排水点）异步完成。
//
// 安全不变量：
// - **drain 先于 Process Drop（防 ht UAF）**：移交的钉住引用（process_start
//   的 retain）由队列独占持有——Process 在 flush 侧 drain→release 序完成前
//   结构性存活；release 仍恒在 drain 后（旧钩子语义序原样，只是迟到）。
// - **唤醒序**：join（thread_key TERMINATED）与 Process TERMINATED 唤醒仍
//   在钩子内同步（mark_exited 之后——不变）；drain 的 Drop 链本就不唤醒
//   任何人（`Vmo::Drop` 只还帧、`Vmar::Drop` 只注销 demand、
//   `ChannelEnd::Drop` 只放互钉引用——全无 wake 面），drain 迟到不改变
//   任何等待者的可见序。
// - **互斥（SMP 参与态）**：push 点（退出钩子）恒 KL 在身（SVC 退出链 /
//   EL0 fault kill 链 SVC_CLASS_ALL / kill_all 的 SVC 主体）；flush 点经
//   [`ht_drain_flush`] 的 `kl_held` 门取 KL——队列访问与 Drop 链全在 KL
//   窗内，跨核 push/flush 由 KL 串行，无需第二把锁（REAPQ_LOCK 的等价物
//   由既有 KL 承担；flush 不持任何结构锁取 KL，无锁序倒置面）。
// - **VMO Drop 链 / demand 注销**：drain 在 KL 窗内执行（同同步时代语境
//   ——S-4 D23「分配器锁内」纪律维持；`Vmar::Drop` 的 kl_held 门自然退化
//   为已持跳过）。
// - **观测滞后（有界）**：PROCESS_LIVE 减点与帧回收水位随 flush 滞后到下
//   一排水点。收割链必经的 `wait TERMINATED` 阻塞必然途经 idle——churn
//   循环每轮至少一次排水，滞后上界 = 一个收割周期；`PROCESS_MAX` 帽在最坏
//   瞬态多计已退出未排水进程（帽余量 ≥2.6×，见 [`MAX_CHILDREN_PER_PROC`]
//   注；soak pages 断言以 5 周期潜伏首拍为基线，本滞后 µs-ms 级不触面）。
// - **溢出面（宁慢勿错）**：cap 256（REAPQ 同容量哲学——SMP churn 实测峰
//   值 33-60 的 4-8× 余量，8B/槽 = 2KiB BSS）；满则退回**同步 drain**
//   （旧路径原样：钩子内 drain + release——延迟尖峰但语义零漂，绝不泄漏
//   钉住引用）+ loud ≤4 次（`reapq overflow` 同款防风暴）。
static mut HT_DRAINQ: [usize; HT_DRAINQ_CAP] = [0; HT_DRAINQ_CAP];
static mut HT_DRAINQ_LEN: usize = 0;
static mut HT_DRAINQ_WARNED: usize = 0;
const HT_DRAINQ_CAP: usize = 256;

/// 单次 flush 的 drain 条数帽：摊平 KL 持有窗（SMP 下对核等锁面）——idle
/// 环逐拍推进，32 条/拍在 churn 峰值（33-60）下 ≤2 拍清空。
const HT_DRAINQ_CHUNK: usize = 32;

/// drain 一个钉住的 Process 表（drain → release 序——flush 异步路与溢出
/// 同步回退共用，语义 = 旧退出钩子尾两步原样）。
///
/// # Safety
/// `pinned` 须为调用方独占持有的有效 Process ObjRef 引用（本函数消费它，
/// 返回后不得再触）；IRQ 屏蔽 + KL 窗内调用（Drop 链纪律同
/// [`HandleTable::drain`]：对象释放窗口内不可被切走）。
unsafe fn drain_pinned_process(pinned: NonNull<ObjRef>) {
    unsafe {
        let r = &mut *pinned.as_ptr();
        if let Some(proc) = r.obj.as_process_mut() {
            proc.ht.drain();
        }
        // 类型不符 = 不可达防御面（移交面恒 Process ObjRef）——引用仍须放
        // （本函数契约 = 消费），不放即泄漏。
        // SAFETY: 调用方契约（独占引用，此后不再使用）。
        ObjRef::release(pinned);
    }
}

/// 退出钩子侧入队（O(1)）：`pinned` = Process 运行钉住引用（process_start
/// 的 retain）——**所有权移交队列**（钩子不再 release；flush 侧 drain 后
/// release 成对）。满队 = 同步回退（[`drain_pinned_process`]——旧路径原样）
/// + loud。
///
/// 调用语境：任务退出钩子（IRQ 屏蔽 + KL 在身——SVC 退出链 / EL0 fault
/// kill 链 / kill_all 的 SVC 主体，三路临界区同构）；队列互斥全靠 KL（见
/// 块头注「互斥」）。
pub(crate) unsafe fn defer_ht_drain(pinned: NonNull<ObjRef>) {
    unsafe {
        if HT_DRAINQ_LEN < HT_DRAINQ_CAP {
            HT_DRAINQ[HT_DRAINQ_LEN] = pinned.as_ptr() as usize;
            HT_DRAINQ_LEN += 1;
            return;
        }
        // 溢出：宁慢勿错——同步 drain+release（语义与异步到达点等价）。
        // SAFETY: 调用方移交的独占引用；钩子语境 IRQ 屏蔽 + KL 在身。
        drain_pinned_process(pinned);
        if HT_DRAINQ_WARNED < 4 {
            HT_DRAINQ_WARNED += 1;
            berry_hal_aarch64::uart_puts(b"ht_drainq overflow (sync fallback)\n");
        }
    }
}

/// REAP 后处理（P2-2 异步半场）：排空 pending 队列——sched `reap_drain` 尾
/// 钩子调用（CPU0 idle/yield 语境，IRQ 屏蔽；KL 或在身〔SVC 0x3F 续行窗〕
/// 或不在——`kl_held` 门收口，`Vmar::Drop` 同款）。逐项 drain → release，
/// 每拍 ≤[`HT_DRAINQ_CHUNK`] 条（摊平 KL 窗）；LIFO 弹出（O(1)，条目间无序
/// 依赖——各自独立 drain）。
pub fn ht_drain_flush() {
    // KL 门：push 侧恒 KL 在身，队列互斥 + Drop 链（分配器/表/demand 面）
    // 都要求 KL 窗——不在身（idle 臂）物理取，在身（yield 续行臂）退化跳过。
    // 取锁时不持任何结构锁（reap_drain 已放 REAPQ；idle/yield 点无 RQ/SQ），
    // 无锁序倒置；持锁者零阻塞零切换（IRQ 屏蔽窗 + 纯释放零分配）。
    let held = berry_hal_aarch64::kern_lock::kl_held();
    if !held {
        berry_hal_aarch64::kern_lock::kern_acquire();
    }
    unsafe {
        let mut n = 0;
        while HT_DRAINQ_LEN > 0 && n < HT_DRAINQ_CHUNK {
            HT_DRAINQ_LEN -= 1;
            let p = HT_DRAINQ[HT_DRAINQ_LEN];
            HT_DRAINQ[HT_DRAINQ_LEN] = 0;
            if p != 0 {
                // SAFETY: 退出钩子移交的独占 Process 钉住引用（队列独占）；
                // 本调用点 IRQ 屏蔽 + KL 窗内。
                drain_pinned_process(NonNull::new_unchecked(p as *mut ObjRef));
            }
            n += 1;
        }
    }
    if !held {
        berry_hal_aarch64::kern_lock::kern_release();
    }
}

// ---- 1.1/S1 调度属性声明：位选律单源已迁 `berry-polface`（W3-Q20，
// host 可测钉板——八位全表/flags=0 回默认/FREEZE_READY 特判/ktest 向量
// 4 测）；此处 re-export 消费（`crate::process::SchedAttr` 既有引用路径
// 零改动）。布局字段见 `调点调度-v1.1.md` §7.2；`flags=0` = 清除覆盖/
// 回继承。`Process` 持默认值、`Thread` 持覆盖值，S2/S5 消费侧从
// effective（默认 ∪ 覆盖）读取——S1 只存不执法。----
pub use berry_polface::SchedAttr;
// SA_FLAG_* 逐名 re-export（不全引 import——FREEZE_CLOCK/RESERVE 在本
// crate 无消费点，全引成 unused import 告警）。
pub use berry_polface::{SA_FLAG_WEIGHT, SA_FLAG_INTENT, SA_FLAG_POLICY,
    SA_FLAG_FREEZE_READY, SA_FLAG_DEADLINE, SA_FLAG_BUDGET};

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
    /// 1.1/S3-b：单线程冻结嵌套深度（最后一个 token close 才 thaw）。
    pub freeze_depth: u32,
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
               sched_attr: SchedAttr::default(), freeze_depth: 0 }
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
