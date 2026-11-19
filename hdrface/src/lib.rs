//! berry-hdrface —— HDR 延迟直方图形状与分位数学纯函数面（W3-Q16）。
//!
//! **动机**：调点调度 v1.3 §9「真机框架——HDR 桶」的对数线性桶直方图
//! （`sched/src/hdr.rs`）此前只活在 sched crate——sched 依赖 HAL
//! global_asm，只 aarch64-unknown-none 可编，host 测不可达，ktest 只覆盖
//! boot 相。桶映射律（clz 八度 + 子桶移位）、桶上界（保守上界/溢出哨兵）、
//! 分位秩（千分比 → rank 单式）、`Hdr` 原子直方图本身（record/percentile/
//! reset 写读序纪律）的回归面需要一张 host 可测的**钉板**：本 crate 把
//! 这些数学按值域抽成纯函数 + `#[repr(C)]` 形状数据结构，`sched` 侧消费
//! 同一函数（零第二实现——抽离即唯一事实源，非镜像；wqmatch/emface
//! 同款纪律）。
//!
//! 抽离面（与 sched 侧逐字对齐，改这里的语义 = 改 sched 行为）：
//! - [`HDR_SUB_BITS`]/[`HDR_SUB`]/[`HDR_OCTAVES`]/[`HDR_BUCKETS`]/
//!   [`HDR_OVERFLOW`]/[`HDR_MAX_VALUE`]：形状常量单源（自
//!   sched/src/hdr.rs 迁此，sched `pub use` re-export）。
//! - [`bucket_of`]：值 → 桶下标 O(1) 映射（clz 八度 + 子桶宽自适应移位；
//!   0 防御归桶 0、超值域钳位顶桶）。
//! - [`bucket_upper`]：桶下标 → 保守上界（分位返回值口径；溢出桶 →
//!   `u64::MAX` 哨兵）。
//! - [`rank_of`]:千分比 + 样本数 → 秩（`pm×n/1000 + 1`，1 起帽 n；
//!   u128 中转防溢出）。
//! - [`Hdr`]/[`CpuHdr`]:`#[repr(C)]` 原子直方图形状单源（尺寸/对齐
//!   day-one 钉板编译期执法）——`record`/`percentile`/`percentile_rank`]/
//!   [`total`]/[`min_value`]/[`max_value`]/[`bucket`]/[`reset`] 语义
//!   逐字随迁（写序末位 Release / 读序 N 先 Acquire 纪律不变）。
//! - [`HDR_NR_CPU`]/[`LatKind`]/[`pick`]:per-CPU 槽数与双路选择子。
//!
//! 与冻结面的关系：per-CPU 实例存储（`static PER_CPU_HDR`）、HAL 时基
//! （`cpu_id` 槽化）与热路径写入口（`record_switch_lat`/
//! `record_wake_lat`）是**裸并发机械**，留 sched（写方限本核纪律不变）
//! ——本面只钉「桶怎么映射/分位怎么算/形状怎么定」的数定律。
//!
//! 测试面：全域钉板（bucket_of/bucket_upper 全 897 桶往返闭环、边界值
//! 0/1/2^27−1/2^27/u64::MAX）、Hdr 记账律（零样本哨兵/单桶/钳位/极值
//! 原值口径）、分位秩（pm 边界 0/1/500/1000/>1000、rank 钳位、first-up
//! 扫描必达）、reset 回出生态、合并读面数学（merged 桶位直和同式）。

#![cfg_attr(not(test), no_std)]

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// 形状常量（单源——自 sched/src/hdr.rs 迁此）
// ---------------------------------------------------------------------------

/// 每八度子桶位宽（2^5 = 32 子桶/八度）。
pub const HDR_SUB_BITS: u32 = 5;

/// 每八度子桶数。
pub const HDR_SUB: u32 = 1 << HDR_SUB_BITS;

/// 八度数（值域 2^0..2^27，即 1..2^28 ticks）。
pub const HDR_OCTAVES: u32 = 28;

/// 桶总数 = 28 × 32 + 1 溢出防御位。
pub const HDR_BUCKETS: usize = (HDR_OCTAVES * HDR_SUB) as usize + 1;

/// 溢出桶下标（防御位——见 [`Hdr::record`] 的钳位注；映射域内不可达）。
pub const HDR_OVERFLOW: usize = HDR_BUCKETS - 1;

/// 值域上界（含）：2^28 − 1 ticks（@62.5MHz CNTVCT ≈ 4.29s）。
pub const HDR_MAX_VALUE: u64 = (1u64 << HDR_OCTAVES) - 1;

// 形状钉板（编译期）：桶数/尺寸/对齐 day-one 定型——将来 dump/导出面
// （§11「HDR 桶 #[repr(C)] day-one 定型」纪律）的 ABI 基线。
const _: () = assert!(HDR_BUCKETS == 897);
/// 897×4 桶 + 3×8 计量 + 4 对齐垫（buckets→count 的 8 对齐垫）。
const _: () = assert!(core::mem::size_of::<Hdr>() == 3616);
const _: () = assert!(core::mem::align_of::<Hdr>() == 8);
/// 2 × 3616 = 7232 = 恰 113 条 64B cacheline（无尾垫）。
const _: () = assert!(core::mem::size_of::<CpuHdr>() == 7232);
const _: () = assert!(core::mem::align_of::<CpuHdr>() == 64);

/// 值 → 桶下标（纯函数；[`Hdr::record`] 的 O(1) 映射独立成函数供全域
/// 钉板）。oct < 5 时子桶宽 1（值 < 32 精确落桶）；oct ≥ 5 时子桶宽
/// 2^(oct−5)。`value == 0` 无八度（clz 未定义域）——record 早退不至此，
/// 防御归桶 0；`value > HDR_MAX_VALUE` 钳位（同 record 口径）。
pub fn bucket_of(value: u64) -> usize {
    if value == 0 {
        return 0;
    }
    let v = value.min(HDR_MAX_VALUE);
    let oct = 63 - v.leading_zeros(); // clz → 八度
    let base = 1u64 << oct;
    let sub = if oct >= HDR_SUB_BITS {
        ((v - base) >> (oct - HDR_SUB_BITS)) as u32
    } else {
        (v - base) as u32
    };
    (oct * HDR_SUB + sub) as usize
}

/// 桶下标 → 桶上界（含）。分位返回值 = 命中桶的**保守上界**
/// （R-0「桶只给保守上界」同口径——与真值差 < 桶宽 ≈ 3% 相对误差；
/// oct ≤ 5 桶宽 1 恰为精确值）。溢出桶 → `u64::MAX` 哨兵（命中即响亮，
/// 不假稳——§12 风险表「长尾进溢出桶 = p99 假稳」的反面）。
pub const fn bucket_upper(idx: usize) -> u64 {
    if idx >= HDR_OVERFLOW {
        return u64::MAX;
    }
    let oct = (idx / HDR_SUB as usize) as u32;
    let sub = (idx % HDR_SUB as usize) as u32;
    let base = 1u64 << oct;
    if oct >= HDR_SUB_BITS {
        base + (((sub + 1) as u64) << (oct - HDR_SUB_BITS)) - 1
    } else {
        base + sub as u64 // 子桶宽 1：上界 = 桶内唯一值
    }
}

/// rank = pm × n / 1000 + 1（1 起、帽 n；pm 帽 1000）。u128 中转防
/// （pm × n）溢出——`util.rs` EWMA 同款。
pub fn rank_of(pm: u64, n: u64) -> u64 {
    let pm = pm.min(1000);
    (((pm as u128) * (n as u128) / 1000) as u64 + 1).min(n)
}

/// 桶零初值常量（数组 repeat 操作数须 const——原子 !Copy）。
const ZERO_BUCKET: AtomicU32 = AtomicU32::new(0);

/// 单路 HDR 直方图（`#[repr(C)]`：将来 dump/导出面的 ABI 形状 day-one
/// 定型——§11「环槽 #[repr(C)] day-one 定型（HDR 桶与簇 util 环同
/// 纪律）」）。
#[repr(C)]
pub struct Hdr {
    /// 对数线性桶（下标 ⇔ [`bucket_of`]；写方限本核 ⇒ 写-写零争用）。
    pub(crate) buckets: [AtomicU32; HDR_BUCKETS],
    /// 总样本数（写序**末位** Release——见模块头并发纪律）。
    count: AtomicU64,
    /// 精确最小值（u64::MAX 哨兵 ⇔ 零样本；**原值**口径——不做钳位）。
    min: AtomicU64,
    /// 精确最大值（0 ⇔ 零样本；原值口径——`> HDR_MAX_VALUE` ⇒ 有样本
    /// 被钳位入顶桶 = 「长尾被 4.3s 封顶吃掉」的唯一可观测失真信号）。
    max: AtomicU64,
}

impl Hdr {
    pub const fn new() -> Self {
        Self {
            buckets: [ZERO_BUCKET; HDR_BUCKETS],
            count: AtomicU64::new(0),
            min: AtomicU64::new(u64::MAX),
            max: AtomicU64::new(0),
        }
    }

    /// 记录一个值（热路径：一次 clz + 一次移位 + 四次本核原子 RMW——
    /// per-CPU 槽单写者无争用，零分配零 panic）。
    ///
    /// - `value == 0` 静默拒（值域 1 起——延迟 0 无分位意义）。
    /// - `value > HDR_MAX_VALUE` **钳位入顶桶**（桶面 ≤ 2^28−1；极值面
    ///   记原值——[`max_value`] 超 [`HDR_MAX_VALUE`] 即失真信号）。
    pub fn record(&self, value: u64) {
        if value == 0 {
            return;
        }
        let idx = bucket_of(value);
        // 防御收敛：映射域已证 idx ≤ 895 < HDR_OVERFLOW（ktest HDR-B
        // 全域钉板执法）；真越界落溢出桶不越数组（热路径零 panic 面）。
        self.buckets[idx.min(HDR_OVERFLOW)].fetch_add(1, Ordering::Relaxed);
        self.min.fetch_min(value, Ordering::Relaxed);
        self.max.fetch_max(value, Ordering::Relaxed);
        // 末位 Release（读者先 Acquire 读 N——Σ桶 ≥ N 恒立，模块头纪律）。
        self.count.fetch_add(1, Ordering::Release);
    }

    /// 分位查询（**千分比**入参：p50 → 500、p99 → 990、p99.9 → 999；
    /// >1000 帽 1000）。整数近似：`rank = pm × N / 1000 + 1`（1 起、帽
    /// N），沿桶累计秩 first-up 扫描，命中桶返回保守上界
    /// （[`bucket_upper`]）。零样本 → 0（非 panic——空路查询合法）。
    pub fn percentile(&self, pm: u64) -> u64 {
        let n = self.total();
        if n == 0 {
            return 0;
        }
        let rank = rank_of(pm, n);
        self.percentile_rank(rank)
    }

    /// 第 rank 小样本所在桶的保守上界（rank ∈ 1..=N，越界帽到界内；零
    /// 样本 → 0）。累计秩实现核——跨核合并走 [`merged_percentile_of`] 同
    /// 式（桶位直和）。扫描必达：读者 N 先 Acquire 读 ⇒ Σ桶 ≥ N ≥ rank
    /// （写序纪律保证）；尾 `u64::MAX` = 不可达防御位。
    pub fn percentile_rank(&self, rank: u64) -> u64 {
        let n = self.total();
        if n == 0 {
            return 0;
        }
        let rank = rank.clamp(1, n);
        let mut cum = 0u64;
        for i in 0..HDR_BUCKETS {
            cum += self.buckets[i].load(Ordering::Relaxed) as u64;
            if cum >= rank {
                return bucket_upper(i);
            }
        }
        u64::MAX // 不可达（Σ桶 ≥ N ≥ rank）
    }

    /// 总样本数（读者 N 的 Acquire 读**最先**——模块头读序纪律）。
    pub fn total(&self) -> u64 {
        self.count.load(Ordering::Acquire)
    }

    /// 精确最小值（u64::MAX ⇔ 零样本——打印端 n>0 门控后必真值，
    /// telemetry `WakeHist` 哨兵同款）。
    pub fn min_value(&self) -> u64 {
        self.min.load(Ordering::Relaxed)
    }

    /// 精确最大值（0 ⇔ 零样本；原值口径——超 [`HDR_MAX_VALUE`] = 钳位
    /// 失真信号，打印端应 loud）。
    pub fn max_value(&self) -> u64 {
        self.max.load(Ordering::Relaxed)
    }

    /// 单桶计数读（dump/导出面原料；越界 → 0）。
    pub fn bucket(&self, idx: usize) -> u64 {
        if idx >= HDR_BUCKETS {
            return 0;
        }
        self.buckets[idx].load(Ordering::Relaxed) as u64
    }

    /// 清零回出生态（ktest 清场 / 基准多轮跑专用——**单写者语境调用**；
    /// `util_reset` 同款政策：生产路径无重置消费面。count 末位 Release
    /// ——清场后读者不见「Σ桶 < N」窗）。
    pub fn reset(&self) {
        for b in &self.buckets {
            b.store(0, Ordering::Relaxed);
        }
        self.min.store(u64::MAX, Ordering::Relaxed);
        self.max.store(0, Ordering::Relaxed);
        self.count.store(0, Ordering::Release);
    }

    /// 桶位直和合并分位（§9 核心问题「p99 跨核怎么并」的最小答案——
    /// 各桶只增不减，合并不引入排序误差、无额外缓冲；精确口径（样本
    /// 归并重排）留导出面后续刀）。`hdrs` = 参与合并的各核同路直方图；
    /// 空切片/全零样本 → 0。读序：各核 N 先 Acquire 读、桶后读——
    /// Σ合并桶 ≥ ΣN ≥ rank 同立。
    pub fn merged_percentile_of(hdrs: &[&Hdr], pm: u64) -> u64 {
        let n: u64 = hdrs.iter().map(|h| h.total()).sum();
        if n == 0 {
            return 0;
        }
        let rank = rank_of(pm, n);
        let mut cum = 0u64;
        for i in 0..HDR_BUCKETS {
            for h in hdrs {
                cum += h.buckets[i].load(Ordering::Relaxed) as u64;
            }
            if cum >= rank {
                return bucket_upper(i);
            }
        }
        u64::MAX // 不可达
    }

    /// 合并总样本数（Σ各核 N）。
    pub fn merged_total_of(hdrs: &[&Hdr]) -> u64 {
        hdrs.iter().map(|h| h.total()).sum()
    }

    /// 合并最小值（跳过空核的 u64::MAX 哨兵；全空 → u64::MAX）。
    pub fn merged_min_of(hdrs: &[&Hdr]) -> u64 {
        let mut m = u64::MAX;
        for h in hdrs {
            if h.total() > 0 {
                m = m.min(h.min_value());
            }
        }
        m
    }

    /// 合并最大值（原值口径；全空 → 0）。
    pub fn merged_max_of(hdrs: &[&Hdr]) -> u64 {
        let mut m = 0u64;
        for h in hdrs {
            m = m.max(h.max_value());
        }
        m
    }
}

// ---------------------------------------------------------------------------
// per-CPU 双路形状（存储/槽化机械留 sched）
// ---------------------------------------------------------------------------

/// per-CPU 槽数（v1 = 2 核——`util.rs::UTIL_NR_CPU` / `Topo::capacity`
/// 定长数组口径同源；拓扑真化后扩）。
pub const HDR_NR_CPU: usize = 2;

/// 双路选择子（合并读面入参）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatKind {
    /// 切换延迟路。
    Switch,
    /// 唤醒延迟路。
    Wake,
}

/// per-CPU 双路直方图槽（64B 对齐 = A72 cacheline——写方限本核 ⇒ 跨核
/// 伪共享只可能在槽边界出现，对齐抹掉；槽内两路同核无伪共享问题）。
#[repr(C, align(64))]
pub struct CpuHdr {
    /// 切换延迟（上下文切换耗时；§9 三路基准之路 3「调度切换」）。
    pub switch_lat: Hdr,
    /// 唤醒延迟（block→wake；§9 三路基准之路 2「唤醒延迟」——BENCHWAKE
    /// 同口径事件的分位收割面）。
    pub wake_lat: Hdr,
}

impl CpuHdr {
    pub const fn new() -> Self {
        Self { switch_lat: Hdr::new(), wake_lat: Hdr::new() }
    }

    /// 双路选择投影（`pick` 机械的单源泛化——sched 侧 `pick` 消费）。
    pub fn pick(&self, k: LatKind) -> &Hdr {
        match k {
            LatKind::Switch => &self.switch_lat,
            LatKind::Wake => &self.wake_lat,
        }
    }
}

// ---------------------------------------------------------------------------
// host 测试面（cfg(test) 链 std——形态抄 berry-emface / berry-intface）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 桶可达性（oct<5 时 sub ≥ 2^oct 的桶值域空——base+sub 已入高八度）。
    fn reachable(idx: usize) -> bool {
        let oct = (idx / HDR_SUB as usize) as u32;
        let sub = (idx % HDR_SUB as usize) as u32;
        oct >= HDR_SUB_BITS || sub < (1u32 << oct)
    }

    /// 桶下界（base + sub×宽；与 bucket_upper 同式取 sub 端）。
    fn bucket_lower(idx: usize) -> u64 {
        let oct = (idx / HDR_SUB as usize) as u32;
        let sub = (idx % HDR_SUB as usize) as u32;
        let base = 1u64 << oct;
        if oct >= HDR_SUB_BITS {
            base + ((sub as u64) << (oct - HDR_SUB_BITS))
        } else {
            base + sub as u64
        }
    }

    /// 全域往返闭环（可达桶）：上界值/下界值经 bucket_of 必落回本桶
    /// （可达桶 × 2 端点——映射律全域钉板）；沿值域 bucket_of 单调不减。
    #[test]
    fn bucket_roundtrip_all_domain() {
        let mut last = 0u64;
        let mut prev_bucket = 0usize;
        for idx in 0..HDR_OVERFLOW {
            if !reachable(idx) {
                continue;
            }
            let upper = bucket_upper(idx);
            let lower = bucket_lower(idx);
            assert!(lower <= upper, "idx={idx} 下界 {lower} ≤ 上界 {upper}");
            assert_eq!(bucket_of(upper), idx, "idx={idx} 上界 {upper} 必落回本桶");
            assert_eq!(bucket_of(lower), idx, "idx={idx} 下界 {lower} 必落回本桶");
            // 可达桶上界沿下标严格递增（first-up 分位扫描的前提）。
            assert!(upper > last, "可达桶 {idx} 上界 {upper} 须 > 前可达桶 {last}");
            last = upper;
            // 单调性抽锚：桶下界值域内 bucket_of ≥ 前桶。
            assert!(bucket_of(lower) >= prev_bucket);
            prev_bucket = bucket_of(lower);
        }
        assert_eq!(last, HDR_MAX_VALUE, "顶可达桶上界 = 值域上界 2^28−1");
    }

    /// 边界值锚：0 防御归桶 0；1→桶 0；2→桶 32（oct1 首）；31→143
    /// （oct4 sub15）；32→160（oct5 首）；63→191（oct5 顶）；64→192
    /// （oct6 首子桶宽 2）；65→192（与 64 同桶）；2^28−1 → 顶实桶 895；
    /// 2^28 与 u64::MAX 同钳位落 895（不落溢出位 896）。
    #[test]
    fn bucket_edges() {
        assert_eq!(bucket_of(0), 0, "v=0 防御归桶 0");
        assert_eq!(bucket_of(1), 0, "v=1 → oct0 sub0");
        assert_eq!(bucket_of(2), 32, "v=2 → oct1 sub0 = 32");
        assert_eq!(bucket_of(3), 33, "v=3 → oct1 sub1");
        assert_eq!(bucket_of(16), 128, "v=16 → oct4 sub0 = 128");
        assert_eq!(bucket_of(31), 143, "v=31 → oct4 sub15 = 143");
        assert_eq!(bucket_of(32), 160, "v=32 → oct5 sub0 = 160");
        assert_eq!(bucket_of(63), 191, "v=63 → oct5 sub31 = 191");
        assert_eq!(bucket_of(64), 192, "v=64 → oct6 sub0 = 192");
        assert_eq!(bucket_of(65), 192, "v=65 与 64 同桶（oct6 宽 2）");
        assert_eq!(bucket_of(67), 193, "v=67 → oct6 sub1");
        assert_eq!(bucket_of((1 << 28) - 1), 895, "值域顶 2^28−1 → 顶实桶 895");
        assert_eq!(bucket_of(1 << 28), 895, "2^28 钳位 → 895（不落溢出位）");
        assert_eq!(bucket_of(u64::MAX), 895, "u64::MAX 钳位 → 895");
        assert_eq!(bucket_upper(0), 1, "桶 0 上界 = 1（唯一值 1）");
        assert_eq!(bucket_upper(160), 32, "oct5 sub0 上界 = 32（宽 1 精确）");
        assert_eq!(bucket_upper(192), 65, "oct6 sub0 上界 = 65（宽 2 保守）");
        assert_eq!(bucket_upper(895), HDR_MAX_VALUE, "顶实桶上界 = 2^28−1");
        assert_eq!(bucket_upper(896), u64::MAX, "溢出桶上界 = MAX 哨兵");
        assert_eq!(bucket_upper(897), u64::MAX, "越界下标同 MAX（防御）");
    }

    /// Hdr 记账律：零样本哨兵（min=MAX/max=0/total=0/percentile=0）；
    /// record(0) 静默拒；单值极值原值口径；超值域钳位入顶桶但 max 记
    /// 原值（失真信号面）；多值 min/max/total 精确。
    #[test]
    fn hdr_accounting() {
        let h = Hdr::new();
        assert_eq!(h.total(), 0);
        assert_eq!(h.min_value(), u64::MAX, "零样本 min 哨兵");
        assert_eq!(h.max_value(), 0, "零样本 max 哨兵");
        assert_eq!(h.percentile(500), 0, "空路查询 → 0 非 panic");

        h.record(0);
        assert_eq!(h.total(), 0, "record(0) 静默拒");

        h.record(100);
        assert_eq!(h.total(), 1);
        assert_eq!(h.min_value(), 100);
        assert_eq!(h.max_value(), 100);
        assert_eq!(h.bucket(bucket_of(100)), 1, "值 100 桶恰一记");
        assert_eq!(h.percentile(1), bucket_upper(bucket_of(100)), "pm=1 → 唯一样本桶上界");

        h.record(7);
        h.record((1 << 30) + 5); // 超值域：钳位入顶桶、max 记原值
        assert_eq!(h.total(), 3);
        assert_eq!(h.min_value(), 7);
        assert_eq!(h.max_value(), (1 << 30) + 5, "max 原值口径（失真信号）");
        assert_eq!(h.bucket(895), 1, "超值域钳位入顶实桶 895");
        assert_eq!(h.bucket(HDR_BUCKETS), 0, "越界桶读 → 0");
    }

    /// 分位秩律：rank_of 单式（pm 帽 1000、rank 1 起帽 n、u128 中转）；
    /// percentile first-up 扫描（升序灌 1..=20 各一，p50 rank 11 → 第
    /// 11 小 = 11——oct<5 桶宽 1 恰精确；p99.9 → 第 20 小 = 20）；rank
    /// 越界帽界内；宽桶命中返回保守上界。
    #[test]
    fn percentile_rank_law() {
        assert_eq!(rank_of(0, 10), 1, "pm=0 → rank 1 起");
        assert_eq!(rank_of(500, 10), 6, "500×10/1000+1 = 6");
        assert_eq!(rank_of(999, 10), 10, "999×10/1000+1 = 10 帽 n");
        assert_eq!(rank_of(1000, 10), 10, "pm=1000 → rank = n");
        assert_eq!(rank_of(5000, 10), 10, "pm 超 1000 帽 1000");
        assert_eq!(rank_of(500, u64::MAX), (u64::MAX / 2) + 1, "u128 中转不溢出");

        let h = Hdr::new();
        for v in 1..=20u64 {
            h.record(v);
        }
        assert_eq!(h.total(), 20);
        // 500×20/1000+1 = 11 → 第 11 小 = 11（oct3 sub3 桶宽 1 精确）。
        assert_eq!(h.percentile(500), 11, "p50 → rank 11 → 第 11 小 = 11");
        assert_eq!(h.percentile_rank(11), 11);
        assert_eq!(h.percentile(999), 20, "p99.9 → 第 20 小 = 20");
        assert_eq!(h.percentile_rank(0), 1, "rank 0 帽 1");
        assert_eq!(h.percentile_rank(999), 20, "rank 超 n 帽 n");

        // 宽桶保守上界：64/65 同桶 192（宽 2），命中返回上界 65 非 64。
        let g = Hdr::new();
        g.record(64);
        g.record(65);
        assert_eq!(g.bucket(192), 2, "64/65 同桶 192");
        assert_eq!(g.percentile(500), 65, "同桶两值 → 保守上界 65");
    }

    /// reset 回出生态：全字段回 new() 等价态（清场后可重灌）。
    #[test]
    fn reset_to_birth_state() {
        let h = Hdr::new();
        h.record(5);
        h.record((1 << 28) + 1);
        assert!(h.total() > 0);
        h.reset();
        assert_eq!(h.total(), 0);
        assert_eq!(h.min_value(), u64::MAX);
        assert_eq!(h.max_value(), 0);
        let mut nonzero = false;
        for i in 0..HDR_BUCKETS {
            if h.bucket(i) != 0 {
                nonzero = true;
            }
        }
        assert!(!nonzero, "reset 后全桶归零");
        h.record(9); // 重灌可用
        assert_eq!(h.total(), 1);
    }

    /// 合并读面数学：桶位直和同式（Σ各核 N、跨核秩扫描、空核哨兵跳过、
    /// 全空 → 0/MAX 口径）——`merged_percentile` 机械的纯面钉板。
    #[test]
    fn merged_read_math() {
        let a = Hdr::new();
        let b = Hdr::new();
        assert_eq!(Hdr::merged_total_of(&[&a, &b]), 0);
        assert_eq!(Hdr::merged_percentile_of(&[&a, &b], 500), 0, "全空 → 0");
        assert_eq!(Hdr::merged_min_of(&[&a, &b]), u64::MAX, "全空 min 哨兵");
        assert_eq!(Hdr::merged_max_of(&[&a, &b]), 0, "全空 max 哨兵");
        assert_eq!(Hdr::merged_percentile_of(&[], 500), 0, "空切片 → 0");

        // 核 A：1..=10；核 B：11..=20。合并 N=20，p50 rank 11 → 11。
        for v in 1..=10u64 {
            a.record(v);
        }
        for v in 11..=20u64 {
            b.record(v);
        }
        let hdrs = [&a, &b];
        assert_eq!(Hdr::merged_total_of(&hdrs), 20);
        assert_eq!(Hdr::merged_percentile_of(&hdrs, 500), 11, "合并 p50 → 第 11 小 = 11");
        assert_eq!(Hdr::merged_percentile_of(&hdrs, 999), 20);
        assert_eq!(Hdr::merged_min_of(&hdrs), 1);
        assert_eq!(Hdr::merged_max_of(&hdrs), 20);

        // 空核哨兵跳过：A 空不污染合并 min。
        let c = Hdr::new();
        let hdrs2 = [&c, &b];
        assert_eq!(Hdr::merged_min_of(&hdrs2), 11, "空核 u64::MAX 不入 min");
        assert_eq!(Hdr::merged_total_of(&hdrs2), 10);
    }

    /// CpuHdr 形状与投影：尺寸/对齐钉板（编译期 const assert 的运行期
    /// 复核）+ pick 双路投影正确 + LatKind 往返。
    #[test]
    fn cpuhdr_shape_and_pick() {
        assert_eq!(core::mem::size_of::<Hdr>(), 3616);
        assert_eq!(core::mem::align_of::<Hdr>(), 8);
        assert_eq!(core::mem::size_of::<CpuHdr>(), 7232);
        assert_eq!(core::mem::align_of::<CpuHdr>(), 64);
        assert_eq!(HDR_NR_CPU, 2, "v1 双核槽");

        let c = CpuHdr::new();
        c.switch_lat.record(10);
        c.wake_lat.record(20);
        c.wake_lat.record(30);
        assert_eq!(c.pick(LatKind::Switch).total(), 1);
        assert_eq!(c.pick(LatKind::Wake).total(), 2);
        assert_eq!(c.pick(LatKind::Wake).min_value(), 20);
        assert_eq!(c.pick(LatKind::Switch).max_value(), 10);
        assert_ne!(LatKind::Switch, LatKind::Wake);
    }
}
