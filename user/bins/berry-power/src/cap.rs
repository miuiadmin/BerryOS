//! 频率帽聚合点 —— 多源帽收拢为**单一输出**（lib 核面 ①）。
//!
//! **执法锚 ADR #66 ②（「帽不新开通道」）**：降档不新开通道——频率帽汇入
//! berry-power 同一内核聚合点取 min（**唯一合法帽书写面**）；berry-thermal
//! 的热帽只投到本聚合点、不另开通道写内核；对调度器 = 系统态叠加票
//! （#42 只叠加）。本面即该聚合点的纯函数形态：三源定席
//! （thermal/调度 hint/低电）各持 `Option<帽>`，[`FreqCapAgg::aggregate`]
//! 取 min 产出唯一 [`CapDecision`]——内核聚合点写面（真实通道 = 窗注释，
//! 见 main.rs 窗 4）只吃这个输出，不存在第二书写面。
//!
//! 帽语义：`None` = 该源不限（缺席不投票）；`Some(khz)` = 该源上限。
//! 全源缺席 → 聚合 `None` = 无帽 = 满频。平票（多源同值）→ 源号小者
//! 胜（确定性归因——遥测面可查「谁在限」）。
//!
//! 源席 v1 定三（任务面枚举 [thermal/调度 hint/低电]）：调度 hint 源 =
//! 调度器经既有信号通道给的保守帽建议（#42 系统态叠加票同源词汇，非新
//! 通道）；真机多频域/更多源（如温控充电联合）随 CP-09/CP-19 扩席，
//! add-only。

/// 帽源席：berry-thermal 热帽（ADR #66 ②——出票方 berry-thermal，经本
/// 聚合点汇入，不新开通道）。
pub const CAP_SRC_THERMAL: u8 = 0;
/// 帽源席：调度 hint 帽（#42 系统态叠加票词汇——调度器保守建议面）。
pub const CAP_SRC_SCHEDHINT: u8 = 1;
/// 帽源席：低电帽（berry-power 自有策略域——深亏电保关机储备，见
/// [`crate::PowerCore`] 低电策略）。
pub const CAP_SRC_LOWBATT: u8 = 2;
/// 源席总数（v1 定三）。
pub const CAP_SOURCES: usize = 3;

/// 帽面写/读错误（坏输入拒面——零 panic 路径）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapErr {
    /// 源号越界（`src >= CAP_SOURCES`）。
    BadSource,
    /// 帽值 0 kHz（0 不是频率——`Some(0)` 一律拒）。
    ZeroCap,
}

/// 聚合裁决：生效帽 + 胜出源（归因面——遥测「谁在限频」单源答案）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapDecision {
    pub cap_khz: u32,
    pub src: u8,
}

/// 频率帽聚合点（三源定席、纯函数状态机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreqCapAgg {
    caps: [Option<u32>; CAP_SOURCES],
}

impl FreqCapAgg {
    /// 新建（全源缺席 = 无帽满频）。
    pub const fn new() -> Self {
        FreqCapAgg { caps: [None; CAP_SOURCES] }
    }

    /// 帽写面（**唯一入口**——内核聚合点写面只消费 [`Self::aggregate`]
    /// 输出，别无帽出口）。`cap_khz = None` = 撤帽（该源解除限制）。
    /// 坏输入拒面：源号越界 → [`CapErr::BadSource`]；`Some(0)` →
    /// [`CapErr::ZeroCap`]（状态零改动——拒不改旧值）。
    pub fn set(&mut self, src: u8, cap_khz: Option<u32>) -> Result<(), CapErr> {
        let i = src as usize;
        if i >= CAP_SOURCES {
            return Err(CapErr::BadSource);
        }
        if matches!(cap_khz, Some(0)) {
            return Err(CapErr::ZeroCap);
        }
        self.caps[i] = cap_khz;
        Ok(())
    }

    /// 帽读面（遥测/测试用；同坏源拒面）。
    pub fn get(&self, src: u8) -> Result<Option<u32>, CapErr> {
        let i = src as usize;
        if i >= CAP_SOURCES {
            return Err(CapErr::BadSource);
        }
        Ok(self.caps[i])
    }

    /// 聚合输出（单一输出面）：在席帽取 min；全缺席 → `None`（无帽 =
    /// 满频）。平票 → 源号小者胜（严格小于才替换——先扫先驻）。
    pub fn aggregate(&self) -> Option<CapDecision> {
        let mut best: Option<CapDecision> = None;
        for (i, c) in self.caps.iter().enumerate() {
            if let Some(k) = c {
                if best.map_or(true, |b| *k < b.cap_khz) {
                    best = Some(CapDecision { cap_khz: *k, src: i as u8 });
                }
            }
        }
        best
    }
}

impl Default for FreqCapAgg {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 多源 min 全序穷举：三源 × 值域 {None,300,600,900,1200} 全组合
    /// （5^3 = 125 形）——聚合 = 在席 min、胜出源 = 首个最小值席、全
    /// 缺席 → None。多源 min 的全序钉板（每个组合独立机，无跨测状态）。
    #[test]
    fn aggregate_total_order_exhaustive() {
        const VALS: [Option<u32>; 5] = [None, Some(300), Some(600), Some(900), Some(1200)];
        for a in VALS {
            for b in VALS {
                for c in VALS {
                    let mut agg = FreqCapAgg::new();
                    agg.set(CAP_SRC_THERMAL, a).unwrap();
                    agg.set(CAP_SRC_SCHEDHINT, b).unwrap();
                    agg.set(CAP_SRC_LOWBATT, c).unwrap();
                    // 期望值独立重算（期望面不引用被测面内部）。
                    let triples =
                        [(a, CAP_SRC_THERMAL), (b, CAP_SRC_SCHEDHINT), (c, CAP_SRC_LOWBATT)];
                    let present: Vec<(u32, u8)> =
                        triples.iter().filter_map(|(v, s)| v.map(|k| (k, *s))).collect();
                    match present.iter().min_by_key(|(k, _)| *k) {
                        None => assert_eq!(agg.aggregate(), None, "{a:?}/{b:?}/{c:?}"),
                        Some((k, s)) => {
                            // min_by_key 取首个最小——与被测面「源号小者胜」
                            // 同判（triples 按源号升序枚举）。
                            assert_eq!(
                                agg.aggregate(),
                                Some(CapDecision { cap_khz: *k, src: *s }),
                                "{a:?}/{b:?}/{c:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// 平票归因：三源同帽 → 源号最小（thermal 席）胜。
    #[test]
    fn tie_breaks_to_lowest_src() {
        let mut agg = FreqCapAgg::new();
        agg.set(CAP_SRC_THERMAL, Some(600)).unwrap();
        agg.set(CAP_SRC_SCHEDHINT, Some(600)).unwrap();
        agg.set(CAP_SRC_LOWBATT, Some(600)).unwrap();
        assert_eq!(
            agg.aggregate(),
            Some(CapDecision { cap_khz: 600, src: CAP_SRC_THERMAL })
        );
        // thermal 撤帽 → 平票移调度 hint 席。
        agg.set(CAP_SRC_THERMAL, None).unwrap();
        assert_eq!(
            agg.aggregate(),
            Some(CapDecision { cap_khz: 600, src: CAP_SRC_SCHEDHINT })
        );
    }

    /// 撤帽生效：单源设-撤 → 回到无帽满频（热服务亡→帽保持的反面：
    /// 显式撤帽是策略动作，非崩溃语义——崩溃保持面归通道层窗注释）。
    #[test]
    fn withdraw_restores_uncapped() {
        let mut agg = FreqCapAgg::new();
        assert_eq!(agg.aggregate(), None);
        agg.set(CAP_SRC_THERMAL, Some(800)).unwrap();
        assert_eq!(agg.aggregate().unwrap().cap_khz, 800);
        agg.set(CAP_SRC_THERMAL, None).unwrap();
        assert_eq!(agg.aggregate(), None);
    }

    /// 坏输入拒面：源号越界（3/255）、Some(0)；拒不改旧值。
    #[test]
    fn rejects_bad_source_and_zero_cap() {
        let mut agg = FreqCapAgg::new();
        agg.set(CAP_SRC_THERMAL, Some(900)).unwrap();
        assert_eq!(agg.set(3, Some(500)), Err(CapErr::BadSource));
        assert_eq!(agg.set(255, None), Err(CapErr::BadSource));
        assert_eq!(agg.set(CAP_SRC_LOWBATT, Some(0)), Err(CapErr::ZeroCap));
        // 拒面零副作用：旧帽仍在、get 同拒。
        assert_eq!(agg.aggregate().unwrap().cap_khz, 900);
        assert_eq!(agg.get(3), Err(CapErr::BadSource));
        assert_eq!(agg.get(CAP_SRC_THERMAL), Ok(Some(900)));
    }
}
