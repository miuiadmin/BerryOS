//! keynav —— 键盘序列重组纯逻辑（键盘消费刀 2026-11-20）。
//!
//! inputd KeyPump 的箭头出向词汇 = ESC 序列三帧单字节连发
//!（`0x1b '[' 'A'/'B'`——xterm 同形，UP/DOWN 各一；LEFT/RIGHT 表外不
//! 出向，gui-inputd 负测钉板）。pump 把键帧逐字节直投
//! [`HostHooks::on_key`](berry_appshell::pump::HostHooks::on_key)——消费
//! 侧（frameloop）须按帧重组序列才能恢复「一次箭头 = 一个动作」的语
//! 义。本模块是该重组的单步纯函数（状态 = 单字节 [`KSEQ`] 三态），动
//! 作判定与桌面翻页执行分离（门面/边界/wmd 覆盖查询在 frameloop——
//! I/O 面不进纯逻辑）。
//!
//! 吞面纪律（bsh editor「非 A/B finals 一律吞」同源）：序列失序（ESC
//! 后非 `[`）、表外 final、基态非 ESC 字节一律静默吞——v1 桌面无文本
//! 输入面（syspages Apps Search Input 件的键盘喂入是 v2 面），launcher
//! 键盘词汇只有箭头翻页一族。

/// 序列重组动作（final 字节裁决后的语义面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyNav {
    /// 箭头 UP（final `A`）——桌面 pager 上一页。
    PagePrev,
    /// 箭头 DOWN（final `B`）——桌面 pager 下一页。
    PageNext,
}

/// 重组态三档：0 基态 / 1 已收 ESC / 2 已收 ESC `[`。
pub const KSEQ_BASE: u8 = 0;
pub const KSEQ_ESC: u8 = 1;
pub const KSEQ_ESC_BRACKET: u8 = 2;

/// 单步重组：`kseq` = 重组态（就地推进），`code` = 本帧字节；返回
/// `Some(动作)` = 序列完成（态已复位），`None` = 吞/续态（无动作）。
///
/// 连发 ESC（态 1 再收 `0x1b`）= 前帧裸 ESC 已吞、本帧新序列头——保持
/// 态 1（两帧语义折算，不产动作）。
pub fn seq_step(kseq: &mut u8, code: u8) -> Option<KeyNav> {
    match (*kseq, code) {
        (KSEQ_BASE, 0x1b) => {
            *kseq = KSEQ_ESC;
            None
        }
        (KSEQ_ESC, b'[') => {
            *kseq = KSEQ_ESC_BRACKET;
            None
        }
        (KSEQ_ESC, 0x1b) => None, // 连发 ESC：保持态 1（新序列头）
        (KSEQ_ESC, _) => {
            *kseq = KSEQ_BASE; // 裸 ESC 后随字节——吞复位
            None
        }
        (KSEQ_ESC_BRACKET, b'A') => {
            *kseq = KSEQ_BASE;
            Some(KeyNav::PagePrev)
        }
        (KSEQ_ESC_BRACKET, b'B') => {
            *kseq = KSEQ_BASE;
            Some(KeyNav::PageNext)
        }
        (KSEQ_ESC_BRACKET, _) => {
            *kseq = KSEQ_BASE; // 非 A/B finals 吞（bsh editor 同纪律）
            None
        }
        _ => None, // 基态非 ESC 字节吞（无文本面）
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 逐帧喂序列，收动作流。
    fn run(kseq: &mut u8, bytes: &[u8]) -> Vec<KeyNav> {
        let mut out = Vec::new();
        for &b in bytes {
            if let Some(a) = seq_step(kseq, b) {
                out.push(a);
            }
        }
        out
    }

    /// UP/DOWN 完整序列恰一动作、第三帧才出（前两帧续态）。
    #[test]
    fn arrow_up_down_complete_sequences() {
        let mut k = KSEQ_BASE;
        assert_eq!(seq_step(&mut k, 0x1b), None);
        assert_eq!(k, KSEQ_ESC);
        assert_eq!(seq_step(&mut k, b'['), None);
        assert_eq!(k, KSEQ_ESC_BRACKET);
        assert_eq!(seq_step(&mut k, b'A'), Some(KeyNav::PagePrev));
        assert_eq!(k, KSEQ_BASE);
        assert_eq!(seq_step(&mut k, 0x1b), None);
        assert_eq!(seq_step(&mut k, b'['), None);
        assert_eq!(seq_step(&mut k, b'B'), Some(KeyNav::PageNext));
        assert_eq!(k, KSEQ_BASE);
    }

    /// 连按两次 UP：两轮序列各自成立（复位正确性）。
    #[test]
    fn consecutive_arrows_two_actions() {
        let mut k = KSEQ_BASE;
        assert_eq!(run(&mut k, b"\x1b[A\x1b[B\x1b[A"), vec![
            KeyNav::PagePrev,
            KeyNav::PageNext,
            KeyNav::PagePrev,
        ]);
    }

    /// 基态非 ESC 字节全吞（字母/数字/控制字节——无文本面）。
    #[test]
    fn base_bytes_swallowed() {
        let mut k = KSEQ_BASE;
        assert_eq!(run(&mut k, b"aZ5 \t\n\x08"), vec![]);
        assert_eq!(k, KSEQ_BASE);
    }

    /// 裸 ESC（ESC 后随非 `[` 字节）：吞复位，后续序列仍可成立。
    #[test]
    fn bare_esc_resets_and_recovers() {
        let mut k = KSEQ_BASE;
        // ESC 'x'：复位吞；随后完整 DOWN 序列照常出动作。
        assert_eq!(run(&mut k, b"\x1bx\x1b[B"), vec![KeyNav::PageNext]);
        assert_eq!(k, KSEQ_BASE);
    }

    /// 连发 ESC：前帧裸吞、后帧新序列头——`ESC ESC [ A` 仍成 UP。
    #[test]
    fn double_esc_keeps_sequence_head() {
        let mut k = KSEQ_BASE;
        assert_eq!(run(&mut k, b"\x1b\x1b[A"), vec![KeyNav::PagePrev]);
        assert_eq!(k, KSEQ_BASE);
    }

    /// 表外 final（C/D/HOME/END 族与字母）吞复位——bsh editor 同纪律。
    #[test]
    fn off_table_final_swallowed() {
        let mut k = KSEQ_BASE;
        assert_eq!(run(&mut k, b"\x1b[C\x1b[D\x1b[H"), vec![]);
        assert_eq!(k, KSEQ_BASE);
        // 复位后新序列不受污染。
        assert_eq!(run(&mut k, b"\x1b[A"), vec![KeyNav::PagePrev]);
    }

    // —— 对拍六景（输入测试补强：一景一测，逐帧态钉板，与上方六测
    // 互补——上方盖双 ESC/表外 final 族，本段盖指定序列逐字形态）——

    /// 景 1：ESC → `[` → A（UP）完整序列——三帧逐帧态推进，终帧恰出
    /// PagePrev 并复位基态。
    #[test]
    fn scene_up_complete_sequence() {
        let mut k = KSEQ_BASE;
        assert_eq!(seq_step(&mut k, 0x1b), None);
        assert_eq!(k, KSEQ_ESC);
        assert_eq!(seq_step(&mut k, b'['), None);
        assert_eq!(k, KSEQ_ESC_BRACKET);
        assert_eq!(seq_step(&mut k, b'A'), Some(KeyNav::PagePrev));
        assert_eq!(k, KSEQ_BASE);
    }

    /// 景 2：ESC → `[` → B（DOWN）完整序列——终帧恰出 PageNext 并复位。
    #[test]
    fn scene_down_complete_sequence() {
        let mut k = KSEQ_BASE;
        assert_eq!(seq_step(&mut k, 0x1b), None);
        assert_eq!(k, KSEQ_ESC);
        assert_eq!(seq_step(&mut k, b'['), None);
        assert_eq!(k, KSEQ_ESC_BRACKET);
        assert_eq!(seq_step(&mut k, b'B'), Some(KeyNav::PageNext));
        assert_eq!(k, KSEQ_BASE);
    }

    /// 景 3：连续两次 ESC → `[` → B（DOWN×2）恰出两个 PageNext——第一
    /// 序列终帧复位正确性（第二序列不被残态污染）。
    #[test]
    fn scene_down_twice_two_actions() {
        let mut k = KSEQ_BASE;
        assert_eq!(run(&mut k, b"\x1b[B\x1b[B"), vec![
            KeyNav::PageNext,
            KeyNav::PageNext,
        ]);
        assert_eq!(k, KSEQ_BASE);
    }

    /// 景 4：失序 ESC → X → A——X 表外字节吞掉并复位，随后 A 落基态
    /// 重组失败（非 ESC 开头一律吞），全程零动作。
    #[test]
    fn scene_misordered_esc_x_then_a() {
        let mut k = KSEQ_BASE;
        assert_eq!(seq_step(&mut k, 0x1b), None);
        assert_eq!(k, KSEQ_ESC);
        assert_eq!(seq_step(&mut k, b'X'), None); // 表外字节：吞 + 复位
        assert_eq!(k, KSEQ_BASE);
        assert_eq!(seq_step(&mut k, b'A'), None); // 基态 A：重组失败吞
        assert_eq!(k, KSEQ_BASE);
    }

    /// 景 5：基态直接收 A（非 ESC 开头）——吞，态不动。
    #[test]
    fn scene_base_a_swallowed() {
        let mut k = KSEQ_BASE;
        assert_eq!(seq_step(&mut k, b'A'), None);
        assert_eq!(k, KSEQ_BASE);
    }

    /// 景 6：基态收 ESC 后跟随机字节再收 `[` → A——随机字节吞复位后，
    /// `[` 与 A 均落基态吞，整流零动作（字节取样含字母/数字/零字节/
    /// 高位字节，均非 `[` 非 ESC）。
    #[test]
    fn scene_esc_random_then_bracket_a() {
        for &rnd in b"Qz7A\x00\xff\x7f" {
            let mut k = KSEQ_BASE;
            assert_eq!(run(&mut k, &[0x1b, rnd, b'[', b'A']), vec![]);
            assert_eq!(k, KSEQ_BASE, "rnd byte {rnd:#04x} 须吞后复位");
        }
    }
}
