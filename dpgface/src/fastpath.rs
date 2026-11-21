//! W3-Q59：dpg P0-1 fastpath 唤醒直切判序纯面（P0-1 线1d 消费面）。
//!
//! **动机**：`dpg_pager` 供给毕唤 faulter 后的 `berry_sched::try_preempt()`
//! 直切（dpg.rs:575）是 P0-1 线1d 的 dpg 形——但其判序律（①`sched::init`
//! 前 CURRENT null 早退护栏 → ②出选门控〔幻影出选防线：环上无严格更高者
//! 零副作用返——`pick_next` 选中即`advance_past` 消耗 head 槽位，「出选后
//! 不切走」= 幻影出选，重复性 IPC节奏会把真实出选相位锁死在固定节点集
//! 结构性饿死，4-6 实测；04 号审计机会一②（2026-11-17）起物理面 =
//! `Fair::pick_next_if_above`（W3-Q93 终形）单圈门控形——旧 `has_eff_above_filtered`
//! 只读探峰圈已并入出选圈退役〕→ ③严格
//! 更高才切〔§A43 同层 FIFO 不插队〕）只活在 sched crate 的 unsafe 物理
//! 臂里，host 测不可达。本模块把「给定当前任务纯投影 + 环快照的直切
//! 资格」抽成纯函数钉板。
//!
//! 与 sched 物理臂的关系：`try_preempt` 的 RQ 锁窗/`context_switch` 舞蹈
//! /`CURRENT` 原子读留 sched（纪律不变）——本面只钉判序；消费点
//! `kernel/src/dpg.rs` 供给唤醒点（线1d）的资格注释锚与本面判序一致性
//! 由测试向量冻结（sched `Fair::pick_next_if_above`（W3-Q93 终形）
//! 门控形的过滤谓词 = `berry_ringface::eligible` 单源，本面直引——零第二实现）。
//!
//! **供给临界段收口序**（同任务行）：dpg_pager 的 `supply_page` + magic
//! 写先于唤醒点（收口序：供给 → 唤醒 → 直切判定）——唤醒时页已可读，
//! 直切不过渡中状态。本面以 [`supply_done_before_wake`] 编号律钉板。
//!
//! **测试向量钉板纪律**：直切资格指纹（null 护栏/严格更高恰界/同层不切/
//! 冻结与限流位过滤/幻影出选防线零消耗语义）冻结进测试——改判序律而不
//! 改锚点 = 测试必红。

use berry_ringface::{
    eligible, TaskDesc, HELD_FROZEN, HELD_THROTTLE, RUNNING,
};

// ---------------------------------------------------------------------------
// CURRENT null 护栏（判序第一门——ktest 复盘雷 3）
// ---------------------------------------------------------------------------

/// 直切判序入参：当前任务投影 + 环快照（判据字段纯结构投影——W3-Q24
/// dpgface 同款纪律；`CURRENT` 原子读留 sched 物理臂）。
#[derive(Clone, Copy, Debug)]
pub struct PreemptCtx<'a> {
    /// 当前任务投影。`None` = `sched::init` 前 `CURRENT == null`（ktest
    /// 护栏：fastpath 在手搭睡眠者在队时被走到〔ktest 纪律明令禁止、但
    /// 仅注释执法〕，下行探峰解引用会直接挂死在 boot 期、难归因——null
    /// 早退把违约显形为「无直切」；正常运行期恒非 null）。
    pub cur: Option<TaskDesc>,
    /// 本核 runqueue 环快照（host 测任意序；sched 侧消费面 =
    /// `Fair::pick_next_if_above`（W3-Q93 终形）门控形单圈扫描的同一节点集
    /// ——04 号审计机会一②（2026-11-17）双圈合一：`has_eff_above_filtered`
    /// 环走查退役，判序并入出选单圈）。
    pub ring: &'a [TaskDesc],
    /// 环上各节点「是当前任务」旗（与 `ring` 等长；sched 侧 = 指针等值
    /// 判——`t as *mut Task == my_cur`；环上 RUNNING 且非 cur 者被
    /// `eligible` 过滤）。
    pub is_cur: &'a [bool],
}

/// 判序第一门：CURRENT null 早退（`sched::init` 前护栏）。
///
/// 与 sched 侧 `if cur.is_null() { return false; }` 逐字对齐——**先于**
/// 出选门控（解引用 `(*cur).prio_effective` 在后〔机会一②起 sched 物理面
/// = `Fair::pick_next_if_above`（W3-Q93 终形）——阈值 v 由调用方
/// `(*cur).prio_effective` 自读传入，null 时先经护栏早退，
/// 违约显形为无直切而非挂死〕）。
#[inline]
pub fn cur_null_early_out(cur: Option<TaskDesc>) -> bool {
    cur.is_none()
}

// ---------------------------------------------------------------------------
// 探峰（判序第二门——幻影出选防线）
// ---------------------------------------------------------------------------

/// 判序第二门：只读探峰——环上是否存在 `prio_effective` **严格高于**
/// `cur` 的 eligible 任务。
///
/// 与 sched `Fair::pick_next_if_above`（W3-Q93 终形）门控形对齐——04 号
/// 审计机会一②（2026-11-17）双圈合一：`Runqueue::has_eff_above_filtered`
/// 探峰退役后，本存在性判据的 sched 侧承载即门控形胜者 eff 严格更高律
/// （存在律在门控形由构造性成立——门过即有严格更高者）。空环恒 false；
/// `eligible` 过滤（Running-elsewhere/HELD_FROZEN/HELD_THROTTLE）
/// 叠加在严格更高比较前——被过滤者不参与竞争。**不动游标不做出选**
/// （纯只读一圈 O(n)）——不可能切走时零副作用返回，这是幻影出选防线
/// 的「探」半边。
#[inline]
pub fn has_eff_above(ctx: &PreemptCtx) -> bool {
    let Some(cur) = ctx.cur else { return false; };
    let mut i = 0usize;
    while i < ctx.ring.len() {
        // is_cur 短缺折 false（全非 cur 语义——sched 侧等长指针等值判
        // 的 host 缺省形；敌意快照零 panic同款值域闭合）。
        let is_cur = ctx.is_cur.get(i).copied().unwrap_or(false);
        if eligible(&ctx.ring[i], is_cur)
            && ctx.ring[i].prio_effective > cur.prio_effective
        {
            return true;
        }
        i += 1;
    }
    false
}

// ---------------------------------------------------------------------------
// 直切资格合判（判序第三门——严格更高才切）
// ---------------------------------------------------------------------------

/// 直切资格合判：三判序单源。
///
/// ①`cur_none` → false（护栏早退）；②无严格更高 eligible → false
/// （出选门控防线——不 pick 不消耗 head；机会一②（2026-11-17）起
/// sched 物理面 = `Fair::pick_next_if_above`（W3-Q93 终形）单圈
/// 带门出选）；③有严格更高
/// → true（§A43：同层 FIFO 不插队——**严格**更高才切，等值不切）。
///
/// sched `try_preempt` 的完整判序投影（出选/切换舞蹈留物理臂）：
/// `Fair::pick_next_if_above` 单圈扫描带门出选（`best_eff` 累加器门——
/// 门过即胜者带严格高于 v），过门后再经调用方守卫验 `next.prio_effective >
/// cur.prio_effective`——本面以 ② 的存在性判据等价钉板（门控集 ==
/// 出选集：S-3b 同 eligible 门，出选者必严格更高，守卫再验恒真——
/// 防线语义「出选后不切走不可能发生」由 ② 的不存在性短路保证）。
#[inline]
pub fn try_preempt_eligible(ctx: &PreemptCtx) -> bool {
    if cur_null_early_out(ctx.cur) {
        return false;
    }
    has_eff_above(ctx)
}

// ---------------------------------------------------------------------------
// 供给临界段收口序（线1d 消费面的另一半纪律）
// ---------------------------------------------------------------------------

/// 供给临界段步骤编号（dpg_pager 供给链的收口序——值域自解释）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupplyStep {
    /// `supply_page`（帧提交/建表）——临界段第一步。
    SupplyPage,
    /// magic 写（`vmo.write(off, DPG_MAGIC)`——经 VMO direct map；跨核
    /// 可见性 = 供给窗 dsb ish + tlbi/broadcast 在场〔supply_page 链〕）。
    MagicWrite,
    /// `wake_matching`（唤 faulter——入环）。
    Wake,
    /// `try_preempt`（直切判定——消费本面判序）。
    TryPreempt,
    /// `klock_exit`（供给窗收口）。
    KlockExit,
}

/// 供给临界段收口序律：`SupplyPage < MagicWrite < Wake < TryPreempt <
/// KlockExit`——**供给与 magic 写先于唤醒**（唤醒时页已可读：faulter
/// 醒后重试读必命中 magic，直切不渡供给中状态）；直切判定在唤醒后、
/// KL 窗收口前（KL 窗内切换合法面：klock_enter 即 svc_enter_class(ALL)
/// 记账 held 位，try_preempt 舞蹈按位放锁/切回重取——与 pager 睡
/// PAGER_KEY 的 block_current 同一舞蹈）。
///
/// 违约示例（host 钉板面）：Wake 先于 MagicWrite = faulter 醒后读到
/// 供给前页（零页）——DPG_MAGIC 断言红，供给链破损；TryPreempt 先于
/// Wake = 直切给旧环（被唤者未入环，探峰可能漏它——唤醒直切语义破）。
#[inline]
pub fn supply_done_before_wake(a: SupplyStep, b: SupplyStep) -> bool {
    rank(a) < rank(b)
}

#[inline]
fn rank(s: SupplyStep) -> u8 {
    match s {
        SupplyStep::SupplyPage => 0,
        SupplyStep::MagicWrite => 1,
        SupplyStep::Wake => 2,
        SupplyStep::TryPreempt => 3,
        SupplyStep::KlockExit => 4,
    }
}

/// 直切判据常量锚：等值不切（§A43 同层 FIFO 不插队——严格大于才切）。
///
/// sched 侧 `(*next_ptr).prio_effective > (*cur).prio_effective` 的纯值
/// 域镜像（`try_preempt` 窗内再验行）——host 钉板等值/恰高 1 两侧。
#[inline]
pub fn strictly_higher(next: i32, cur: i32) -> bool {
    next > cur
}

/// 冻结/限流位过滤镜（直切资格的 eligible 半边——`berry_ringface::
/// eligible` 单源直引，此处只钉「这两位在直切语境也生效」的消费面
/// 锚：HELD_FROZEN/HELD_THROTTLE 持有者不参与探峰竞争，即便更高带）。
///
/// 返回 true = 该环成员参与探峰竞争（还需严格更高才成立）。
#[inline]
pub fn ring_member_competes(desc: &TaskDesc, is_cur: bool) -> bool {
    eligible(desc, is_cur)
}

// ---------------------------------------------------------------------------
// 常量冻结锚（sched 侧判序纪律的纯面镜像——漂移即测试红）
// ---------------------------------------------------------------------------

/// `HELD_FROZEN` 位值（ringface 单源 re-export——sched 侧同值）。
pub const HELD_FROZEN_BIT: u8 = HELD_FROZEN;
/// `HELD_THROTTLE` 位值（同上）。
pub const HELD_THROTTLE_BIT: u8 = HELD_THROTTLE;
/// `RUNNING` 状态值（同上）。
pub const RUNNING_STATE: u8 = RUNNING;

#[cfg(test)]
mod tests;
