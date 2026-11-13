//! 充电限流票仲裁 —— JEITA 票 + 低电票 + 用户帽 → min（lib 核面 ②）。
//!
//! **契约原文锚 ADR #68（同窗冻结件）**：「P11 充电限值票
//! （`CHG_OP_VOTE_LIMIT{limit_pct, reason:u8}`，最少者赢、断线自动撤票、
//! chargerd 持有）」；出票边界锚 ADR #66 ⑤：JEITA 充电温区五档 = 第一张
//! 系统热表，票由 **berry-thermal 出票**（本服务不养第二套热表——sim 热
//! 快照只进遥测，不旁路出票）。本面 = 仲裁数学**单源**：三席定票
//! （JEITA/低电/用户帽）各持 `Option<{limit_pct, reason}>`，
//! [`ChgArbiter::arbitrate`] 最少者赢（min `limit_pct`）。
//!
//! **chargerd 契约位**（留窗如实注）：票持有者/动态票主注册/断线撤票的
//! 通道执行面 = chargerd 服务本体（立法件无代码——本面只提供它将消费的
//! 仲裁纯函数与 [`ChgArbiter::withdraw`] 撤票库面形态；断链事件 → 撤票
//! 的映射挂 chargerd/通道刀）。百分比 vs 电流：#68 冻结形态 = `limit_pct`
//! （百分比）；P11 电流版（mA）= 真机充电固件面，百分比先行——推断标注。
//!
//! 票面值域：`limit_pct` 0..=100——`0` = 禁充（JEITA 低温区最严档，锂电
//! 低温禁充同义；推断标注）、`100` = 满充不限。无票（全席缺席）→
//! `{100, NONE}` = 不限。reason 词表 add-only；`CHG_REASON_NONE` = 仲裁
//! 面专属（无票形态），出票面不可投。

/// 票席：JEITA 温区票（出票方 = berry-thermal，ADR #66 ⑤）。
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

/// 满充不限档（`limit_pct` 上界；无票裁决的票面值）。
pub const CHG_LIMIT_UNLIMITED: u8 = 100;

/// 票面写错误（坏输入拒面——零 panic 路径）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChgErr {
    /// 席号越界（`slot >= CHG_SLOTS`）。
    BadSlot,
    /// 票面越界（`limit_pct > 100`）。
    BadLimit,
    /// reason 未知名（词表外 u8——add-only 纪律下未知值 = 契约漂移，拒）。
    BadReason,
}

/// 仲裁裁决：生效限流票面 + 胜出归因（「谁在限充」单源答案）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChgDecision {
    pub limit_pct: u8,
    pub reason: u8,
}

/// 充电限流票仲裁机（三席定票、纯函数状态机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChgArbiter {
    votes: [Option<(u8, u8)>; CHG_SLOTS], // (limit_pct, reason)
}

impl ChgArbiter {
    /// 新建（全席撤票 = 不限）。
    pub const fn new() -> Self {
        ChgArbiter { votes: [None; CHG_SLOTS] }
    }

    /// 投票（`CHG_OP_VOTE_LIMIT{limit_pct, reason}` 契约位的库面形态）。
    /// 坏输入拒面：席号越界 → [`ChgErr::BadSlot`]；票面 >100 →
    /// [`ChgErr::BadLimit`]；reason 未知名或 `NONE` → [`ChgErr::BadReason`]
    /// （拒面零改动——旧票保留）。
    pub fn vote(&mut self, slot: u8, limit_pct: u8, reason: u8) -> Result<(), ChgErr> {
        let i = slot as usize;
        if i >= CHG_SLOTS {
            return Err(ChgErr::BadSlot);
        }
        if limit_pct > CHG_LIMIT_UNLIMITED {
            return Err(ChgErr::BadLimit);
        }
        if !matches!(reason, CHG_REASON_JEITA | CHG_REASON_LOWBATT | CHG_REASON_USER) {
            return Err(ChgErr::BadReason);
        }
        self.votes[i] = Some((limit_pct, reason));
        Ok(())
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

    /// 票读面（遥测/测试用）。
    pub fn get(&self, slot: u8) -> Result<Option<(u8, u8)>, ChgErr> {
        let i = slot as usize;
        if i >= CHG_SLOTS {
            return Err(ChgErr::BadSlot);
        }
        Ok(self.votes[i])
    }

    /// 仲裁：**最少者赢**（在席票取 min `limit_pct`）；平票 → 席号小者胜
    /// （严格小于才替换——确定性归因）。全席缺席 →
    /// `{CHG_LIMIT_UNLIMITED, CHG_REASON_NONE}`（无票 = 不限——chargerd
    /// 满充档）。
    pub fn arbitrate(&self) -> ChgDecision {
        let mut best: Option<ChgDecision> = None;
        for v in self.votes.iter().flatten() {
            if best.map_or(true, |b| v.0 < b.limit_pct) {
                best = Some(ChgDecision { limit_pct: v.0, reason: v.1 });
            }
        }
        best.unwrap_or(ChgDecision {
            limit_pct: CHG_LIMIT_UNLIMITED,
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

    /// 票仲裁全序：三席值域 {None,(100,J),(60,L),(30,U)} 穷举（4^3 = 64 形）
    /// ——最少者赢、平票席号小者、全缺席 = 不限。仲裁面的全序钉板。
    #[test]
    fn arbitrate_total_order_exhaustive() {
        #[derive(Clone, Copy)]
        struct Form {
            vote: Option<(u8, u8)>,
            expect: Option<(u8, u8)>, // 期望裁决（None = 不限档）
        }
        const J: u8 = CHG_REASON_JEITA;
        const L: u8 = CHG_REASON_LOWBATT;
        const U: u8 = CHG_REASON_USER;
        let forms: [[Form; 4]; 3] = [
            // 席 0（JEITA）
            [
                Form { vote: None, expect: None },
                Form { vote: Some((100, J)), expect: Some((100, J)) },
                Form { vote: Some((60, J)), expect: Some((60, J)) },
                Form { vote: Some((30, J)), expect: Some((30, J)) },
            ],
            // 席 1（低电）
            [
                Form { vote: None, expect: None },
                Form { vote: Some((100, L)), expect: Some((100, L)) },
                Form { vote: Some((60, L)), expect: Some((60, L)) },
                Form { vote: Some((30, L)), expect: Some((30, L)) },
            ],
            // 席 2（用户）
            [
                Form { vote: None, expect: None },
                Form { vote: Some((100, U)), expect: Some((100, U)) },
                Form { vote: Some((60, U)), expect: Some((60, U)) },
                Form { vote: Some((30, U)), expect: Some((30, U)) },
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
                        .map(|(l, r)| ChgDecision { limit_pct: l, reason: r })
                        .unwrap_or(ChgDecision {
                            limit_pct: CHG_LIMIT_UNLIMITED,
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
        arb.vote(CHG_SLOT_JEITA, 60, CHG_REASON_JEITA).unwrap();
        arb.vote(CHG_SLOT_LOWBATT, 30, CHG_REASON_LOWBATT).unwrap();
        arb.vote(CHG_SLOT_USER, 80, CHG_REASON_USER).unwrap();
        assert_eq!(arb.arbitrate().limit_pct, 30); // 低电票最少者赢
        arb.withdraw(CHG_SLOT_LOWBATT).unwrap(); // 断链 → 撤票
        assert_eq!(
            arb.arbitrate(),
            ChgDecision { limit_pct: 60, reason: CHG_REASON_JEITA }
        );
        arb.withdraw(CHG_SLOT_JEITA).unwrap();
        assert_eq!(
            arb.arbitrate(),
            ChgDecision { limit_pct: 80, reason: CHG_REASON_USER }
        );
        arb.withdraw(CHG_SLOT_USER).unwrap();
        assert_eq!(arb.arbitrate().reason, CHG_REASON_NONE);
        arb.withdraw(CHG_SLOT_USER).unwrap(); // 幂等
        assert_eq!(arb.arbitrate().limit_pct, CHG_LIMIT_UNLIMITED);
    }

    /// 禁充档：`limit_pct = 0` 合法（JEITA 低温最严档——推断标注见模块注）。
    #[test]
    fn zero_limit_is_legal_strictest() {
        let mut arb = ChgArbiter::new();
        arb.vote(CHG_SLOT_JEITA, 0, CHG_REASON_JEITA).unwrap();
        assert_eq!(
            arb.arbitrate(),
            ChgDecision { limit_pct: 0, reason: CHG_REASON_JEITA }
        );
    }

    /// 满票档：票面恰 100 也归因（不落回 NONE——出票即有责）。
    #[test]
    fn full_limit_vote_still_attributed() {
        let mut arb = ChgArbiter::new();
        arb.vote(CHG_SLOT_USER, 100, CHG_REASON_USER).unwrap();
        assert_eq!(
            arb.arbitrate(),
            ChgDecision { limit_pct: 100, reason: CHG_REASON_USER }
        );
    }

    /// 坏输入拒面：席号越界（3/255）、票面 101、reason 未知名（0/4/255）；
    /// 拒面零副作用（旧票保留）。
    #[test]
    fn rejects_bad_slot_limit_reason() {
        let mut arb = ChgArbiter::new();
        arb.vote(CHG_SLOT_JEITA, 40, CHG_REASON_JEITA).unwrap();
        assert_eq!(arb.vote(3, 50, CHG_REASON_USER), Err(ChgErr::BadSlot));
        assert_eq!(arb.vote(255, 50, CHG_REASON_USER), Err(ChgErr::BadSlot));
        assert_eq!(arb.withdraw(3), Err(ChgErr::BadSlot));
        assert_eq!(arb.get(3), Err(ChgErr::BadSlot));
        assert_eq!(arb.vote(CHG_SLOT_USER, 101, CHG_REASON_USER), Err(ChgErr::BadLimit));
        assert_eq!(arb.vote(CHG_SLOT_USER, 50, CHG_REASON_NONE), Err(ChgErr::BadReason));
        assert_eq!(arb.vote(CHG_SLOT_USER, 50, 4), Err(ChgErr::BadReason));
        assert_eq!(arb.vote(CHG_SLOT_USER, 50, 255), Err(ChgErr::BadReason));
        // 旧票原样：JEITA 40 仍是最少者。
        assert_eq!(arb.arbitrate().limit_pct, 40);
        assert_eq!(arb.get(CHG_SLOT_USER), Ok(None));
    }
}
