//! berry-susface —— process/thread suspend 冻结深度判序纯函数面（W3-Q38）。
//!
//! **动机**：`sys_process_suspend`（0x3E，1.1/S3）的准入判序此前内联在
//! `kernel/src/syscall.rs` 两臂的裸 unsafe 段里——进程全冻六门
//! （token 帽 64 / Exited / pager_load>0 / 粒度互斥 / depth≥8 /
//! precheck）与单线程冻七门（started∧¬exited / owner 活∧started /
//! pager / FREEZE_READY / 互斥 / depth 帽 / precheck）的**判序**（先拒
//! 便宜门再进贵门、回滚路径的对称性）host 不可测——kernel crate 依赖
//! 整个内核环境。本 crate 把判序按纯投影入参抽成纯函数 + 表驱动测试，
//! kernel 侧消费同一函数（零第二实现——抽离即唯一事实源）。
//!
//! 抽离面（与 kernel 侧逐字对齐，改这里的语义 = 改 syscall 行为）：
//! - [`SUSPEND_TOKEN_CAP`]：每 caller 活跃 SuspendToken 帽（64——S5.12
//!   防 freeze DoS；安全审计 F1 修的帽值）。
//! - [`FREEZE_DEPTH_CAP`]：嵌套 depth 帽（8）。
//! - [`ProcGateInput`]/[`proc_admission`]：进程全冻六门判序——序钉死：
//!   token 帽 → Exited → pager → 粒度互斥（`freeze_depth==0 ∧
//!   thread_freezes>0`——首枚进程 token 与既有单线程冻互斥）→ depth 帽 →
//!   precheck。每门带拒绝码（`EBUSY`/`EBAD_STATE`），判序即错误码优先
//!   级序（同刻多门红时返回哪一个是 ABI 面）。
//! - [`ThreadGateInput`]/[`thread_admission`]：单线程冻七门判序——
//!   started∧¬exited → owner 活∧started → pager → FREEZE_READY（应用
//!   自证 D8）→ 互斥（`thrd.freeze_depth==0 ∧ p.freeze_depth>0`）→
//!   depth 帽 → precheck。
//! - [`thaw_route`]：thaw 键路由——Process 目标 = pinned、Thread 目标 =
//!   Thread ObjRef 地址（SuspendTok::key 的装载律）。
//! - [`drop_effect`]：SuspendTok Drop 判定段——depth 递减、归零触发的
//!   三面（Process：wake_pending 清 + 端点冻标撤 + thaw；Thread：
//!   FREEZE_READY 清 + Process 占位递减 + thaw）与中间层不解冻的幂等
//!   面。
//!
//! 不在此 crate（留 kernel 壳）：指针解引用/retain-release（unsafe 物理
//! 臂）、freeze_by_pinned/freeze_by_thread 扫描（frzface 消费面）、
//! insert_suspend_token 失败回滚的机械序（kernel 侧装配序已由
//! admission 判序钉住的对称性约束）、遥测计数。

#![cfg_attr(not(test), no_std)]

/// 每 caller 活跃 SuspendToken 上限（S5.12 防 freeze DoS——安全审计 F1
/// 修：检查在 token 生产者〔本臂 + thread 臂〕而非消费者）。
pub const SUSPEND_TOKEN_CAP: u32 = 64;

/// 嵌套冻结深度帽（进程/线程同值 8）。
pub const FREEZE_DEPTH_CAP: u32 = 8;

/// 进程状态纯投影（Exited 门入参）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcStateP {
    Running,
    Exited,
}

/// 冻结准入拒绝（错误码投影——kernel 侧映射 `-EBUSY`/`-EBAD_STATE`；
/// Ok = 六门/七门全过，可进 depth++/retain 段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Ok,
    /// -EBUSY 面：token 帽/pager/互斥/depth 帽/precheck。
    Busy,
    /// -EBAD_STATE 面：Exited / 未 started / 已 exited。
    BadState,
}

/// 进程全冻六门纯投影入参（kernel 侧从 `&Process` 逐字段装包——
/// dpgface SlotSnapshot 同款，零 trait 依赖）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcGateInput {
    /// caller 持有的活跃 SuspendToken 数（token 帽门入参）。
    pub caller_tokens: u32,
    /// 目标进程状态（Exited 门）。
    pub state: ProcStateP,
    /// pager 供给在飞数（D6：冻 pager = 缺页者永久阻塞）。
    pub pager_load: u32,
    /// 目标进程当前嵌套深度（互斥门 + depth 帽门）。
    pub freeze_depth: u32,
    /// 目标进程名下单线程冻占位数（互斥门——首枚进程 token 与既有
    /// 单线程 token 互斥：交叉 close 提前解冻面）。
    pub thread_freezes: u32,
    /// sched 侧 precheck（frzface 谓词装包后的合成结果——本 crate 不
    /// 重算 frzface 的队列扫描，只钉判序位）。
    pub precheck: bool,
}

impl ProcGateInput {
    /// 干净目标全过（六门皆绿）。
    pub const CLEAN: ProcGateInput = ProcGateInput {
        caller_tokens: 0,
        state: ProcStateP::Running,
        pager_load: 0,
        freeze_depth: 0,
        thread_freezes: 0,
        precheck: true,
    };
}

/// 进程全冻六门判序（sys_process_suspend !single 臂 :1995-2040 的判定
/// 段投影）。**判序即错误码优先级**：同刻多门红时先到先赢——
/// token 帽（Busy）→ Exited（BadState）→ pager（Busy）→ 粒度互斥
/// （Busy）→ depth 帽（Busy）→ precheck（Busy）。
pub fn proc_admission(g: &ProcGateInput) -> Admission {
    if g.caller_tokens >= SUSPEND_TOKEN_CAP {
        return Admission::Busy; // S5.12 防 freeze DoS
    }
    if g.state == ProcStateP::Exited {
        return Admission::BadState;
    }
    if g.pager_load > 0 {
        return Admission::Busy; // D6：冻 pager = 缺页者永久阻塞
    }
    // 单一位图不能同时表达进程冻与线程冻的交叉 close；两种粒度互斥。
    if g.freeze_depth == 0 && g.thread_freezes > 0 {
        return Admission::Busy;
    }
    if g.freeze_depth >= FREEZE_DEPTH_CAP {
        return Admission::Busy;
    }
    if !g.precheck {
        return Admission::Busy;
    }
    Admission::Ok
}

/// 单线程冻七门纯投影入参。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreadGateInput {
    /// caller 活跃 token 数（与进程臂同帽；kernel 侧 base 形本臂无 token
    /// 帽检查——装包恒 0，字段保留为判序钉板的完整面）。
    pub caller_tokens: u32,
    /// 目标线程 started/exited（S3-b：未启动或已退即 BadState）。
    pub started: bool,
    pub exited: bool,
    /// 所属进程四门投影（owner 活 ∧ started / pager / 互斥门入参）。
    pub owner: ThreadOwnerGate,
    /// 目标线程 FREEZE_READY 自证位（D8：应用自证非内核担保）。
    pub freeze_ready: bool,
    /// 目标线程当前嵌套深度（互斥门 + depth 帽门）。
    pub freeze_depth: u32,
    /// sched 侧 precheck 合成结果。
    pub precheck: bool,
}

/// 所属进程门投影（thread_admission 的 owner 侧入参——owner 缺席在
/// kernel 侧先拒 EBAD_STATE，到本面必为活 Process 的纯投影）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreadOwnerGate {
    /// owner 状态（Exited 门）。
    pub owner_state: ProcStateP,
    /// owner 已启动位。
    pub owner_started: bool,
    /// owner pager 在飞（保守同进程全冻口径）。
    pub pager_load: u32,
    /// owner freeze_depth（互斥门：进程级 token 在席时不再引入
    /// 单线程 token——交叉 close 提前解冻面）。
    pub owner_freeze_depth: u32,
}

impl ThreadGateInput {
    /// 干净目标全过（七门皆绿）。
    pub const CLEAN: ThreadGateInput = ThreadGateInput {
        caller_tokens: 0,
        started: true,
        exited: false,
        owner: ThreadOwnerGate {
            owner_state: ProcStateP::Running,
            owner_started: true,
            pager_load: 0,
            owner_freeze_depth: 0,
        },
        freeze_ready: true,
        freeze_depth: 0,
        precheck: true,
    };
}

/// 单线程冻七门判序（sys_process_suspend SINGLE_THREAD 臂 :2113-2156
/// 的判定段投影）。判序：token 帽（Busy）→ started∧¬exited
/// （BadState）→ owner 活∧started（BadState）→ pager（Busy）→
/// FREEZE_READY（Busy）→ 粒度互斥（Busy）→ depth 帽（Busy）→
/// precheck（Busy）。
pub fn thread_admission(g: &ThreadGateInput) -> Admission {
    if g.caller_tokens >= SUSPEND_TOKEN_CAP {
        return Admission::Busy;
    }
    if !g.started || g.exited {
        return Admission::BadState;
    }
    if g.owner.owner_state == ProcStateP::Exited || !g.owner.owner_started {
        return Admission::BadState;
    }
    if g.owner.pager_load > 0 {
        return Admission::Busy; // 保守同进程口径
    }
    // FREEZE_READY 是应用自证，不是内核安全担保（D8）。
    if !g.freeze_ready {
        return Admission::Busy;
    }
    // 进程级 token 在席时不再引入单线程 token。
    if g.freeze_depth == 0 && g.owner.owner_freeze_depth > 0 {
        return Admission::Busy;
    }
    if g.freeze_depth >= FREEZE_DEPTH_CAP {
        return Admission::Busy;
    }
    if !g.precheck {
        return Admission::Busy;
    }
    Admission::Ok
}

/// SuspendTarget 纯投影（thaw 键路由 + Drop 分流轴）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Process,
    Thread,
}

/// thaw 键路由（SuspendTok::key 装载律）：Process 目标 = pinned；
/// Thread 目标 = Thread ObjRef 地址。
pub const fn thaw_route(kind: TargetKind, pinned: usize, thread_addr: usize) -> usize {
    match kind {
        TargetKind::Process => pinned,
        TargetKind::Thread => thread_addr,
    }
}

/// Drop 判定段入参（depth 记账纯投影——非零减一、归零触发解冻三面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropInput {
    /// 目标种类（分流轴）。
    pub kind: TargetKind,
    /// Drop 前冻结深度（>0 才递减；防御 0 不下溢）。
    pub freeze_depth: u32,
}

/// Drop 判定段输出：是否递减 + 是否触发**全解冻**（depth 归零——中间
/// 嵌套层不解冻：Process 的 wake_pending 清/端点冻标撤、Thread 的
/// FREEZE_READY 清/Process 占位递减、thaw 键路由）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropEffect {
    /// 递减后的 depth（入参 0 时保持 0——防御不下溢）。
    pub depth_after: u32,
    /// 全解冻触发位（true 才走 thaw_by_* + 清面三件）。
    pub fully_thawed: bool,
}

/// SuspendTok Drop 判定段（suspend.rs Drop 臂投影）：depth>0 → 减一；
/// 归零 → fully_thawed（kernel 侧清 wake_pending/端点冻标 或
/// FREEZE_READY/占位，并按 [`thaw_route`] 路由 thaw）。
pub const fn drop_effect(d: &DropInput) -> DropEffect {
    if d.freeze_depth == 0 {
        return DropEffect { depth_after: 0, fully_thawed: false };
    }
    let after = d.freeze_depth - 1;
    DropEffect { depth_after: after, fully_thawed: after == 0 }
}

/// depth++ 装配段投影（admission Ok 后的记账步）：入参 depth 上限保护
/// 由 admission 帽门保证（< FREEZE_DEPTH_CAP 才 Ok），装配恒 +1。
pub const fn depth_push(depth: u32) -> u32 {
    depth + 1
}

/// freeze_by_pinned 返回 0 的回滚判定（进程臂 :2050-2058——扫描命中 0
/// 个任务 = BAD_STATE 回滚 depth/retain）。
pub const fn rollback_on_zero_scan(matched: u32) -> bool {
    matched == 0
}

/// freeze_by_thread 返回 ≠1 的回滚判定（线程臂 :2173-2188——单目标
/// 恰命中 1，其他值全回滚）。
pub const fn rollback_on_thread_scan(matched: u32) -> bool {
    matched != 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_six_gates_clean_pass() {
        assert_eq!(proc_admission(&ProcGateInput::CLEAN), Admission::Ok);
    }

    #[test]
    fn proc_gate_order_token_cap_first() {
        // token 帽最优先：即使 Exited + pager + 互斥全红，也只报 token 帽 Busy。
        let g = ProcGateInput {
            caller_tokens: SUSPEND_TOKEN_CAP,
            state: ProcStateP::Exited,
            pager_load: 3,
            freeze_depth: 0,
            thread_freezes: 2,
            precheck: false,
        };
        assert_eq!(proc_admission(&g), Admission::Busy);
        // 与 cap-1 对照：落到 Exited 门（BadState）——证明序在钉。
        let g2 = ProcGateInput { caller_tokens: SUSPEND_TOKEN_CAP - 1, ..g };
        assert_eq!(proc_admission(&g2), Admission::BadState);
    }

    #[test]
    fn proc_gate_order_exited_before_pager() {
        let g = ProcGateInput {
            state: ProcStateP::Exited,
            pager_load: 1,
            ..ProcGateInput::CLEAN
        };
        assert_eq!(proc_admission(&g), Admission::BadState);
        // 对照：Exited 绿后 pager 门接报 Busy。
        let g2 = ProcGateInput { pager_load: 1, ..ProcGateInput::CLEAN };
        assert_eq!(proc_admission(&g2), Admission::Busy);
    }

    #[test]
    fn proc_gate_granularity_mutex_only_first_token() {
        // 互斥门只对首枚进程 token 生效：freeze_depth==0 ∧ thread_freezes>0 拒。
        let g = ProcGateInput { thread_freezes: 1, ..ProcGateInput::CLEAN };
        assert_eq!(proc_admission(&g), Admission::Busy);
        // 嵌套第二枚（freeze_depth>0）不再查互斥门。
        let g2 = ProcGateInput { freeze_depth: 1, thread_freezes: 1, ..ProcGateInput::CLEAN };
        assert_eq!(proc_admission(&g2), Admission::Ok);
    }

    #[test]
    fn proc_gate_depth_cap_boundary() {
        // 7 过 8 拒——帽值恰界。
        let g7 = ProcGateInput { freeze_depth: FREEZE_DEPTH_CAP - 1, ..ProcGateInput::CLEAN };
        assert_eq!(proc_admission(&g7), Admission::Ok);
        let g8 = ProcGateInput { freeze_depth: FREEZE_DEPTH_CAP, ..ProcGateInput::CLEAN };
        assert_eq!(proc_admission(&g8), Admission::Busy);
    }

    #[test]
    fn proc_gate_precheck_last() {
        // precheck 是最后门：全绿 + precheck 红 = Busy；与 depth 帽同刻红
        // 时 depth 帽先报（同刻不可分——用 precheck 单红钉位）。
        let g = ProcGateInput { precheck: false, ..ProcGateInput::CLEAN };
        assert_eq!(proc_admission(&g), Admission::Busy);
    }

    #[test]
    fn thread_seven_gates_clean_pass() {
        assert_eq!(thread_admission(&ThreadGateInput::CLEAN), Admission::Ok);
    }

    #[test]
    fn thread_gate_not_started_or_exited_bad_state() {
        let g = ThreadGateInput { started: false, ..ThreadGateInput::CLEAN };
        assert_eq!(thread_admission(&g), Admission::BadState);
        let g2 = ThreadGateInput { exited: true, ..ThreadGateInput::CLEAN };
        assert_eq!(thread_admission(&g2), Admission::BadState);
    }

    #[test]
    fn thread_gate_owner_gates() {
        // owner 退 = BadState；owner 未启动 = BadState。
        let g = ThreadGateInput {
            owner: ThreadOwnerGate {
                owner_state: ProcStateP::Exited,
                ..ThreadGateInput::CLEAN.owner
            },
            ..ThreadGateInput::CLEAN
        };
        assert_eq!(thread_admission(&g), Admission::BadState);
        let g2 = ThreadGateInput {
            owner: ThreadOwnerGate { owner_started: false, ..ThreadGateInput::CLEAN.owner },
            ..ThreadGateInput::CLEAN
        };
        assert_eq!(thread_admission(&g2), Admission::BadState);
    }

    #[test]
    fn thread_gate_freeze_ready_required() {
        let g = ThreadGateInput { freeze_ready: false, ..ThreadGateInput::CLEAN };
        assert_eq!(thread_admission(&g), Admission::Busy);
    }

    #[test]
    fn thread_gate_granularity_mutex_first_thread_token() {
        // 互斥门只对首枚单线程 token 生效：进程级冻在席时首枚拒。
        let g = ThreadGateInput {
            owner: ThreadOwnerGate { owner_freeze_depth: 1, ..ThreadGateInput::CLEAN.owner },
            ..ThreadGateInput::CLEAN
        };
        assert_eq!(thread_admission(&g), Admission::Busy);
        // 嵌套第二枚（thrd.freeze_depth>0）不查互斥门。
        let g2 = ThreadGateInput {
            freeze_depth: 1,
            owner: ThreadOwnerGate { owner_freeze_depth: 1, ..ThreadGateInput::CLEAN.owner },
            ..ThreadGateInput::CLEAN
        };
        assert_eq!(thread_admission(&g2), Admission::Ok);
    }

    #[test]
    fn thread_gate_order_ready_before_mutex() {
        // FREEZE_READY 门先于互斥门：同刻双红报 Busy（READY 门也是 Busy，
        // 不可分——用 READY 绿对照互斥门接报）。
        let both = ThreadGateInput {
            freeze_ready: false,
            owner: ThreadOwnerGate { owner_freeze_depth: 1, ..ThreadGateInput::CLEAN.owner },
            ..ThreadGateInput::CLEAN
        };
        assert_eq!(thread_admission(&both), Admission::Busy);
        let only_mutex = ThreadGateInput {
            owner: ThreadOwnerGate { owner_freeze_depth: 1, ..ThreadGateInput::CLEAN.owner },
            ..ThreadGateInput::CLEAN
        };
        assert_eq!(thread_admission(&only_mutex), Admission::Busy);
    }

    #[test]
    fn thread_gate_depth_cap_boundary() {
        let g7 = ThreadGateInput {
            freeze_depth: FREEZE_DEPTH_CAP - 1,
            ..ThreadGateInput::CLEAN
        };
        assert_eq!(thread_admission(&g7), Admission::Ok);
        let g8 = ThreadGateInput { freeze_depth: FREEZE_DEPTH_CAP, ..ThreadGateInput::CLEAN };
        assert_eq!(thread_admission(&g8), Admission::Busy);
    }

    #[test]
    fn thread_gate_token_cap_shared_with_proc() {
        // token 帽与进程臂同值同优先级（F1 修：生产者侧检查）。
        let g = ThreadGateInput { caller_tokens: SUSPEND_TOKEN_CAP, ..ThreadGateInput::CLEAN };
        assert_eq!(thread_admission(&g), Admission::Busy);
        let g2 = ThreadGateInput {
            caller_tokens: SUSPEND_TOKEN_CAP - 1,
            ..ThreadGateInput::CLEAN
        };
        assert_eq!(thread_admission(&g2), Admission::Ok);
    }

    #[test]
    fn thaw_route_pinned_vs_thread_addr() {
        // Process = pinned；Thread = ObjRef 地址（suspend.rs key 装载律）。
        assert_eq!(thaw_route(TargetKind::Process, 0x1000, 0x2000), 0x1000);
        assert_eq!(thaw_route(TargetKind::Thread, 0x1000, 0x2000), 0x2000);
    }

    #[test]
    fn drop_effect_full_lifecycle() {
        // 三层嵌套：drop 两次不解冻，第三次全解冻。
        let d3 = DropInput { kind: TargetKind::Process, freeze_depth: 3 };
        let e2 = drop_effect(&d3);
        assert_eq!((e2.depth_after, e2.fully_thawed), (2, false));
        let e1 = drop_effect(&DropInput { kind: TargetKind::Process, freeze_depth: e2.depth_after });
        assert_eq!((e1.depth_after, e1.fully_thawed), (1, false));
        let e0 = drop_effect(&DropInput { kind: TargetKind::Process, freeze_depth: e1.depth_after });
        assert_eq!((e0.depth_after, e0.fully_thawed), (0, true));
    }

    #[test]
    fn drop_effect_zero_depth_no_underflow() {
        // 防御面：depth=0 的 Drop（目标已退等）不下溢、不解冻。
        let e = drop_effect(&DropInput { kind: TargetKind::Thread, freeze_depth: 0 });
        assert_eq!((e.depth_after, e.fully_thawed), (0, false));
    }

    #[test]
    fn drop_effect_depth_one_direct_thaw() {
        let e = drop_effect(&DropInput { kind: TargetKind::Thread, freeze_depth: 1 });
        assert_eq!((e.depth_after, e.fully_thawed), (0, true));
    }

    #[test]
    fn rollback_scan_judgements() {
        // 进程臂：0 命中回滚；线程臂：恰 1 过、0/2 回滚。
        assert!(rollback_on_zero_scan(0));
        assert!(!rollback_on_zero_scan(1));
        assert!(!rollback_on_thread_scan(1));
        assert!(rollback_on_thread_scan(0));
        assert!(rollback_on_thread_scan(2));
    }

    #[test]
    fn depth_push_monotonic() {
        assert_eq!(depth_push(0), 1);
        assert_eq!(depth_push(7), 8);
    }

    #[test]
    fn caps_frozen_values() {
        // ABI 冻结值钉死：帽值变化 = 行为变化（S5.12 / S3 嵌套语义）。
        assert_eq!(SUSPEND_TOKEN_CAP, 64);
        assert_eq!(FREEZE_DEPTH_CAP, 8);
    }
}
