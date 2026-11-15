//! berry-polface —— policy O(1) 快照查表数学 + SchedAttr 位选律纯函数面（W3-Q20）。
//!
//! **动机**：v1.3 策略注入三槽 RCU 换表（`sched/src/policy.rs`）的查表数学
//! ——黄金比例乘移散列、开放寻址线性探测缺席判、first-wins 插入/窗满拒收、
//! EPP 两位打包——与 `kernel/src/process.rs` 的 SchedAttr 默认∪覆盖位选律，
//! 此前只活在各自模块里（sched 依赖 HAL global_asm 只 aarch64-unknown-none
//! 可编、process.rs 在 kernel crate 同样 host 不可达），host 测不可达、ktest
//! 只覆盖 boot 相。散列分布律、探测窗不变式（「键在槽 i 当且仅当其探测序列
//! 0..i 全被占」——空槽早退可靠性的根）、重复键 first-wins、TBL_MAX 截尾、
//! 位选律 FREEZE_READY 特判的回归面需要一张 host 可测的**钉板**：本 crate
//! 把这些数学按值域抽成纯函数 + 表驱动测试，`sched`/`kernel` 侧消费同一
//! 函数（零第二实现——抽离即唯一事实源，非镜像；bandface/dtbface 同款
//! 纪律，先例族第 14 件）。
//!
//! 抽离面（与消费侧逐字对齐，改这里的语义 = 改消费侧行为）：
//! - [`BUCKETS`]/[`PROBE_MAX`]/[`TBL_MAX`]/[`WEIGHT_MAX`]/[`KEY_EMPTY`]/
//!   [`EPP_HAS`]/[`EPP_VAL`]:常量族单源（自 sched/src/policy.rs 迁此，sched
//!   `pub use` re-export——既有引用路径零改动）。
//! - [`bucket_of`]:黄金比例乘移散列（乘 2^64/φ 取高 8 位 = 桶号；`>> 56`
//!   与 BUCKETS = 2^8 绑定）。
//! - [`lookup_o1`]:开放寻址线性探测查表——命中返槽、空槽早退（缺席判定
//!   可靠性根 = 插入与查表同窗同序列）、窗尽保守缺席。
//! - [`snap_insert`]:快照插槽——重复键 first-wins / 首个空槽写入 /
//!   窗满拒收；返回 [`InsertOut`] 三态 + build 点夹紧。
//! - [`epp_pack`]/[`epp_unpack`]:EPP 两位打包律（bit0 = 有无覆盖，bit1 =
//!   取值——`Option<EppHint>` 的两位表示）。
//! - [`SchedAttr`]/[`SA_FLAG_*`]:调度属性声明与 flags 位（自 kernel
//!   process.rs 单源迁此，kernel re-export——ABI §7.2 位词汇表）+ 默认∪
//!   覆盖位选律 [`SchedAttr::merge`]（FREEZE_READY 只可能来自 Thread 覆盖
//!   ——over 未置位即从 out 清除的特判）。
//! - [`HazardGate`]:多槽换表 hazard-lite 并发协议律（读者验证环/写者
//!   避让扫描/发布定序——调度终窗第一刀新增，sched policy.rs shadow 谱系
//!   消费；host 线程竞争钉板在测试面）。
//!
//! 与冻结面的关系：SNAPS/SHD `static mut` 槽静态区 + POLICY_LOCK + promote
//! 4KiB 受控拷贝留 sched（纪律不变）——本面钉「表怎么散列/怎么探测/怎么
//! 插/属性怎么合」的数定律，不钉槽存储。**换表并发协议律例外单源在此**
//! （调度终窗第一刀，SMP-01 §5.1）：[`HazardGate`]（读者 hazard 验证环 +
//! 写者避让扫描 + 发布定序——shadow 侧撕裂窗闭合件）协议判定律单源在
//! 本面、host 线程竞争可测（sched 依赖 HAL 只 aarch64 可编——先例族
//! W3-Q20 同款「抽离即唯一事实源」纪律）；槽数据静态区与裸指针 SAFETY
//! 仍留消费侧。EppHint 单源在 berry-iprofile（W3-Q1），本 crate 消费不
//! 二写。
//!
//! **测试向量钉板纪律**：散列锚点值（如 key 1 → 158、key 2 → 60）是把
//! `0x9E3779B97F4A7C15` 乘移律冻结进测试——改散列常量/移位量而不改锚点
//! = 测试必红，锚点即散列指纹。
//!
//! 钉板向量（先经独立 Python 仿真验证再落测试）：饱和窗组 = keys
//! [112,201,57,146,2,91,324,36]（桶 56..=63 每桶恰一键，八插后窗 56..=63
//! 全占）；窗满 victim = 345（桶 56，探测序列全占 → 窗尽缺席 + 插入
//! ProbeFull）；回绕组 = 233（桶 0）/144（桶 255）/377（桶 255，回绕落
//! 槽 1）；早退缺席 = 1313（桶 122，槽 126 空早退）。

#![cfg_attr(not(test), no_std)]

use berry_iprofile::EppHint;
// 换表槽门协议面（HazardGate）——core 原子族，no_std/host 双可编。
use core::sync::atomic::{fence, AtomicU8, Ordering};

// ---------------------------------------------------------------------------
// 常量族（单源迁此——sched policy.rs / kernel process.rs re-export）
// ---------------------------------------------------------------------------

/// 快照桶数（2^8——与 [`bucket_of`] 的移位量 56 绑定，改桶数须同步改散列）。
pub const BUCKETS: usize = 256;
/// 开放寻址线性探测步数上界（首桶 + 7 步；插入/查表同窗——插入必落在查表
/// 窗内，查表遇空槽早退才可靠，见 [`lookup_o1`]）。
pub const PROBE_MAX: usize = 8;
/// 单次绑定收表上限（装载因子 ≤ 50%——线性探测聚簇概率的上界控制）。
pub const TBL_MAX: usize = 128;
/// 权重微调包络（±）：注入值由内核**夹紧**到本界内（动作空间③的 min/max
/// 夹紧——包络是内核执法，不信任注入方自律）。
pub const WEIGHT_MAX: i32 = 64;

/// [`Slot::key`] 的空槽哨兵：0 = 空槽（`key == 0` 不可注入——内核任务
/// `proc_tag = 0` 本就不可注入，注入条目带 0 键直接丢弃）。
pub const KEY_EMPTY: u64 = 0;

/// [`Slot::epp_flags`] 位定义：bit0 = 带 EPP 覆盖，bit1 = EPP 取值
/// （0 = Efficiency / 1 = Perf）——`Option<EppHint>` 的两位打包。
pub const EPP_HAS: u8 = 1 << 0;
pub const EPP_VAL: u8 = 1 << 1;

/// [`Slot::band`] 的「无意见」值（M4 band 选择位——ADR #42 第二段骨架，
/// 2026-11-15）：0 = 不选带（缺省，与 placement 的 0 词汇同形）；有效域 =
/// ×4 现役带 id 0x01..=0x04（带键 = intent 编码，`berry-iprofile::
/// band_enabled`/`band_id_of_intent` 单源；0x05 冻结、0x00 无专带）。
/// **纯数据位**：本 crate 只存取不执法——词汇执法与写路径随 v3
/// `policy_bind` 协议落（v1 内核 ABI `PolicyEntryABI._rsv` 仍整 16 位保留
/// 且非零拒收，布局兼容不破：band 落偏移 14 = 旧 `_rsv` 低字节）。
pub const BAND_SEL_NONE: u8 = 0;

/// 桶数须为 2 的幂（探测用 `& (BUCKETS-1)` 回绕 + 散列取高 log2 位）。
const _: () = assert!(BUCKETS.is_power_of_two());

// ---------------------------------------------------------------------------
// 槽与快照值形（#[repr(C)]：槽内 key 落偏移 0，可按位审计）
// ---------------------------------------------------------------------------

/// 快照槽：一条指令的扁平表示（16 字节）。`key == 0` = 空槽。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct Slot {
    pub key: u64,
    pub weight: i32,
    pub placement: u8,
    /// bit0 = has_epp，bit1 = epp_val（见 [`EPP_HAS`]/[`EPP_VAL`]）。
    pub epp_flags: u8,
    /// band 选择位（偏移 14；见 [`BAND_SEL_NONE`]——M4 骨架预留，v1 写
    /// 路恒 0）。
    pub band: u8,
    /// 保留（偏移 15；v3 协议扩位继续自此处取字节——band 取走旧 `_rsv`
    /// 低字节后余一字节）。
    pub _rsv: u8,
}

/// 槽尺寸钉板：band 位自旧 `_rsv: u16` 取低字节，16B 布局与既有
/// `PolicyEntryABI` 镜像（kernel syscall.rs）保持兼容——扩位不扩槽。
const _: () = assert!(core::mem::size_of::<Slot>() == 16);

impl Slot {
    /// 空槽（const 语境可用的全零字面量）。
    pub const EMPTY: Slot = Slot {
        key: KEY_EMPTY,
        weight: 0,
        placement: 0,
        epp_flags: 0,
        band: BAND_SEL_NONE,
        _rsv: 0,
    };
}

/// 快照槽数组形（固定容量 [`BUCKETS`]）——插入/查表操作的值域载体。
/// gen/count 等 meta 字段不属查表数学面，留消费侧 `Snap` 壳。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slots {
    pub slots: [Slot; BUCKETS],
}

impl Slots {
    /// 全空槽表。
    pub const fn empty() -> Slots {
        Slots { slots: [Slot::EMPTY; BUCKETS] }
    }
}

// ---------------------------------------------------------------------------
// 散列与查表数学（单源——sched policy.rs 消费）
// ---------------------------------------------------------------------------

/// 黄金比例乘移散列（乘 2^64/φ 取高 8 位 = 桶号）：键位（指针/标签）
/// 分布不均也摊平；`>> 56` 与 [`BUCKETS`] = 2^8 绑定。
#[inline]
pub fn bucket_of(key: u64) -> usize {
    (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 56) as usize
}

/// O(1) 查表原语（切片形）：开放寻址线性探测，≤[`PROBE_MAX`] 步；`key
/// == 0`（空槽哨兵）早退。**空槽早退可靠性**：插入与查表走同一探测序列、
/// 插入落**首个**空槽 → 键在槽 i 当且仅当其探测序列 0..i 全被占——查表
/// 先遇空槽即可断缺席。返命中槽下标（消费侧零拷贝取引用/值的自由面）。
pub fn lookup_slot_idx(slots: &[Slot], key: u64) -> Option<usize> {
    if key == KEY_EMPTY {
        return None; // 哨兵键：不可注入
    }
    let mut idx = bucket_of(key);
    for _ in 0..PROBE_MAX {
        let slot = &slots[idx];
        if slot.key == key {
            return Some(idx);
        }
        if slot.key == KEY_EMPTY {
            return None; // 探测序列遇空槽 → 键必不在表内（见上不变式）
        }
        idx = (idx + 1) & (BUCKETS - 1); // 2 的幂回绕（无除法）
    }
    None // 探测窗尽（表聚簇超窗）——缺席判定的保守侧
}

/// O(1) 查表（[`Slots`] 值形包装——探测环单源 [`lookup_slot_idx`]）。
pub fn lookup_o1(t: &Slots, key: u64) -> Option<Slot> {
    lookup_slot_idx(&t.slots, key).map(|i| t.slots[i])
}

/// [`snap_insert`] 三态出（消费侧按态入账 REJECT_* 计数器）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOut {
    /// 真插入：build 点夹紧后的权重（第一道；消费点再夹——三点执法）。
    Inserted { weight: i32 },
    /// 重复键：first-wins 拒（旧线性扫 `find` 语义保持）。
    Duplicate,
    /// 探测窗满：拒收（O(1) 查表上界的代价面）。
    ProbeFull,
}

/// 快照插槽原语（切片形）：探测窗内首个空槽写入；重复键 first-wins；窗满
/// 拒收。消费侧按 [`InsertOut`] 三态入账（REJECT_PROBE_FULL/CLAMP_WEIGHT
/// 归 sched 原子账——纯面无存储）。band 选择位原样透存（词汇执法不在本面
/// ——v1 写路恒 [`BAND_SEL_NONE`]，v3 `policy_bind` 协议开写真值）。
pub fn insert_slot(
    slots: &mut [Slot],
    key: u64,
    weight: i32,
    placement: u8,
    band: u8,
    epp: Option<EppHint>,
) -> InsertOut {
    let mut idx = bucket_of(key);
    for _ in 0..PROBE_MAX {
        let slot = &mut slots[idx];
        if slot.key == key {
            return InsertOut::Duplicate; // 重复键：先到先得
        }
        if slot.key == KEY_EMPTY {
            slot.key = key;
            // build 点夹紧（第一道；消费点 apply/weight_of 再夹——三点执法）。
            let clamped = weight.clamp(-WEIGHT_MAX, WEIGHT_MAX);
            slot.weight = clamped;
            slot.placement = placement;
            slot.band = band;
            slot.epp_flags = epp_pack(epp);
            return InsertOut::Inserted { weight: clamped };
        }
        idx = (idx + 1) & (BUCKETS - 1); // 2 的幂回绕（无除法）
    }
    InsertOut::ProbeFull // 探测窗满：拒收
}

/// [`Slots`] 值形包装（插入环单源 [`insert_slot`]）。
pub fn snap_insert(
    t: &mut Slots,
    key: u64,
    weight: i32,
    placement: u8,
    band: u8,
    epp: Option<EppHint>,
) -> InsertOut {
    insert_slot(&mut t.slots, key, weight, placement, band, epp)
}

/// EPP 两位打包律：`None` → 0 / `Some(Efficiency)` → [`EPP_HAS`] /
/// `Some(Perf)` → `EPP_HAS | EPP_VAL`。
#[inline]
pub fn epp_pack(e: Option<EppHint>) -> u8 {
    match e {
        None => 0,
        Some(EppHint::Efficiency) => EPP_HAS,
        Some(EppHint::Perf) => EPP_HAS | EPP_VAL,
    }
}

/// EPP 两位解包律（打包的逆——三位词汇 0 / EPP_HAS / EPP_HAS|EPP_VAL）。
#[inline]
pub fn epp_unpack(flags: u8) -> Option<EppHint> {
    if flags & EPP_HAS == 0 {
        None
    } else if flags & EPP_VAL != 0 {
        Some(EppHint::Perf)
    } else {
        Some(EppHint::Efficiency)
    }
}

// ---------------------------------------------------------------------------
// SchedAttr 位选律（单源迁此——kernel process.rs re-export）
// ---------------------------------------------------------------------------

/// 1.1/S1：调度属性声明（ABI `berry_sched_attr` 的内核侧浓缩——布局字段
/// 见 `调点调度-v1.1.md` §7.2；`flags=0` = 清除覆盖/回继承）。
///
/// `Process` 持有的是该进程线程的默认值；`Thread` 持有的是覆盖值。S2/S5
/// 消费侧（weight/intent/policy/deadline）从 **effective**（默认 ∪ 覆盖）
/// 读取；S1 只存不执法。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SchedAttr {
    /// 生效字段位（bit0 weight / bit1 intent / bit2 policy / bit3 freeze_clock /
    /// bit4 FREEZE_READY / bit5 reserve / bit6 deadline / bit7 budget）。
    pub flags: u16,
    pub weight: u32,
    pub policy: u8,
    pub intent: u8,
    pub freeze_clock: u8,
    pub min_ns: u64,
    pub max_ns: u64,
    pub deadline_ns: u64,
    pub budget_ns: u64,
    pub period_ns: u64,
}

/// `SchedAttr.flags` 位（ABI §7.2；与 berry-sys 侧同名位语义一致）。
pub const SA_FLAG_WEIGHT: u16 = 1 << 0;
pub const SA_FLAG_INTENT: u16 = 1 << 1;
pub const SA_FLAG_POLICY: u16 = 1 << 2;
pub const SA_FLAG_FREEZE_CLOCK: u16 = 1 << 3;
pub const SA_FLAG_FREEZE_READY: u16 = 1 << 4;
pub const SA_FLAG_RESERVE: u16 = 1 << 5;
pub const SA_FLAG_DEADLINE: u16 = 1 << 6;
pub const SA_FLAG_BUDGET: u16 = 1 << 7;

impl SchedAttr {
    /// Process 默认 ∪ Thread 覆盖：位置位取覆盖值，未置位继承默认；
    /// FREEZE_READY 只可能来自 Thread 覆盖（Process 声明该位非法）。
    pub fn merge(base: Self, over: Self) -> Self {
        let mut out = base;
        out.flags |= over.flags;
        if over.flags & SA_FLAG_WEIGHT != 0 { out.weight = over.weight; }
        if over.flags & SA_FLAG_INTENT != 0 { out.intent = over.intent; }
        if over.flags & SA_FLAG_POLICY != 0 { out.policy = over.policy; }
        if over.flags & SA_FLAG_FREEZE_CLOCK != 0 { out.freeze_clock = over.freeze_clock; }
        if over.flags & SA_FLAG_RESERVE != 0 {
            out.min_ns = over.min_ns;
            out.max_ns = over.max_ns;
        }
        if over.flags & SA_FLAG_DEADLINE != 0 { out.deadline_ns = over.deadline_ns; }
        if over.flags & SA_FLAG_BUDGET != 0 {
            out.budget_ns = over.budget_ns;
            out.period_ns = over.period_ns;
        }
        if over.flags & SA_FLAG_FREEZE_READY == 0 {
            out.flags &= !SA_FLAG_FREEZE_READY;
        }
        out
    }
}

// ---------------------------------------------------------------------------
// 换表槽门 hazard-lite 协议面（调度终窗第一刀——SMP-01 §5.1 shadow 缺口闭合）
// ---------------------------------------------------------------------------

/// hazard 位「未在读」哨兵：0xFF，远离槽号值域（0..=2）——写者扫描视同
/// 「该核未在读」。
const GATE_NONE: u8 = 0xFF;

/// 多槽 RCU 式换表的**并发协议判定律**单源（读者验证环 + 写者避让扫描 +
/// 发布定序）——槽存储本身留消费侧（`sched/src/policy.rs` 的 SNAPS/SHD
/// 静态区），本结构只持协调状态（活跃槽号 + 每核 hazard 位），全部公共
/// 方法零 `unsafe`（裸指针/槽解引用的 SAFETY 由消费侧挂——静态区纪律
/// 不越 crate 边界）。
///
/// **读者协议** [`HazardGate::reader_enter`]（hazard-lite 验证环，模板 =
/// policy.rs `active_snap` 先例）：load(Acquire) 活跃槽 → hazard[cpu]
/// store(Release) 发布在读槽号 → fence(Acquire) → 复读验证活跃槽未在
/// 发布窗内翻转；翻了 = 发布迟到（写者背靠背换表可回收该槽——撕裂窗
/// 现场），换新槽号重发再验。单（串行）写者下活跃位静默期必至，环有限
/// 步收敛。
///
/// **写者协议** [`HazardGate::pick_write`] + [`HazardGate::publish`]（避让
/// 扫描，模板 = policy.rs `publish` 先例）：扫描跳过①当前活跃槽（新读者
/// 持续进入）②各核 hazard 位（读者出窗不清的简化下含最近用——陈旧位只
/// 让写者多避让一拍，不损正确性）；满覆盖即 panic 响（wedge 优于静默
/// 跨核腐坏——协议违约当场抓住）。写者须**串行**（消费侧 POLICY_LOCK
/// 持锁窗内调用）——避让判定与槽写入无 CAS 防线。`SLOTS=3`/`CPUS=2`
/// 形态下「活跃 1 + hazard 收敛 ≤2」恒有空闲（读窗串行形态两核 hazard
/// 收敛同槽——policy.rs 三件套②同款论证）。
///
/// **定序配对**（撕裂窗闭合的根）：读者先发 hazard 后读槽数据、写者先
/// 读 hazard 后写槽——hazard 位 store(Release)/load(Acquire) 与两侧
/// fence 使在窗读者的槽永不被写者选中复用；发布侧「槽写完 →
/// fence(Release) → active.store(Release)」与读者 Acquire 载入配对——
/// 见新槽号必见新槽内容。
///
/// 消费侧：sched policy.rs shadow 谱系（SHD 三槽区 + SHD_GATE）——
/// SMP-01 §5.1「shadow 侧 hazard 未覆盖」缺口的机制闭合件。
pub struct HazardGate<const SLOTS: usize, const CPUS: usize> {
    /// 当前活跃槽号（0..SLOTS）。发布序见类型注；读取 Acquire。
    active: AtomicU8,
    /// 每核在读槽号发布位（读者 store(Release)、写者 load(Acquire)）。
    hazard: [AtomicU8; CPUS],
}

impl<const SLOTS: usize, const CPUS: usize> HazardGate<SLOTS, CPUS> {
    /// 全新门：活跃槽 0、各核 hazard 空。
    pub const fn new() -> Self {
        assert!(SLOTS <= 255, "HazardGate 槽号值域 u8");
        HazardGate {
            active: AtomicU8::new(0),
            hazard: [const { AtomicU8::new(GATE_NONE) }; CPUS],
        }
    }

    /// 读者进窗（hazard-lite 验证环）：发布本核在读槽号并验证活跃槽未在
    /// 发布窗内翻转，返回可安全解引用的活跃槽号——**调用侧持此槽号读槽
    /// 数据的整个窗内，串行写者的 [`HazardGate::pick_write`] 恒避让本槽**。
    /// `cpu` 的拓扑外防御收敛在此收口（超界核号钳到最后一位）。
    pub fn reader_enter(&self, cpu: usize) -> u8 {
        let c = cpu.min(CPUS - 1);
        let mut idx = self.active.load(Ordering::Acquire);
        loop {
            // 发布：本核在读 idx（值域即槽号——写者扫描直接比对）。
            self.hazard[c].store(idx, Ordering::Release);
            fence(Ordering::Acquire); // 发布先于验证读/数据读
            // 验证：hazard 生效前活跃槽未翻转。翻转 ⇒ 换当前槽号重发；
            // 串行写者下活跃位静默期必至，环有限步收敛。
            let recheck = self.active.load(Ordering::Acquire);
            if recheck == idx {
                return idx;
            }
            idx = recheck;
        }
    }

    /// 写者选槽（避让扫描）：返回非活跃且无 hazard 的槽号。**须在写者
    /// 串行锁窗内调用**（消费侧 POLICY_LOCK）；满覆盖 = 协议违约，当场
    /// panic 响（同 policy.rs `publish` 的 expect 执法）。
    pub fn pick_write(&self) -> u8 {
        let active = self.active.load(Ordering::Relaxed); // 锁内单写者自见
        for i in 0..SLOTS as u8 {
            if i == active {
                continue; // 活跃槽：新读者持续进入，不可覆写
            }
            let mut in_use = false;
            for h in &self.hazard {
                if h.load(Ordering::Acquire) == i {
                    in_use = true; // 他核在用（不清简化下含最近用）
                    break;
                }
            }
            if !in_use {
                return i;
            }
        }
        panic!("HazardGate 满覆盖：活跃+hazard 阻塞全部槽 = 协议违约");
    }

    /// 写者发布：**槽数据写完后**调用——fence(Release) 定序 + 活跃位翻转
    /// （Release）。与 [`HazardGate::reader_enter`] 的 Acquire 载入配对：
    /// 见新槽号必见新槽内容。
    pub fn publish(&self, w: u8) {
        fence(Ordering::Release);
        self.active.store(w, Ordering::Release);
    }

    /// 测试观察口：各核 hazard 位快照（Relaxed——诊断面，无同步语义；
    /// 竞争测试的「轮制新鲜度」自旋判据）。
    #[cfg(test)]
    pub fn hazard_probe(&self) -> [u8; CPUS] {
        let mut out = [0u8; CPUS];
        for (i, h) in self.hazard.iter().enumerate() {
            out[i] = h.load(Ordering::Relaxed);
        }
        out
    }

    /// 测试观察口：当前活跃槽号（Relaxed——诊断面）。
    #[cfg(test)]
    pub fn active_probe(&self) -> u8 {
        self.active.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// 表驱动测试（host 钉板——散列/探测/插入/打包/位选五面）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 计数三态的速记断言辅助：真插入返夹紧权重。
    fn inserted(out: InsertOut) -> i32 {
        match out {
            InsertOut::Inserted { weight } => weight,
            other => panic!("期望 Inserted，实得 {:?}", other),
        }
    }

    // ---- V1：散列分布律（黄金比例乘移 + 锚点指纹）----

    /// V1-a：全域无越界 + 关键锚点值钉板——锚点 = 散列指纹，改乘数/移位量
    /// 不改锚点即红。
    #[test]
    fn hash_domain_and_anchors() {
        for k in 0u64..5000 {
            assert!(bucket_of(k) < BUCKETS, "桶号越界：key {k}");
        }
        // 锚点表（python `(k*0x9E3779B97F4A7C15 mod 2^64)>>56` 独立算得）。
        let anchors: &[(u64, usize)] = &[
            (1, 158), (2, 60), (3, 218), (7, 83), (8, 241), (42, 245),
            (0xFF, 153), (0x100, 55), (0x1000, 119), (0xDEAD_BEEF, 0),
            (0x1234_5678_9ABC_DEF, 12), (0x8000_0000_0000_0000, 128),
            (0xFFFF_FFFF_FFFF_FFFF, 97),
        ];
        for &(k, expect) in anchors {
            assert_eq!(bucket_of(k), expect, "散列锚点漂移：key {k:#x}");
        }
    }

    /// V1-b：值域均匀性抽查——256 连续键覆盖 >220 桶（黄金比例乘移的摊平
    /// 律；退化散列（如 `key & 255`）恒覆盖 1 桶——错必红）。
    #[test]
    fn hash_spread() {
        let mut seen = [false; BUCKETS];
        for k in 1u64..=256 {
            seen[bucket_of(k)] = true;
        }
        let covered = seen.iter().filter(|&&s| s).count();
        assert!(covered > 220, "256 连续键桶覆盖 {covered} < 221：散列摊平退化");
    }

    // ---- V2：探测窗查表缺席判 ----

    /// V2-a：饱和窗向量——八键 [112,201,57,146,2,91,324,36] 桶恰 56..=63
    /// 每桶一键，八插后窗全占；已插键全命中；victim 345（桶 56）探测序列
    /// 全占 → 窗尽缺席（慢缺席形）。
    #[test]
    fn lookup_probe_window_saturate() {
        let mut t = Slots::empty();
        let sat: &[(u64, usize)] = &[
            (112, 56), (201, 57), (57, 58), (146, 59),
            (2, 60), (91, 61), (324, 62), (36, 63),
        ];
        for &(k, b) in sat {
            assert_eq!(bucket_of(k), b, "饱和窗键桶号漂移：key {k}");
            assert!(matches!(snap_insert(&mut t, k, 0, 0, BAND_SEL_NONE, None), InsertOut::Inserted { .. }));
        }
        for &(k, _) in sat {
            let got = lookup_o1(&t, k).expect("已插键查表缺席");
            assert_eq!(got.key, k);
        }
        assert_eq!(bucket_of(345), 56);
        assert_eq!(lookup_o1(&t, 345), None, "victim 345 须窗尽缺席");
    }

    /// V2-b：早退缺席（快缺席）——1313（桶 122）在 121..=125 已占、126 空的
    /// 表上探测两步即遇空槽早退缺席。哨兵键 0 恒 None。空表一步早退。
    #[test]
    fn lookup_miss_early_exit() {
        let mut t = Slots::empty();
        // 桶 121..=125 的五键（每桶恰一键；槽 126 留空）。
        let fill: &[(u64, usize)] = &[(237, 121), (93, 122), (182, 123), (38, 124), (127, 125)];
        for &(k, b) in fill {
            assert_eq!(bucket_of(k), b, "早退向量键桶号漂移：key {k}");
            assert!(matches!(snap_insert(&mut t, k, 0, 0, BAND_SEL_NONE, None), InsertOut::Inserted { .. }));
        }
        assert_eq!(bucket_of(1313), 122);
        assert_eq!(lookup_o1(&t, 1313), None, "1313 须槽 126 空槽早退缺席");
        // 哨兵键恒 None（不可注入面）。
        assert_eq!(lookup_o1(&t, 0), None);
        // 空表上任意键一步早退。
        assert_eq!(lookup_o1(&Slots::empty(), 237), None);
    }

    /// V2-c：回绕——桶 255 溢出回 0：B=377（桶 255）在 A=144（桶 255，落
    /// 槽 255）与 C=233（桶 0，落槽 0）之后插入，探测序列 255→0→1 落槽 1；
    /// 三键查表全命中（回绕不丢键）。
    #[test]
    fn lookup_wraparound() {
        let mut t = Slots::empty();
        assert_eq!(bucket_of(233), 0);
        assert_eq!(bucket_of(144), 255);
        assert_eq!(bucket_of(377), 255);
        assert!(matches!(snap_insert(&mut t, 233, 0, 0, BAND_SEL_NONE, None), InsertOut::Inserted { .. }));
        assert!(matches!(snap_insert(&mut t, 144, 0, 0, BAND_SEL_NONE, None), InsertOut::Inserted { .. }));
        assert!(matches!(snap_insert(&mut t, 377, 0, 0, BAND_SEL_NONE, None), InsertOut::Inserted { .. }));
        assert_eq!(t.slots[0].key, 233);
        assert_eq!(t.slots[255].key, 144);
        assert_eq!(t.slots[1].key, 377, "B 须回绕落槽 1（序列 255→0→1）");
        for k in [233u64, 144, 377] {
            assert!(lookup_o1(&t, k).is_some(), "回绕键 {k} 查表缺席");
        }
    }

    // ---- V3：first-wins / 窗满拒收 / 夹紧 ----

    /// V3-a：first-wins——同键二插返 [`InsertOut::Duplicate`] 且首值不覆写
    /// （旧线性扫 `find` 语义保持）。
    #[test]
    fn insert_first_wins() {
        let mut t = Slots::empty();
        assert_eq!(inserted(snap_insert(&mut t, 7, 5, 0, BAND_SEL_NONE, None)), 5);
        assert_eq!(snap_insert(&mut t, 7, 99, 0, BAND_SEL_NONE, None), InsertOut::Duplicate);
        let got = lookup_o1(&t, 7).unwrap();
        assert_eq!(got.weight, 5, "first-wins：重复插入不得覆写首值");
    }

    /// V3-b：窗满拒收——饱和窗（56..=63）后第 9 键 345（桶 56）返
    /// [`InsertOut::ProbeFull`] 且不入表（拒收键不可查）。
    #[test]
    fn insert_probe_full_reject() {
        let mut t = Slots::empty();
        for &k in &[112u64, 201, 57, 146, 2, 91, 324, 36] {
            assert!(matches!(snap_insert(&mut t, k, 0, 0, BAND_SEL_NONE, None), InsertOut::Inserted { .. }));
        }
        assert_eq!(snap_insert(&mut t, 345, 0, 0, BAND_SEL_NONE, None), InsertOut::ProbeFull);
        assert_eq!(lookup_o1(&t, 345), None, "拒收键不可查");
    }

    /// V3-c：build 点夹紧——越 ±WEIGHT_MAX 的注入值 build 时夹入（第一道
    /// 夹紧）；恰上下界不夹、零注入直通；夹紧值落槽可回。
    #[test]
    fn insert_clamp_at_build() {
        let mut t = Slots::empty();
        assert_eq!(inserted(snap_insert(&mut t, 11, 100, 0, BAND_SEL_NONE, None)), 64);
        assert_eq!(inserted(snap_insert(&mut t, 12, -100, 0, BAND_SEL_NONE, None)), -64);
        assert_eq!(inserted(snap_insert(&mut t, 13, 64, 0, BAND_SEL_NONE, None)), 64);
        assert_eq!(inserted(snap_insert(&mut t, 14, -64, 0, BAND_SEL_NONE, None)), -64);
        assert_eq!(inserted(snap_insert(&mut t, 15, 0, 0, BAND_SEL_NONE, None)), 0);
        assert_eq!(lookup_o1(&t, 11).unwrap().weight, 64);
    }

    // ---- V4：EPP 两位打包律 ----

    /// V4：三值词汇往返 + 域外位语义：`pack → unpack` 恒等；bit0 缺位恒
    /// None（语义位独裁——有取值无覆盖位 = 无效形）；高位噪声被掩。
    #[test]
    fn epp_pack_roundtrip() {
        use EppHint::*;
        assert_eq!(epp_pack(None), 0);
        assert_eq!(epp_pack(Some(Efficiency)), EPP_HAS);
        assert_eq!(epp_pack(Some(Perf)), EPP_HAS | EPP_VAL);
        for v in [None, Some(Efficiency), Some(Perf)] {
            assert_eq!(epp_unpack(epp_pack(v)), v, "EPP 打包往返");
        }
        assert_eq!(epp_unpack(0b100), None);
        assert_eq!(epp_unpack(EPP_VAL), None);
        assert_eq!(epp_unpack(EPP_HAS | 0b100), Some(Efficiency));
    }

    // ---- M4：band 选择位（ADR #42 第二段骨架——纯数据位钉板）----

    /// band 位插入→查表持久化（透存律：插入给什么查表读什么——词汇执法
    /// 不在纯面）；未给（[`BAND_SEL_NONE`]）与既有插入路径缺省恒 0；空槽
    /// [`Slot::EMPTY`] band 位 0。
    #[test]
    fn band_byte_persists_through_insert_lookup() {
        let mut t = Slots::empty();
        assert!(matches!(
            snap_insert(&mut t, 42, 0, 0, 0x02, None),
            InsertOut::Inserted { .. }
        ));
        assert_eq!(lookup_o1(&t, 42).unwrap().band, 0x02);
        // 缺省位 = 无意见。
        assert!(matches!(
            snap_insert(&mut t, 43, 0, 0, BAND_SEL_NONE, None),
            InsertOut::Inserted { .. }
        ));
        assert_eq!(lookup_o1(&t, 43).unwrap().band, BAND_SEL_NONE);
        // 全字节域透存（词汇门在 iprofile band_enabled——v3 写路才执法，
        // 本面不拒任何字节）。
        for b in [0x00u8, 0x01, 0x04, 0x05, 0xFF] {
            let mut t = Slots::empty();
            assert!(matches!(
                snap_insert(&mut t, 7, 0, 0, b, Some(EppHint::Perf)),
                InsertOut::Inserted { .. }
            ));
            assert_eq!(lookup_o1(&t, 7).unwrap().band, b);
            // 其余位不受 band 位沾染（同槽共存的既有轴零扰动）。
            assert_eq!(lookup_o1(&t, 7).unwrap().epp_flags, EPP_HAS | EPP_VAL);
        }
        assert_eq!(Slot::EMPTY.band, BAND_SEL_NONE);
    }

    // ---- V5：SchedAttr 位选律 ----

    /// V5-a：默认∪覆盖位选——八位全开覆盖逐字段断言 + 未置位字段保默认。
    #[test]
    fn sched_attr_merge_bit_selection() {
        let base = SchedAttr {
            flags: 0, weight: 1, policy: 1, intent: 1, freeze_clock: 1,
            min_ns: 1, max_ns: 1, deadline_ns: 1, budget_ns: 1, period_ns: 1,
        };
        let over = SchedAttr {
            flags: 0b1110_1111, // bit0..3+5..7 全开（bit4 FREEZE_READY 不置——特判位）
            weight: 2, policy: 2, intent: 2, freeze_clock: 2,
            min_ns: 2, max_ns: 2, deadline_ns: 2, budget_ns: 2, period_ns: 2,
        };
        let eff = SchedAttr::merge(base, over);
        assert_eq!(eff.flags, 0b1110_1111);
        assert_eq!(eff.weight, 2, "位置位取覆盖值");
        assert_eq!(eff.intent, 2);
        assert_eq!(eff.policy, 2);
        assert_eq!(eff.freeze_clock, 2);
        assert_eq!(eff.min_ns, 2);
        assert_eq!(eff.max_ns, 2);
        assert_eq!(eff.deadline_ns, 2);
        assert_eq!(eff.budget_ns, 2);
        assert_eq!(eff.period_ns, 2);
        // 未置位继承默认对照：INTENT 位关、其余开——intent 保 1。
        let over2 = SchedAttr { flags: 0b1110_1101, intent: 9, ..over };
        let eff2 = SchedAttr::merge(base, over2);
        assert_eq!(eff2.intent, 1, "未置位继承默认");
        assert_eq!(eff2.weight, 2);
    }

    /// V5-b：`flags=0` = 清除覆盖回默认（ktest ATTR-K 同款断言）。
    #[test]
    fn sched_attr_merge_zero_over() {
        let base = SchedAttr {
            flags: SA_FLAG_WEIGHT | SA_FLAG_POLICY, weight: 100, policy: 1, intent: 2,
            ..SchedAttr::default()
        };
        let eff = SchedAttr::merge(base, SchedAttr::default());
        assert_eq!(eff, base);
    }

    /// V5-c：FREEZE_READY 特判——base 置位 + over 未置位 → 清除（Thread
    /// 覆盖不声明 = 回 Process 默认，而 Process 不得声明该位）；over 置位
    /// → 保留（只可能来自 Thread）。
    #[test]
    fn sched_attr_merge_freeze_ready() {
        let base = SchedAttr { flags: SA_FLAG_FREEZE_READY, ..SchedAttr::default() };
        let eff = SchedAttr::merge(base, SchedAttr::default());
        assert_eq!(eff.flags & SA_FLAG_FREEZE_READY, 0, "over 未声明 FREEZE_READY 须清除");
        let over = SchedAttr { flags: SA_FLAG_FREEZE_READY, ..SchedAttr::default() };
        let eff2 = SchedAttr::merge(SchedAttr::default(), over);
        assert_ne!(eff2.flags & SA_FLAG_FREEZE_READY, 0, "over 声明 FREEZE_READY 须保留");
    }

    /// V5-d：字面量覆盖对（ktest test_sched_attr 同款向量）——merge 结果
    /// 全字段断言。
    #[test]
    fn sched_attr_merge_ktest_vector() {
        let base = SchedAttr {
            flags: SA_FLAG_WEIGHT | SA_FLAG_POLICY,
            weight: 100, policy: 1, intent: 2,
            ..SchedAttr::default()
        };
        let over = SchedAttr {
            flags: SA_FLAG_INTENT | SA_FLAG_POLICY,
            intent: 4, policy: 3,
            ..SchedAttr::default()
        };
        let eff = SchedAttr::merge(base, over);
        assert_eq!(eff.flags, SA_FLAG_WEIGHT | SA_FLAG_INTENT | SA_FLAG_POLICY);
        assert_eq!((eff.weight, eff.intent, eff.policy), (100, 4, 3));
    }

    // ---- 换表槽门协议面：确定性律 + 背靠背双写竞争钉板 ----

    use std::hint::black_box;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize};
    use std::sync::Mutex;

    /// V6-a：确定性协议律——reader_enter 恒返当前活跃槽；pick_write 避让
    /// 活跃槽**与**各核 hazard 位（含读者出窗后的陈旧位——「不清简化」的
    /// 避让面）；满覆盖 panic 响（should_panic 子测试）。
    #[test]
    fn hazard_gate_avoidance_law() {
        let g: HazardGate<3, 2> = HazardGate::new();
        // 初始：活跃槽 0；两核读者进窗恒 0（hazard 双双落 0）。
        assert_eq!(g.reader_enter(0), 0);
        assert_eq!(g.reader_enter(1), 0);
        // 写者：活跃 0 + hz{0,0} → 首个空闲 = 1。
        assert_eq!(g.pick_write(), 1);
        g.publish(1);
        assert_eq!(g.reader_enter(0), 1, "翻转后读者须见新槽");
        // 写者：活跃 1 + hz{1, 0}——cpu1 的陈旧位 0 被避让 → 只剩 2。
        assert_eq!(g.pick_write(), 2, "须避让 cpu1 陈旧 hazard 位 0");
        g.publish(2);
        assert_eq!(g.reader_enter(1), 2);
        // 写者：活跃 2 + hz{1, 2}——槽 0 已无人读 → 回收 0。
        assert_eq!(g.pick_write(), 0);
        g.publish(0);
        assert_eq!(g.reader_enter(0), 0);
        // 拓扑外防御收敛：超界核号钳到最后一位，不 panic。
        assert_eq!(g.reader_enter(99), 0, "cpu 越界须钳位收口");
    }

    /// V6-b：满覆盖 canary——活跃 + 两核 hazard 三槽全阻塞时写者当场响
    /// （wedge 优于静默腐坏——policy.rs `publish` expect 执法同款）。
    #[test]
    #[should_panic(expected = "HazardGate 满覆盖")]
    fn hazard_gate_full_block_canary() {
        let g: HazardGate<3, 2> = HazardGate::new();
        assert_eq!(g.reader_enter(0), 0); // hz0 = 0
        assert_eq!(g.pick_write(), 1);
        g.publish(1);
        assert_eq!(g.reader_enter(1), 1); // hz1 = 1
        assert_eq!(g.pick_write(), 2); // 避让 {0,1}
        g.publish(2); // 活跃 2 + hz{0,1} = 三槽全阻塞
        let _ = g.pick_write(); // 须 panic 响
    }

    /// 竞争载荷几何（与 sched policy.rs SHD 消费面同形）：三槽 + 两核
    /// hazard；槽载荷 = 128×u64 全行同 tag（tag = 发布世代）——撕裂 =
    /// 同一读窗内出现两种 tag；tag 回退 = 读到被回收复写的陈旧槽。
    const R_SLOTS: usize = 3;
    const R_CPUS: usize = 2;
    const R_LEN: usize = 128;
    /// 轮数：每轮一对背靠背换表（两写者互抢串行锁各写一表）。
    const R_ROUNDS: usize = 3000;

    /// V6-c：**修复协议**下的背靠背双写竞争——两写者（Mutex 串行 =
    /// POLICY_LOCK 同形契约）+ 两读者（全程紧循环、轮间无阻塞），断言
    /// 零撕裂 + tag 单调不回退（「要么旧表要么新表」的强形式：读者按
    /// 发布序见整表）。
    ///
    /// **轮制新鲜度**（满覆盖 canary 不响的构造性保证——纯速度竞争下
    /// 读者 hazard 陈旧可跨两世代、三槽满覆盖可达〔首轮 V6-c 实测〕，
    /// 协议 canary 前置「读窗串行形态两核 hazard 收敛同槽」须由测试侧
    /// 兑现）：每轮放行前主线程自旋等两读者 hazard 均收敛到当前活跃槽
    /// ——此后 W1 阻塞集 = {活跃}（必空两槽）、W2 阻塞集 ⊆ {旧活跃,
    /// 新活跃}（必空一槽），**背靠背两写恒有空闲槽**；轮内读者自由竞速
    /// 穿过撕裂窗（负对照 V6-d 证载荷灵敏度）。
    #[test]
    fn hazard_swap_no_tear_backtoback_writers() {
        static GATE: HazardGate<R_SLOTS, R_CPUS> = HazardGate::new();
        // 槽存储（消费侧 static mut SHD 的测试同形；raw 指针纪律同内核）。
        static mut ROWS: [[u64; R_LEN]; R_SLOTS] = [[0; R_LEN]; R_SLOTS];
        static STOP: AtomicBool = AtomicBool::new(false);
        static TAG: AtomicU64 = AtomicU64::new(2);
        static TORN: AtomicUsize = AtomicUsize::new(0);
        static REGRESS: AtomicUsize = AtomicUsize::new(0);
        static SERIAL: Mutex<()> = Mutex::new(());

        // SAFETY(测试侧)：写者只写 pick_write 避让后的槽（协议保证在窗
        // 读者的槽恒被避让——这正是被测定理）；读者只读 reader_enter
        // 验证后的槽。单测原生跑（非 MIRI）——协议破防时数据竞争即
        // 撕裂观测面本身。
        unsafe {
            let init: *mut [u64; R_LEN] = &raw mut ROWS[0];
            for j in 0..R_LEN {
                (*init)[j] = 1; // 初始活跃槽 = tag 1（GATE 活跃位初值 0）
            }
        }

        let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let go_rx = std::sync::Arc::new(std::sync::Mutex::new(go_rx));
        let done_tx = std::sync::Arc::new(std::sync::Mutex::new(done_tx));
        let readers: Vec<_> = (0..R_CPUS)
            .map(|cpu| {
                std::thread::spawn(move || {
                    let (mut last, mut transitions, mut reads) = (0u64, 0u64, 0u64);
                    while !STOP.load(Ordering::Relaxed) {
                        let idx = GATE.reader_enter(cpu);
                        // SAFETY：idx 源自验证环——本读窗内该槽不被写者
                        // 复用（hazard 已发布 + pick_write 避让）。
                        let row: *const [u64; R_LEN] = unsafe { &raw const ROWS[idx as usize] };
                        let first = unsafe { black_box((*row)[0]) };
                        let mut torn_at: Option<(usize, u64)> = None;
                        for j in 0..R_LEN {
                            let v = unsafe { black_box((*row)[j]) };
                            if v != first && torn_at.is_none() {
                                torn_at = Some((j, v));
                            }
                        }
                        if let Some((j, v)) = torn_at {
                            // 撕裂：同窗两种 tag——修复协议下不可达。
                            eprintln!("TEAR @cpu{cpu} slot{idx}[{j}]: {first} vs {v}");
                            TORN.fetch_add(1, Ordering::Relaxed);
                        }
                        assert_ne!(first, 0, "读到未发布槽（tag 0）——协议破防");
                        if first < last {
                            // tag 回退：读到被复写回收的陈旧槽。
                            eprintln!("REGRESS @cpu{cpu}: {first} < {last}");
                            REGRESS.fetch_add(1, Ordering::Relaxed);
                        }
                        if first != last {
                            transitions += 1;
                            last = first;
                        }
                        reads += 1;
                        if TORN.load(Ordering::Relaxed) > 0
                            || REGRESS.load(Ordering::Relaxed) > 0
                        {
                            break; // 已捕违约——及早收卷
                        }
                    }
                    (reads, transitions)
                })
            })
            .collect();
        let writers: Vec<_> = (0..2)
            .map(|_| {
                let go = go_rx.clone();
                let done = done_tx.clone();
                std::thread::spawn(move || {
                    // 每信号恰一换：两写者互抢串行锁 = 轮内背靠背对
                    // （谁先谁后随机——并发入口、串行临界区）。
                    while go.lock().unwrap().recv().is_ok() {
                        let _ser = SERIAL.lock().unwrap(); // POLICY_LOCK 同形
                        let tag = TAG.fetch_add(1, Ordering::Relaxed);
                        let w = GATE.pick_write();
                        // SAFETY：w 避让活跃 ∪ hazard——在窗读者不在该槽。
                        let dst: *mut [u64; R_LEN] = unsafe { &raw mut ROWS[w as usize] };
                        for j in 0..R_LEN {
                            unsafe { (*dst)[j] = tag };
                        }
                        GATE.publish(w);
                        drop(_ser);
                        let _ = done.lock().unwrap().send(());
                    }
                })
            })
            .collect();
        // 主循环：每轮等 hazard 新鲜（两读者位 == 当前活跃槽）→ 放两写
        // → 收双报。新鲜自旋上限 5s——读者饿死即响（不静默挂）。
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        for _ in 0..R_ROUNDS {
            loop {
                let hz = GATE.hazard_probe();
                let act = GATE.active_probe();
                if hz[0] == act && hz[1] == act {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "hazard 新鲜度等待超时——读者饿死？hz={hz:?} act={act}"
                );
            }
            let _ = go_tx.send(());
            let _ = go_tx.send(());
            let _ = done_rx.recv();
            let _ = done_rx.recv();
        }
        drop(go_tx);
        STOP.store(true, Ordering::Release);
        let mut total_reads = 0u64;
        let mut total_trans = 0u64;
        for rd in readers {
            let (reads, transitions) = rd.join().expect("读者线程异常");
            assert!(reads > 1000, "读者循环强度不足（{reads} 读）——竞争面存疑");
            assert!(transitions >= 1, "读者未观察到换表——撕裂窗未被穿过");
            total_reads += reads;
            total_trans += transitions;
        }
        for wr in writers {
            wr.join().expect("写者线程异常（canary/协议违约）");
        }
        assert_eq!(TORN.load(Ordering::Relaxed), 0, "撕裂读出现——验证环失守");
        assert_eq!(REGRESS.load(Ordering::Relaxed), 0, "tag 回退——槽回收复写陈读者");
        assert_eq!(TAG.load(Ordering::Relaxed), 2 + 2 * R_ROUNDS as u64, "写者换表计数");
        eprintln!("no-tear: {total_reads} 读 / {total_trans} 次换表观察，零撕裂零回退");
    }

    /// V6-d：**负对照**——修复前 shadow 协议形态（双槽 XOR 翻转，无
    /// hazard 避让、无验证环 = policy.rs 旧 shadow_snap/bind_gen_shadow），
    /// 同载荷下**必须**捕到撕裂：证明 V6-c 的绿是协议在承压，不是竞争
    /// 载荷强度不足（测的是缺口不是协议——SMP-01 §5.1 行动项原文）。
    /// 原生跑（非 MIRI）：双槽无避让下读者/写者对同行槽的真实数据竞争
    /// 正是被观测的撕裂窗本体。
    #[test]
    fn shadow_pristine_protocol_tears_under_same_load() {
        static WHERE: AtomicU8 = AtomicU8::new(0);
        static mut ROWS: [[u64; R_LEN]; 2] = [[0; R_LEN]; 2];
        static RUN: AtomicBool = AtomicBool::new(false);
        static TAG: AtomicU64 = AtomicU64::new(2);
        static TORN: AtomicUsize = AtomicUsize::new(0);
        static SERIAL: Mutex<()> = Mutex::new(());

        unsafe {
            let init: *mut [u64; R_LEN] = &raw mut ROWS[0];
            for j in 0..R_LEN {
                (*init)[j] = 1;
            }
        }

        RUN.store(true, Ordering::Release);
        let readers: Vec<_> = (0..2)
            .map(|cpu| {
                std::thread::spawn(move || {
                    // cpu 只作标记（旧协议读者无 hazard 发布位）。
                    let _ = cpu;
                    while RUN.load(Ordering::Relaxed) {
                        // 旧 shadow_snap：Acquire 读槽号即解引用——无验证环。
                        let idx = WHERE.load(Ordering::Acquire) & 1;
                        let row: *const [u64; R_LEN] = unsafe { &raw const ROWS[idx as usize] };
                        let first = unsafe { black_box((*row)[0]) };
                        let mut torn_at: Option<(usize, u64)> = None;
                        for j in 0..R_LEN {
                            let v = unsafe { black_box((*row)[j]) };
                            if v != first && torn_at.is_none() {
                                torn_at = Some((j, v));
                            }
                        }
                        if let Some((j, v)) = torn_at {
                            eprintln!("NEG-TEAR slot{idx}[{j}]: {first} vs {v}");
                            TORN.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                })
            })
            .collect();
        let writers: Vec<_> = (0..2)
            .map(|_| {
                std::thread::spawn(move || {
                    for _ in 0..4000 {
                        let _ser = SERIAL.lock().unwrap();
                        let tag = TAG.fetch_add(1, Ordering::Relaxed);
                        // 旧 bind_gen_shadow：固定写非活跃槽再翻转——背靠背
                        // 第二写即覆写读者刚经旧槽号进入的槽（撕裂窗）。
                        let next = WHERE.load(Ordering::Relaxed) ^ 1;
                        let dst: *mut [u64; R_LEN] = unsafe { &raw mut ROWS[next as usize] };
                        for j in 0..R_LEN {
                            unsafe { (*dst)[j] = tag };
                        }
                        fence(Ordering::Release);
                        WHERE.store(next, Ordering::Release);
                    }
                })
            })
            .collect();
        for wr in writers {
            wr.join().expect("负对照写者线程异常");
        }
        RUN.store(false, Ordering::Release);
        for rd in readers {
            rd.join().expect("负对照读者线程异常");
        }
        assert!(
            TORN.load(Ordering::Relaxed) > 0,
            "负对照未捕到撕裂——竞争载荷强度不足，V6-c 的绿不作数"
        );
        eprintln!("neg-ctrl: 捕到 {} 次撕裂（载荷灵敏度实证）", TORN.load(Ordering::Relaxed));
    }
}
