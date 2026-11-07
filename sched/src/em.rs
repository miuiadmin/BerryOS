//! EM（Energy Model）能量模型表——**只落形状，不落算法**（v1.3
//! `调点调度-v1.3.md` §7 预研建议②：EM 表落形状不落算法；ADR #53
//! 「砍实现不砍协议位」纪律的调度面同款——形状在，算法可换可废）。
//!
//! 形状 = 每性能域（= 同频簇）一张 {freq, power, capacity} 升序点表
//! （Linux EM 同口径：freq kHz / power mW / capacity 与
//! [`crate::runqueue::Topo::capacity`] 同尺度、1024 = 域内最高频点满格）。
//! 算法（compute_energy/FEuC 选核公式）**不落仓库**——留 M2/真机
//! （§7 kill criteria 退路）。overutilized 阈值/簇 util 聚合是后续消费者，
//! 本模块只做数据结构 + 注册/查询。
//!
//! 数据源 v1.3 = boot/ktest 期手动注册（真机 DT/OPP 解析后由 boot 调
//! [`em_register`] 换注——填充路径今日即明日）；QEMU virt 默认表 =
//! [`em_register_qemu_virt`]（单域 CPU0-1 三点）。
//!
//! 并发纪律（同 [`crate::band::BANDS`]）：`static mut` 写面只经 `&raw
//! mut`/`&raw const` 裸指针（不立引用）；注册 = boot/ktest 期单写者
//! （boot DAIF 屏蔽窗内、IRQ 未开），读面发布序收口——先写表、后置
//! `EM_VALID`（Release），读者 Acquire 载入真值后才见表。表已入册
//! `tools/writable_globals.txt`（scheduler 组）。

use core::sync::atomic::{AtomicBool, Ordering};

/// 每性能域（= 同频簇）的能量模型条目。Day-one：只落形状（每簇 2-3 个
/// {freq, power} 点）——算法（compute_energy/FEuC）留 M2/真机。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmPoint {
    /// 频率（kHz——与 Linux EM 同口径）。
    pub freq_khz: u32,
    /// 功率（mW——该频点稳态功耗）。
    pub power_mw: u32,
    /// 容量（该频点的算力基准——DMIPS/MHz × freq / 1024 缩放；与
    /// `Topo::capacity` 字段同口径，1024 = 域内最高频点满格）。
    pub capacity: u32,
}

impl EmPoint {
    /// 零点（表尾无效槽填充位）。
    const ZERO: EmPoint = EmPoint { freq_khz: 0, power_mw: 0, capacity: 0 };
}

/// 每性能域的 EM 表频点数上限（OPP 表常见 3-5 点，取 4）。
pub const EM_POINTS_MAX: usize = 4;

/// 每性能域的 EM 表（点表按频率**严格升序**——入口 [`em_register`] 收口，
/// 尾点即最高频点、首点即最低频点）。PartialEq = ktest 形状逐位对拍面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmDomain {
    /// 域内 CPU 掩码（bit i = CPU i 属于此域）。
    pub cpu_mask: u32,
    /// EM 点表（按频率升序；有效前缀 = `[0..npoints]`，尾槽恒零填充）。
    pub points: [EmPoint; EM_POINTS_MAX],
    /// 有效点数（1..=EM_POINTS_MAX）。
    pub npoints: usize,
}

impl EmDomain {
    /// 空域（表初值；拒收路径不动已注册内容，无需「空化」写面）。
    const EMPTY: EmDomain = EmDomain {
        cpu_mask: 0,
        points: [EmPoint::ZERO; EM_POINTS_MAX],
        npoints: 0,
    };
}

/// 全局 EM 表域数上限（每域一条——QEMU virt 单域，RPi4 单域 4 核同构：
/// 1 用 1 备，真机大小核 = 2 域即满配）。
pub const EM_DOMAINS_MAX: usize = 2;

/// 全局 EM 表。G0 单核静态例外——同 `band.rs::BANDS` 纪律：写面只经
/// `&raw mut` 裸指针（不立引用），注册点在 boot DAIF 屏蔽/ktest 窗内
/// 单写者；表已入册 `tools/writable_globals.txt`（scheduler 组）。
static mut EM_TABLE: [EmDomain; EM_DOMAINS_MAX] = [const { EmDomain::EMPTY }; EM_DOMAINS_MAX];

/// 域有效位（发布序：先写表后置 Release true——读者不见半表）。
static EM_VALID: [AtomicBool; EM_DOMAINS_MAX] = [const { AtomicBool::new(false) }; EM_DOMAINS_MAX];

/// 注册/重注册某域 EM 表（boot/ktest 期单写者；真机 DT 解析后调用——
/// v1.3 只手动注册）。形状护栏（非算法）：域号在册、点数 1..=上限、
/// cpu_mask 非零、频率严格升序（表不变量入口收口）。**拒收不动已注册
/// 内容**（失败路径零写入——已 valid 的域保持旧表）。重注册 = 整表
/// 覆盖（真机换占位表同通路）。返回是否受理。
pub fn em_register(domain: usize, cpu_mask: u32, points: &[EmPoint]) -> bool {
    if domain >= EM_DOMAINS_MAX
        || points.is_empty()
        || points.len() > EM_POINTS_MAX
        || cpu_mask == 0
    {
        return false; // 形状拒收：零写入
    }
    for w in points.windows(2) {
        if w[0].freq_khz >= w[1].freq_khz {
            return false; // 非严格升序拒收（等频/降序同罪）
        }
    }
    let mut d = EmDomain::EMPTY;
    d.cpu_mask = cpu_mask;
    d.points[..points.len()].copy_from_slice(points);
    d.npoints = points.len();
    // SAFETY: boot/ktest 期 IRQ 屏蔽窗内单写者（同 BANDS 契约）；裸指针
    // 写不立引用。拒收路径已全 early-return，此处必为受理写。
    unsafe { *(&raw mut EM_TABLE[domain]) = d };
    EM_VALID[domain].store(true, Ordering::Release); // 发布：表先于位可见
    true
}

/// 域是否已注册有效 EM 表（Acquire 载入——true 保证表内容已同步可见）。
pub fn em_valid(domain: usize) -> bool {
    domain < EM_DOMAINS_MAX && EM_VALID[domain].load(Ordering::Acquire)
}

/// 拷出某域 EM 表快照（ktest/消费者观测面；未注册/越界 → None）。
pub fn em_domain(domain: usize) -> Option<EmDomain> {
    if !em_valid(domain) {
        return None;
    }
    // SAFETY: 只读拷出（Copy 整取）；valid Acquire 已同步注册写，且写者
    // 只在 boot 屏蔽窗内（读窗内无重注册踩踏——同 policy.rs SNAPS 读纪律）。
    Some(unsafe { *(&raw const EM_TABLE[domain]) })
}

/// 最高频点容量（overutilized 阈值/选核的容量基准——升序不变量下尾点
/// 即最高频点；未注册/越界 → 0）。
pub fn em_max_capacity(domain: usize) -> u32 {
    match em_domain(domain) {
        Some(d) => d.points[d.npoints - 1].capacity, // npoints≥1 注册面保证
        None => 0,
    }
}

/// 最低频点功耗（域最低能耗/空闲基准——首点即最低频点；未注册/越界 → 0）。
pub fn em_min_power(domain: usize) -> u32 {
    match em_domain(domain) {
        Some(d) => d.points[0].power_mw,
        None => 0,
    }
}

/// QEMU virt 默认表：单域 CPU0-1、三点 250M/500M/1G（TCG 对称核建模
/// 不了大小核——§7「仿真不对称」的真机前占位形态：容量按频点缩放、
/// 功耗随频超线性，只验机制不验收益）。
pub fn em_register_qemu_virt() -> bool {
    em_register(
        0,
        0b11,
        &[
            EmPoint { freq_khz: 250_000, power_mw: 5, capacity: 400 },
            EmPoint { freq_khz: 500_000, power_mw: 15, capacity: 750 },
            EmPoint { freq_khz: 1_000_000, power_mw: 50, capacity: 1024 },
        ],
    )
}
