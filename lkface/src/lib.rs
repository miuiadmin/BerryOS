//! berry-lkface——lockdep 检查面纯函数（W3-Q36 host 钉板）。
//!
//! 单源面 = G3-3 锁序检测器（`arch/aarch64/src/lockdep.rs`）的**纯判定律**：
//! - 七类全序 rank 表（S-3 §9.7 立法：SQ > 队列 > REAPQ > 表 > 分配器 >
//!   ASID > 打印，KL 最内/末取；RQ0/RQ1 分立类号同 rank）；
//! - [`check_acquire`] 三检判序（重入 → 先验序 → RQ 升序专项）；
//! - [`check_release`] 两臂（地板下放锁 underflow / 栈顶非本类 unpair）；
//! - [`check_irq_exit`] 配平判（深度恰归地板）。
//!
//! 消费侧：`berry-hal-aarch64::lockdep` re-export（类号/rank/检查面单源）；
//! 物理臂（链槽寻址/记账/violation 停机/selftest）留 HAL 壳——本 crate 零
//! unsafe、零原子、零裸指针：检查入参全是值拷贝（chain 以 `&[u8]` 切片视图
//! 传入，宿主侧直接造数组钉板）。
//!
//! 判序合同（与 HAL 侧逐字等价——`check_acquire` 检查域 = `chain[floor..depth]`，
//! 前置 F5 类号域校验先于环扫，链深帽先行）：
//!
//! ```text
//! check_acquire(chain, depth, floor, class):
//!   ① depth ≥ DEPTH_MAX          → Violation::ChainOverflow
//!   ② class == 0 ∨ class > MAX   → Violation::UnknownClass   （F5 前置）
//!   ③ ∀i ∈ [floor, depth):
//!        held == class           → Violation::Reentry          （同类二取）
//!        rank(held) > rank(class)→ Violation::Order            （先验序）
//!        class==RQ0 ∧ held==RQ1 → Violation::RqDescend       （升序专项）
//!   ④ 全过                        → Ok
//! ```
#![cfg_attr(not(test), no_std)]

/// SLEEPQ 锁类（最外层 rank 1）。
pub const CLASS_SQ: u8 = 1;
/// CPU0 就绪队列锁类（与 RQ1 同 rank=2——分立类号先例）。
pub const CLASS_RQ0: u8 = 2;
/// CPU1 就绪队列锁类。
pub const CLASS_RQ1: u8 = 3;
/// REAPQ 锁类（rank 3）。
pub const CLASS_REAPQ: u8 = 4;
/// 表类（KL——KERN_LOCK，类内末取；rank 4）。
pub const CLASS_TABLE: u8 = 5;
/// 分配器锁类（**预留**：帧分配器纯 KL 串行——无实例；rank 5）。
pub const CLASS_ALLOC: u8 = 6;
/// ASID 锁类（rank 6）。
pub const CLASS_ASID: u8 = 7;
/// 打印锁类（最内层 rank 7——「持 PRINT_LOCK 者禁取任何锁」由 rank 检执法）。
pub const CLASS_PRINT: u8 = 8;
/// PT 页表锁类（与 TABLE 同 rank=4 的分立类号；类号是标签非 rank）。
pub const CLASS_PT: u8 = 9;
/// 类号域上界（1..=CLASS_MAX 在册；F5 前置校验用）。
pub const CLASS_MAX: u8 = CLASS_PT;
/// 链深帽（g3-3c 抬 8→12：PT/ASID 入检测域 + 未来类扩展裕量）。
pub const DEPTH_MAX: usize = 12;

/// 类号 → 立法 rank（1 最外 … 7 最内；S-3 §9.7 七类全序）。
///
/// RQ0/RQ1 同 rank=2（跨取合法，升降序由 [`check_acquire`] 专项检）；
/// TABLE/PT 同 rank=4（层内无序，双向跨取合法）。未知类号 = 腐写兜底
/// （入检侧 F5 已拦，检查面值域恒合法——本臂对合法流不可达）。
#[inline]
pub const fn rank(class: u8) -> u8 {
    match class {
        CLASS_SQ => 1,
        CLASS_RQ0 | CLASS_RQ1 => 2,
        CLASS_REAPQ => 3,
        CLASS_TABLE | CLASS_PT => 4,
        CLASS_ALLOC => 5,
        CLASS_ASID => 6,
        CLASS_PRINT => 7,
        _ => 7,
    }
}

/// 违约定名（与 HAL violation 报告的 kind 字节串一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Violation {
    /// 链深帽越界（`b"chain-overflow"`）。
    ChainOverflow,
    /// 类号域外（`b"unknown-class"`——F5 前置）。
    UnknownClass,
    /// 地板上同类二取（`b"reentry"`——非重入 TTAS 同实例自死锁形）。
    Reentry,
    /// 先验序违约（`b"order"`——已持类 rank > 新类 rank）。
    Order,
    /// RQ 降序（`b"rq-descend"`——持 RQ1 再取 RQ0）。
    RqDescend,
    /// 地板下放锁（`b"release-underflow"`——越流放锁/配平早已破）。
    ReleaseUnderflow,
    /// 栈顶非本类（`b"release-unpair"`——放错对/序错）。
    ReleaseUnpair,
    /// IRQ 出口不配平（`b"irq-unbalanced"`——handler 留残跨流泄漏）。
    IrqUnbalanced,
}

impl Violation {
    /// HAL violation 报告的 kind 字节串（定名单源——报告面与检查面同词）。
    #[inline]
    pub const fn as_bytes(self) -> &'static [u8] {
        match self {
            Violation::ChainOverflow => b"chain-overflow",
            Violation::UnknownClass => b"unknown-class",
            Violation::Reentry => b"reentry",
            Violation::Order => b"order",
            Violation::RqDescend => b"rq-descend",
            Violation::ReleaseUnderflow => b"release-underflow",
            Violation::ReleaseUnpair => b"release-unpair",
            Violation::IrqUnbalanced => b"irq-unbalanced",
        }
    }
}

/// 取锁检查面（纯检查——HAL `acquire` 与 selftest 阳性对照共用）：
/// 返回违约定名（`Some`）或通过（`None`）。检查域 = `chain[floor..depth]`
/// （地板上 = 本流词条——IRQ 跨流分层，被断流词条不参与序检）。
///
/// 判序（逐字对齐 HAL）：链深帽 → F5 类号域前置 → 环扫三检（重入 →
/// 先验序 → RQ 升序专项，逐词条短路）。
pub fn check_acquire(chain: &[u8], depth: u8, floor: u8, class: u8) -> Option<Violation> {
    if depth as usize >= DEPTH_MAX {
        return Some(Violation::ChainOverflow);
    }
    // 前置校验（F5）：类号域外 = fail loud——防未来加类漏登 rank 表被静默
    // 错分（原兜底静默映射 rank 7=PRINT，错分方向不可预测）。
    if class == 0 || class > CLASS_MAX {
        return Some(Violation::UnknownClass);
    }
    for i in floor..depth {
        let held = chain[i as usize];
        // ① 重入：地板上同类二取 = 非重入 TTAS 同实例自死锁形（RQ0/RQ1
        //    分立类号故同类必同锁）。
        if held == class {
            return Some(Violation::Reentry);
        }
        // ② 先验序：地板上已持类 rank 须 ≤ 新类 rank（七类全序执法）。
        if rank(held) > rank(class) {
            return Some(Violation::Order);
        }
        // ③ RQ 升序专项：「双核升序」立法（持 RQ1 再取 RQ0 = 降序；偷取
        //    特则「只持远端」不构成双持——S-3 §9.7，无误报面）。
        if class == CLASS_RQ0 && held == CLASS_RQ1 {
            return Some(Violation::RqDescend);
        }
    }
    None
}

/// 放锁检查面（纯检查——HAL `release` 与 selftest 阳性对照共用，F4）：
/// 两臂：地板下放锁（underflow——越流放锁/配平早已破）+ 栈顶非本类
/// （unpair——放错对/序错）。
pub fn check_release(chain: &[u8], depth: u8, floor: u8, class: u8) -> Option<Violation> {
    if depth == floor {
        return Some(Violation::ReleaseUnderflow);
    }
    if chain[depth as usize - 1] != class {
        return Some(Violation::ReleaseUnpair);
    }
    None
}

/// IRQ 出口配平检查面（纯检查——HAL `irq_floor_lower` 与 selftest 阳性
/// 对照共用，F4）：深度须恰归地板（handler 配平执法——留残 = 有锁未放
/// 跨流泄漏）。
pub fn check_irq_exit(depth: u8, floor: u8) -> Option<Violation> {
    if depth != floor {
        Some(Violation::IrqUnbalanced)
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
