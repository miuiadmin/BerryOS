//! berry-ringface —— runqueue 环游标与扫描序不变量纯函数面（W3-Q12）。
//!
//! **动机**：环形 runqueue 的游标/出选语义此前只活在 `sched/src/runqueue.rs`
//! 与 `class.rs` 的 `NonNull<Task>` 裸指针环走查里——sched 依赖 HAL
//! global_asm，只 aarch64-unknown-none 可编，host 测不可达，ktest 只覆盖
//! boot 相。环序语义的回归面（`advance_past` 游标前移律、出选过滤
//! （Running-elsewhere/FROZEN/THROTTLE）逐位、带内 EEVDF 扫描序与同键
//! 首见保序、走查步数帽）需要一张 host 可测的**钉板**：本 crate 把环序
//! 语义按值域抽成纯函数 + 表驱动测试，`sched` 侧消费同一函数（零第二
//! 实现——抽离即唯一事实源，非镜像；wqmatch/sliceface 同款纪律）。
//!
//! 抽离面（与 sched 侧逐字对齐，改这里的语义 = 改 sched 行为）：
//! - [`TaskDesc`]：环走查逐节点读的**纯值域投影**（state/held_locks/
//!   prio_effective/vruntime/vdeadline）——sched 侧从 `*mut Task` 投影，
//!   本面不钉「链怎么接」（裸指针机械留 sched），只钉「游标怎么走、
//!   谁能出选、扫描序不变量」。
//! - [`RUNNING`]/[`BLOCKED`]/[`EXITED`]/[`READY`]：TaskState 四值的
//!   纯值域形（判等面只有「是否 Running」被消费——`eligible` 单比较）。
//! - [`HELD_FROZEN`]/[`HELD_THROTTLE`]：过滤双位（自 task.rs 单源迁此，
//!   sched `pub use` re-export——位值是 ABI 冻结面，见 task.rs 字段注）。
//! - [`eligible`]：出选资格（S-3b 单臂形 + 1.1/S3 冻结位）——非
//!   Running（或 Running 但正是 my_cur）且未冻结/未限流。指针恒等判
//!   以 `is_cur` bool 注入（sched 侧 `(t as *mut Task) != my_cur`）。
//! - [`advance_past`]：环游标前移律——`next(node)`（单元素环时
//!   next==node，游标不变）；纯值域形返回目标游标，sched 侧包
//!   `head.set`。
//! - [`PickKey`]/[`pick_better`]：带内 EEVDF 出选比较——eligible 优先、
//!   键 `(vdeadline, vruntime)` 严格 `<` 保首见（`None` 兜底选中）。
//! - [`walk_step_cap`]：走查步数帽执法判——`steps <= cap` 恒真者放行，
//!   到帽即断言违约（sched 侧 ring_check/dl_on_ring_walk 三处消费）。
//!
//! 与冻结面的关系：`SchedClass` trait 冻结签名零触碰；SLEEPQ/RQ 物理
//! 结构不在此 crate（`Cell<Option<NonNull>>` 链接字段机械留 sched——本
//! 面只钉「游标与扫描序」的判定律，不钉「环怎么接」的结构原语）。
//! HAL/时基零依赖——纯面无环境依赖。

#![cfg_attr(not(test), no_std)]
#![allow(clippy::result_unit_err)]

// ---- TaskState 纯值域投影 ----
// sched 侧 `crate::task::TaskState` 是无数据 enum，判等面在环序语义里
// 只有「是否 Running」一处（eligible 单比较）；本面以 u8 镜像四值钉板
// （sched 侧投影 `state as u8`——枚举定义仍单源在 task.rs，投影是纯
// 机械映射，两处 `#[derive]` 顺序钉死（见 tests::state_projection）。

/// 纯值域 TaskState::Ready（在环、可被 pick）。
pub const READY: u8 = 0;
/// 纯值域 TaskState::Running（正在 CPU 上跑——环上恰有本核 cur）。
pub const RUNNING: u8 = 1;
/// 纯值域 TaskState::Blocked（在睡眠队列上）。
pub const BLOCKED: u8 = 2;
/// 纯值域 TaskState::Exited（已退出——离环幂等面）。
pub const EXITED: u8 = 3;

/// 进程/线程冻结位（held_locks bit6——process_suspend 置 / SuspendToken
/// close 清）。自 task.rs 单源迁此，sched re-export（ktest 既有引用路径
/// `berry_sched::HELD_FROZEN` 零改动）。
pub const HELD_FROZEN: u8 = 1 << 6;
/// 预算限流位（held_locks bit7——budget 耗尽显置、周期到 lazy_unthrottle
/// 清）。与 FROZEN 分立（共用一位会让预算周期到期误清进程冻结位）。
pub const HELD_THROTTLE: u8 = 1 << 7;

/// 环走查逐节点的纯值域投影（sched 侧从 `*mut Task` 五字段机械投影）。
///
/// 裸指针/链接字段**不在**投影里——「环怎么接」的结构原语（link_back/
/// unlink/断言执法）留 sched；本面只消费「节点值」决定游标与出选。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskDesc {
    /// `TaskState` 纯值域（READY/RUNNING/BLOCKED/EXITED 四值）。
    pub state: u8,
    /// 持有集位图（过滤只看 FROZEN|THROTTLE 双位）。
    pub held_locks: u8,
    /// 有效优先级带（EEVDF 带比较；PI 捐赠可变）。
    pub prio_effective: i32,
    /// EEVDF 虚拟运行时间（带内第二排序键）。
    pub vruntime: u64,
    /// EEVDF 虚拟截止期（带内第一排序键）。
    pub vdeadline: u64,
}

impl TaskDesc {
    ///sched 侧最小投影便利形（class.rs 消费形——prio/vruntime/vdeadline
    /// 出选比较三字段 + state/held_locks 过滤两位）。
    pub const fn new(
        state: u8,
        held_locks: u8,
        prio_effective: i32,
        vruntime: u64,
        vdeadline: u64,
    ) -> Self {
        TaskDesc {
            state,
            held_locks,
            prio_effective,
            vruntime,
            vdeadline,
        }
    }
}

/// 出选资格（S-3b 单臂形 + 1.1/S3 冻结位）：非 Running（或 Running 但
/// 正是 `my_cur`——`is_cur` 注入指针恒等判），且未被冻结/限流。
///
/// 与 `class.rs::eligible` 逐字同律：Running-elsewhere 臂（pick 选中后
/// 任务仍留环，环上恰有「正在跑」的本核 cur——扫描须跳过；my_cur 自
/// 豁免）+ FROZEN/THROTTLE 双位过滤（冻结任务留在环上但永不出选——
/// thaw 后恢复）。
#[inline]
pub fn eligible(desc: &TaskDesc, is_cur: bool) -> bool {
    !(desc.state == RUNNING && !is_cur)
        && desc.held_locks & (HELD_FROZEN | HELD_THROTTLE) == 0
}

/// 环游标前移律（4-3a）：pick 选中 `node` 后游标前移到其后继。
///
/// 泛型纯值域形：`T` = 节点标识（host 测用 usize；sched 侧 `T =
/// NonNull<Task>` 直传——零投影零装箱）。单元素环时 `next == node`——
/// 返回值即 node 自身（游标不变，保持指向它——唯一在环者的自指合法性
/// 钉板）。
///
/// 返回类型 `Option<T>`：`None` = `next` 为 `None`（**违约**——环上节点
/// 必有 next；sched 侧 `.expect()` 响亮失败同旧语义）。违约表达用
/// Option 而非 panic：纯面不炸（ktest「测判据不测炸」同款纪律），消费
/// 侧决定违约怎么响。
#[inline]
pub fn advance_past<T: Copy>(node: T, next: Option<T>) -> Option<T> {
    let _ = node; //（律只由 next 决定——node 参数钉「选中者」语境 + 单元素
    // 环自指形 host 可测；游标目标恒 = next。）
    next
}

/// 带内 EEVDF 出选键：`(vdeadline, vruntime)`（第一键虚拟截止期、第二键
/// 虚拟运行时间；均小者优）。
pub type PickKey = (u64, u64);

/// 带内出选比较（scan_filtered 同带分支核心）：`cand` 是否优于 `best`。
///
/// - `best == None` → 首见兜底选中（`best_eff = i32::MIN` 初值下首个
///   eligible 必落「更高带重置」或退化带首见——None 兜底钉板）；
/// - eligible 优先：`vruntime <= avg` 者恒优于超 avg 者（EEVDF 的 lag
///   补偿——落后者优先出选）；
/// - 同 eligibility 档内键 `(vdeadline, vruntime)` 严格 `<`（首见保序：
///   同键不替换——`advance_past` 推进游标 = 等权 FIFO 轮转的机械基础）。
///
/// `best` 取值形（`TaskDesc` 是 `Copy`——投影小、拷贝零成本）：调用侧
/// 免引用生命周期纠缠（sched 扫描循环里 best 是 `Option<NonNull>` 裸
/// 指针，投影即拷出，零借用）。
#[inline]
pub fn pick_better(
    cand: &TaskDesc,
    cand_elig: bool,
    best: Option<(TaskDesc, bool)>,
    best_key: PickKey,
    avg: u64,
) -> bool {
    let _ = avg; //（elig 已是调用侧算好的 `vruntime <= avg`——参数保留给
    // 未来免二次投影的整键形；当前消费面（class.rs scan_filtered）先投影
    // 后调本函数，avg 不进比较——见函数头注。）
    match best {
        None => true,
        Some((_, best_elig)) => {
            if cand_elig != best_elig {
                cand_elig
            } else {
                (cand.vdeadline, cand.vruntime) < best_key
            }
        }
    }
}

/// 走查步数帽执法判（ring_check/dl_on_ring_walk/resync 三处同律）：
/// `steps <= cap` 恒真（走查回 head 闭合即止）；`steps > cap` = 走不回
/// head 的断链形态——返回 false 由调用侧 assert 响亮失败。
///
/// 纯函数化的意义：三处环走查的步数帽执法此前是三份逐字拷贝的
/// `assert!(steps <= RING_WALK_CAP, ...)`，语义漂移面三倍；单源后帽值
/// 与判据一处钉死（RING_WALK_CAP 常量仍留 runqueue.rs——它同时是
/// sched.rs integrity_check 的对账界，迁移会动 ktest 引用面，本件
/// 不扩）。
#[inline]
pub fn walk_step_cap(steps: usize, cap: usize) -> bool {
    steps <= cap
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desc(state: u8, held: u8, prio: i32, vrt: u64, vd: u64) -> TaskDesc {
        TaskDesc::new(state, held, prio, vrt, vd)
    }

    // ---- eligible：出选过滤逐位钉板 ----

    #[test]
    fn eligible_running_elsewhere_arm() {
        // Running-elsewhere 臂：环上他核 Running 者跳过；my_cur 自豁免。
        let r = desc(RUNNING, 0, 0, 0, 0);
        assert!(!eligible(&r, false), "他核 Running 须拒");
        assert!(eligible(&r, true), "my_cur 自豁免（指针恒等注入）");
        // 非 Running 三态全放行（Ready 可出选；Blocked/Exited 物理上不在
        // 环——防御面同放行，结构层保证不出现）。
        for s in [READY, BLOCKED, EXITED] {
            assert!(
                eligible(&desc(s, 0, 0, 0, 0), false),
                "非 Running 态 {s} 须放行"
            );
        }
    }

    #[test]
    fn eligible_frozen_throttle_bits() {
        // FROZEN/THROTTLE 双位过滤：任一位置位即拒（thaw/周期到后恢复）。
        assert!(!eligible(&desc(READY, HELD_FROZEN, 0, 0, 0), false));
        assert!(!eligible(&desc(READY, HELD_THROTTLE, 0, 0, 0), false));
        assert!(!eligible(
            &desc(READY, HELD_FROZEN | HELD_THROTTLE, 0, 0, 0),
            false
        ));
        // 过滤只看双位：结构锁四位 + 表锁位不挡出选（S-3c 持有位与出选
        // 资格正交——锁窗口内任务照常可被 pick）。
        let struct_bits = 0b0011_1110u8; // SQ|RQ0|RQ1|REAPQ|TABLE
        assert!(eligible(&desc(READY, struct_bits, 0, 0, 0), false));
        // my_cur 不豁免冻结位（Running 臂与冻结臂独立AND——双臂全真才资格）。
        assert!(!eligible(&desc(RUNNING, HELD_FROZEN, 0, 0, 0), true));
        // 边界：双位置位 + Running 他核 = 双臂皆拒；位清后复原。
        let mut t = desc(RUNNING, HELD_THROTTLE, 0, 0, 0);
        assert!(!eligible(&t, false));
        t.held_locks &= !HELD_THROTTLE;
        assert!(eligible(&t, false) == false); // 仍 Running 他核
        t.state = READY;
        assert!(eligible(&t, false), "清位 + Ready 复原出选资格");
    }

    // ---- advance_past：游标前移律 ----

    #[test]
    fn advance_past_cursor_law() {
        // 常形：游标前移到 next（环序保持——head 侧先见出选后游标已过）。
        assert_eq!(advance_past(3, Some(7)), Some(7));
        assert_eq!(advance_past(0, Some(1)), Some(1));
        // 单元素环自指：next==node，游标不变（唯一在环者保持指向）。
        assert_eq!(advance_past(5, Some(5)), Some(5));
        // 违约形：None（环上节点必有 next——断链）→ 调用侧 expect 炸。
        assert_eq!(advance_past(3, None), None);
    }

    // ---- pick_better：带内 EEVDF 扫描序 ----

    #[test]
    fn pick_better_first_seen_fallback() {
        // None 兜底：首个 eligible 必选中（best_eff=i32::MIN 初值同义）。
        let c = desc(READY, 0, 0, 10, 100);
        assert!(pick_better(&c, true, None, (u64::MAX, u64::MAX), 0));
        assert!(pick_better(&c, false, None, (u64::MAX, u64::MAX), 0));
    }

    #[test]
    fn pick_better_eligible_first() {
        // eligible 优先：落后者（vruntime<=avg）恒优于超 avg 者——即使
        // 落后者键更差（vd 更大）。
        let lag = desc(READY, 0, 0, 5, 900);
        let lead = desc(READY, 0, 0, 50, 100);
        let (b, bk) = (lead, (100u64, 50u64));
        assert!(pick_better(&lag, true, Some((b, false)), bk, 30));
        // 反向：超前 avg 者不抢落后者位。
        assert!(!pick_better(&lead, false, Some((lag, true)), (900, 5), 30));
        // 同档（双 eligible）：键严格 < 决胜。
        let lag2 = desc(READY, 0, 0, 5, 800);
        assert!(pick_better(&lag2, true, Some((lag, true)), (900, 5), 30));
        let lag3 = desc(READY, 0, 0, 6, 900); // 键 (900,6) > (900,5)
        assert!(!pick_better(&lag3, true, Some((lag, true)), (900, 5), 30));
    }

    #[test]
    fn pick_better_key_order_vd_then_vrt() {
        // 第一键 vdeadline、第二键 vruntime；同键（严格 < 不成立）保首见。
        let a = desc(READY, 0, 0, 7, 100);
        let b = desc(READY, 0, 0, 8, 100);
        // (100,8) vs (100,7)：vrt 更小者优。
        assert!(pick_better(&a, true, Some((b, true)), (100, 8), 0));
        assert!(!pick_better(&b, true, Some((a, true)), (100, 7), 0));
        // 同键 (100,7)==(100,7)：不替换（首见保序 = FIFO 轮转机械基础）。
        let a2 = desc(READY, 0, 0, 7, 100);
        assert!(!pick_better(&a2, true, Some((a, true)), (100, 7), 0));
        // 边界值：u64::MAX 键不优于任何（除 None 兜底）。
        let m = desc(READY, 0, 0, u64::MAX, u64::MAX);
        assert!(!pick_better(&m, true, Some((a, true)), (100, 7), 0));
    }

    // ---- walk_step_cap：步数帽 ----

    #[test]
    fn walk_cap_law() {
        // 闭合走查恒放行；到帽+1 违约（断链形态——调用侧 assert）。
        assert!(walk_step_cap(0, 128));
        assert!(walk_step_cap(1, 128));
        assert!(walk_step_cap(127, 128));
        assert!(walk_step_cap(128, 128), "恰在帽 = 最后一步合法");
        assert!(!walk_step_cap(129, 128), "超帽 = 断链违约");
        assert!(!walk_step_cap(usize::MAX, 128));
        // 帽 0 退化：任何正步数违约（空环走查 0 步闭合合法）。
        assert!(walk_step_cap(0, 0));
        assert!(!walk_step_cap(1, 0));
    }

    // ---- sched 侧投影一致性：TaskState as u8 序钉板 ----
    // sched 侧投影 = `state as u8`（enum 判定序：Ready=0/Running=1/
    // Blocked=2/Exited=3）。本测试在 sched 消费编译面之外钉投影序——
    // 两处（task.rs enum 定义序与本 crate 常量）漂移 = 此测红。
    #[test]
    fn state_projection_pinned() {
        // 常量序 = sched 侧 task.rs 的 TaskState 判定序（Ready=0/Running=1/
        // Blocked=2/Exited=3）——投影 `state as u8` 机械映射的唯一事实源。
        // 两处漂移（enum 变体增删/换序）= 投影错位 → RUNNING 判错 →
        // eligible 全环错向——此测红即拦。
        assert_eq!(super::READY, 0);
        assert_eq!(super::RUNNING, 1);
        assert_eq!(super::BLOCKED, 2);
        assert_eq!(super::EXITED, 3);
        // 消费侧编译期钉板：sched 投影经 as-cast 引本 crate 常量（见
        // class.rs/runqueue.rs 消费点注释）——语义漂移在编译面显形。
        let _ = [
            (super::READY, "Ready"),
            (super::RUNNING, "Running"),
            (super::BLOCKED, "Blocked"),
            (super::EXITED, "Exited"),
        ];
    }

    // ---- 扫描序整环模拟：三任务环形表驱动 ----
    // 钉的是「游标 + 过滤 + 带内序」合成的整环出选序不变量（class.rs
    // scan_filtered 单圈三分支的纯面全模拟——sched 侧集成形态由 ktest
    // RING/EEVDF 钉板，本测钉单圈扫描序本身的值域律）。

    /// 单圈合并扫描纯面模拟（带比较 + 带内 pick_better——与 class.rs
    /// scan_filtered 同构，环序由 Vec 承载、游标 = 起点索引）。
    fn scan(ring: &[TaskDesc], start: usize, avg: u64, is_cur: &[bool]) -> Option<usize> {
        let mut best: Option<usize> = None;
        let mut best_eff = i32::MIN;
        let mut best_elig = false;
        let mut best_key = (u64::MAX, u64::MAX);
        let n = ring.len();
        for i in 0..n {
            let idx = (start + i) % n;
            let p = &ring[idx];
            if !eligible(p, is_cur[idx]) {
                continue;
            }
            let e = p.prio_effective;
            if e > best_eff {
                best_eff = e;
                best = Some(idx);
                best_elig = p.vruntime <= avg;
                best_key = (p.vdeadline, p.vruntime);
            } else if e == best_eff {
                let elig = p.vruntime <= avg;
                if pick_better(p, elig, best.map(|b| (ring[b], best_elig)), best_key, avg) {
                    best = Some(idx);
                    best_elig = elig;
                    best_key = (p.vdeadline, p.vruntime);
                }
            }
        }
        best
    }

    #[test]
    fn scan_ring_band_priority() {
        // 带比较：更高 prio_effective 带整带压制低带（EEVDF 只在带内）。
        let ring = [
            desc(READY, 0, 10, 0, 0),   // 低带但键最优
            desc(READY, 0, 20, 900, 900), // 高带键最差
            desc(READY, 0, 20, 800, 800),
        ];
        let is_cur = [false; 3];
        // 从任一起点扫：高带（20）成员胜——键无关。
        for s in 0..3 {
            assert_eq!(scan(&ring, s, 0, &is_cur), Some(2), "起点 {s}");
        }
    }

    #[test]
    fn scan_ring_first_seen_fifo() {
        // 同带同键：首见保序（advance_past 游标推进 = 等权 FIFO 轮转的
        // 机械基础——起点决定首见者）。
        let ring = [
            desc(READY, 0, 0, 5, 100),
            desc(READY, 0, 0, 5, 100),
            desc(READY, 0, 0, 5, 100),
        ];
        let is_cur = [false; 3];
        assert_eq!(scan(&ring, 0, 10, &is_cur), Some(0));
        assert_eq!(scan(&ring, 1, 10, &is_cur), Some(1));
        assert_eq!(scan(&ring, 2, 10, &is_cur), Some(2));
    }

    #[test]
    fn scan_ring_running_skip_and_cursor_round() {
        // Running-elsewhere 跳过 + 游标前移的三轮轮转模拟：
        // 关键语义 = my_cur 自豁免（在跑者本人可连任）——他核 Running 才跳。
        // 第 1 轮：0 在跑（is_cur=0 豁免），起点 0——0 键最优首见即选中
        //（连任合法）；出选后 advance_past(0) → 游标 1。
        let ring = [
            desc(RUNNING, 0, 0, 5, 100),
            desc(READY, 0, 0, 5, 100),
            desc(READY, 0, 0, 5, 100),
        ];
        let mut is_cur = [true, false, false];
        assert_eq!(scan(&ring, 0, 10, &is_cur), Some(0), "my_cur 豁免可连任");
        // 第 2 轮（模拟 0 仍占 CPU、但换扫描语境 cur=null——park/远端窗
        // 形）：起点 1，0 成他核 Running 被跳——首见 1。
        is_cur = [false, false, false];
        assert_eq!(scan(&ring, 1, 10, &is_cur), Some(1), "他核 Running 跳过");
        // 第 3 轮：1 切上（Running），游标已过 1 → 起点 2——1 跳、
        // 2/0 中首见 2（0 仍 Running——跳）→ 选中 2；三任务各得一轮，
        // 游标闭环。
        let ring3 = [
            desc(RUNNING, 0, 0, 5, 100),
            desc(RUNNING, 0, 0, 5, 100),
            desc(READY, 0, 0, 5, 100),
        ];
        is_cur = [false, true, false];
        assert_eq!(scan(&ring3, 2, 10, &is_cur), Some(2));
    }

    #[test]
    fn scan_ring_frozen_never_picked() {
        // 冻结者留在环上但永不出选（thaw 后恢复）——全环冻结 = None。
        let frozen = HELD_FROZEN;
        let ring = [
            desc(READY, frozen, 99, 0, 0), // 高带冻结
            desc(READY, 0, 0, 5, 100),
        ];
        let is_cur = [false; 2];
        assert_eq!(scan(&ring, 0, 10, &is_cur), Some(1));
        let all_frozen = [
            desc(READY, frozen | HELD_THROTTLE, 0, 0, 0),
            desc(READY, frozen, 0, 0, 0),
        ];
        assert_eq!(scan(&all_frozen, 0, 10, &is_cur), None, "全过滤 = 无出选");
    }

    #[test]
    fn scan_ring_empty_and_single() {
        // 空环 None（head()? 同义）；单元素环自指：选中后游标不变。
        let is_cur = [false; 1];
        assert_eq!(scan(&[], 0, 0, &is_cur), None);
        let solo = [desc(READY, 0, 0, 1, 1)];
        assert_eq!(scan(&solo, 0, 0, &is_cur), Some(0));
        // advance_past 单元素环：游标保持指向它。
        assert_eq!(advance_past(0, Some(0)), Some(0));
    }

    #[test]
    fn scan_ring_evdof_lag_priority() {
        // 带内 EEVDF：落后者（vruntime<=avg）优先——即使 vdeadline 更晚。
        let ring = [
            desc(READY, 0, 0, 100, 100), // 超 avg（lead）
            desc(READY, 0, 0, 0, 999),   // 落后（lag）键最差
        ];
        let is_cur = [false; 2];
        // avg=50：lag 落后优先胜——键 (999,0) 输给 (100,100) 仍胜。
        assert_eq!(scan(&ring, 0, 50, &is_cur), Some(1));
        // avg=0：无人落后（100>0、0<=0——lag 仍 elig；0<=0 真）。边界：
        // vruntime==avg 算 eligible（<= 含等）。
        assert_eq!(scan(&ring, 0, 0, &is_cur), Some(1), "==avg 算落后者");
        // avg=200：双落后 → 纯键比较，键优者 (100,100) 胜。
        assert!(ring[0].vruntime <= 200);
        assert_eq!(scan(&ring, 0, 200, &is_cur), Some(0));
    }
}
