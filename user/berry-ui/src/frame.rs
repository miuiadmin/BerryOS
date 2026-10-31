//! berry-ui 帧循环态（GUI-3——`docs/GUI-3实施设计草案.md` §3.8：验收②
//! 主体 + 软 vsync；ADR #53 三态停帧 RUN/SETTLE/IDLE + FPS
//! expected/actual 双轨）。
//!
//! - **三态纯函数状态机**（[`next_phase`]）：任一工作信号 → Run；全假
//!   但在途帧回调未清 → Settle；全假无在途 → Idle；静默超
//!   [`crate::IDLE_AFTER_MS`]（OH 常量 2000ms）**强降 Idle 兜底**——
//!   Run 与卡死的 Settle 同裁（信号在先：有工作即 Run，强降只裁静默）。
//! - **IDLE = [`next_wakeup`] == None**（撤软 vsync 定时器、睡事件——
//!   R3 反面清单对策）；引擎不持定时器（request-driven——GUI-10 ⑤
//!   形态的最薄实现）：算出下一应醒**绝对 ns** 交 embedder 睡/等。
//! - **vsync 拍沿含沿不欠拍**（[`next_vsync_tick`]）：now 恰在拍上 =
//!   本拍即返——wake 后立即渲不欠拍（§3.8）。
//! - **credits 背压簿记**（[`Credits`]）：≤ `CREDITS_MAX`（guiproto 单
//!   源）——帽满不提交不轰炸（对齐条 8）。
//! - **vote 产票不仲裁**（[`ballot_for`]）：仲裁在显示服务（对齐条 12）；
//!   v1 票值定值表（scene 取 guiproto 单源）。
//! - 零 panic / 零分配：period 0（vsync 未协商）退化即时，不除零。
//!
//! - **集成接缝（已收敛）**：next_vsync_tick / next_wakeup / Credits /
//!   ballot_for 均为单源——lib.rs 的 Ui 内联面（next_tick_at / credits
//!   u8 记账 / vote_hint 派生票 / next_wakeup 内联）已迁本模块消费
//!   （`#[allow(dead_code)]` 全摘）；语义以 §6+§5 wire 语义对齐为准
//!   （credits：显示服务报的**剩余信用**是真值源，outstanding =
//!   MAX − remaining 互为补码）。

use crate::damage::DamageSet;

/// 帧循环三态（§6 冻结；Debug+Clone+Copy+PartialEq+Eq）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramePhase {
    /// 有工作（damage/anim/input 任一）——持续渲。
    Run,
    /// 工作已空但在途帧回调未清——等 settle 收口。
    Settle,
    /// 停帧：零 SUBMIT 零 FRAME_CALLBACK（对齐条 6）——撤定时器睡事件。
    Idle,
}

/// 帧循环输入信号（§6 冻结字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameSignals {
    pub damage: bool,
    pub anim: bool,
    pub input: bool,
    /// 在途帧回调未清（present_pending——Settle 态的判定源）。
    pub present_pending: bool,
}

/// 三态转移纯函数（§6 冻结签名；验收②）。`idle_ms_since_work` = 自上
/// 次工作以来的静默毫秒数。真值表（§3.8）：
///
/// | 条件 | 次态 |
/// |------|------|
/// | damage ∨ anim ∨ input | Run |
/// | 静默 > IDLE_AFTER_MS | Idle（强降——Run/卡死 Settle 同裁兜底） |
/// | present_pending | Settle |
/// | 其余 | Idle |
///
/// `cur` 逻辑上被上表覆盖（每行结论与 cur 无关）——保留在签名里是 §6
/// 冻结面（调用方调试打印用），真值不读它。
pub fn next_phase(cur: FramePhase, sig: FrameSignals, idle_ms_since_work: u64) -> FramePhase {
    let _ = cur; // 见上：真值表与现态无关（§3.8「保持/升级」由信号自证）
    if sig.damage || sig.anim || sig.input {
        return FramePhase::Run;
    }
    if idle_ms_since_work > crate::IDLE_AFTER_MS {
        return FramePhase::Idle;
    }
    if sig.present_pending {
        return FramePhase::Settle;
    }
    FramePhase::Idle
}

/// 下一个 vsync 拍点（绝对 ns）。period 0（未协商/无效）→ now（退化即
/// 时——不除零）；**now 恰在拍沿 → 返 now 本拍**（含沿不欠拍：wake 后
/// 立即渲不欠拍——§3.8）。phase 以 mod 归一（敌意 phase ≥ period 容忍）。
pub(crate) fn next_vsync_tick(now_ns: u64, period_ns: u64, phase_ns: u64) -> u64 {
    if period_ns == 0 {
        return now_ns;
    }
    let phase = phase_ns % period_ns;
    let now_phase = now_ns % period_ns;
    if now_phase == phase {
        return now_ns;
    }
    let delta = if now_phase < phase {
        phase - now_phase
    } else {
        period_ns - (now_phase - phase)
    };
    now_ns.saturating_add(delta)
}

/// 下一应醒时刻（绝对 ns；§3.8 request-driven——引擎不持定时器，本值
/// 交 embedder 睡/等事件）。**Idle → None = 撤软 vsync 定时器、睡事
/// 件**（R3 反面清单对策）。Run = min(vsync 拍点, 动画期限, 强降闹钟)；
/// Settle = 强降闹钟（在途回调迟到的兜底醒）。所有期限取 max(now)——
/// 过点期限 = 立即醒（不回拨等待）。
pub(crate) fn next_wakeup(
    phase: FramePhase,
    now_ns: u64,
    last_work_ns: u64,
    period_ns: u64,
    phase_ns: u64,
    anim_deadline_ns: Option<u64>,
) -> Option<u64> {
    if phase == FramePhase::Idle {
        return None;
    }
    // 静默强降闹钟（Run/卡死 Settle 共用兜底）
    let idle_at = last_work_ns.saturating_add(crate::IDLE_AFTER_MS * 1_000_000);
    let mut t = idle_at.max(now_ns);
    if phase == FramePhase::Run {
        let vs = next_vsync_tick(now_ns, period_ns, phase_ns);
        if vs < t {
            t = vs;
        }
        if let Some(d) = anim_deadline_ns {
            let d = d.max(now_ns); // 过点动画期限 → 立即
            if d < t {
                t = d;
            }
        }
    }
    Some(t)
}

/// credits 背压簿记（§3.8：≤ `CREDITS_MAX`——guiproto 单源；对齐条 8
/// 「不轰炸」：帽满即不提交）。outstanding = 已提交未应答数。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Credits {
    outstanding: u8,
}

impl Credits {
    pub(crate) const fn new() -> Self {
        Self { outstanding: 0 }
    }

    /// 试图占一信用：true = 可提交；false = 帽满（调用方不提交——
    /// backpressure 是义务不是建议）。
    pub(crate) fn submit(&mut self) -> bool {
        if self.outstanding >= berry_guiproto::CREDITS_MAX {
            return false;
        }
        self.outstanding += 1;
        true
    }

    /// present 应答回填：显示服务报的**剩余信用**是真值源
    /// （outstanding = MAX − remaining；未知应答不猜测）。
    pub(crate) fn on_presented(&mut self, credits_remaining: u8) {
        let remaining = credits_remaining.min(berry_guiproto::CREDITS_MAX);
        self.outstanding = berry_guiproto::CREDITS_MAX - remaining;
    }
}

/// 单帧产物（§6 冻结字段；render_frame 返回值）。`drew=false`（damage
/// 空）= **不提交**（「无 damage 不提交」+ IDLE 零 SUBMIT 零
/// FRAME_CALLBACK，对齐条 6）。
#[derive(Debug, Clone)]
pub struct FrameOutcome {
    pub drew: bool,
    pub damage: DamageSet,
    /// 期望呈现时刻（未来拍点 = expected 轨数据源，对齐条 7）。
    pub expected_present_ns: u64,
    /// 每帧必填的帧率提示（对齐条 12；drew=false 的跳过帧诚实填 0）。
    pub rate_hint_hz: u8,
}

impl FrameOutcome {
    /// 跳过帧（damage 空——无提交）：damage 空集、rate_hint 0（不提交
    /// 的帧无帧率话语权）。lib.rs `render_frame` 的 `drew=false` 出口走
    /// 此构造器——跳过帧语义单源（字段注「诚实填 0」的执法点）。
    pub(crate) fn skipped(expected_present_ns: u64) -> Self {
        Self { drew: false, damage: crate::damage::DamageSet::new(), expected_present_ns, rate_hint_hz: 0 }
    }
}

/// 帧率协商票面（§6 冻结——**引擎只产票不仲裁**，仲裁在显示服务，
/// 对齐条 12）。全字段 pub（§6 逐字——sticky 为粘性场景位，v1 票源恒
/// 0 除 IDLE 场景置 1 停帧粘滞）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoteBallot {
    pub min_hz: u16,
    pub max_hz: u16,
    pub preferred_hz: u16,
    pub scene: u8,
    pub sticky: u8,
}

/// 场景票面定值表（v1）：idle（**可全停**——min/max/preferred 全 0 +
/// sticky=1 停帧粘滞）/ 交互进行中（SCENE_DRAG 120 档）/ 动画
/// （SCENE_ANIMATED 60 档）/ 常态（SCENE_NORMAL 60 档上限）。scene
/// 取 guiproto 单源常量；sticky 唯置位点 = idle（停帧粘滞——显示服务
/// 收到 sticky 票后不再唤醒引擎，§3.8）。
pub(crate) fn ballot_for(anim_active: bool, interacting: bool, idle: bool) -> VoteBallot {
    if idle {
        return VoteBallot {
            min_hz: 0,
            max_hz: 0,
            preferred_hz: 0,
            scene: berry_guiproto::SCENE_IDLE,
            sticky: 1,
        };
    }
    if interacting {
        return VoteBallot {
            min_hz: 60,
            max_hz: 120,
            preferred_hz: 120,
            scene: berry_guiproto::SCENE_DRAG,
            sticky: 0,
        };
    }
    if anim_active {
        return VoteBallot {
            min_hz: 30,
            max_hz: 120,
            preferred_hz: 60,
            scene: berry_guiproto::SCENE_ANIMATED,
            sticky: 0,
        };
    }
    VoteBallot {
        min_hz: 0,
        max_hz: 60,
        preferred_hz: 60,
        scene: berry_guiproto::SCENE_NORMAL,
        sticky: 0,
    }
}
