//! scan 模块（W3-Q51）：class.rs `scan_filtered` 单圈合并扫描骨架纯面外迁。
//!
//! **动机**：scan_filtered 的「三分支带序状态机」（更高带重置 best / 同带
//! EEVDF pick_better / 低带跳过）此前只活在 sched 的 `NonNull<Task>` 裸指针
//! 环走查里——与 W3-Q12 迁出的 `pick_better` 是同一条扫描循环的两半：比较
//! 律单源了、**扫描骨架本身**（累加器初值律/带序三分支/PURE_EEVDF 开关/
//! eligible 前置）仍留 sched 内联。本模块把骨架整体外迁为泛型累加器
//! [`ScanAcc`]：sched 侧 `T = NonNull<Task>` 直传（advance_past 同款纪律），
//! host 测 `T = usize` 钉板全表。
//!
//! 与 sched 侧逐字对齐（改这里的语义 = 改 sched 行为）：
//! - [`ScanAcc::new`]：`best_eff = i32::MIN` + `best = None` 初值律——
//!   首个 eligible 必落「更高带重置」（e > i32::MIN）或退化
//!   i32::MIN 带「同带首见」（best=None 兜底）。
//! - [`ScanAcc::offer`]：逐元素三分支。eligible 过滤**前置于带比较**——
//!   被过滤者不参与 best_eff 竞争（Running-elsewhere/FROZEN/THROTTLE
//!   不占带）。
//! - [`pure_eevdf`] 开关：`!pure && e > best_eff` 带比较生效；
//!   `pure == true` 时带比较恒假 → 所有人同带（S2b 实验形，const 折叠
//!   零运行时开销——v1 恒 false）。
//!
//! 物理臂留壳（sched 侧）：环走查（`rq_next` 链推进 + 回 head 闭合判）、
//! `NonNull` 解引用与投影、`advance_past` 游标前移、`policy::apply`。
//! 本面只钉「一圈扫描怎么选出 best」的判定序。

use crate::{eligible, pick_better, TaskDesc};

/// S2b PURE_EEVDF 实验开关（class.rs `PURE_EEVDF` const 的单源迁此——
/// sched 侧 re-export，两处漂移 = 编译红）。
///
/// - `false`（v1 生产形）：严格带序 + 带内 EEVDF——PI boost 即时生效
///   （高带先出），deadline 带（≥1000）恒先于 fair。
/// - `true`（S2b 实验形）：带比较恒假 → 所有任务纯
///   `(eligible, vdeadline, vruntime)` 竞争（deadline 带语义失效——
///   绝对截止期 VD 与 fair VD 尺度不可比；实验轨道，v1 测试全按
///   false 假设构建）。
pub const PURE_EEVDF: bool = false;

/// 单圈合并扫描累加器（scan_filtered 三分支状态机的纯值域形）。
///
/// 泛型 `T: Copy` = 节点标识（sched 侧 `NonNull<Task>`；host 测 usize）。
/// 不变式（任一已 offer 前缀上）：
/// - `best == None` ⟺ 尚无 eligible 前缀元素；
/// - `best == Some(_)` 时 `best_eff` = 前缀 eligible 者的最高带、
///   `best` = 该带内 EEVDF 最优（同键首见保序——`pick_better` 严格 `<`）。
#[derive(Debug, Clone)]
pub struct ScanAcc<T: Copy> {
    best: Option<T>,
    best_desc: TaskDesc,
    best_eff: i32,
    best_elig: bool,
    best_key: (u64, u64),
}

impl<T: Copy> ScanAcc<T> {
    /// 初值律：`best_eff = i32::MIN` + `best = None` + 键 u64::MAX——
    /// 首个 eligible 必落「更高带重置」（e > i32::MIN，除非 e 本身就是
    /// i32::MIN 的退化带）或退化带「同带首见」（best=None 兜底选中）。
    pub const fn new() -> Self {
        ScanAcc {
            best: None,
            best_desc: TaskDesc::new(0, 0, i32::MIN, u64::MAX, u64::MAX),
            best_eff: i32::MIN,
            best_elig: false,
            best_key: (u64::MAX, u64::MAX),
        }
    }

    /// 是否已有出选者（None = 全前缀被过滤/空环——sched 侧 `?` 早退同义）。
    pub fn best(&self) -> Option<T> {
        self.best
    }

    /// 当前已追踪的最高 eligible 带（W3-Q93：try_preempt 双圈合一消费——
    /// 「环上存在 eligible 且 eff 严格高于 v」⟺ `best_eff() > v`，零第二
    /// 实现——旧 sched `has_eff_above_filtered` 探峰谓词的累加器内单源形）。
    ///
    /// 值域律：初值 `i32::MIN` = 空前缀/全过滤（旧探峰空环返 false 同义）；
    /// 有 eligible 前缀时 = 前缀最高带（与 [`Self::best`] 的选中带同源——
    /// `best_eff() > i32::MIN` ⟹ `best().is_some()` 不变式）。PURE_EEVDF
    /// 开形下带比较恒假、本值恒哨兵（实验轨道，v1 恒 false 不经此）。
    pub fn best_eff(&self) -> i32 {
        self.best_eff
    }

    /// 逐元素三分支（scan_filtered 循环体单源）。
    ///
    /// `cand` = 候选者纯投影；`cand_node` = 候选者标识（直通存入累加器）；
    /// `is_cur` = 指针恒等判注入（eligible 过滤）；`avg` = EEVDF 平均
    /// 虚拟时间（带内 lag 判据 `vruntime <= avg`）。
    ///
    /// 判定序（与 class.rs 逐字对齐）：
    /// 1. eligible 过滤前置（被过滤者不参与带竞争）；
    /// 2. `!pure && e > best_eff` → 更高带重置（旧两圈形的段 1 max_eff
    ///    更新 + 段 2 最优清空的合并点）；
    /// 3. `pure || e == best_eff` → 同带 EEVDF（pick_better 单源）；
    /// 4. 其余（e < best_eff 且 !pure）→ 低带跳过。
    pub fn offer(&mut self, cand: &TaskDesc, cand_node: T, is_cur: bool, avg: u64) {
        self.offer_with(cand, cand_node, is_cur, avg, PURE_EEVDF)
    }

    /// [`offer`] 的开关参数化形（host 测钉 PURE_EEVDF 两形；sched 侧
    /// const 折叠用默认形——`offer` 即生产路径）。
    pub fn offer_with(
        &mut self,
        cand: &TaskDesc,
        cand_node: T,
        is_cur: bool,
        avg: u64,
        pure: bool,
    ) {
        // 过滤前置：Running-elsewhere/FROZEN/THROTTLE 不占带（不参与
        // best_eff 竞争——旧两段同口径）。
        if !eligible(cand, is_cur) {
            return;
        }
        let e = cand.prio_effective;
        if !pure && e > self.best_eff {
            // 更高带 → 重置带内最优。
            self.best_eff = e;
            self.best = Some(cand_node);
            self.best_desc = *cand;
            self.best_elig = cand.vruntime <= avg;
            self.best_key = (cand.vdeadline, cand.vruntime);
        } else if pure || e == self.best_eff {
            // 同带 → EEVDF（eligible 优先；同键首见保序——严格 `<`）。
            // best 投影真值直传（class.rs 调用形同构：非占位非镜像）。
            let elig = cand.vruntime <= avg;
            let better = pick_better(
                cand,
                elig,
                self.best.map(|_| (self.best_desc, self.best_elig)),
                self.best_key,
                avg,
            );
            if better {
                self.best = Some(cand_node);
                self.best_desc = *cand;
                self.best_elig = elig;
                self.best_key = (cand.vdeadline, cand.vruntime);
            }
        }
        // e < best_eff 且 !pure → 低带无资格，跳过。
    }
}
