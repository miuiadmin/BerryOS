//! HDR（High Dynamic Range）延迟直方图——no_std 机械壳（W3-Q16 外迁后）。
//!
//! 桶映射律/分位秩/形状常量/`Hdr`·`CpuHdr` 原子直方图已单源外迁
//! `berry-hdrface`（host 可测钉板——sched 依赖 HAL global_asm 只
//! aarch64-unknown-none 可编，纯数学面 host 测不可达的债由此清偿）。
//! 本文件只留**裸并发机械**：per-CPU 实例存储（`static
//! PER_CPU_HDR`）、HAL `cpu_id` 槽化、热路径写入口与跨核合并读面
//! （`merged_*` 经 `Hdr::merged_*_of` 同式单源）——判定律与形状
//! 全部 `pub use` re-export 自 `berry_hdrface`（wqmatch/emface 同款
//! 纪律：改 face crate 的语义 = 改 sched 行为）。
//!
//! 并发纪律不变（per-CPU 原子计数器模式，telemetry S-3c B13 /
//! `util.rs` 同源）：写方限本核、全原子无 `static mut`、写序末位
//! Release / 读序 N 先 Acquire——详见 `berry-hdrface` 模块头。

use berry_hal_aarch64 as hal;

// per-CPU 实例（静态预储 .bss ≈ 2 × 7232B；boot 后永不增长）。
static PER_CPU_HDR: [berry_hdrface::CpuHdr; berry_hdrface::HDR_NR_CPU] =
    [berry_hdrface::CpuHdr::new(), berry_hdrface::CpuHdr::new()];

/// 热路径写：记一笔**本核**切换延迟（`cpu_id` 内读 + 拓扑槽外防御收敛
/// 尾槽——telemetry `ring_write` 同款；boot 期 mp 不明窗安全）。
pub fn record_switch_lat(v: u64) {
    let cpu = hal::smp::cpu_id().min(berry_hdrface::HDR_NR_CPU - 1);
    PER_CPU_HDR[cpu].switch_lat.record(v);
}

/// 热路径写：记一笔**本核**唤醒延迟。
pub fn record_wake_lat(v: u64) {
    let cpu = hal::smp::cpu_id().min(berry_hdrface::HDR_NR_CPU - 1);
    PER_CPU_HDR[cpu].wake_lat.record(v);
}

/// 取 `cpu` 槽只读引用（越界 None）——读面/测试面直索引用；生产热路径
/// 写走 [`record_switch_lat`]/[`record_wake_lat`]（写方限本核纪律）。
pub fn cpu_hdr(cpu: usize) -> Option<&'static berry_hdrface::CpuHdr> {
    PER_CPU_HDR.get(cpu)
}

/// 清 `cpu` 槽双路（ktest 清场；单写者语境——`Hdr::reset` 同政策）。
pub fn cpu_reset(cpu: usize) {
    if let Some(c) = PER_CPU_HDR.get(cpu) {
        c.switch_lat.reset();
        c.wake_lat.reset();
    }
}

// ---- 跨核合并读面（§9 核心问题「p99 跨核怎么并」——桶位直和同式，
// 数学已在 berry_hdrface::Hdr::merged_*_of 单源钉板；此处只做槽收集）----

fn pick_all(k: LatKind) -> [&'static berry_hdrface::Hdr; berry_hdrface::HDR_NR_CPU] {
    let mut out = [&PER_CPU_HDR[0].switch_lat; berry_hdrface::HDR_NR_CPU];
    for (i, c) in PER_CPU_HDR.iter().enumerate() {
        out[i] = c.pick(k);
    }
    out
}

/// 合并分位（千分比入参，语义同 `Hdr::percentile`；各核 N 先 Acquire
/// 读、桶后读——Σ合并桶 ≥ ΣN ≥ rank 同立）。读路径成本 = 897 × 核数
/// 次 Relaxed 原子读（收集/打印面，非热路径）。
pub fn merged_percentile(k: LatKind, pm: u64) -> u64 {
    berry_hdrface::Hdr::merged_percentile_of(&pick_all(k), pm)
}

/// 合并总样本数（Σ各核 N）。
pub fn merged_total(k: LatKind) -> u64 {
    berry_hdrface::Hdr::merged_total_of(&pick_all(k))
}

/// 合并最小值（跳过空核的 u64::MAX 哨兵；全空 → u64::MAX）。
pub fn merged_min(k: LatKind) -> u64 {
    berry_hdrface::Hdr::merged_min_of(&pick_all(k))
}

/// 合并最大值（原值口径；全空 → 0）。
pub fn merged_max(k: LatKind) -> u64 {
    berry_hdrface::Hdr::merged_max_of(&pick_all(k))
}

// 判定律/形状单源 re-export（消费面零改动——ktest/kernel 侧 import 路径
// 不变；sched/src/lib.rs 的 `pub use hdr::{...}` 全名单继续成立）。
pub use berry_hdrface::{
    bucket_of, bucket_upper, rank_of, CpuHdr, Hdr, LatKind, HDR_BUCKETS, HDR_MAX_VALUE,
    HDR_NR_CPU, HDR_OCTAVES, HDR_OVERFLOW, HDR_SUB, HDR_SUB_BITS,
};
