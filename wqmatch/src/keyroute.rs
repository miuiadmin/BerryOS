//! berry-wqmatch keyroute 模块——等待键路由判定律纯面（W3-Q75）。
//!
//! 单源面 = `sched/src/sched.rs` P1/P2-1 等待键路由族**纯判定律**：
//! - [`wait_bucket`] Fibonacci 散列桶号（`0x9E3779B97F4A7C15` 乘移取高
//!   6 位，`% WAIT_BUCKETS` 保险钉域——改桶数时防越界）；
//! - [`key_chainable`] 真键判据（入同键链的键形：`obj ∉ {0 纯睡,
//!   WAIT_ANY 哨兵}`——与 [`crate::WaitKey::Obj`] 值域恰等）；
//! - [`CountKind`] 计数三形分账（纯睡不计数/哨兵独立计数/真键入桶）；
//! - [`pi_donor_candidate`] PI donor 判据（真键 ∧ 派生位请求）与
//!   [`pi_boost_counted`]（pi≠0 冻结 boost 清零面）；
//! - [`gate_clear`] 门控判据骨架（键桶计数 + 哨兵计数双零 → 可跳全扫；
//!   [`pi_gate_needed`] PI 重推双零门同构）。
//!
//! 消费侧：sched.rs 判定点换单源 re-export（原子 fetch/KEY_HEADS 裸链
//! 留壳）；host 钉板域 = 判据全域（不依赖真原子真链）。
//!
//! **值域合同**（与 sched.rs 现役逐字节一致——镜像漂移 = 编译红，
//! sched 侧 const assert 执法）：
//! - `WAIT_BUCKETS == 64`（计数桶与键链桶同数钉死：`KEY_BUCKETS` 与
//!   `WAIT_BUCKETS` 相等断言在 sched 侧）；
//! - `SIG_DERIVED == 1|2`（ABI 冻结值 SIGNAL_READABLE/WRITABLE——
//!   sched crate 不依赖 kernel handle 模块，同值镜像纪律）。

/// 计数桶数（键链桶同数——sched 侧 `KEY_BUCKETS == WAIT_BUCKETS` 钉死）。
pub const WAIT_BUCKETS: usize = 64;

/// 派生位掩码（ABI 冻结值 SIGNAL_READABLE=1 / SIGNAL_WRITABLE=2）。
pub const SIG_DERIVED: u64 = 1 | 2;

/// 键 → 计数/键链桶号（Fibonacci hashing 乘移散列取高 6 位）。
///
/// `>> 58` 已落 [0,64)，`% WAIT_BUCKETS` 保险钉域（改桶数时防越界——
/// 乘移结果高 6 位域随桶数联动改移位量，`%` 是防御性第二钉）。
#[inline]
pub fn wait_bucket(key: usize) -> usize {
    (key.wrapping_mul(0x9E3779B97F4A7C15) >> 58) as usize % WAIT_BUCKETS
}

/// 真键判据：入同键链的键形（0 纯睡 / WAIT_ANY 哨兵不入——见 sched.rs
/// KEY_HEADS 块注）。与 [`crate::WaitKey::Obj`] 值域恰等（三形投影）。
#[inline]
pub fn key_chainable(obj: usize) -> bool {
    obj != 0 && obj != crate::WAIT_ANY
}

/// 等待计数三形分账（P1 wait_count_add/sub 的判定段单源）。
///
/// - [`CountKind::Skip`]：纯睡（obj=0）——不在任何唤醒集内，不计数
///   （减一处假阳性源）；
/// - [`CountKind::AnySentinel`]：WAIT_ANY 哨兵——单键桶罩不住（匹配任何
///   obj），独立计数面；
/// - [`CountKind::Bucket`]：真键——入 [`wait_bucket`] 桶。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountKind {
    /// 纯睡：不计数。
    Skip,
    /// WAIT_ANY 哨兵：独立计数面。
    AnySentinel,
    /// 真键：入桶（携带桶号）。
    Bucket(usize),
}

/// 入队/离队计数分账判定（add/sub 共用——同一 obj 值恒同形，加减对称）。
#[inline]
pub fn count_kind(obj: usize) -> CountKind {
    if obj == 0 {
        CountKind::Skip
    } else if obj == crate::WAIT_ANY {
        CountKind::AnySentinel
    } else {
        CountKind::Bucket(wait_bucket(obj))
    }
}

/// PI donor 判据（pi_count_add/sub 的 donor 面判定段单源）：真对象键
/// （哨兵多对象等待者被 crate 键门恒拒，§A34 常驻服务 wait_many 稳态
/// 形态不钉死门控）∧ 请求派生位（READABLE|WRITABLE——channel 派生
/// 唤醒面）。
#[inline]
pub fn pi_donor_candidate(obj: usize, signals: u64) -> bool {
    key_chainable(obj) && signals & SIG_DERIVED != 0
}

/// PI boost 计数判据（pi≠0 冻结 boost 清零面——重推尾回写配对）。
#[inline]
pub fn pi_boost_counted(pi: i32) -> bool {
    pi != 0
}

/// P1 wait 门控判据骨架：键桶计数 + 哨兵计数**双零** → SLEEPQ 上必无
/// 「obj 键相等或 WAIT_ANY 哨兵」的等待者，可跳过全扫。原子读留壳
/// （sched 侧把两计数读出后传入）。
#[inline]
pub fn gate_clear(any_waiters: u32, bucket_count: u32) -> bool {
    any_waiters == 0 && bucket_count == 0
}

/// P2 PI 重推门控判据（pi_recompute_needed 判定段单源）：双零 ⇒ 队上
/// 既无 donor 可能（①零 ⇒ 不动点零传播）也无待清 boost（②零 ⇒ 清零趟
/// 空转）——重推可整体跳过。原子读留壳。
#[inline]
pub fn pi_gate_needed(derived_waiters: u32, boosted_sleepers: u32) -> bool {
    derived_waiters != 0 || boosted_sleepers != 0
}
