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
//!
//! 与冻结面的关系：SNAPS/SHD `static mut` 三槽/双槽区 + HAZARD/ACTIVE/
//! POLICY_LOCK 原子机械 + promote 4KiB 受控拷贝留 sched（纪律不变）——
//! 本面只钉「表怎么散列/怎么探测/怎么插/属性怎么合」的数定律，不钉存储
//! 机械。EppHint 单源在 berry-iprofile（W3-Q1），本 crate 消费不二写。
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
    pub _rsv: u16,
}

impl Slot {
    /// 空槽（const 语境可用的全零字面量）。
    pub const EMPTY: Slot = Slot {
        key: KEY_EMPTY,
        weight: 0,
        placement: 0,
        epp_flags: 0,
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
/// 归 sched 原子账——纯面无存储）。
pub fn insert_slot(
    slots: &mut [Slot],
    key: u64,
    weight: i32,
    placement: u8,
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
    epp: Option<EppHint>,
) -> InsertOut {
    insert_slot(&mut t.slots, key, weight, placement, epp)
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
            assert!(matches!(snap_insert(&mut t, k, 0, 0, None), InsertOut::Inserted { .. }));
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
            assert!(matches!(snap_insert(&mut t, k, 0, 0, None), InsertOut::Inserted { .. }));
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
        assert!(matches!(snap_insert(&mut t, 233, 0, 0, None), InsertOut::Inserted { .. }));
        assert!(matches!(snap_insert(&mut t, 144, 0, 0, None), InsertOut::Inserted { .. }));
        assert!(matches!(snap_insert(&mut t, 377, 0, 0, None), InsertOut::Inserted { .. }));
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
        assert_eq!(inserted(snap_insert(&mut t, 7, 5, 0, None)), 5);
        assert_eq!(snap_insert(&mut t, 7, 99, 0, None), InsertOut::Duplicate);
        let got = lookup_o1(&t, 7).unwrap();
        assert_eq!(got.weight, 5, "first-wins：重复插入不得覆写首值");
    }

    /// V3-b：窗满拒收——饱和窗（56..=63）后第 9 键 345（桶 56）返
    /// [`InsertOut::ProbeFull`] 且不入表（拒收键不可查）。
    #[test]
    fn insert_probe_full_reject() {
        let mut t = Slots::empty();
        for &k in &[112u64, 201, 57, 146, 2, 91, 324, 36] {
            assert!(matches!(snap_insert(&mut t, k, 0, 0, None), InsertOut::Inserted { .. }));
        }
        assert_eq!(snap_insert(&mut t, 345, 0, 0, None), InsertOut::ProbeFull);
        assert_eq!(lookup_o1(&t, 345), None, "拒收键不可查");
    }

    /// V3-c：build 点夹紧——越 ±WEIGHT_MAX 的注入值 build 时夹入（第一道
    /// 夹紧）；恰上下界不夹、零注入直通；夹紧值落槽可回。
    #[test]
    fn insert_clamp_at_build() {
        let mut t = Slots::empty();
        assert_eq!(inserted(snap_insert(&mut t, 11, 100, 0, None)), 64);
        assert_eq!(inserted(snap_insert(&mut t, 12, -100, 0, None)), -64);
        assert_eq!(inserted(snap_insert(&mut t, 13, 64, 0, None)), 64);
        assert_eq!(inserted(snap_insert(&mut t, 14, -64, 0, None)), -64);
        assert_eq!(inserted(snap_insert(&mut t, 15, 0, 0, None)), 0);
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
}
