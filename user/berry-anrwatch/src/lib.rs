//! berry-anrwatch —— 生命周期死锁 ANR 判定核（am 总体设计契约二.3 首刀
//! ：`.agents/plans/am/design.md`「生命周期死锁触发 ANR 取证——若应用
//! 在收到前台 `Launch`、`Resume` 或 `PauseSave` 指令后超过阈值（前台
//! 交互 5 秒、后台保存 2 秒）未应答，`appd` 先触发 cbox 抓栈生成 ANR
//! 诊断报告，随后果断终止清理」）。本 crate 钉**判定面**：指令类 ×
//! 前后台 × 未应答时长 → [`Verdict`]（两段语义：先取证、取证窗尽或
//! 完成即终止——「绝不让假死进程挂住监督器」）；cbox 栈抓取与 job
//! 终止执行归服务刀。
//!
//! 语义律：
//! - 阈值 [`FG_DEADLINE_MS`] = 5000 / [`BG_DEADLINE_MS`] = 2000 是契约
//!   钉值；前台指令（Launch/Resume）用前台阈、PauseSave（保存指令）
//!   用后台阈——契约原文「前台交互 5 秒、后台保存 2 秒」按指令类分档。
//! - 两段态机：`Overdue`（应答超时——发取证）→ `EvidenceSent`（取证
//!   已发，`forensic_grace_ms` 内等 cbox 落盘）→ `Terminate`（宽限尽
//!   即杀，取证完成提前杀）。宽限是工程默认（契约未钉——真值归 appd
//!   配置刀）。
//! - 应答在任意态到达即 `Healthy`（撤销链）。

#![cfg_attr(not(test), no_std)]

use core::fmt;

/// 前台交互阈（契约钉值：5 秒）。
pub const FG_DEADLINE_MS: u32 = 5_000;
/// 后台保存阈（契约钉值：2 秒）。
pub const BG_DEADLINE_MS: u32 = 2_000;
/// 取证落盘宽限（工程默认——契约未钉，真值归 appd 配置刀）。
pub const FORENSIC_GRACE_MS: u32 = 3_000;

/// 生命周期指令类（契约枚举的三种）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    Launch,
    Resume,
    PauseSave,
}

impl Cmd {
    /// 该指令的应答阈（Launch/Resume 前台 5s、PauseSave 保存 2s）。
    pub const fn deadline_ms(self) -> u32 {
        match self {
            Self::Launch | Self::Resume => FG_DEADLINE_MS,
            Self::PauseSave => BG_DEADLINE_MS,
        }
    }
}

/// 判定（两段语义态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// 阈内——健康等应答。
    Healthy,
    /// 已超时——发 cbox 取证。
    Overdue,
    /// 取证在途（宽限窗内）。
    EvidenceSent,
    /// 宽限尽（或取证已完成）——果断终止。
    Terminate,
}

/// 判定核：`elapsed_ms` = 派发后未应答时长；`forensic_done` = 取证
/// 已落盘（提前终止依据）；`forensic_sent` = 取证已发出。
pub fn verdict(
    cmd: Cmd,
    elapsed_ms: u32,
    forensic_sent: bool,
    forensic_done: bool,
) -> Verdict {
    if elapsed_ms < cmd.deadline_ms() {
        return Verdict::Healthy;
    }
    if forensic_done {
        return Verdict::Terminate; // 证据在手——不空等宽限
    }
    if !forensic_sent {
        return Verdict::Overdue; // 第一步：发取证
    }
    // 取证在途：宽限尽即杀。
    if elapsed_ms >= cmd.deadline_ms() + FORENSIC_GRACE_MS {
        Verdict::Terminate
    } else {
        Verdict::EvidenceSent
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Verdict::Healthy => "阈内健康",
            Verdict::Overdue => "超时发取证",
            Verdict::EvidenceSent => "取证宽限中",
            Verdict::Terminate => "果断终止",
        };
        f.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadlines_per_command_class() {
        assert_eq!(Cmd::Launch.deadline_ms(), FG_DEADLINE_MS);
        assert_eq!(Cmd::Resume.deadline_ms(), FG_DEADLINE_MS);
        assert_eq!(Cmd::PauseSave.deadline_ms(), BG_DEADLINE_MS);
        assert_eq!(FG_DEADLINE_MS, 5_000);
        assert_eq!(BG_DEADLINE_MS, 2_000);
    }

    #[test]
    fn healthy_under_threshold() {
        for c in [Cmd::Launch, Cmd::Resume, Cmd::PauseSave] {
            assert_eq!(verdict(c, c.deadline_ms() - 1, true, true), Verdict::Healthy);
            assert_eq!(verdict(c, 0, false, false), Verdict::Healthy);
        }
    }

    #[test]
    fn overdue_then_forensic_then_terminate() {
        let c = Cmd::Launch;
        // 超时第一步：发取证（即使取证已发的旗在场——判定按未发路径
        // 只在旗假时走；本腿钉旗假路径）。
        assert_eq!(verdict(c, FG_DEADLINE_MS, false, false), Verdict::Overdue);
        // 取证在途：宽限窗内。
        assert_eq!(
            verdict(c, FG_DEADLINE_MS + 1, true, false),
            Verdict::EvidenceSent
        );
        assert_eq!(
            verdict(c, FG_DEADLINE_MS + FORENSIC_GRACE_MS - 1, true, false),
            Verdict::EvidenceSent
        );
        // 宽限尽：杀。
        assert_eq!(
            verdict(c, FG_DEADLINE_MS + FORENSIC_GRACE_MS, true, false),
            Verdict::Terminate
        );
    }

    #[test]
    fn pause_save_uses_bg_deadline() {
        // 后台 2s 阈：1.9s 健康、2s 超时。
        assert_eq!(verdict(Cmd::PauseSave, 1_999, false, false), Verdict::Healthy);
        assert_eq!(verdict(Cmd::PauseSave, 2_000, false, false), Verdict::Overdue);
        // 同时刻前台指令仍在阈内——分档语义。
        assert_eq!(verdict(Cmd::Launch, 2_000, false, false), Verdict::Healthy);
    }

    #[test]
    fn forensic_done_short_circuits_grace() {
        // 证据在手即杀，不空等宽限（假死不挂监督器）。
        assert_eq!(
            verdict(Cmd::Resume, FG_DEADLINE_MS + 10, true, true),
            Verdict::Terminate
        );
        // 取证完成但在阈内（时钟回拨态）——健康优先（应答撤销链）。
        assert_eq!(verdict(Cmd::Resume, 100, true, true), Verdict::Healthy);
    }
}
