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
//!
//! W3-Q14：表形状（`EmPoint`/`EmDomain`/常量）与拒收判据/装配/极值律
//! 单源已迁 `berry-emface`（host 可测钉板）；本模块保留全局表存储 +
//! 发布序机械，判定面消费 emface 纯函数（零第二实现——`PI_MAX_HOPS`
//! 迁 `berry-piface` 同款纪律）。

use core::sync::atomic::{AtomicBool, Ordering};

// ---- 形状数据结构 + 判定律（单源 = berry-emface；此处 re-export 供
// kernel/ktest 既有消费面零改动）----

pub use berry_emface::{
    em_domain_from, em_max_capacity_of, em_min_power_of, em_shape_ok, EmDomain, EmPoint,
    EM_DOMAINS_MAX, EM_POINTS_MAX,
};

/// 全局 EM 表。G0 单核静态例外——同 `band.rs::BANDS` 纪律：写面只经
/// `&raw mut` 裸指针（不立引用），注册点在 boot DAIF 屏蔽/ktest 窗内
/// 单写者；表已入册 `tools/writable_globals.txt`（scheduler 组）。
static mut EM_TABLE: [EmDomain; EM_DOMAINS_MAX] = [const { EmDomain::EMPTY }; EM_DOMAINS_MAX];

/// 域有效位（发布序：先写表后置 Release true——读者不见半表）。
static EM_VALID: [AtomicBool; EM_DOMAINS_MAX] = [const { AtomicBool::new(false) }; EM_DOMAINS_MAX];

/// 注册/重注册某域 EM 表（boot/ktest 期单写者；真机 DT 解析后调用——
/// v1.3 只手动注册）。形状护栏（非算法）：域号在册、点数 1..=上限、
/// cpu_mask 非零、频率严格升序（表不变量入口收口）——判据单源
/// `berry_emface::em_shape_ok`/`em_domain_from`。**拒收不动已注册
/// 内容**（失败路径零写入——已 valid 的域保持旧表）。重注册 = 整表
/// 覆盖（真机换占位表同通路）。返回是否受理。
pub fn em_register(domain: usize, cpu_mask: u32, points: &[EmPoint]) -> bool {
    if !em_shape_ok(domain, cpu_mask, points) {
        return false; // 形状拒收：零写入（判据单源 berry-emface）
    }
    let d = match em_domain_from(cpu_mask, points) {
        Some(d) => d,
        None => return false, // 不可达（em_shape_ok 已含同判据）；防御收口
    };
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
/// 即最高频点；未注册/越界 → 0）。极值投影律单源 `berry_emface::
/// em_max_capacity_of`（本面只做存储读桥接）。
pub fn em_max_capacity(domain: usize) -> u32 {
    match em_domain(domain) {
        Some(d) => em_max_capacity_of(&d), // npoints≥1 注册面保证
        None => 0,
    }
}

/// 最低频点功耗（域最低能耗/空闲基准——首点即最低频点；未注册/越界 → 0）。
pub fn em_min_power(domain: usize) -> u32 {
    match em_domain(domain) {
        Some(d) => em_min_power_of(&d),
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
