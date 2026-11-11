//! band 预算包络（v1.2 `调点调度-v1.2.md` §7 / ADR #42 第二段）——S5-c
//! 逐任务 deadline 预算的**带级**同构最小面。
//!
//! 已裁机制清单的最小子集：静态 8 槽 [`BandBudget`] 表 + **整数微记账**
//! （QNX 三件套之一——CNTVCT 周期数整数累加，无小数无换算堆）+ **事件驱动
//! 周期翻转**（[`period_check`] 对 CNTVCT 一比较，消费点顺带调——绝不复活
//! 周期 tick）。带键 = intent（§11「band 不加 attr 字段（intent 即带键）」
//! ）：任务侧读 `intent_effective`（§3.6 调度决策一律看 effective），进程
//! 侧 `Process.band_id` 记账（kernel crate——sched 不反向依赖内核，同
//! `proc_tag` 的 crate 边界纪律）。
//!
//! **默认全关**：槽初值 quota/period = 0 = 该带不设包络——`consume` 恒
//! false、`throttle_all` 零扫描，与接口缺席逐位等价（v1.2 §7 入场券〔v1
//! 遥测实证 + loadgen + 尾延迟断言〕未过前 band 执法不并线；本刀 = 机制
//! 先行，执法开关在启动配置 [`configure`]，trait 签名零改动）。
//!
//! **SMP 安全账面**（v1.3 §5 预研建议③ / `调度器SMP部署设计-v1.0.md` §3
//! 「SMP band 计账」）：热路径两热字——consumed/deadline——自 S6-k 单核
//! 形态升**全局原子账**（[`CONSUMED`]/[`DEADLINES`] 两 `[AtomicU64]` 数组
//! ，BANDS 槽内同名字段退化为 [`state`] 快照镜像）：消费 [`consume`]
//! fetch_add（Relaxed——纯计数不通信），首越线判定用返回值（单赢家——
//! 多核同挤也恰一次 true）；翻期 [`period_check`] deadline CAS 防双翻（
//! 只有翻中旧值的核兑现清账）。单核 fetch_add/CAS 首试即成 = 行为逐位
//! 零变化；BANDS 冷配置（quota/period/throttled 镜像）仍 `static mut`
//! 屏蔽窗纪律（表已入册 `tools/writable_globals.txt`，同款例外先例 =
//! policy.rs 三件套①：SHD_WHERE 原子化不拆 SNAPS 的 static mut 面）。
//!
//! **与 S5-c 逐任务预算分立**：deadline 任务（`prio_effective ≥
//! PRIO_DEADLINE`）被 RT/deadline 屏蔽不变量豁免（§10）——本模块环扫均
//! 跳过之；其 [`HELD_THROTTLE`] 位只归 S5-c 逐任务执法（`class.rs::tick`
//! 置 / `lazy_unthrottle_deadlines` 清），带级执法永不触碰。fair 任务的
//! THROTTLE 位同理只归本模块（双位分立纪律的位内版本：一消费者一位）。
//!
//! W3-Q19：预算数学判定律（首越线单赢家/翻期追赶环/CAS 防双翻入环门/
//! 配置三态/带成员豁免判）单源已迁 `berry-bandface`（host 可测钉板）；
//! 本模块保留 BANDS static mut 槽镜像 + CONSUMED/DEADLINES 原子账 +
//! throttle/unthrottle 环扫裸并发机械，判定面消费 bandface 纯函数
//! （零第二实现——`berry-emface` 同款纪律）。

use core::sync::atomic::{AtomicU64, Ordering};

use crate::runqueue::Runqueue;
use crate::task::{Task, HELD_THROTTLE};

// ---- 槽形与判定律（单源 = berry-bandface；此处 re-export 供
// kernel/ktest 既有消费面零改动）----

pub use berry_bandface::{
    armed, band_member, configure_slot, consumed_crossed, deadline_next, period_due, slot_ok,
    BandBudget, BAND_SLOTS,
};

/// band 静态表（8 槽）——冷配置 + 观测镜像。G0 单核静态例外——与
/// `class.rs::LAST_VCT` / `policy.rs::SNAPS` 同款纪律：`static mut` 写面只
/// 经 `&raw mut` 取裸指针（不立引用），全部调用点在 IRQ 屏蔽/调度窗内
/// （tick / pick_next 语境）单核顺序访问；表已入册
/// `tools/writable_globals.txt`（scheduler 组）。热路径两热字
/// consumed/deadline 已升全局原子账（[`CONSUMED`]/[`DEADLINES`]）——槽内
/// 同名字段只是 [`configure`] 写入的初始镜像，[`state`] 合并点以原子账
/// 为准覆盖读；槽内仍具语义的字段 = quota/period（冷配置，configure 启动
/// 面单点写）+ throttled（观测镜像位，首越线赢家置 / 翻期赢家清——SMP
/// lazy-pick 执法刀（SMP §3）再升 AtomicBool）。
static mut BANDS: [BandBudget; BAND_SLOTS] = [const { BandBudget::OFF }; BAND_SLOTS];

/// band 全局原子账·consumed（SMP §3「SMP band 计账」/ v1.3 §5 预研建议③
/// ）：跨核一份原子计数——消费点 fetch_add 累计（Relaxed——纯计数不
/// 通信），首越线判定用返回值（单赢家：`prev < quota ≤ prev+cycles` 的
/// 那次 fetch_add 全局恰一次）。翻期清账 = [`period_check`] CAS 赢家单点
/// store(0)。真账在本数组；[`BandBudget::consumed`] 只是快照镜像。
static CONSUMED: [AtomicU64; BAND_SLOTS] = [const { AtomicU64::new(0) }; BAND_SLOTS];

/// band 全局原子账·deadline（同上刀）：事件驱动翻期的 CAS 面——只有把
/// deadline 从旧值翻到新值的核才兑现清账（防双翻）。真账在本数组；
/// [`BandBudget::deadline`] 只是快照镜像。非 `static mut`——原子内部可
/// 变性免入 `tools/writable_globals.txt`（同 policy.rs `ACTIVE`/`HAZARD`
/// 先例）。
static DEADLINES: [AtomicU64; BAND_SLOTS] = [const { AtomicU64::new(0) }; BAND_SLOTS];

/// 取带槽裸指针（band 越界 → None）。界门单源 `berry_bandface::slot_ok`。
///
/// # Safety（解引用窗，各访问点挂）
/// 调用点须在 IRQ 屏蔽/调度窗内（单核独占写，同 `LAST_VCT`）。
#[inline]
fn slot(band: u8) -> Option<*mut BandBudget> {
    if !slot_ok(band) {
        return None;
    }
    // SAFETY: 声明点取裸指针不立引用（E0133 解除的就是这一层，同 policy.rs
    // SNAPS）；不解引用、无别名窗问题。
    Some(unsafe { &raw mut BANDS[band as usize] })
}

/// 带成员桥接：读 Task 的 intent/prio effective 后交纯面判（判定律单源
/// `berry_bandface::band_member`——intent 即带键 §11 + deadline 豁免 §10）。
#[inline]
fn task_member(t: *const Task, band: u8) -> bool {
    // SAFETY: 环上任务均调度器拥有；调用侧屏蔽窗内（同 pick）。
    unsafe { band_member((*t).intent_effective, (*t).prio_effective, band) }
}

/// 配置某带包络（启动配置面——§7「数字进启动配置」；quota/period **均正**
/// 才开执法，任一 0 = 关）。`now` = 当前 CNTVCT（首期起点）。
///
/// 返回是否已武装。重配即清账翻期（本期已耗归零）——mid-flight 翻配置的
/// 收口义务：**先 `unthrottle_all` 清环上残留限流位，再 configure(0)**。
/// 原子账（[`CONSUMED`]/[`DEADLINES`]）与槽镜像同批重臂/归零——[`state`]
/// 合并读的逐位等价义务（关态 = `BandBudget::OFF` 逐字段成立）。判序
/// 单源 `berry_bandface::configure_slot`（三态）。
pub fn configure(band: u8, quota: u64, period: u64, now: u64) -> bool {
    let Some(out) = configure_slot(band, quota, period, now) else { return false };
    let Some(s) = slot(band) else { return false }; // 不可达（configure_slot 已界检）
    let i = band as usize;
    let deadline = match out {
        berry_bandface::ConfigOut::Armed { deadline } => deadline,
        berry_bandface::ConfigOut::Off => 0,
    };
    // SAFETY: 屏蔽窗内单核独占写（slot 契约）。
    unsafe {
        *s = if let berry_bandface::ConfigOut::Armed { .. } = out {
            BandBudget { quota, period, consumed: 0, deadline, throttled: false }
        } else {
            BandBudget::OFF
        }
    }
    CONSUMED[i].store(0, Ordering::Relaxed);
    DEADLINES[i].store(deadline, Ordering::Relaxed);
    matches!(out, berry_bandface::ConfigOut::Armed { .. })
}

/// 该带包络是否在执法（quota/period 均正且带在表内）。判据单源
/// `berry_bandface::armed`。
pub fn band_on(band: u8) -> bool {
    let Some(s) = slot(band) else { return false };
    // SAFETY: 只读；屏蔽窗内无并发写（slot 契约）。
    unsafe { armed(&*s) }
}

/// 槽快照拷出（ktest / telemetry 观测面；越界 → None）。**真账合并读**：
/// consumed/deadline 取自全局原子账（槽内同名字段已退化为镜像）。读序 = deadline Acquire 先行、consumed Relaxed 后随——与 [`period_check`] 翻
/// 期「清账先行 + deadline CAS(Release)」配对：见新期 deadline 必见新账
/// （SMP §3）。
pub fn state(band: u8) -> Option<BandBudget> {
    let s = slot(band)?;
    let i = band as usize;
    // SAFETY: 只读拷贝（quota/period/throttled 镜像）；屏蔽窗内无并发写
    // （slot 契约）。
    let mut snap = unsafe { *s };
    snap.deadline = DEADLINES[i].load(Ordering::Acquire);
    snap.consumed = CONSUMED[i].load(Ordering::Relaxed);
    Some(snap)
}

/// 微记账：把 `cycles` 周期数记到 `band` 本期账上（tick 记账点按 CNTVCT
/// 增量调——同 `Fair::tick` 的 delta 口径）。返回 true = 本期预算**首**
/// 越线（调用方随后 [`throttle_all`]——记账与环执法分两步，热路径可合并）。
/// 包络关 / 带越界 → false 不记账。
///
/// SMP 安全（SMP §3 / v1.3 §5 预研建议③）：全局原子账 [`CONSUMED`]
/// fetch_add（Relaxed——纯计数不通信，同 HDR 桶计数口径）累计；首越线
/// 判定用返回值——`prev < quota ≤ prev + cycles` 的那次 fetch_add 全局恰
/// 一次（**单赢家**：多核同挤越线也只有一个 true，无环扫无标志位竞争）
/// 。判定律单源 `berry_bandface::consumed_crossed`。u64 环绕（旧
/// saturating 语义的极端差）= 单期累计 2^64 CNTVCT 周期（@1GHz ≈ 584
/// 年）——不可达，不设防。
pub fn consume(band: u8, cycles: u64) -> bool {
    let Some(s) = slot(band) else { return false };
    // SAFETY: quota/period 冷配置只读（configure 启动面写，slot 契约）。
    let (quota, period) = unsafe { ((*s).quota, (*s).period) };
    if quota == 0 || period == 0 {
        return false;
    }
    let prev = CONSUMED[band as usize].fetch_add(cycles, Ordering::Relaxed);
    let crossed = consumed_crossed(band, unsafe { &*s }, cycles, prev);
    // 不可达 None：上面已界检 + 包络门（consumed_crossed 同判重入防御）。
    let crossed = crossed.unwrap_or(false);
    if crossed {
        // SAFETY: throttled = 观测镜像位，首越线赢家单点置位（period_check
        // 翻期赢家单点清——两写点均在屏蔽窗契约内，slot 契约同上）。
        unsafe { (*s).throttled = true }
    }
    crossed
}

/// 事件驱动周期翻转：`now ≥ deadline` 即翻期（tick / pick_next 顺带调——
/// 零额外定时器，同 S5-c lazy unthrottle 的消费点形态）。翻期 = 清账 +
/// 清限流标记 + 重臂 deadline（+= period；长眠一次跨多期时饱和步进追赶）。
///
/// 返回 true = 已翻期（调用方随后 [`unthrottle_all`]）。包络关 / 未到期 /
/// 带越界 → false。
///
/// SMP 防双翻（SMP §3 / v1.3 §5 预研建议③）：翻期 = [`DEADLINES`]
/// deadline CAS **单点**——只有把 deadline 从旧值 `cur` 翻到新值 `next`
/// 的那个核才兑现翻期（多核同到期也恰一个 true；输家 CAS 失败重读，
/// 多半已见他核翻过的未来期 → false）。追赶环与入环门单源
/// `berry_bandface::deadline_next`/`period_due`。清账（consumed 归零 +
/// throttled 镜像清）先行、CAS(Release) 发布——Acquire 侧（翻期判定点 /
/// [`state`] 快照）见新期 deadline 必见新账。清账-翻序间他核 fetch_add
/// 的落账归属 = 翻期边界的固有竞态（与消费-翻期同窗，上界每核一份
/// delta——超卖窗断言归 BAND-S 相，本刀不执法）。
pub fn period_check(band: u8, now: u64) -> bool {
    let Some(s) = slot(band) else { return false };
    let i = band as usize;
    // SAFETY: quota/period 冷配置只读（configure 启动面写，slot 契约）。
    let (quota, period) = unsafe { ((*s).quota, (*s).period) };
    if quota == 0 || period == 0 {
        return false;
    }
    // 翻期判定点 Acquire：与翻期 CAS(Release) 配对（SMP §3）。
    let mut cur = DEADLINES[i].load(Ordering::Acquire);
    loop {
        if !period_due(band, unsafe { &*s }, cur, now).unwrap_or(false) {
            return false; // 未到期（含他核刚翻过的未来期）；不可达 None 同上
        }
        // 追赶环：饱和步进（deadline 钉 u64::MAX 即出——防死循环）。
        let next = deadline_next(cur, period, now);
        // 翻期赢家义务先行（清账 + 清限流镜像）——下述 CAS(Release) 发布。
        // 输家同走这段无碍：CAS 失败即真赢家存在且同在清账（同窗幂等归零
        // ，账面终态一致）。
        CONSUMED[i].store(0, Ordering::Relaxed);
        // SAFETY: throttled 观测镜像位，翻期赢家清（首越线赢家置——两写
        // 点均在屏蔽窗契约内，slot 契约同上）。
        unsafe { (*s).throttled = false }
        // CAS 防双翻：只有把 deadline 从 cur 翻到 next 的核才算赢家。
        // 成功 Release = 发布先行清账；失败 Acquire = 他核已翻——同步纳入
        // 其发布序，重读再判。
        match DEADLINES[i].compare_exchange(cur, next, Ordering::Release, Ordering::Acquire) {
            Ok(_) => return true,
            Err(actual) => cur = actual,
        }
    }
}

/// 带级限流：扫环上带成员（intent_effective == band 的 fair 任务）置
/// [`HELD_THROTTLE`]（eligible 过滤 → 不再出选，同 S5-c）。幂等（已置位
/// 不重复计数）。deadline 任务豁免（[`band_member`]）；FROZEN 位不碰
/// （进程冻结独占语义）。返回本次置位数。
///
/// # Safety 契约（调用方）
/// rq 环节点有效且在环；屏蔽窗内调用（tick / pick_next 语境——同
/// `lazy_unthrottle_deadlines`）。包络关 / 带越界 → 0 零扫描。
pub fn throttle_all(band: u8, rq: &Runqueue) -> usize {
    if !band_on(band) {
        return 0;
    }
    let mut count = 0;
    let head = rq.head();
    let mut cur = head;
    while let Some(c) = cur {
        let t = c.as_ptr();
        // SAFETY: 环上任务均调度器拥有且在环；调用侧屏蔽窗内（同 pick）。
        unsafe {
            // §A136：先访当前节点，再推进并判回 head。
            if task_member(t, band) && (*t).held_locks & HELD_THROTTLE == 0 {
                (*t).held_locks |= HELD_THROTTLE;
                count += 1;
            }
            cur = (*t).rq_next.get();
        }
        if cur == head {
            break; // 环终止（尾节点处理后才判——§A136）
        }
    }
    count
}

/// 带级解限流：扫环上带成员清 [`HELD_THROTTLE`]（**只清本位**——FROZEN
/// 是 SuspendToken close 独占解除语义，同 S5-c 惰性解冻的单位纪律）。
/// 翻期（[`period_check`] 返 true）后调；也可独立调（关包络前清残留）。
/// 返回本次清位数。
///
/// # Safety 契约（调用方）
/// 同 [`throttle_all`]（环节点有效、屏蔽窗内；带越界 → 0）。
pub fn unthrottle_all(band: u8, rq: &Runqueue) -> usize {
    let mut count = 0;
    let head = rq.head();
    let mut cur = head;
    while let Some(c) = cur {
        let t = c.as_ptr();
        // SAFETY: 环上任务均调度器拥有且在环；调用侧屏蔽窗内（同 pick）。
        unsafe {
            // §A136：先访当前节点，再推进并判回 head。
            if task_member(t, band) && (*t).held_locks & HELD_THROTTLE != 0 {
                (*t).held_locks &= !HELD_THROTTLE;
                count += 1;
            }
            cur = (*t).rq_next.get();
        }
        if cur == head {
            break; // 环终止（尾节点处理后才判——§A136）
        }
    }
    count
}
