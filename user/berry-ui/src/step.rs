//! AP2 固定时步 + AP3 时长整帧量化（刷新率规范 §9——刀6 应用面）。
//!
//! - **AP2 固定时步**（[`FixedStep`]）：仿真与渲染解耦——仿真 dt 恒定
//!   [`STEP_DT_NS`]（基准 1/120s，不随档位变），渲染按 [`StepPlan`]
//!   的 `alpha_permille` 帧内插值。时长不变性（§10-T5）：总步数 =
//!   floor(总墙钟/dt) 与分帧 cadence 无关（累加器携余——60/120/165/
//!   180 混合 cadence 下动画时长不变）；追帧帽 [`STEP_CATCHUP_MAX`]
//!   防停帧后追帧螺旋（超帽丢弃 = 仿真瞬移到墙钟）。
//! - **AP3 时长整帧量化**（[`quantize_duration_ns`]）：动效时长按当前
//!   档周期取整到整帧数（最近侧取整，最少 1 帧——250ms → 60Hz 15 帧
//!   / 120Hz 30 帧），禁小数帧尾抖。
//! - 零 panic / 零分配：period 0 / delta 0 退化安全（不除零）。

/// AP2 仿真步长（1/120s；数值 = guiproto TIERS 120Hz 档周期单源对齐
/// ——整数域无 8_333_333.33 循环，0.33ns/步偏置为立法取整）。
pub const STEP_DT_NS: u64 = 8_333_333;

/// 追帧帽：单帧最多追 8 步（≈66ms）——停帧/卡顿后不追帧螺旋，超帽
/// 丢弃（仿真瞬移——`dropped_steps` 透出供遥测）。
pub const STEP_CATCHUP_MAX: u32 = 8;

/// 单帧推进计划（AP2 消费面：`steps` 次固定步仿真 + 按 `alpha_permille`
/// 插值渲染）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepPlan {
    /// 本帧应执行的固定步数（0 = 纯插值帧——仿真不动渲染先行）。
    pub steps: u32,
    /// 帧内插值系数（0..=1000 千分比——渲染在上一仿真态与本态间内插）。
    pub alpha_permille: u16,
    /// 追帧帽丢弃步数（>0 = 仿真瞬移——停帧后追不齐的诚实透出）。
    pub dropped_steps: u32,
}

/// AP2 固定时步累加器（纯核——host 单测面；embedder 每帧喂墙钟帧距）。
#[derive(Debug, Clone)]
pub struct FixedStep {
    /// 未消费墙钟余数（< STEP_DT_NS）。
    acc_ns: u64,
    /// 累计仿真步数（sim 时间 = steps × STEP_DT_NS——不持浮点）。
    sim_steps: u64,
}

impl FixedStep {
    pub const fn new() -> Self {
        Self { acc_ns: 0, sim_steps: 0 }
    }

    /// 累计仿真步数（AP6 可复现面——DETERMINISTIC 同输入同计数）。
    pub fn steps(&self) -> u64 {
        self.sim_steps
    }

    /// 仿真时间（ns）。
    pub fn sim_ns(&self) -> u64 {
        self.sim_steps.saturating_mul(STEP_DT_NS)
    }

    /// 帧推进：喂墙钟帧距（now − 上帧 now；首帧喂 0）。步数与 alpha 由
    /// 累加器裁决——同一总墙钟下与分帧 cadence 无关（时长不变性）。
    pub fn advance(&mut self, frame_delta_ns: u64) -> StepPlan {
        self.acc_ns = self.acc_ns.saturating_add(frame_delta_ns);
        let due = self.acc_ns / STEP_DT_NS;
        self.acc_ns -= due * STEP_DT_NS;
        // 追帧帽：due 超帽即丢弃（acc 已扣 = 仿真瞬移到墙钟）。
        let dropped = due.saturating_sub(STEP_CATCHUP_MAX as u64) as u32;
        let steps = (due as u32).min(STEP_CATCHUP_MAX);
        self.sim_steps += steps as u64;
        let alpha_permille = (self.acc_ns * 1000 / STEP_DT_NS) as u16;
        StepPlan { steps, alpha_permille, dropped_steps: dropped }
    }

    /// 重锚（长停后调用方主动弃账——清余数保 alpha 从 0 起步；步数账保留）。
    pub fn reanchor(&mut self) {
        self.acc_ns = 0;
    }
}

/// AP3 时长整帧量化：按当前档周期取整到整帧（最近侧，最少 1 帧）。
/// period 0（档未协商）= 退化直通（不量化）；dur 0 = 0（零时长不动）。
pub fn quantize_duration_ns(dur_ns: u64, period_ns: u64) -> u64 {
    if period_ns == 0 || dur_ns == 0 {
        return dur_ns;
    }
    let frames = ((dur_ns + period_ns / 2) / period_ns).max(1);
    frames * period_ns
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_step_alpha_and_steps() {
        let mut s = FixedStep::new();
        // 首帧零距：纯插值帧（steps 0、alpha 0）。
        let p = s.advance(0);
        assert_eq!((p.steps, p.alpha_permille, p.dropped_steps), (0, 0, 0));
        // 恰一步：整步落地、余 0。
        let p = s.advance(STEP_DT_NS);
        assert_eq!((p.steps, p.alpha_permille, p.dropped_steps), (1, 0, 0));
        // 整域可表 alpha：8ms = 0.96 步 → 960 千分比（无半步截断假象）。
        let p = s.advance(8_000_000);
        assert_eq!((p.steps, p.alpha_permille), (0, 960));
        // 又三分之一步：累加器携上帧余（8ms + DT/3 = 10_777_777 = 1 步 +
        // 2_444_444 余）→ 整步 + alpha 293（跨帧携余 = 不变性的机制本体）。
        let p = s.advance(STEP_DT_NS / 3);
        assert_eq!((p.steps, p.alpha_permille), (1, 293));
        assert_eq!(s.steps(), 2);
        assert_eq!(s.sim_ns(), 2 * STEP_DT_NS);
    }

    #[test]
    fn fixed_step_duration_invariance() {
        // T5 时长不变性：总墙钟 1s 下总步数与分帧 cadence 无关（累加器
        // 携余）；60Hz / 165Hz 混合 / 单发爆发三形同计。
        let total = 1_000_000_000u64;
        let expect_steps = (total / STEP_DT_NS) as u64;
        let mut a = FixedStep::new();
        for _ in 0..60 {
            a.advance(16_666_667);
        }
        assert_eq!(a.steps(), expect_steps, "60Hz cadence");
        let mut b = FixedStep::new();
        for _ in 0..164 {
            b.advance(6_060_606); // 165Hz 档距（1e9/165 取整）
        }
        b.advance(1_000_000_000 - 164 * 6_060_606); // 尾差补齐（非整除分频）
        assert_eq!(b.steps(), expect_steps, "165Hz 非整除分频");
        let mut c = FixedStep::new();
        // 帽内爆发混合分帧（40ms × 25 帧——单帧 due ≤ 4 步不触追帧帽）。
        for _ in 0..25 {
            c.advance(40_000_000);
        }
        assert_eq!(c.steps(), expect_steps, "帽内爆发混合分帧");
        // 三形仿真时间一致（无累计漂移）。
        assert_eq!(a.sim_ns(), c.sim_ns());
    }

    #[test]
    fn fixed_step_catchup_cap() {
        // 追帧帽：停帧 1s 单帧到席 → due 120 步超帽 → 执行 8 丢 112。
        let mut s = FixedStep::new();
        let p = s.advance(1_000_000_000);
        assert_eq!((p.steps, p.dropped_steps), (STEP_CATCHUP_MAX, 112));
        assert_eq!(s.steps(), STEP_CATCHUP_MAX as u64, "只追帽内步数");
        // 后续正常帧恢复（帽不被污染）。
        let p = s.advance(16_666_667);
        assert_eq!((p.steps, p.dropped_steps), (2, 0));
        // 重锚：清余数 alpha 归零、步数账保留。
        let mut r = FixedStep::new();
        r.advance(STEP_DT_NS / 2);
        r.reanchor();
        let p = r.advance(0);
        assert_eq!((p.steps, p.alpha_permille), (0, 0));
    }

    #[test]
    fn quantize_duration_frames() {
        // AP3 例：250ms → 60Hz 15 帧 / 120Hz 30 帧。
        assert_eq!(quantize_duration_ns(250_000_000, 16_666_667), 15 * 16_666_667);
        assert_eq!(quantize_duration_ns(250_000_000, 8_333_333), 30 * 8_333_333);
        // 非整倍数最近侧取整：105ms @60Hz = 6.3 帧 → 6 帧。
        assert_eq!(quantize_duration_ns(105_000_000, 16_666_667), 6 * 16_666_667);
        // 整域半帧界：25ms @60Hz = 1.4999…9 帧（16_666_667×1.5 =
        // 25_000_000.5）→ 界下 1 帧；界上（+1ns）→ 2 帧。
        assert_eq!(quantize_duration_ns(25_000_000, 16_666_667), 16_666_667);
        assert_eq!(quantize_duration_ns(25_000_001, 16_666_667), 2 * 16_666_667);
        // 亚帧最少 1 帧；零时长直通；档未协商直通。
        assert_eq!(quantize_duration_ns(1_000_000, 16_666_667), 16_666_667);
        assert_eq!(quantize_duration_ns(0, 16_666_667), 0);
        assert_eq!(quantize_duration_ns(250_000_000, 0), 250_000_000);
    }
}
