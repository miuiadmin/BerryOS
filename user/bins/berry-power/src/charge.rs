//! 充电限流票仲裁 —— JEITA 票 + 低电票 + 用户帽 → min（lib 核面 ②）。
//!
//! **票面单位 = mA（F1 终裁，04 号复核 §6-3/§7-F1）**：单位统一 mA——
//! ADR #66 ⑤「JEITA 充电温区五档**电流版**（charge_current_limit 票由
//! berry-thermal 出票）」为终态形态，出票面 `charge_limit_ma`（mA，见
//! `user/bins/berry-thermal/src/lib.rs` `Verdict`）即正统；ADR #68
//! `CHG_OP_VOTE_LIMIT{limit_pct, reason:u8}` 的 pct 面 = 协议冻结时的
//! 快照形态，以设计容量字段换算桥兼容读写（[`pct_of_ma`]/[`ma_of_pct`]
//! + [`ChgArbiter::vote_pct`]/[`ChgDecision::limit_pct`]），**协议升版携
//! mA 直传时兼容面退役改口**（F1 兼容锚）。
//!
//! **契约原文锚 ADR #68（同窗冻结件）**：「P11 充电限值票
//! （`CHG_OP_VOTE_LIMIT{limit_pct, reason:u8}`，最少者赢、断线自动撤票、
//! chargerd 持有）」；出票边界锚 ADR #66 ⑤：JEITA 充电温区五档 = 第一张
//! 系统热表，票由 **berry-thermal 出票**（本服务不养第二套热表——sim 热
//! 快照只进遥测，不旁路出票）。本面 = 仲裁数学**单源**：三席定票
//! （JEITA/低电/用户帽）各持 `Option<{limit_ma, reason}>`，
//! [`ChgArbiter::arbitrate`] 最少者赢（min `limit_ma`）。
//!
//! **chargerd 契约位**（留窗如实注）：票持有者/动态票主注册/断线撤票的
//! 通道执行面 = chargerd 服务本体（立法件无代码——本面只提供它将消费的
//! 仲裁纯函数与 [`ChgArbiter::withdraw`] 撤票库面形态；断链事件 → 撤票
//! 的映射挂 chargerd/通道刀）。
//!
//! 票面值域（mA 域）：`0` = 禁充（JEITA 低温区最严档，锂电低温禁充同义；
//! 推断标注）、[`CHG_LIMIT_UNLIMITED_MA`]（u32::MAX）= 无限流约束——与
//! berry-thermal 出票面哨兵 `CHARGE_NO_CAP_MA` 同值同语义（min 组合下
//! 穿透：min(u32::MAX, x) = x；跨 crate 注释锚，哨兵终态随 chargerd 契约
//! 立法收进 berry-pwproto 单源）。无票（全席缺席）→
//! `{UNLIMITED_MA, NONE}` = 不限。reason 词表 add-only；
//! `CHG_REASON_NONE` = 仲裁面专属（无票形态），出票面不可投。
//!
//! **兼容桥（#68 pct 面 ↔ mA 面，F1）**：百分比基 = C 率（1C = 设计容量
//! 数值 mA——JEITA 电流档惯例域；`BatterySnapshot.charge_full_uah` 为
//! 换算单源字段，µAh）。双向**向下取整 = 只收紧不放宽**（限值换算保守侧
//! 纪律，在 pct 表达域 <1C 内成立：pct 回投所得 mA ≤ 原 mA，往返允收严
//! ≤1 个百分点）；**表达力边界**：pct 面 100 = 不限（≥1C 读 100——C 率
//! 域上界即「不再限流」，超域电流往返放宽到不限哨兵——#68 冻结形表达
//! 不了的域，mA 面为真相，F1 裁 mA 的实证面）；`pct 100` ↔ u32::MAX
//! 不限语义直映；设计容量 0 = 换算无据 → 拒面（不虚构换算）。

/// 票席：JEITA 温区票（出票方 = berry-thermal，ADR #66 ⑤；票面 mA）。
pub const CHG_SLOT_JEITA: u8 = 0;
/// 票席：低电票（出票方 = berry-power 自有策略域——深亏电预充限流，见
/// [`crate::PowerCore`] 低电策略）。
pub const CHG_SLOT_LOWBATT: u8 = 1;
/// 票席：用户帽（设置面——用户充电上限偏好，如 80% 延寿档）。
pub const CHG_SLOT_USER: u8 = 2;
/// 票席总数（v1 定三）。
pub const CHG_SLOTS: usize = 3;

/// reason：无票（仲裁面专属——全席缺席时的裁决归因；出票面投此值 → 拒）。
pub const CHG_REASON_NONE: u8 = 0;
/// reason：JEITA 温区（berry-thermal 出票）。
pub const CHG_REASON_JEITA: u8 = 1;
/// reason：低电保护（berry-power 出票）。
pub const CHG_REASON_LOWBATT: u8 = 2;
/// reason：用户帽（设置面出票）。
pub const CHG_REASON_USER: u8 = 3;

/// 不限流票面（mA 域哨兵 = u32::MAX；无票裁决的票面值）。与 berry-thermal
/// 出票面哨兵 `CHARGE_NO_CAP_MA` 同值同语义——min 组合下穿透（F1 并轨锚）。
pub const CHG_LIMIT_UNLIMITED_MA: u32 = u32::MAX;
/// #68 冻结形 pct 面的不限值（`CHG_OP_VOTE_LIMIT` 线值域 0..=100 的
/// 100 = 满充不限）。**F1 兼容锚：仅兼容读写面使用，升版携 mA 时退役。**
pub const CHG_LIMIT_UNLIMITED_PCT: u8 = 100;

/// 票面写错误（坏输入拒面——零 panic 路径）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChgErr {
    /// 席号越界（`slot >= CHG_SLOTS`）。
    BadSlot,
    /// 兼容面票面越界（`limit_pct > 100`——#68 冻结形 pct 面专属）。
    BadLimit,
    /// reason 未知名（词表外 u8——add-only 纪律下未知值 = 契约漂移，拒）。
    BadReason,
    /// 兼容面换算无据（设计容量字段 = 0——mA 面为真相，pct 桥拒面）。
    BadCap,
}

/// 仲裁裁决：生效限流票面（mA，F1 终裁单位）+ 胜出归因（「谁在限充」
/// 单源答案）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChgDecision {
    /// 生效限流（mA；0 = 禁充，u32::MAX = 不限）。
    pub limit_ma: u32,
    pub reason: u8,
}

impl ChgDecision {
    /// #68 冻结形兼容读（mA → pct，设计容量换算桥 [`pct_of_ma`]）。
    /// **F1 兼容锚**：`CHG_OP_VOTE_LIMIT{limit_pct}` 线面读出用；升版携
    /// mA 时本读面退役。设计容量 0 → `None`（换算无据——调用面持 mA
    /// 真相，不虚构 pct）。
    pub fn limit_pct(&self, design_cap_uah: u32) -> Option<u8> {
        pct_of_ma(self.limit_ma, design_cap_uah)
    }
}

// ---- F1 桥（mA ↔ pct，设计容量单源字段换算；纯函数、host 可测）----

/// mA → pct 兼容读桥（F1）：`pct = limit_ma × 100_000 / design_cap_uah`
/// （容量字段 µAh、电流 mA——C 率百分比基，1C = 设计容量数值 mA）。
/// 向下取整（pct 表达域 <1C 内只收紧不放宽：pct 回投所得 mA ≤ 原 mA）。
/// 不限哨兵与 ≥1C → 100（C 率域上界 = 不再限流——超域电流 pct 面表达
/// 不了，往返放宽到不限哨兵，mA 面为真相）；容量 0 → `None`（无据）。
pub fn pct_of_ma(limit_ma: u32, design_cap_uah: u32) -> Option<u8> {
    if design_cap_uah == 0 {
        return None;
    }
    if limit_ma == CHG_LIMIT_UNLIMITED_MA {
        return Some(CHG_LIMIT_UNLIMITED_PCT);
    }
    let pct = (limit_ma as u64) * 100_000 / design_cap_uah as u64;
    Some(pct.min(CHG_LIMIT_UNLIMITED_PCT as u64) as u8)
}

/// pct → mA 兼容投递桥（F1）：`ma = limit_pct × design_cap_uah / 100_000`，
/// 向下取整（同只收紧纪律）。`100` → 不限哨兵（#68「满充不限」语义直映
/// ——容量是电荷量非限流值，不落容量数值）；`0` → 0（禁充）；容量 0 →
/// `None`（无据）。
pub fn ma_of_pct(limit_pct: u8, design_cap_uah: u32) -> Option<u32> {
    if design_cap_uah == 0 {
        return None;
    }
    if limit_pct >= CHG_LIMIT_UNLIMITED_PCT {
        return Some(CHG_LIMIT_UNLIMITED_MA);
    }
    Some((limit_pct as u64 * design_cap_uah as u64 / 100_000) as u32)
}

/// 充电限流票仲裁机（三席定票、纯函数状态机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChgArbiter {
    votes: [Option<(u32, u8)>; CHG_SLOTS], // (limit_ma, reason)
}

impl ChgArbiter {
    /// 新建（全席撤票 = 不限）。
    pub const fn new() -> Self {
        ChgArbiter { votes: [None; CHG_SLOTS] }
    }

    /// 投票（mA 面，F1 终裁单位——berry-thermal `charge_limit_ma` 出票面
    /// 的仲裁对口）。坏输入拒面：席号越界 → [`ChgErr::BadSlot`]；reason
    /// 未知名或 `NONE` → [`ChgErr::BadReason`]（拒面零改动——旧票保留）。
    /// 票面全 u32 合法：0 = 禁充、u32::MAX = 不限（mA 域无越界形）。
    pub fn vote(&mut self, slot: u8, limit_ma: u32, reason: u8) -> Result<(), ChgErr> {
        let i = slot as usize;
        if i >= CHG_SLOTS {
            return Err(ChgErr::BadSlot);
        }
        if !matches!(reason, CHG_REASON_JEITA | CHG_REASON_LOWBATT | CHG_REASON_USER) {
            return Err(ChgErr::BadReason);
        }
        self.votes[i] = Some((limit_ma, reason));
        Ok(())
    }

    /// 投票（#68 冻结形 pct 面兼容投递——线面 `CHG_OP_VOTE_LIMIT{limit_pct}`
    /// 到达时经 [`ma_of_pct`] 桥换 mA 落票）。**F1 兼容锚：协议升版携 mA
    /// 时本面退役、线值直达 [`ChgArbiter::vote`]**。坏输入拒面：票面 >100
    /// → [`ChgErr::BadLimit`]；设计容量 0 → [`ChgErr::BadCap`]；余同
    /// [`ChgArbiter::vote`]（拒面零改动——旧票保留）。
    pub fn vote_pct(
        &mut self,
        slot: u8,
        limit_pct: u8,
        reason: u8,
        design_cap_uah: u32,
    ) -> Result<(), ChgErr> {
        if limit_pct > CHG_LIMIT_UNLIMITED_PCT {
            return Err(ChgErr::BadLimit);
        }
        let ma = ma_of_pct(limit_pct, design_cap_uah).ok_or(ChgErr::BadCap)?;
        self.vote(slot, ma, reason)
    }

    /// 撤票（ADR #68「断线自动撤票」的库面形态——通道层断链/出票方亡
    /// 事件映射到此；未投席撤 = 幂等 Ok）。
    pub fn withdraw(&mut self, slot: u8) -> Result<(), ChgErr> {
        let i = slot as usize;
        if i >= CHG_SLOTS {
            return Err(ChgErr::BadSlot);
        }
        self.votes[i] = None;
        Ok(())
    }

    /// 票读面（遥测/测试用；票面 mA）。
    pub fn get(&self, slot: u8) -> Result<Option<(u32, u8)>, ChgErr> {
        let i = slot as usize;
        if i >= CHG_SLOTS {
            return Err(ChgErr::BadSlot);
        }
        Ok(self.votes[i])
    }

    /// 仲裁：**最少者赢**（在席票取 min `limit_ma`）；平票 → 席号小者胜
    /// （严格小于才替换——确定性归因）。全席缺席 →
    /// `{CHG_LIMIT_UNLIMITED_MA, CHG_REASON_NONE}`（无票 = 不限——chargerd
    /// 满充档）。
    pub fn arbitrate(&self) -> ChgDecision {
        let mut best: Option<ChgDecision> = None;
        for v in self.votes.iter().flatten() {
            if best.map_or(true, |b| v.0 < b.limit_ma) {
                best = Some(ChgDecision { limit_ma: v.0, reason: v.1 });
            }
        }
        best.unwrap_or(ChgDecision {
            limit_ma: CHG_LIMIT_UNLIMITED_MA,
            reason: CHG_REASON_NONE,
        })
    }
}

impl Default for ChgArbiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// sim 设计容量单源值（`BatterySnapshot.charge_full_uah` 同源测试值，
    /// 4800 mAh——lib/session 测试面同款样本）。
    const CAP_UAH: u32 = 4_800_000;

    /// 票仲裁全序：三席值域 {None,(4800,J),(2400,L),(1200,U)} 穷举
    /// （4^3 = 64 形）——最少者赢、平票席号小者、全缺席 = 不限。仲裁面的
    /// 全序钉板（mA 域）。
    #[test]
    fn arbitrate_total_order_exhaustive() {
        #[derive(Clone, Copy)]
        struct Form {
            vote: Option<(u32, u8)>,
            expect: Option<(u32, u8)>, // 期望裁决（None = 不限档）
        }
        const J: u8 = CHG_REASON_JEITA;
        const L: u8 = CHG_REASON_LOWBATT;
        const U: u8 = CHG_REASON_USER;
        let forms: [[Form; 4]; 3] = [
            // 席 0（JEITA）
            [
                Form { vote: None, expect: None },
                Form { vote: Some((4800, J)), expect: Some((4800, J)) },
                Form { vote: Some((2400, J)), expect: Some((2400, J)) },
                Form { vote: Some((1200, J)), expect: Some((1200, J)) },
            ],
            // 席 1（低电）
            [
                Form { vote: None, expect: None },
                Form { vote: Some((4800, L)), expect: Some((4800, L)) },
                Form { vote: Some((2400, L)), expect: Some((2400, L)) },
                Form { vote: Some((1200, L)), expect: Some((1200, L)) },
            ],
            // 席 2（用户）
            [
                Form { vote: None, expect: None },
                Form { vote: Some((4800, U)), expect: Some((4800, U)) },
                Form { vote: Some((2400, U)), expect: Some((2400, U)) },
                Form { vote: Some((1200, U)), expect: Some((1200, U)) },
            ],
        ];
        for f0 in forms[0] {
            for f1 in forms[1] {
                for f2 in forms[2] {
                    let mut arb = ChgArbiter::new();
                    if let Some((l, r)) = f0.vote {
                        arb.vote(CHG_SLOT_JEITA, l, r).unwrap();
                    }
                    if let Some((l, r)) = f1.vote {
                        arb.vote(CHG_SLOT_LOWBATT, l, r).unwrap();
                    }
                    if let Some((l, r)) = f2.vote {
                        arb.vote(CHG_SLOT_USER, l, r).unwrap();
                    }
                    // 期望 = 席序枚举下首个最小票（平票席号小者）。
                    let all = [f0.expect, f1.expect, f2.expect];
                    let expect = all
                        .into_iter()
                        .flatten()
                        .min_by_key(|(l, _)| *l)
                        .map(|(l, r)| ChgDecision { limit_ma: l, reason: r })
                        .unwrap_or(ChgDecision {
                            limit_ma: CHG_LIMIT_UNLIMITED_MA,
                            reason: CHG_REASON_NONE,
                        });
                    assert_eq!(arb.arbitrate(), expect, "{:?}/{:?}/{:?}", f0.vote, f1.vote, f2.vote);
                }
            }
        }
    }

    /// 断线撤票链（ADR #68「断线自动撤票」库面）：最小票撤 → 次小顶上；
    /// 全撤 → 不限档；未投席撤 = 幂等。
    #[test]
    fn withdraw_promotes_next_winner() {
        let mut arb = ChgArbiter::new();
        arb.vote(CHG_SLOT_JEITA, 1200, CHG_REASON_JEITA).unwrap();
        arb.vote(CHG_SLOT_LOWBATT, 600, CHG_REASON_LOWBATT).unwrap();
        arb.vote(CHG_SLOT_USER, 1600, CHG_REASON_USER).unwrap();
        assert_eq!(arb.arbitrate().limit_ma, 600); // 低电票最少者赢
        arb.withdraw(CHG_SLOT_LOWBATT).unwrap(); // 断链 → 撤票
        assert_eq!(
            arb.arbitrate(),
            ChgDecision { limit_ma: 1200, reason: CHG_REASON_JEITA }
        );
        arb.withdraw(CHG_SLOT_JEITA).unwrap();
        assert_eq!(
            arb.arbitrate(),
            ChgDecision { limit_ma: 1600, reason: CHG_REASON_USER }
        );
        arb.withdraw(CHG_SLOT_USER).unwrap();
        assert_eq!(arb.arbitrate().reason, CHG_REASON_NONE);
        arb.withdraw(CHG_SLOT_USER).unwrap(); // 幂等
        assert_eq!(arb.arbitrate().limit_ma, CHG_LIMIT_UNLIMITED_MA);
    }

    /// 禁充档：`limit_ma = 0` 合法（JEITA 低温最严档——推断标注见模块注）。
    #[test]
    fn zero_limit_is_legal_strictest() {
        let mut arb = ChgArbiter::new();
        arb.vote(CHG_SLOT_JEITA, 0, CHG_REASON_JEITA).unwrap();
        assert_eq!(
            arb.arbitrate(),
            ChgDecision { limit_ma: 0, reason: CHG_REASON_JEITA }
        );
    }

    /// 不限哨兵票（mA 域满票档）：单席 u32::MAX 也归因（出票即有责）；
    /// 与他席并投时 min 穿透让位（不限不压过任何实际限值）——与
    /// berry-thermal `CHARGE_NO_CAP_MA` 同语义的仲裁面钉板（F1 并轨）。
    #[test]
    fn unlimited_sentinel_attributed_and_penetrates() {
        let mut arb = ChgArbiter::new();
        arb.vote(CHG_SLOT_USER, CHG_LIMIT_UNLIMITED_MA, CHG_REASON_USER)
            .unwrap();
        assert_eq!(
            arb.arbitrate(),
            ChgDecision { limit_ma: CHG_LIMIT_UNLIMITED_MA, reason: CHG_REASON_USER }
        );
        arb.vote(CHG_SLOT_JEITA, 1200, CHG_REASON_JEITA).unwrap();
        assert_eq!(arb.arbitrate().limit_ma, 1200); // 哨兵穿透不拦实际限值
    }

    /// 坏输入拒面：席号越界（3/255）、reason 未知名（0/4/255）；兼容面
    /// 票面 101 → BadLimit、容量 0 → BadCap；拒面零副作用（旧票保留）。
    #[test]
    fn rejects_bad_slot_reason_and_compat_face() {
        let mut arb = ChgArbiter::new();
        arb.vote(CHG_SLOT_JEITA, 1200, CHG_REASON_JEITA).unwrap();
        assert_eq!(arb.vote(3, 2400, CHG_REASON_USER), Err(ChgErr::BadSlot));
        assert_eq!(arb.vote(255, 2400, CHG_REASON_USER), Err(ChgErr::BadSlot));
        assert_eq!(arb.withdraw(3), Err(ChgErr::BadSlot));
        assert_eq!(arb.get(3), Err(ChgErr::BadSlot));
        assert_eq!(arb.vote(CHG_SLOT_USER, 2400, CHG_REASON_NONE), Err(ChgErr::BadReason));
        assert_eq!(arb.vote(CHG_SLOT_USER, 2400, 4), Err(ChgErr::BadReason));
        assert_eq!(arb.vote(CHG_SLOT_USER, 2400, 255), Err(ChgErr::BadReason));
        assert_eq!(
            arb.vote_pct(CHG_SLOT_USER, 101, CHG_REASON_USER, CAP_UAH),
            Err(ChgErr::BadLimit)
        );
        assert_eq!(
            arb.vote_pct(CHG_SLOT_USER, 50, CHG_REASON_USER, 0),
            Err(ChgErr::BadCap)
        );
        // 旧票原样：JEITA 1200 仍是最少者。
        assert_eq!(arb.arbitrate().limit_ma, 1200);
        assert_eq!(arb.get(CHG_SLOT_USER), Ok(None));
    }

    /// F1 桥边界：容量 0 无据 → None 双向；0/100 两极直映（禁充/不限）；
    /// 不限哨兵 ↔ 100；≥1C 饱和 100；单源值精确换算（4800 mAh 基）。
    #[test]
    fn bridge_boundaries() {
        // 无据拒面。
        assert_eq!(pct_of_ma(1500, 0), None);
        assert_eq!(ma_of_pct(50, 0), None);
        // 极值直映。
        assert_eq!(ma_of_pct(0, CAP_UAH), Some(0)); // 禁充
        assert_eq!(ma_of_pct(100, CAP_UAH), Some(CHG_LIMIT_UNLIMITED_MA)); // 不限
        assert_eq!(pct_of_ma(0, CAP_UAH), Some(0));
        assert_eq!(pct_of_ma(CHG_LIMIT_UNLIMITED_MA, CAP_UAH), Some(100));
        // 饱和：≥1C 读 100（C 率域上界 = 不再限流）。
        assert_eq!(pct_of_ma(4800, CAP_UAH), Some(100)); // 恰 1C
        assert_eq!(pct_of_ma(10_000_000, CAP_UAH), Some(100)); // 远超 1C
        // 单源精确/取整值（4800 mAh 基，JEITA 档域实值）。
        assert_eq!(pct_of_ma(2400, CAP_UAH), Some(50)); // 0.5C 恰整
        assert_eq!(ma_of_pct(50, CAP_UAH), Some(2400)); // 往返恰还原
        assert_eq!(pct_of_ma(1500, CAP_UAH), Some(31)); // 31.25 → 收紧取整
        assert_eq!(pct_of_ma(750, CAP_UAH), Some(15)); // 15.625 → 15
        assert_eq!(ma_of_pct(33, CAP_UAH), Some(1584)); // 33% × 4800 恰整
        // 野容量上界不溢出（u64 中间量纪律：99% × u32::MAX 容量）。
        assert_eq!(ma_of_pct(99, u32::MAX), Some((99u64 * u32::MAX as u64 / 100_000) as u32));
    }

    /// F1 桥只收紧不放宽（往返性质律，pct 表达域 <1C 内）：pct→mA→pct
    /// 全域 ≤ 原值；mA→pct→mA 在 <1C 域 ≤ 原值（允收严、禁放宽）——
    /// 限值换算保守侧纪律的 host 判定律。**表达力边界钉板**：≥1C 电流
    /// 读 100=不限（#68 冻结形表达不了的域，超域往返放宽到不限哨兵——
    /// mA 面为真相，F1 裁 mA 的实证）。
    #[test]
    fn bridge_roundtrip_never_loosens() {
        let caps = [CAP_UAH, 4_000_000, 3_040_000, 1_000_000, 500_000, u32::MAX];
        for cap in caps {
            for pct in 0u8..=100 {
                let Some(ma) = ma_of_pct(pct, cap) else { panic!("cap={cap}") };
                let pct2 = pct_of_ma(ma, cap).expect("cap 非 0 必有据");
                assert!(pct2 <= pct, "pct 环放宽: {pct} → {ma}mA → {pct2} (cap={cap})");
                let ma2 = ma_of_pct(pct2, cap).expect("同上");
                assert!(ma2 <= ma, "ma 环放宽: {ma} → {pct2}pct → {ma2} (cap={cap})");
            }
            // 非整除容量（4750 mAh）：33% → 1567.5 收紧为 1567 → 回读 32
            // ——收严 1 个百分点合法、放宽非法的实例钉板。
            if cap == 4_750_000 {
                assert_eq!(ma_of_pct(33, cap), Some(1567));
                assert_eq!(pct_of_ma(1567, cap), Some(32));
            }
        }
        // mA 起环，<1C 表达域内（JEITA/性能档实值域扫：0..cap_mA）。
        for cap in [CAP_UAH, 4_750_000] {
            let one_c_ma = cap / 1000; // 1C = 设计容量数值 mA
            for ma in [0u32, 1, 100, 500, 750, 1200, 1500, 2400, one_c_ma - 1] {
                let pct = pct_of_ma(ma, cap).expect("cap 非 0 必有据");
                assert!(pct < 100, "表达域内不该饱和: {ma}mA (cap={cap})");
                let ma2 = ma_of_pct(pct, cap).expect("同上");
                assert!(ma2 <= ma, "ma 起环放宽: {ma} → {pct}pct → {ma2} (cap={cap})");
            }
            // 表达力边界：≥1C（含恰 1C）读 100 → 回投不限哨兵（超域放宽
            // ——pct 冻结形表达不了的域，F1 裁 mA 的实证面）。
            let one_c_ma = cap / 1000;
            for ma in [one_c_ma, one_c_ma + 1, u32::MAX - 1] {
                assert_eq!(pct_of_ma(ma, cap), Some(100), "超域读面: {ma}mA (cap={cap})");
                let ma2 = ma_of_pct(100, cap).expect("同上");
                assert_eq!(ma2, CHG_LIMIT_UNLIMITED_MA);
            }
        }
    }

    /// F1 兼容投递链：vote_pct(#68 冻结形)经桥落 mA 票 → 仲裁 mA 面 →
    /// `ChgDecision::limit_pct` 兼容读回 pct——线面进/线面出全链对拍，
    /// 与 mA 直投面同一状态机（不留第二套仲裁）。
    #[test]
    fn vote_pct_compat_chain_matches_ma_face() {
        let mut via_pct = ChgArbiter::new();
        via_pct.vote_pct(CHG_SLOT_USER, 50, CHG_REASON_USER, CAP_UAH).unwrap();
        via_pct.vote_pct(CHG_SLOT_LOWBATT, 33, CHG_REASON_LOWBATT, CAP_UAH).unwrap();
        let mut via_ma = ChgArbiter::new();
        via_ma.vote(CHG_SLOT_USER, 2400, CHG_REASON_USER).unwrap();
        via_ma.vote(CHG_SLOT_LOWBATT, 1584, CHG_REASON_LOWBATT).unwrap();
        assert_eq!(via_pct.get(CHG_SLOT_USER), via_ma.get(CHG_SLOT_USER));
        assert_eq!(via_pct.arbitrate(), via_ma.arbitrate());
        let d = via_pct.arbitrate();
        assert_eq!(d, ChgDecision { limit_ma: 1584, reason: CHG_REASON_LOWBATT });
        assert_eq!(d.limit_pct(CAP_UAH), Some(33)); // 兼容读回还原线值
        assert_eq!(d.limit_pct(0), None); // 无据拒面
        // 100% 兼容投递 = 不限哨兵（不落容量数值）。
        via_pct.vote_pct(CHG_SLOT_JEITA, 100, CHG_REASON_JEITA, CAP_UAH).unwrap();
        assert_eq!(via_pct.get(CHG_SLOT_JEITA), Ok(Some((CHG_LIMIT_UNLIMITED_MA, CHG_REASON_JEITA))));
    }
}
