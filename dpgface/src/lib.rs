//! berry-dpgface —— demand paging EL0 槽池认领制判定律纯面（W3-Q24）。
//!
//! **动机**：`kernel/src/dpg.rs` 的 EL0 请求面是 K=32 槽认领制——
//! `probe_el0` 扫池取空闲槽武装（池满睡 `EOVERFLOW_KEY` 溢出 convoy，
//! 放槽即唤重扫）；`el0_take_for(vd)` 属主过滤取（错配窗让位不消费）+
//! 孤儿槽 GC 臂（消费 + 定向唤 + 放槽）+ `any_armed` 三态返回
//! （Supplied/NoneForVd/PoolEmpty）；探针入口的 fsc 滤形（DFSC =
//! ESR[5:0] ∈ 4..=7 才是翻译缺页）——这些判定律此前只活在 kernel
//! crate 里（ktest E3d/E3e/E3f 只覆盖六条路径），交叉面（池满后放槽
//! 重扫的收敛性、孤儿 GC 不置 any_armed 的池态如实律、fsc 位域边界
//! 3/8 恰拒）的表驱动回归面需要一张 host 可测的**钉板**。本 crate 把
//! 「给定池快照 + 属主表 + DFSC/esr 输入的纯判定」按值域抽成纯函数
//! + 表驱动测试（先例族第 17 件，形态抄 berry-bandface/berry-utrace
//! ——原子槽机械与 wake 调用留 kernel，判定律单源外迁）。
//!
//! 抽离面（与消费侧逐字对齐，改这里的语义 = 改消费侧行为）：
//! - [`DFSC_TRANSLATION_MIN`]/[`DFSC_TRANSLATION_MAX`]/[`fsc_is_translation`]:
//!   fsc 滤形（probe/probe_el0 同款 `matches!(esr & 0x3f, 4..=7)`）。
//! - [`SlotPool`]:K 槽池纯结构快照（`armed`/`far`/`asid` 数组镜像
//!   `EREQ` 池）——认领/消费/放槽的操作由 kernel 原子机械执行，本面
//!   只做「扫描序 + 判定」的纯投影。
//! - [`claim_free_slot`]:空闲槽选择（最低索引空闲 armed=false 槽）；
//!   None = 池满 → 溢出 convoy 路由（睡 `EOVERFLOW_KEY`）。
//! - [`OwnerResolution`]/[`take_for`]:`el0_take_for` 的属主过滤判定表
//!   ——属主匹配取最低索引（Supplied）；有武装槽但无一匹配
//!   （NoneForVd）；全池空（PoolEmpty）；孤儿槽（解析失败）= GC 消费
//!   臂（消费 + 放槽 + 续扫，**不置 any_armed**——消费后池态如实）。
//!
//! 与 kernel 机械面的关系：`EREQ` 原子池、`block_current`/
//! `wake_matching` 调用、KL 窗舞蹈留 kernel（纪律不变）——本面只钉
//! 「这池快照 + 这个属主，走哪条臂、消费哪个槽」的判定律。
//!
//! **测试向量钉板纪律**：池序锚点（满装 K 臂全属 vd → 首收最低位 0；
//! 消费即放槽 → 复臂回收同号——ktest E3f ①② 在 host 复现）与 GC 锚点
//! （孤儿臂消费后 any_armed 不残留 → PoolEmpty——ktest E3e ③ 同律）是
//! 把认领制指纹冻结进测试；改判定律而不改锚点 = 测试必红。

#![cfg_attr(not(test), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]

// ---------------------------------------------------------------------------
// fsc 滤形（probe / probe_el0 入口同款——DFSC = ISS[5:0] = ESR[5:0]）
// ---------------------------------------------------------------------------

/// DFSC 翻译缺页档下界（L3）：4 = level 3 translation fault。
pub const DFSC_TRANSLATION_MIN: u32 = 4;
/// DFSC 翻译缺页档上界（L0）：7 = level 0 translation fault。
pub const DFSC_TRANSLATION_MAX: u32 = 7;
/// DFSC 提取掩码（ISS 低 6 位直达，勿移位——ESR[5:0]）。
pub const DFSC_MASK: usize = 0x3f;

/// fsc 滤形判定（三镜 mech-M-2：只收翻译缺页 4..=7——权限/访问标志
/// fault 走原杀链，否则 armed→供给幂等短路→回 block = 活锁株）。
///
/// 与 kernel 侧 `matches!(esr & 0x3f, 4..=7)` 逐字对齐（消费点：
/// `probe` / `probe_el0` 入口——非翻译缺页 return false 原杀链）。
#[inline]
pub fn fsc_is_translation(esr: usize) -> bool {
    let dfsc = (esr & DFSC_MASK) as u32;
    (DFSC_TRANSLATION_MIN..=DFSC_TRANSLATION_MAX).contains(&dfsc)
}

// ---------------------------------------------------------------------------
// 槽池纯结构快照（镜像 kernel `EREQ: [EReqSlot; EREQ_SLOTS]`）
// ---------------------------------------------------------------------------

/// 槽池快照——kernel 侧 `EREQ` 池的纯投影（armed/far/asid 三元组）。
///
/// 认领（store + Release）/消费（swap(false, Acquire)）的原子机械由
/// kernel 执行；本结构只承载「判定所需的池状态」，操作后由 kernel
/// 侧同步快照（或测试直接构造）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotPool {
    /// 每槽武装位（`EREQ[i].armed` 镜像）。
    pub armed: [bool; SLOTS],
    /// 每槽故障地址（`EREQ[i].far` 镜像）。
    pub far: [u64; SLOTS],
    /// 每槽地址空间标识（`EREQ[i].asid` 镜像）。
    pub asid: [u16; SLOTS],
}

/// 槽池容量（与 kernel `EREQ_SLOTS` / 用户态 svcmgr `PAGER_SLOTS`
/// 三源镜像——X-6 冻结 N=32；值冻结面在 kernel abi_gen，本处单源
/// 供测试锚定）。
pub const SLOTS: usize = 32;

impl SlotPool {
    /// 全空池（boot 态——ktest 前置窗池零占用）。
    pub const fn empty() -> Self {
        SlotPool { armed: [false; SLOTS], far: [0; SLOTS], asid: [0; SLOTS] }
    }

    /// 武装单槽（认领直路径——`probe_el0` 认领环一次迭代的纯投影：
    /// far/asid 先行、armed 收口）。
    pub fn arm(&mut self, slot: usize, far: usize, asid: u16) {
        assert!(slot < SLOTS, "dpgface: 槽号越界（{} ≥ K={}）", slot, SLOTS);
        self.far[slot] = far as u64;
        self.asid[slot] = asid;
        self.armed[slot] = true;
    }

    /// 消费单槽（`swap(false, Acquire)` 的纯投影——返回消费前是否
    /// 真在武装态；false = KL 契约破的防御跳过，零半态）。
    pub fn disarm(&mut self, slot: usize) -> bool {
        assert!(slot < SLOTS, "dpgface: 槽号越界（{} ≥ K={}）", slot, SLOTS);
        let was = self.armed[slot];
        self.armed[slot] = false;
        was
    }

    /// 武装槽计数（观测面——溢出 convoy 有界性 `≤ N-K` 的分母）。
    pub fn armed_count(&self) -> usize {
        self.armed.iter().filter(|&&a| a).count()
    }
}

// ---------------------------------------------------------------------------
// 空闲槽认领（probe_el0 认领环 / ktest_arm_el0 同律）
// ---------------------------------------------------------------------------

/// 空闲槽选择结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claim {
    /// 认领到空闲槽（最低索引 armed=false 槽——`find` 扫描序不变量）。
    Free { slot: usize },
    /// 池满（无空闲槽）→ 溢出 convoy 路由：睡 `EOVERFLOW_KEY`，take
    /// 侧消费放槽即唤、醒后重扫续环（poll-first 同构）。
    PoolFull,
}

/// 空闲槽选择（认领制扫描序：`0..K` 首个 `!armed` 槽——kernel 侧
/// `(0..EREQ_SLOTS).find(|i| !EREQ[*i].armed)` 逐字对齐）。
///
/// 池满 = `PoolFull` → 溢出 convoy（`DPG ovfl` 观测面 + 睡溢出键 +
/// take 消费放槽唤醒 + 醒后重扫——N>K 时 convoy ≤ N-K 有界，N≤K 恒零）。
#[inline]
pub fn claim_free_slot(pool: &SlotPool) -> Claim {
    match (0..SLOTS).find(|&i| !pool.armed[i]) {
        Some(slot) => Claim::Free { slot },
        None => Claim::PoolFull,
    }
}

// ---------------------------------------------------------------------------
// 属主过滤取（el0_take_for 判定表——X-5 错配消耗舞修刀）
// ---------------------------------------------------------------------------

/// 槽属主解析结果（kernel 侧 `vmar::demand_root(asid)` →
/// `demand_region_at(far)` → rvmo 与 vd 比对的纯投影——指针域以 id
/// 代表，None = 解析失败〔孤儿〕）。
pub type OwnerId = u64;

/// `take_for` 单槽判定输入（kernel 扫描环一次迭代）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotView {
    pub far: usize,
    pub asid: u16,
}

/// 单槽属主解析（纯回调面——kernel 侧经 demand_root/region_at 双跳
/// 查得 rvmo；本面以 `resolve(far, asid) -> Option<OwnerId>` 注入，
/// None = demand_root miss 或区间 miss = 孤儿槽）。
pub type OwnerResolve = dyn Fn(usize, u16) -> Option<OwnerId>;

/// 属主过滤取结果（kernel `El0Take` 三态镜像）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TakeFor {
    /// 属主匹配槽已消费（`(far, asid, slot)`）——供给毕定向唤本槽。
    Supplied { far: usize, asid: u16, slot: usize },
    /// 池有武装槽但无一属主匹配 `vd`——零消费零唤醒（错配窗让位）。
    NoneForVd,
    /// 全池空（无武装槽）——pager 睡 EL0 pager 键。
    PoolEmpty,
}

/// 属主过滤取判定表（kernel `el0_take_for` 的纯投影）。
///
/// 扫 `0..K`：对每个武装槽解析属主——
/// - **匹配 vd** → 消费该槽（最低索引优先），返回 [`TakeFor::Supplied`]；
/// - **合法他窗**（属主在册且非本 vd）→ 跳过（让位面，置 any_armed）；
/// - **孤儿槽**（解析失败）→ GC 臂：消费 + 定向唤本槽 faulter + 放槽
///   唤溢出 convoy + 续扫——**不置 any_armed**（消费后池态如实：无他
///   臂则 PoolEmpty）；
/// - 扫毕无匹配：any_armed=true → [`TakeFor::NoneForVd`]，否则
///   [`TakeFor::PoolEmpty`]。
///
/// 返回 `(判定, 消费槽数)`——GC 消费数 = 孤儿臂活证（kernel 侧散在
/// 循环里，此处聚合计数供测试断言「GC 消费后池态如实」）。
pub fn take_for(
    pool: &mut SlotPool,
    vd: OwnerId,
    resolve: &OwnerResolve,
) -> (TakeFor, usize) {
    let mut any_armed = false;
    let mut gc_consumed = 0;
    for i in 0..SLOTS {
        if !pool.armed[i] {
            continue;
        }
        let far = pool.far[i] as usize;
        let asid = pool.asid[i];
        match resolve(far, asid) {
            // 合法他窗（属主在册且非本 vd）——让位面：跳过不消费。
            Some(o) if o != vd => {
                any_armed = true;
                continue;
            }
            // 孤儿槽（GC 臂）：消费 + 唤本槽 + 放槽唤溢出 + 续扫——
            // 不置 any_armed（消费后池态如实）。
            None => {
                if pool.disarm(i) {
                    gc_consumed += 1;
                }
                continue;
            }
            // 属主匹配（Some(vd)）——落出 match 走下方消费。
            Some(_) => {}
        }
        // 消费（latch 收口——false 臂 = KL 契约破的防御跳过）。
        if !pool.disarm(i) {
            continue;
        }
        return (TakeFor::Supplied { far, asid, slot: i }, gc_consumed);
    }
    if any_armed {
        (TakeFor::NoneForVd, gc_consumed)
    } else {
        (TakeFor::PoolEmpty, gc_consumed)
    }
}

// ---------------------------------------------------------------------------
// 溢出 convoy 路由判定（池满 faulter 的键选择——faulter[i] 定向键 vs
// EOVERFLOW_KEY 溢出键分流）
// ---------------------------------------------------------------------------

/// faulter 睡键选择（认领路径 vs 溢出路径分流）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaulterKey {
    /// 认领成功——睡本槽定向键 `EFAULT_KEYS[slot]`（0x21 供给毕**定向**
    /// 唤醒，N-1 广播错醒清零）。
    Slot { slot: usize },
    /// 池满溢出——睡 `EOVERFLOW_KEY`（take 侧消费放槽即唤，醒后重扫；
    /// convoy N>K 时 ≤N-K 有界）。
    Overflow,
}

/// 溢出 convoy 键路由（`probe_el0` 认领环出口的纯投影：认领成 → 本槽
/// 定向键；池满 → 溢出键——「faulter[i] 定向 vs EOVERFLOW_KEY 溢出键
/// 分流」的判定律单源）。
#[inline]
pub fn faulter_key_route(claim: Claim) -> FaulterKey {
    match claim {
        Claim::Free { slot } => FaulterKey::Slot { slot },
        Claim::PoolFull => FaulterKey::Overflow,
    }
}

/// W3-Q59：dpg P0-1 fastpath 唤醒直切判序纯面（P0-1 线1d 消费面——
/// `dpg_pager` 供给毕唤 faulter 点的 `try_preempt` 直切资格 + 供给
/// 临界段收口序 host 钉板；sched 物理臂/RQ 锁窗/切换舞蹈留壳）。
pub mod fastpath;
pub mod tblwin;
