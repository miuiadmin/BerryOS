//! HDR（High Dynamic Range）延迟直方图——no_std 最小版（`调点调度-v1.3.md`
//! §9「真机框架——HDR 桶」预研建议③的第一刀：v1.2 遗留「真机 p99 需
//! HDR + ≥1e4 事件」的桶侧地基。1024 桶定容对齐、数据导出/收割面（§9
//! 核心问题：环 vs dump）、p50/p99/p99.9 断言写法留后续刀——本刀只立
//! **桶形状 + per-CPU 记账 + 分位查询**）。
//!
//! 形状 = 对数线性桶（Gil Tene HDR 同构的幂次桶）：28 八度 × 每八度
//! 32 子桶 + 1 溢出防御位 = 897 桶，值域 1..2^28 ticks（@62.5MHz ≈
//! 4.3s 封顶）；每八度 32 子桶 ⇒ 相对精度 ≈ 1/32（≈1.5 位有效数字，
//! p99 粒度够用）。桶映射 O(1)：一次 clz 定八度 + 移位取子桶。
//!
//! 与 `kernel/src/telemetry.rs` R-0 的 64 桶 log2 唤醒直方图的关系：
//! 那是**粗粒度形状面**（log2 一桶看不到八度内分布）；本模块是**细粒度
//! 分位面**（§9 三路基准 p50/p99/p99.9 门禁判据数字的收割载体）。两者
//! 并存不替代——看形状靠 R-0，收分位靠 HDR。
//!
//! 并发纪律（per-CPU 原子计数器模式，telemetry S-3c B13 / `util.rs`
//! 同源）：
//! - **写方限本核**（[`PER_CPU_HDR`] 按 `cpu_id` 槽化）——写-写零争用；
//!   [`CpuHdr`] 64B 对齐（A72 cacheline）防跨核伪共享。
//! - 全原子、无 `static mut`（不入 `writable_globals` 册）；写序 =
//!   桶/极值在前、N 的 Release 递增**最后**，读者 N 的 Acquire 读
//!   **最先** ⇒ Σ桶 ≥ N 恒立，分位累计秩扫描必达不耗尽
//!   （telemetry `wake_hist` 读序纪律同款）。
//! - **零浮点**：仓内 aarch64-unknown-none 构建禁 FP codegen
//!   （`.cargo/config.toml` `-fp-armv8,-neon`——雷册 §A1「内核新代码
//!   禁引入 FP/SIMD」），分位入参用**千分比整数**（p99.9 → 999）而非
//!   f64——`util.rs` EWMA 移位零浮点同纪律。
//! - 热路径零分配零 panic（[`record_switch_lat`]/[`record_wake_lat`]
//!   只做本核原子 RMW——遥测钩子「零分配、短平快」纪律）。

use berry_hal_aarch64 as hal;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

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

/// 值 → 桶下标（纯函数；[`Hdr::record`] 的 O(1) 映射独立成函数供 ktest
/// 全域钉板）。oct < 5 时子桶宽 1（值 < 32 精确落桶）；oct ≥ 5 时子桶宽
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

/// 桶零初值常量（数组 repeat 操作数须 const——原子 !Copy）。
const ZERO_BUCKET: AtomicU32 = AtomicU32::new(0);

/// 单路 HDR 直方图（`#[repr(C)]`：将来 dump/导出面的 ABI 形状 day-one
/// 定型——§11「环槽 #[repr(C)] day-one 定型（HDR 桶与簇 util 环同
/// 纪律）」）。
#[repr(C)]
pub struct Hdr {
    /// 对数线性桶（下标 ⇔ [`bucket_of`]；写方限本核 ⇒ 写-写零争用）。
    buckets: [AtomicU32; HDR_BUCKETS],
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

    /// 第 `rank` 小样本所在桶的保守上界（rank ∈ 1..=N，越界帽到界内；
    /// 零样本 → 0）。累计秩实现核——跨核合并走 [`merged_percentile`]
    /// 同式（桶位直和）。扫描必达：读者 N 先 Acquire 读 ⇒ Σ桶 ≥ N ≥
    /// rank（写序纪律保证）；尾 `u64::MAX` = 不可达防御位。
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
}

/// rank = pm × n / 1000 + 1（1 起、帽 n；pm 帽 1000）。u128 中转防
/// （pm × n）溢出——`util.rs` EWMA 同款。
fn rank_of(pm: u64, n: u64) -> u64 {
    let pm = pm.min(1000);
    (((pm as u128) * (n as u128) / 1000) as u64 + 1).min(n)
}

// ---- per-CPU 实例（切换延迟/唤醒延迟各一路——§9 三路基准之路 2/3）----

/// per-CPU 槽数（v1 = 2 核——`util.rs::UTIL_NR_CPU` / `Topo::capacity`
/// 定长数组口径同源；拓扑真化后扩）。
pub const HDR_NR_CPU: usize = 2;

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
}

/// per-CPU 实例（静态预储 .bss ≈ 2 × 7232B；boot 后永不增长）。
static PER_CPU_HDR: [CpuHdr; HDR_NR_CPU] = [CpuHdr::new(), CpuHdr::new()];

/// 热路径写：记一笔**本核**切换延迟（`cpu_id` 内读 + 拓扑槽外防御收敛
/// 尾槽——telemetry `ring_write` 同款；boot 期 mp 不明窗安全）。
pub fn record_switch_lat(v: u64) {
    let cpu = hal::smp::cpu_id().min(HDR_NR_CPU - 1);
    PER_CPU_HDR[cpu].switch_lat.record(v);
}

/// 热路径写：记一笔**本核**唤醒延迟。
pub fn record_wake_lat(v: u64) {
    let cpu = hal::smp::cpu_id().min(HDR_NR_CPU - 1);
    PER_CPU_HDR[cpu].wake_lat.record(v);
}

/// 取 `cpu` 槽只读引用（越界 None）——读面/测试面直索引用；生产热路径
/// 写走 [`record_switch_lat`]/[`record_wake_lat`]（写方限本核纪律）。
pub fn cpu_hdr(cpu: usize) -> Option<&'static CpuHdr> {
    PER_CPU_HDR.get(cpu)
}

/// 清 `cpu` 槽双路（ktest 清场；单写者语境——[`Hdr::reset`] 同政策）。
pub fn cpu_reset(cpu: usize) {
    if let Some(c) = PER_CPU_HDR.get(cpu) {
        c.switch_lat.reset();
        c.wake_lat.reset();
    }
}

// ---- 跨核合并读面（§9 核心问题「p99 跨核怎么并」的最小答案：**桶位
// 直和**后同式走累计秩——各桶只增不减，合并不引入排序误差、无额外
// 缓冲（7KB 栈副本免）；精确口径（样本归并重排）留导出面后续刀）----

/// 双路选择子（合并读面入参）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatKind {
    /// 切换延迟路。
    Switch,
    /// 唤醒延迟路。
    Wake,
}

fn pick<'a>(c: &'a CpuHdr, k: LatKind) -> &'a Hdr {
    match k {
        LatKind::Switch => &c.switch_lat,
        LatKind::Wake => &c.wake_lat,
    }
}

/// 合并分位（千分比入参，语义同 [`Hdr::percentile`]；各核 N 先 Acquire
/// 读、桶后读——Σ合并桶 ≥ ΣN ≥ rank 同立）。读路径成本 = 897 × 核数
/// 次 Relaxed 原子读（收集/打印面，非热路径）。
pub fn merged_percentile(k: LatKind, pm: u64) -> u64 {
    let n = merged_total(k);
    if n == 0 {
        return 0;
    }
    let rank = rank_of(pm, n);
    let mut cum = 0u64;
    for i in 0..HDR_BUCKETS {
        for c in &PER_CPU_HDR {
            cum += pick(c, k).buckets[i].load(Ordering::Relaxed) as u64;
        }
        if cum >= rank {
            return bucket_upper(i);
        }
    }
    u64::MAX // 不可达（Σ桶 ≥ ΣN ≥ rank）
}

/// 合并总样本数（Σ各核 N）。
pub fn merged_total(k: LatKind) -> u64 {
    PER_CPU_HDR.iter().map(|c| pick(c, k).total()).sum()
}

/// 合并最小值（跳过空核的 u64::MAX 哨兵；全空 → u64::MAX）。
pub fn merged_min(k: LatKind) -> u64 {
    let mut m = u64::MAX;
    for c in &PER_CPU_HDR {
        let h = pick(c, k);
        if h.total() > 0 {
            m = m.min(h.min_value());
        }
    }
    m
}

/// 合并最大值（原值口径；全空 → 0）。
pub fn merged_max(k: LatKind) -> u64 {
    let mut m = 0u64;
    for c in &PER_CPU_HDR {
        m = m.max(pick(c, k).max_value());
    }
    m
}
