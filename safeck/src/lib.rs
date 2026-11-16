//! berry-safeck —— `berry_sched_attr` 结构解码 + 校验纯函数面（W3-Q22）。
//!
//! **动机**：`kernel/src/syscall.rs` 的 `sa_parse` 是 sched_setattr 的
//! 底座验证器——size 门 / 前向兼容尾段全零律 / version 门 / reserved0 与
//! 未知 flags 位门 / weight·intent·policy·freeze_clock 各自域门 /
//! policy=3↔BUDGET 联动 / RESERVE 与 policy=3 互斥 / BUDGET 五拒面 /
//! S5 admission Q12 恰过恰拒界 / DEADLINE 依赖 BUDGET——此前只活在
//! kernel crate 里（同 chanface 外迁前的处境：host 测不可达、ktest 只覆
//! 盖七条负向），交叉门（policy=3 必带 BUDGET、RESERVE 互斥、admission
//! 3891 边界）的表驱动回归面需要一张 host 可测的**钉板**。本 crate 把
//! 「给定字节 buf + 当前利用率 cur_u 的纯判定」按值域抽成纯函数 +
//! 表驱动测试，kernel 侧 `sa_parse` 变薄壳（copy_from_user 物理读臂 +
//! `DL_TOTAL_UTIL_Q12` 原子读留壳——纪律同 polface/chanface 先例族，
//! 先例族第 15 件）。
//!
//! 抽离面（与消费侧逐字对齐，改这里的语义 = 改消费侧行为）：
//! - [`SCHED_ATTR_SIZE`]/[`SCHED_ATTR_MAX`]/[`SA_F_KNOWN`]:常量单源（自
//!   syscall.rs 迁此，kernel re-export——SA_F_* 位名已单源在 berry-polface
//!   [`SA_FLAG_*`]，本 crate re-export 对齐词汇表）。
//! - [`sa_check`]:`berry_sched_attr` 解码 + 全部门校验的纯判定——输入
//!   `buf[..size]`（调用方已 copy_from_user 的原始字节，size ∈ [56, 256]）
//!   与 `cur_u`（kernel 侧 DL_TOTAL_UTIL_Q12 原子快照），输出
//!   `(SchedAttr, size)` 或 [`SaError`]。**不读内核状态、不触碰用户内存**。
//! - [`SaError`]:crate 内独立错误枚举——kernel 侧 match 映射回 ERR_*
//!   常量（值域：EINVAL/-22、E2BIG/-7、ENOTSUP/-95、EBUSY/-16）。
//!
//! 与 admission 记账面的关系：`DL_TOTAL_UTIL_Q12` 的 fetch_add/fetch_sub
//! （sched_setattr 成功尾 ± 旧值退回）与 copy_from_user 物理臂留 kernel
//! （纪律不变）——本面只钉「这 56..256 字节 + 这个 cur_u，放不放行」的
//! 判定律，不钉记账机械。SchedAttr 布局与 SA_FLAG_* 位单源在
//! berry-polface（W3-Q20），本 crate 消费不二写。
//!
//! **测试向量钉板纪律**：admission 锚点（budget=1 period=4096 → new_u
//! = 1024、cur_u 2867 → 恰过 / 2868 → 恰拒，因 2867+1024=3891 ≤ 3891
//! < 2868+1024=3892）是把 `1.0−0.05 = 3891 Q12` 帽冻结进测试——改帽值
//! 或 Q12 底数而不改锚点 = 测试必红，锚点即 admission 指纹。

#![cfg_attr(not(test), no_std)]

use berry_polface::SchedAttr;

// ---------------------------------------------------------------------------
// 常量族（单源迁此——kernel syscall.rs re-export）
// ---------------------------------------------------------------------------

/// `berry_sched_attr` 内核已知布局长度（ABI §7.2；前向兼容读入上限 256B）。
pub const SCHED_ATTR_SIZE: usize = 56;
/// `berry_sched_attr` 读入上限（size > 此值 → EINVAL；调用方 size 声明帽）。
pub const SCHED_ATTR_MAX: usize = 256;
/// 内核已知的 flags 位全集（低 8 位——bit0 weight … bit7 budget；高位未
/// 在本 ABI 版本定义，置位 → EINVAL 而非 ENOTSUP：调用方可通过清零修复）。
pub const SA_F_KNOWN: u16 = 0x00FF;

/// SA_FLAG_* 位名对齐 re-export（单源在 berry-polface——词汇表一处冻结，
/// 消费侧 `berry_safeck::SA_F_WEIGHT` 与 `berry_polface::SA_FLAG_WEIGHT`
/// 同值同源）。
pub use berry_polface::{
    SA_FLAG_BUDGET as SA_F_BUDGET, SA_FLAG_DEADLINE as SA_F_DEADLINE,
    SA_FLAG_FREEZE_CLOCK as SA_F_FREEZE_CLOCK, SA_FLAG_FREEZE_READY as SA_F_FREEZE_READY,
    SA_FLAG_INTENT as SA_F_INTENT, SA_FLAG_POLICY as SA_F_POLICY,
    SA_FLAG_RESERVE as SA_F_RESERVE, SA_FLAG_WEIGHT as SA_F_WEIGHT,
};

// ---------------------------------------------------------------------------
// 错误枚举（kernel 侧 match 映射回 ERR_* 常量）
// ---------------------------------------------------------------------------

/// `sa_check` 独立错误枚举——kernel 侧映射：InvalidArg → ERR_INVALID_ARG
/// (-22) / E2big → ERR_E2BIG (-7) / NotSupported → ERR_NOT_SUPPORTED (-95)
/// / Busy → ERR_BUSY (-16)。物理读臂（copy_from_user）的 ERR_FAULT 留壳
/// 不进本枚举——纯判定面无 IO。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaError {
    /// size 域外 / 保留字段或未知 flags 非零 / 各字段域门外 / 交叉门拒。
    InvalidArg,
    /// size > 内核布局但尾段非零（前向兼容违约——调用方该用 E2BIG 语义
    /// 被告知「尾部字段本内核不识且必须为零」）。
    E2big,
    /// version != 1（ABI 版本不识——调用方无法通过改字段修复）。
    NotSupported,
    /// S5 admission：Σ利用率 + 新申请 > 1 − reserve（诚实拒——不静默丢帧）。
    Busy,
}

// ---------------------------------------------------------------------------
// 纯判定面
// ---------------------------------------------------------------------------

/// 解码并校验一个 `berry_sched_attr`（纯函数——无 IO、无内核状态）。
///
/// - `buf`:调用方声明的完整原始字节（kernel 侧已 copy_from_user；长度即
///   调用方声明 size，须 ≥ [`SCHED_ATTR_SIZE`]——首 4 字节 LE u32 自述）。
///   buf 超出 size 的部分不读（消费侧只传 `buf[..size]`）。
/// - `cur_u`:kernel 侧 `DL_TOTAL_UTIL_Q12` 原子快照（Q12 定点——1.0 =
///   4096）。仅 BUDGET 置位时参与 admission 判定。
///
/// 返回 `(SchedAttr, size)`——`getattr` 需要原 size 决定回写尾部长度。
///
/// 判序（与外迁前 sa_parse 逐字对齐——次序本身是 ABI 面：weight 域门先
/// 于 intent 域门，交叉拒同码 EINVAL 不分先后可观测）：
/// size 门 → 尾零门 → version 门 → 解码 → reserved0/未知 flags →
/// weight → intent → policy 域 → policy=3↔BUDGET → freeze_clock →
/// RESERVE → BUDGET（含 admission）→ DEADLINE。
pub fn sa_check(buf: &[u8], cur_u: u64) -> Result<(SchedAttr, usize), SaError> {
    let size = buf.len();
    if size < SCHED_ATTR_SIZE || size > SCHED_ATTR_MAX {
        return Err(SaError::InvalidArg);
    }
    let size = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if size != buf.len() {
        // 消费侧契约：buf 长度即声明 size（首 4 字节自述须与切片长一致——
        // 否则尾段判定窗口失真）。
        return Err(SaError::InvalidArg);
    }
    // 前向兼容：size > 内核布局时尾部必须全零（Linux sched_attr 同款）。
    if size > SCHED_ATTR_SIZE && buf[SCHED_ATTR_SIZE..size].iter().any(|&b| b != 0) {
        return Err(SaError::E2big);
    }
    let version = u16::from_le_bytes([buf[4], buf[5]]);
    if version != 1 {
        return Err(SaError::NotSupported);
    }
    let a = SchedAttr {
        flags: u16::from_le_bytes([buf[6], buf[7]]),
        weight: u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]),
        policy: buf[12],
        intent: buf[13],
        freeze_clock: buf[14],
        min_ns: u64::from_le_bytes(buf[16..24].try_into().unwrap()),
        max_ns: u64::from_le_bytes(buf[24..32].try_into().unwrap()),
        deadline_ns: u64::from_le_bytes(buf[32..40].try_into().unwrap()),
        budget_ns: u64::from_le_bytes(buf[40..48].try_into().unwrap()),
        period_ns: u64::from_le_bytes(buf[48..56].try_into().unwrap()),
    };
    // reserved0 与未知 flags：保留字段错 = EINVAL（不是 NOT_SUPPORTED——
    // 调用方可通过清零修复；高 flags 位未在本 ABI 版本定义）。
    if buf[15] != 0 || a.flags & !SA_F_KNOWN != 0 {
        return Err(SaError::InvalidArg);
    }
    if a.flags & SA_F_WEIGHT != 0 && (a.weight == 0 || a.weight > 10000) {
        return Err(SaError::InvalidArg);
    }
    if a.flags & SA_F_INTENT != 0 && a.intent > 0x05 {
        return Err(SaError::InvalidArg);
    }
    if a.flags & SA_F_POLICY != 0 && !(1..=4).contains(&a.policy) {
        return Err(SaError::InvalidArg);
    }
    // policy=3 是 admission-aware deadline 档：必须同时携带 budget/period
    //（§7.2「policy=3 必须置 bit7」），否则 Task 无重臂周期可消费。
    if a.flags & SA_F_POLICY != 0 && a.policy == 3 && a.flags & SA_F_BUDGET == 0 {
        return Err(SaError::InvalidArg);
    }
    if a.flags & SA_F_FREEZE_CLOCK != 0 && a.freeze_clock > 1 {
        return Err(SaError::InvalidArg);
    }
    if a.flags & SA_F_RESERVE != 0 {
        if a.min_ns > a.max_ns || a.flags & SA_F_POLICY != 0 && a.policy == 3 {
            return Err(SaError::InvalidArg);
        }
    }
    if a.flags & SA_F_BUDGET != 0 {
        if a.budget_ns == 0
            || a.period_ns == 0
            || a.budget_ns > a.period_ns
            || a.flags & SA_F_POLICY == 0
            || a.policy != 3
        {
            return Err(SaError::InvalidArg);
        }
        // S5 admission：Σ(C_i/T_i) + 新申请 ≤ 1 − reserve（5% 固定——
        // 计划 §6.3 EDF 可行性）。超载 → EBUSY（诚实拒——不静默丢帧）。
        let new_u = a.budget_ns as u128 * 4096 / a.period_ns as u128; // Q12 定点
        let cur_u = cur_u as u128;
        // 1.0 − 0.05 = 0.95 → Q12 = 3891
        if cur_u + new_u > 3891 {
            return Err(SaError::Busy);
        }
        // 计数在 sys_sched_setattr 成功尾累加（此处仅校验——防错误路径泄漏）。
    }
    if a.flags & SA_F_DEADLINE != 0 {
        if a.flags & SA_F_BUDGET == 0 || a.deadline_ns == 0 || a.deadline_ns > a.period_ns {
            return Err(SaError::InvalidArg);
        }
    }
    Ok((a, size))
}

// ---------------------------------------------------------------------------
// 表驱动钉板（host 可测——W3-Q22；先经独立 Python 仿真验证锚点再落测试，
// polface 散列指纹同款纪律）
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    /// 构造 56B attr：version=1 默认，字段按参填（其余零）。
    fn mk(version: u16, flags: u16, weight: u32, policy: u8, intent: u8,
          freeze_clock: u8, min: u64, max: u64, deadline: u64,
          budget: u64, period: u64) -> Vec<u8> {
        let mut b = vec![0u8; SCHED_ATTR_SIZE];
        b[0..4].copy_from_slice(&(SCHED_ATTR_SIZE as u32).to_le_bytes());
        b[4..6].copy_from_slice(&version.to_le_bytes());
        b[6..8].copy_from_slice(&flags.to_le_bytes());
        b[8..12].copy_from_slice(&weight.to_le_bytes());
        b[12] = policy;
        b[13] = intent;
        b[14] = freeze_clock;
        b[16..24].copy_from_slice(&min.to_le_bytes());
        b[24..32].copy_from_slice(&max.to_le_bytes());
        b[32..40].copy_from_slice(&deadline.to_le_bytes());
        b[40..48].copy_from_slice(&budget.to_le_bytes());
        b[48..56].copy_from_slice(&period.to_le_bytes());
        b
    }

    /// 合法基准向量：POLICY=4 + WEIGHT + INTENT + FREEZE_CLOCK（无 BUDGET/
    /// RESERVE/DEADLINE——避免交叉门牵连）。
    fn base_ok() -> Vec<u8> {
        mk(1, SA_F_WEIGHT | SA_F_POLICY | SA_F_INTENT | SA_F_FREEZE_CLOCK,
           100, 4, 3, 1, 0, 0, 0, 0, 0)
    }

    /// 合法 deadline 档基准：POLICY=3 + BUDGET + DEADLINE + WEIGHT。
    fn dl_ok() -> Vec<u8> {
        mk(1, SA_F_WEIGHT | SA_F_POLICY | SA_F_BUDGET | SA_F_DEADLINE,
           100, 3, 0, 0, 0, 0, 1_000_000, 1_000_000, 2_000_000)
    }

    /// 解码字段回读对拍辅助。
    fn check_ok(buf: &[u8], cur: u64) -> (SchedAttr, usize) {
        sa_check(buf, cur).expect("须过")
    }

    // ---- V1：size 门 + 前向兼容尾零律 ----

    /// V1-a：size 55 → EINVAL；size 56 恰过；size 257 → EINVAL。
    #[test]
    fn v1a_size_gate() {
        let mut short = base_ok();
        short[0..4].copy_from_slice(&55u32.to_le_bytes());
        assert_eq!(sa_check(&short[..55], 0), Err(SaError::InvalidArg),
            "size=55 须拒");
        let ok = sa_check(&base_ok(), 0);
        assert!(ok.is_ok(), "size=56 须过");
        let mut long = vec![0u8; 257];
        long[0..4].copy_from_slice(&257u32.to_le_bytes());
        assert_eq!(sa_check(&long, 0), Err(SaError::InvalidArg),
            "size=257 须拒");
    }

    /// V1-b：size > 56 尾段全零 → 过；任一非零 → E2BIG；恰 56 无尾段免检。
    #[test]
    fn v1b_tail_zero() {
        let mut big = base_ok();
        big.resize(64, 0);
        big[0..4].copy_from_slice(&64u32.to_le_bytes());
        assert!(sa_check(&big, 0).is_ok(), "尾段全零须过");
        big[63] = 1;
        assert_eq!(sa_check(&big, 0), Err(SaError::E2big),
            "尾段非零须 E2BIG");
        // buf 长度与自述 size 失配 → EINVAL（消费契约门）。
        let mut mis = base_ok();
        mis[0..4].copy_from_slice(&64u32.to_le_bytes());
        assert_eq!(sa_check(&mis, 0), Err(SaError::InvalidArg),
            "自述 size ≠ buf 长须拒");
    }

    /// V1-c：size=256 恰帽 + 尾全零 → 过（上限边界正例）。
    #[test]
    fn v1c_size_cap() {
        let mut big = base_ok();
        big.resize(256, 0);
        big[0..4].copy_from_slice(&256u32.to_le_bytes());
        let (a, sz) = check_ok(&big, 0);
        assert_eq!(sz, 256);
        assert_eq!(a.policy, 4);
    }

    // ---- V2：version 门 ----

    /// V2：version=0 / 2 / 0xFFFF → ENOTSUP；version=1 过。
    #[test]
    fn v2_version_gate() {
        for v in [0u16, 2, 0xFFFF] {
            let b = mk(v, 0, 100, 4, 0, 0, 0, 0, 0, 0, 0);
            assert_eq!(sa_check(&b, 0), Err(SaError::NotSupported),
                "version={} 须 ENOTSUP", v);
        }
        assert!(sa_check(&mk(1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0), 0).is_ok(),
            "version=1 flags=0 须过");
    }

    // ---- V3：reserved0 + 未知 flags 门 ----

    /// V3-a：reserved0 非零 → EINVAL；flags 高位（bit8+）→ EINVAL。
    #[test]
    fn v3a_reserved_and_unknown_flags() {
        let mut b = base_ok();
        b[15] = 1;
        assert_eq!(sa_check(&b, 0), Err(SaError::InvalidArg),
            "reserved0 非零须拒");
        let mut b2 = base_ok();
        b2[6..8].copy_from_slice(&(SA_F_POLICY | 0x0100).to_le_bytes());
        assert_eq!(sa_check(&b2, 0), Err(SaError::InvalidArg),
            "flags bit8 未定义须拒");
        // 恰低 8 位全置（0xFF 已知位全集）+ 各域合法 → 过。
        let b3 = mk(1, 0x00FF & !(SA_F_RESERVE | SA_F_DEADLINE | SA_F_FREEZE_READY),
                    100, 3, 3, 1, 0, 0, 0, 1_000_000, 2_000_000);
        assert!(sa_check(&b3, 0).is_ok(), "已知位全置合法档须过");
    }

    // ---- V4：weight 域门 ----

    /// V4：weight 置位时 0 / 10001 拒，1 / 10000 恰界过；不置位不看。
    #[test]
    fn v4_weight_domain() {
        for w in [0u32, 10001, u32::MAX] {
            let b = mk(1, SA_F_WEIGHT, w, 0, 0, 0, 0, 0, 0, 0, 0);
            assert_eq!(sa_check(&b, 0), Err(SaError::InvalidArg),
                "weight={} 须拒", w);
        }
        for w in [1u32, 10000] {
            let b = mk(1, SA_F_WEIGHT, w, 0, 0, 0, 0, 0, 0, 0, 0);
            assert!(sa_check(&b, 0).is_ok(), "weight={} 恰界须过", w);
        }
        // 位不置 → weight 值不校（含 0/越界垃圾值）。
        let b = mk(1, 0, 10001, 0, 0, 0, 0, 0, 0, 0, 0);
        assert!(sa_check(&b, 0).is_ok(), "weight 位未置须不校");
    }

    // ---- V5：intent 域门 ----

    /// V5：置位时 0..=5 过（含界），6/0xFF 拒；不置位不看。
    #[test]
    fn v5_intent_domain() {
        for i in [0u8, 5] {
            let b = mk(1, SA_F_INTENT, 0, 0, i, 0, 0, 0, 0, 0, 0);
            assert!(sa_check(&b, 0).is_ok(), "intent={} 须过", i);
        }
        for i in [6u8, 0xFF] {
            let b = mk(1, SA_F_INTENT, 0, 0, i, 0, 0, 0, 0, 0, 0);
            assert_eq!(sa_check(&b, 0), Err(SaError::InvalidArg),
                "intent={} 须拒", i);
        }
        let b = mk(1, 0, 0, 0, 0xFF, 0, 0, 0, 0, 0, 0);
        assert!(sa_check(&b, 0).is_ok(), "intent 位未置须不校");
    }

    // ---- V6：policy 域门 + policy=3↔BUDGET 联动 ----

    /// V6-a：置位时 0/5/0xFF 拒，1..=4 过（policy=3 例外见 V6-b）。
    #[test]
    fn v6a_policy_domain() {
        for p in [1u8, 2, 4] {
            let b = mk(1, SA_F_POLICY, 0, p, 0, 0, 0, 0, 0, 0, 0);
            assert!(sa_check(&b, 0).is_ok(), "policy={} 须过", p);
        }
        for p in [0u8, 5, 0xFF] {
            let b = mk(1, SA_F_POLICY, 0, p, 0, 0, 0, 0, 0, 0, 0);
            assert_eq!(sa_check(&b, 0), Err(SaError::InvalidArg),
                "policy={} 须拒", p);
        }
        // 位不置 → policy 值不校。
        let b = mk(1, 0, 0, 0xFF, 0, 0, 0, 0, 0, 0, 0);
        assert!(sa_check(&b, 0).is_ok(), "policy 位未置须不校");
    }

    /// V6-b：policy=3 必须 BUDGET 同置——单置 POLICY=policy3 → EINVAL；
    /// POLICY|BUDGET+合法预算 → 过（§7.2 联动律）。
    #[test]
    fn v6b_policy3_budget_link() {
        let b = mk(1, SA_F_POLICY, 0, 3, 0, 0, 0, 0, 0, 0, 0);
        assert_eq!(sa_check(&b, 0), Err(SaError::InvalidArg),
            "policy=3 无 BUDGET 须拒");
        let ok = mk(1, SA_F_POLICY | SA_F_BUDGET, 0, 3, 0, 0, 0, 0, 0,
                    1_000_000, 2_000_000);
        assert!(sa_check(&ok, 0).is_ok(), "policy=3+BUDGET 合法档须过");
        // 反向：BUDGET 置位而 policy≠3（或无 POLICY 位）→ BUDGET 门拒。
        let b2 = mk(1, SA_F_BUDGET, 0, 0, 0, 0, 0, 0, 0, 1_000_000, 2_000_000);
        assert_eq!(sa_check(&b2, 0), Err(SaError::InvalidArg),
            "BUDGET 无 POLICY 须拒");
        let b3 = mk(1, SA_F_POLICY | SA_F_BUDGET, 0, 2, 0, 0, 0, 0, 0,
                    1_000_000, 2_000_000);
        assert_eq!(sa_check(&b3, 0), Err(SaError::InvalidArg),
            "BUDGET+policy=2 须拒");
    }

    // ---- V7：freeze_clock 域门 ----

    /// V7：置位时 0/1 过，2/0xFF 拒；不置位不看。
    #[test]
    fn v7_freeze_clock_domain() {
        for f in [0u8, 1] {
            let b = mk(1, SA_F_FREEZE_CLOCK, 0, 0, 0, f, 0, 0, 0, 0, 0);
            assert!(sa_check(&b, 0).is_ok(), "freeze_clock={} 须过", f);
        }
        for f in [2u8, 0xFF] {
            let b = mk(1, SA_F_FREEZE_CLOCK, 0, 0, 0, f, 0, 0, 0, 0, 0);
            assert_eq!(sa_check(&b, 0), Err(SaError::InvalidArg),
                "freeze_clock={} 须拒", f);
        }
        let b = mk(1, 0, 0, 0, 0, 0xFF, 0, 0, 0, 0, 0);
        assert!(sa_check(&b, 0).is_ok(), "freeze_clock 位未置须不校");
    }

    // ---- V8：RESERVE 门（min≤max + 与 policy=3 互斥）----

    /// V8：min>max 拒；RESERVE+POLICY(p=3) 互斥拒；RESERVE+POLICY(p=4) 过；
    /// RESERVE 单置 + min==max 恰界过。
    #[test]
    fn v8_reserve_gate() {
        let b = mk(1, SA_F_RESERVE, 0, 0, 0, 0, 200, 100, 0, 0, 0);
        assert_eq!(sa_check(&b, 0), Err(SaError::InvalidArg),
            "min>max 须拒");
        let b2 = mk(1, SA_F_RESERVE | SA_F_POLICY, 0, 3, 0, 0, 100, 200, 0, 0, 0);
        assert_eq!(sa_check(&b2, 0), Err(SaError::InvalidArg),
            "RESERVE+policy=3 互斥须拒");
        let b3 = mk(1, SA_F_RESERVE | SA_F_POLICY, 0, 4, 0, 0, 100, 200, 0, 0, 0);
        assert!(sa_check(&b3, 0).is_ok(), "RESERVE+policy=4 须过");
        let b4 = mk(1, SA_F_RESERVE, 0, 0, 0, 0, 100, 100, 0, 0, 0);
        assert!(sa_check(&b4, 0).is_ok(), "min==max 恰界须过");
        // policy=3 但 POLICY 位未置 → 互斥门不看（位选 vocab 语义）。
        let b5 = mk(1, SA_F_RESERVE, 0, 3, 0, 0, 100, 200, 0, 0, 0);
        assert!(sa_check(&b5, 0).is_ok(), "policy=3 值无 POLICY 位须不触发互斥");
    }

    // ---- V9：BUDGET 门五拒面 ----

    /// V9：budget=0 / period=0 / budget>period / 无 POLICY / policy≠3 各拒；
    /// budget==period 恰界过。
    #[test]
    fn v9_budget_gates() {
        let base = |budget: u64, period: u64, flags: u16, policy: u8| {
            mk(1, flags, 0, policy, 0, 0, 0, 0, 0, budget, period)
        };
        assert_eq!(sa_check(&base(0, 100, SA_F_POLICY | SA_F_BUDGET, 3), 0),
            Err(SaError::InvalidArg), "budget=0 须拒");
        assert_eq!(sa_check(&base(100, 0, SA_F_POLICY | SA_F_BUDGET, 3), 0),
            Err(SaError::InvalidArg), "period=0 须拒");
        assert_eq!(sa_check(&base(100, 50, SA_F_POLICY | SA_F_BUDGET, 3), 0),
            Err(SaError::InvalidArg), "budget>period 须拒");
        assert_eq!(sa_check(&base(100, 200, SA_F_BUDGET, 0), 0),
            Err(SaError::InvalidArg), "无 POLICY 须拒");
        assert_eq!(sa_check(&base(100, 200, SA_F_POLICY | SA_F_BUDGET, 2), 0),
            Err(SaError::InvalidArg), "policy≠3 须拒");
        assert!(sa_check(&base(100, 200, SA_F_POLICY | SA_F_BUDGET, 3), 0)
            .is_ok(), "budget=100 period=200（new_u=2048<3891）须过");
    }

    // ---- V10：S5 admission 3891 Q12 恰过/恰拒 ----

    /// V10-a：new_u = budget×4096/period Q12；cur+new > 3891 → EBUSY。
    /// 锚点：budget=1024 period=4096 → new_u=1024；cur=2867 恰过（Σ=3891）
    /// / cur=2868 恰拒（Σ=3892）——admission 指纹钉板。
    #[test]
    fn v10a_admission_boundary() {
        let b = mk(1, SA_F_POLICY | SA_F_BUDGET, 0, 3, 0, 0, 0, 0, 0,
                   1024, 4096);
        assert!(sa_check(&b, 2867).is_ok(),
            "cur=2867 + new=1024 → Σ=3891 恰帽须过");
        assert_eq!(sa_check(&b, 2868), Err(SaError::Busy),
            "cur=2868 + new=1024 → Σ=3892 恰拒须 EBUSY");
    }

    /// V10-b：BUDGET 位不置 → admission 不参与（cur_u 再大也过）；零占位
    /// 任务（cur=3891）下非 BUDGET 档仍过。
    #[test]
    fn v10b_admission_only_budget() {
        let b = base_ok(); // POLICY=4 无 BUDGET
        assert!(sa_check(&b, 3891).is_ok(),
            "无 BUDGET 位 admission 须不参与");
        assert!(sa_check(&b, u64::MAX).is_ok(), "cur 极值无 BUDGET 须过");
    }

    /// V10-c：全幅占用（budget=period → new_u=4096 > 3891）→ 单任务即拒
    ///（reserve 5% 铁律——1.0 整幅不可独占）。v9 恰界 budget==period=
    /// 100/100 → new_u=4096 恰帽？不——4096 > 3891 须拒。对拍修正锚点：
    /// new_u=4096 时 cur=0 也拒。
    #[test]
    fn v10c_full_band_rejected() {
        let b = mk(1, SA_F_POLICY | SA_F_BUDGET, 0, 3, 0, 0, 0, 0, 0,
                   100, 100);
        assert_eq!(sa_check(&b, 0), Err(SaError::Busy),
            "new_u=4096 > 3891 单任务全幅须 EBUSY");
    }

    // ---- V11：DEADLINE 门（依赖 BUDGET + deadline 域）----

    /// V11：无 BUDGET 拒 / deadline=0 拒 / deadline>period 拒 / deadline
    /// ==period 恰界过 / deadline<period 过。
    #[test]
    fn v11_deadline_gate() {
        let base = |deadline: u64, flags: u16| {
            // POLICY=policy3 底座（BUDGET 门要求 POLICY 同置 policy=3）。
            mk(1, flags | SA_F_POLICY, 0, 3, 0, 0, 0, 0, deadline, 1_000_000, 2_000_000)
        };
        assert_eq!(sa_check(&base(1, SA_F_DEADLINE), 0), Err(SaError::InvalidArg),
            "DEADLINE 无 BUDGET 须拒");
        assert_eq!(sa_check(&base(0, SA_F_BUDGET | SA_F_DEADLINE), 0),
            Err(SaError::InvalidArg), "deadline=0 须拒");
        assert_eq!(sa_check(&base(2_000_001, SA_F_BUDGET | SA_F_DEADLINE), 0),
            Err(SaError::InvalidArg), "deadline>period 须拒");
        assert!(sa_check(&base(2_000_000, SA_F_BUDGET | SA_F_DEADLINE), 0)
            .is_ok(), "deadline==period 恰界须过");
        assert!(sa_check(&dl_ok(), 0).is_ok(), "deadline<period 须过");
    }

    // ---- V12：解码字段回读（布局指纹 @6/8/12/13/14/16/24/32/40/48）----

    /// V12：全字段非零向量解码回读逐位对拍——偏移钉板（改布局即红）。
    #[test]
    fn v12_decode_layout() {
        let b = mk(1, 0x001F, 10000, 4, 5, 1, 111, 222, 333, 444, 555);
        let (a, sz) = check_ok(&b, 0);
        assert_eq!(sz, 56);
        assert_eq!(a.flags, 0x001F);
        assert_eq!(a.weight, 10000);
        assert_eq!(a.policy, 4);
        assert_eq!(a.intent, 5);
        assert_eq!(a.freeze_clock, 1);
        assert_eq!(a.min_ns, 111);
        assert_eq!(a.max_ns, 222);
        assert_eq!(a.deadline_ns, 333);
        assert_eq!(a.budget_ns, 444);
        assert_eq!(a.period_ns, 555);
    }

    /// V12-b：SA_F_* 别名与 polface SA_FLAG_* 同源同值对拍（词汇表单源）。
    #[test]
    fn v12b_flag_alias_same_source() {
        assert_eq!(SA_F_WEIGHT, berry_polface::SA_FLAG_WEIGHT);
        assert_eq!(SA_F_KNOWN, 0x00FF);
        assert_eq!(SA_F_KNOWN, SA_F_WEIGHT | SA_F_INTENT | SA_F_POLICY
            | SA_F_FREEZE_CLOCK | SA_F_FREEZE_READY | SA_F_RESERVE
            | SA_F_DEADLINE | SA_F_BUDGET);
    }

    /// V12-c：sa_check 判序 ABI 面——version 门在尾零门之后：size>56 尾非零
    /// + version=2 双错并发 → E2big 先报（外迁前同序对拍）。
    #[test]
    fn v12c_order_version_after_tail() {
        let mut b = base_ok();
        b.resize(64, 0);
        b[0..4].copy_from_slice(&64u32.to_le_bytes());
        b[63] = 1; // 尾非零
        b[4..6].copy_from_slice(&2u16.to_le_bytes()); // version=2
        assert_eq!(sa_check(&b, 0), Err(SaError::E2big),
            "尾零门先于 version 门（判序 ABI 面）");
    }
}
