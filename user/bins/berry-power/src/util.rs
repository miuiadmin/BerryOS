//! util EWMA 消费位 —— 调度器产信号、本面消费（lib 核面 ③）。
//!
//! 接缝语境（docs/调度彻底计划-v1.2.md 裁九 + CP-08 验收线）：调度器只产
//! 信号（sched/src/util.rs per-CPU EWMA——「窗口内运行时间/窗长」的指数
//! 平滑，0..=1024 尺度与 `Topo::capacity`/EM capacity 同尺度）、绝不写
//! 频率；频率裁决单点归 PM 服务 berry-power——本面即消费侧最小件。
//! **CP-08 验收线原文**：berry-power 消费 util 信号（schedutil 同构
//! `f ≈ util × fmax / capacity`）。
//!
//! **衰减算式零第二实现**：折叠数学单源 `berry-emface::ewma_fold`
//! （α = 1/8 移位、量化死区 <8、饱和钳 1024——W3-Q14 起 sched 生产侧同源
//! 消费；常量 [`EWMA_ALPHA_SHIFT`]/[`UTIL_WINDOW_HZ`]/[`UTIL_NR_CPU`]
//! re-export 单源）。本面只加**输入口**与消费侧状态：
//!
//! - [`UtilTrack::ingest_window`]——窗账输入口 `(accum_cycles,
//!   elapsed_cycles)`（生产侧 `util_tick_window` 每窗折叠的同款载荷）；
//! - [`UtilTrack::ingest_sample`]——裸样本输入口（util 0..=1024 直投；
//!   换算恒等 `ingest_sample(x) ≡ ingest_window(x, 1024)`——emface 折叠
//!   内式 `accum × 1024 / elapsed` 在 `elapsed = 1024` 时恰还原裸值，
//!   不长第二套平滑）。
//!
//! **参数推断标注**：α=1/8、窗 10ms（`UTIL_WINDOW_HZ = 100`）为 emface
//! 现役值（生产侧同款）；消费侧读出节奏（10-100ms 慢循环级，裁十债二
//! 注入形态语境）与窗折叠节奏的相位差**未定**。**通道形态首环已裁**
//! （CP-08 OQ-8：快照 syscall `sched_util_snapshot` 0x40——pull 模式，
//! PM-15 §4.1 零新号裁决辖 SDK 服务 op 面、不辖内核→服务数据通道；
//! 共享环 push 形态随 CP-10 通道窗叠加，add-only 不互斥）——
//! [`UtilTrack::ingest_snapshot`] 即其载荷面；消费侧对快照的再折叠 =
//! 消费侧二次平滑（生产侧已是同源 EWMA），节奏自定随读出循环落。
//! 真机 fmax/capacity/OPP 表随 CP-09/CP-18 落。

use berry_emface::ewma_fold;

// 常量单源 re-export（α/核数/窗频——sched 生产侧同源；消费面零第二套）。
pub use berry_emface::{EWMA_ALPHA_SHIFT, UTIL_NR_CPU, UTIL_WINDOW_HZ};

/// util 满格（0..=1024 尺度——与 emface 折叠内式/调度器 `Topo::capacity`
/// 同尺度；emface 内联字面 1024 的具名锚，非第二实现）。
pub const UTIL_FULL: u32 = 1024;

/// per-CPU util EWMA 消费位状态机（初值 0 = 空闲假设——sched 生产侧静态
/// 槽同款初值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UtilTrack {
    ewma: [u32; UTIL_NR_CPU],
}

impl UtilTrack {
    /// 新建（全核 0）。
    pub const fn new() -> Self {
        UtilTrack { ewma: [0; UTIL_NR_CPU] }
    }

    /// 窗账输入口（CP-08 通道 ABI 的骨架载荷形态）：折叠一步并返回该核
    /// 新 EWMA。坏输入拒面：cpu 越界 / `elapsed == 0`（除零前置——emface
    /// 契约 `Fold` 门保证 elapsed ≥ 窗长 ≥ 1，坏值在此拒不透传）→ `None`
    /// 且状态零改动。
    pub fn ingest_window(&mut self, cpu: usize, accum: u64, elapsed: u64) -> Option<u32> {
        if cpu >= UTIL_NR_CPU || elapsed == 0 {
            return None;
        }
        let next = ewma_fold(accum, elapsed, self.ewma[cpu]);
        self.ewma[cpu] = next;
        Some(next)
    }

    /// 裸样本输入口（util 0..=1024 直投）：换算恒等见模块注。越界样本
    /// （>1024）→ `None` 拒。
    pub fn ingest_sample(&mut self, cpu: usize, util: u32) -> Option<u32> {
        if util > UTIL_FULL {
            return None;
        }
        self.ingest_window(cpu, util as u64, UTIL_FULL as u64)
    }

    /// 全核快照输入口（CP-08 首环通道载荷面——内核 `sched_util_snapshot`
    /// 0x40 号产出的 per-CPU u32 数组逐核投 [`ingest_sample`]）。坏载荷
    /// 拒面**全或无**：长度 ≠ `UTIL_NR_CPU`（通道载荷定长——两端同源
    /// 常量，长度错 = 契约违例）或任一核样本 >1024 → `None` 且状态
    /// 零改动（不半折——通道载荷与单核样本同律）。
    pub fn ingest_snapshot(&mut self, util: &[u32]) -> Option<()> {
        if util.len() != UTIL_NR_CPU || util.iter().any(|&u| u > UTIL_FULL) {
            return None;
        }
        for (cpu, &u) in util.iter().enumerate() {
            self.ingest_sample(cpu, u);
        }
        Some(())
    }

    /// 某核当前 EWMA（读面宽容：越界 → 0——sched 生产侧 `cpu_util` 同口径；
    /// 坏 cpu 是读面噪声不是契约违例）。
    pub fn est(&self, cpu: usize) -> u32 {
        if cpu < UTIL_NR_CPU {
            self.ewma[cpu]
        } else {
            0
        }
    }

    /// 全核最大 EWMA（v1 单频域口径：QEMU virt 单簇，域内取最大 = 保守
    /// 侧；多频域随真机拓扑 CP-19 扩）。
    pub fn max_est(&self) -> u32 {
        self.ewma.iter().copied().max().unwrap_or(0)
    }

    /// EAS placement hint（生产激活首步——EAS-PA 刀）：消费 per-CPU EWMA
    /// 产出放置建议——「把 util 高的核避开」（hint 选低 util 核）。词汇与
    /// 内核消费面同源：1 = 偏 little（CPU0）/ 2 = 偏 big（CPU1）
    /// （`sched/src/policy.rs` `Directive::placement` 三值词汇表；0 = 无意见
    /// 不由本产面发出——有数据即有意见）。平局（两核相等）→ 1。
    ///
    /// **推断标注**：平局偏 little 初值 = 省电核惯例（真机非对称拓扑下
    /// little 域能耗低；QEMU 对称表无实际差异）。overutil 饱和门与
    /// 词汇执法都不在产面——内核消费面收口（`policy::placement_hint`
    /// overutil 回退恒 0 / `eas_pick_place` 非 {1,2} 拒），产面无需预判。
    ///
    /// 镜像钉板：kernel ktest `test_eas_prod`（EASPROD）以同律独立实现
    /// 对拍（ktest 侧故意不复用本 crate——独立副本 = 对抗性对拍，防两侧
    /// 同源漂移互证假绿）。
    pub fn placement_hint(&self) -> u8 {
        if self.est(0) <= self.est(1) {
            1 // 偏 little（CPU0 较闲或平局）
        } else {
            2 // 偏 big（CPU1 较闲——避开高 util 的 CPU0）
        }
    }
}

impl Default for UtilTrack {
    fn default() -> Self {
        Self::new()
    }
}

/// schedutil 同构频率请求：`f ≈ util × fmax / capacity`（CP-08 验收线原文
/// 形态）。u64 中间积零溢出；结果钳 `fmax`（capacity < util 时商超顶钳回）。
///
/// 坏输入拒面：`capacity == 0`（EM 未注册形态——无据不求频）→ `None`；
/// `util > 1024`（尺度违约）→ `None`。
///
/// **参数推断标注**：`fmax`/`capacity` v1 由调用面注入（QEMU 期无 EM/OPP
/// 真表——虚拟基准；真机 EM/OPP 随 CP-09/CP-18 落）；`fmin` 侧钉 OPP 表
/// （util=0 → 0 是线性式原值，非可运行频率——地板档裁决留 OPP 面刀）。
pub fn schedutil_freq_khz(util: u32, fmax_khz: u32, capacity: u32) -> Option<u32> {
    if capacity == 0 || util > UTIL_FULL {
        return None;
    }
    let f = ((util as u64) * (fmax_khz as u64) / (capacity as u64)).min(fmax_khz as u64);
    Some(f as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 折叠数学与 emface 单源对拍：满忙一窗（accum == elapsed →
    /// util_raw = 1024）自 0 起 → 0 + 1024>>3 = 128；影子状态全程对拍
    /// （emface `ewma_fold` 语义钉板——本面零第二实现，对拍防接线漂）。
    #[test]
    fn fold_matches_emface_semantics() {
        let mut t = UtilTrack::new();
        assert_eq!(t.ingest_window(0, 625_000, 625_000), Some(128));
        // 影子状态全程对拍：同载荷逐窗直算 emface，两轨逐位一致。
        let mut t2 = UtilTrack::new();
        let mut shadow = 0u32;
        for _ in 0..64 {
            shadow = ewma_fold(625_000, 625_000, shadow);
            assert_eq!(t2.ingest_window(0, 625_000, 625_000), Some(shadow));
        }
        assert!(shadow <= UTIL_FULL);
    }

    /// EWMA 边界三钉：深空闲衰减到量化死区地板（差 <8 停——EWMA 自 8
    /// 以上永不精确归零，emface 语义）；满忙单调爬升但死区上停摆；死区
    /// 停点再喂同样本 = 恒等不动。
    #[test]
    fn ewma_boundaries() {
        // 深空闲：自 128 起逐窗 0 样本——每窗减 (差>>3)，衰减到量化死区
        // 地板 ≤7 后恒停（差 <8 折叠 0——不到 0，频率信号无害口径）。
        let mut t = UtilTrack::new();
        t.ingest_sample(0, UTIL_FULL);
        assert_eq!(t.est(0), UTIL_FULL >> EWMA_ALPHA_SHIFT); // 1024>>3=128
        for _ in 0..64 {
            t.ingest_window(0, 0, 625_000);
        }
        assert!(t.est(0) <= 7, "空窗衰减到死区地板：est={}", t.est(0));
        let floor = t.est(0);
        for _ in 0..10 {
            t.ingest_window(0, 0, 625_000);
        }
        assert_eq!(t.est(0), floor, "死区地板恒停");
        // 自 <8 初值直接恒停（差恒 <8）。
        let mut t = UtilTrack::new();
        t.ingest_sample(0, 5); // 0 + 5>>3 = 0
        assert_eq!(t.est(0), 0);

        // 满忙爬升：单调不减、永不破 1024、死区上停（差 <8 → 增量为 0）。
        let mut t = UtilTrack::new();
        let mut last = 0;
        let mut v = 0;
        for _ in 0..300 {
            v = t.ingest_sample(1, UTIL_FULL).unwrap();
            assert!(v >= last && v <= UTIL_FULL);
            last = v;
        }
        assert!(v >= UTIL_FULL - 8, "量化死区上停摆（差<8）：v={v}");
        // 停点恒等：死区停住后再喂同样本 → 值不动（diff <8 折叠 0）。
        assert_eq!(t.ingest_sample(1, UTIL_FULL), Some(v));
        assert_eq!(t.est(1), v);
    }

    /// 输入口换算恒等：`ingest_sample(x) ≡ ingest_window(x, 1024)`（同初
    /// 值逐位一致——不长第二套平滑的构造性钉板）。
    #[test]
    fn sample_window_identity() {
        for x in [0u32, 1, 100, 512, 777, 1023, UTIL_FULL] {
            let mut a = UtilTrack::new();
            let mut b = UtilTrack::new();
            a.ingest_sample(0, 300).unwrap(); // 先铺非零初值再验
            b.ingest_window(0, 300, UTIL_FULL as u64).unwrap();
            let ra = a.ingest_sample(0, x);
            let rb = b.ingest_window(0, x as u64, UTIL_FULL as u64);
            assert_eq!(ra, rb, "x={x}");
            assert_eq!(a.est(0), b.est(0), "x={x}");
        }
    }

    /// 快照输入口（CP-08 首环载荷面 loopback 扩展）：通道路径 ≡ 逐核
    /// 单核路径（同初值下 `ingest_snapshot([a,b])` 与逐核 `ingest_sample`
    /// 逐位一致——快照面不长第二套折叠的构造性钉板）；坏载荷全或无
    /// （长度错/任一核越界 → None 且状态零改动）。
    #[test]
    fn snapshot_ingest_loopback_and_rejects() {
        // 等价主形：两轨同初值，一轨走快照、一轨逐核 sample，逐位一致。
        let mut s = UtilTrack::new();
        let mut p = UtilTrack::new();
        for t in [&mut s, &mut p] {
            t.ingest_sample(0, 300); // 铺非零初值再验（非平凡折叠）
            t.ingest_sample(1, 100);
        }
        assert_eq!(s.ingest_snapshot(&[1024, 0]), Some(()));
        p.ingest_sample(0, 1024);
        p.ingest_sample(1, 0);
        assert_eq!((s.est(0), s.est(1)), (p.est(0), p.est(1)),
            "快照路 ≡ 单核路（CP-08 首环载荷面）");
        assert_eq!(s.max_est(), p.max_est());
        // 多轮等价（EWMA 连续折叠下两轨不漂）。
        for (k, snap) in [[512u32, 512], [0, 1024], [777, 3]].iter().enumerate() {
            assert_eq!(s.ingest_snapshot(snap), Some(()), "轮 {k}");
            p.ingest_sample(0, snap[0]);
            p.ingest_sample(1, snap[1]);
        }
        assert_eq!((s.est(0), s.est(1)), (p.est(0), p.est(1)), "多轮不漂");
        // 坏载荷全或无：长度错（0/1/3）或任一核 >1024 → None 且零改动。
        let before = (s.est(0), s.est(1));
        assert_eq!(s.ingest_snapshot(&[]), None, "空载荷");
        assert_eq!(s.ingest_snapshot(&[512]), None, "缺核");
        assert_eq!(s.ingest_snapshot(&[512, 512, 512]), None, "超长");
        assert_eq!(s.ingest_snapshot(&[512, UTIL_FULL + 1]), None, "越界核");
        assert_eq!(s.ingest_snapshot(&[u32::MAX, 0]), None, "wild 越界核");
        assert_eq!((s.est(0), s.est(1)), before, "拒面零改动（全或无）");
    }

    /// placement hint 产面律（EAS 生产激活首步）：低核偏好三态 + 平局 +
    /// 快照通道同律（`ingest_snapshot` 后同一样本集与逐核 `ingest_sample`
    /// 产同 hint——hint 只读 EWMA 态，通道无关）。
    #[test]
    fn placement_hint_avoids_busy_core() {
        // CPU0 忙（600 > 100）→ 偏 big（CPU1 = 低 util 核）。
        let mut t = UtilTrack::new();
        t.ingest_sample(0, 600);
        t.ingest_sample(1, 100);
        assert_eq!(t.placement_hint(), 2, "CPU0 高 util 须避开（hint=2 偏 big）");
        // 镜像：CPU1 忙 → 偏 little。
        let mut t = UtilTrack::new();
        t.ingest_sample(0, 100);
        t.ingest_sample(1, 600);
        assert_eq!(t.placement_hint(), 1, "CPU1 高 util 须避开（hint=1 偏 little）");
        // 平局（含双零初值）→ 1（偏 little——推断标注见方法注）。
        let mut t = UtilTrack::new();
        t.ingest_sample(0, 300);
        t.ingest_sample(1, 300);
        assert_eq!(t.placement_hint(), 1, "平局须偏 little");
        assert_eq!(UtilTrack::new().placement_hint(), 1, "双零初值平局同律");
        // 深空闲死区地板（≤7）对远端 300 仍是低核——EWMA 态直接驱动。
        let mut t = UtilTrack::new();
        t.ingest_sample(0, UTIL_FULL);
        for _ in 0..64 {
            t.ingest_window(0, 0, 625_000);
        }
        t.ingest_sample(1, 300);
        assert!(t.est(0) <= 7, "前置：CPU0 须已衰减到死区地板");
        assert_eq!(t.placement_hint(), 1, "深空闲核胜出（hint 只随 EWMA 态）");
        // 通道同律：快照路与逐核路产同 hint。
        let mut s = UtilTrack::new();
        let mut p = UtilTrack::new();
        s.ingest_snapshot(&[600, 100]).unwrap();
        p.ingest_sample(0, 600);
        p.ingest_sample(1, 100);
        assert_eq!(s.placement_hint(), p.placement_hint(), "快照通道同律");
        assert_eq!(s.placement_hint(), 2);
    }

    /// 坏输入拒面：cpu 越界（UTIL_NR_CPU/255）、elapsed=0（除零前置）、
    /// 裸样本 >1024；拒面零改动。
    #[test]
    fn rejects_bad_inputs() {
        let mut t = UtilTrack::new();
        assert_eq!(t.ingest_window(UTIL_NR_CPU, 100, 100), None);
        assert_eq!(t.ingest_window(255, 100, 100), None);
        assert_eq!(t.ingest_window(0, 100, 0), None);
        assert_eq!(t.ingest_sample(0, UTIL_FULL + 1), None);
        assert_eq!(t.ingest_sample(0, u32::MAX), None);
        assert_eq!(t.est(0), 0, "拒面零改动");
        // 读面宽容：越界 cpu → 0（同 sched cpu_util 口径）。
        assert_eq!(t.est(UTIL_NR_CPU), 0);
        assert_eq!(UtilTrack::new().max_est(), 0);
    }

    /// per-CPU 独立性与 max_est（v1 单频域保守侧口径）。
    #[test]
    fn per_cpu_independent_and_max() {
        let mut t = UtilTrack::new();
        t.ingest_sample(0, UTIL_FULL); // → 128
        t.ingest_sample(1, 512); // → 64
        assert_eq!(t.est(0), 128);
        assert_eq!(t.est(1), 64);
        assert_eq!(t.max_est(), 128);
    }

    /// schedutil 同构映射：线性式、钳顶、坏输入拒面。
    #[test]
    fn schedutil_freq_mapping() {
        // 线性主形：util=512（半格）、capacity=1024、fmax=1GHz → 500MHz。
        assert_eq!(schedutil_freq_khz(512, 1_000_000, 1024), Some(500_000));
        // 满格 = fmax；零 util = 0（线性式原值——fmin 地板留 OPP 刀，模块注）。
        assert_eq!(schedutil_freq_khz(1024, 1_000_000, 1024), Some(1_000_000));
        assert_eq!(schedutil_freq_khz(0, 1_000_000, 1024), Some(0));
        // 小核（capacity=512）：util=1024 → 商 2GHz 超顶 → 钳 fmax。
        assert_eq!(schedutil_freq_khz(1024, 1_000_000, 512), Some(1_000_000));
        // util 超小核容量一半：512×1e6/512 = 1e6 = fmax。
        assert_eq!(schedutil_freq_khz(512, 1_000_000, 512), Some(1_000_000));
        // 除法截断：util=1 → 1e6/1024 = 976（截断向零）。
        assert_eq!(schedutil_freq_khz(1, 1_000_000, 1024), Some(976));
        // 溢出安全：fmax 近 u32::MAX、util 满格（u64 中间积 4.4e12 不溢）。
        assert_eq!(schedutil_freq_khz(1024, 4_000_000_000, 1024), Some(4_000_000_000));
        // 坏输入拒面：capacity=0（EM 未注册）→ None；util 越界 → None。
        assert_eq!(schedutil_freq_khz(512, 1_000_000, 0), None);
        assert_eq!(schedutil_freq_khz(1025, 1_000_000, 1024), None);
        assert_eq!(schedutil_freq_khz(u32::MAX, 1_000_000, 1024), None);
    }
}
