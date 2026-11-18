//! berry-jobs —— bsh 后台作业控制纯函数面（作业控制 v2 刀，2026-11-19
//! 刀谱 .agents/plans/2026-11-19-bsh作业控制v2刀实施设计.md 契约 J）。
//! `&` 后台作业的语法层剥离/表编号/引用解析/渲染字节外迁成可 host 测试
//! 的纯函数（裁决 10——`berry-complete`/`berry-poll` 镜像形态），job 表
//! 与 syscall 面（terminate/wait/reap）全留 bsh：本 crate 零 syscall 零 IO。
//!
//! `cfg_attr(not(test), no_std)` + `extern crate alloc`：库本体 no_std
//! （workspace none-target 全量构建可编），test 构建放开 std 仅为测试
//! 挂具；std 消费侧（bsh，musl target）alloc 由 std 提供。host 测试经
//! justfile test-host 显式行跑。
//!
//! 四件原语（API 瘦身——裁决 2 砍泵缓冲账后的最小面）：
//! - [`strip_bg`]：run_line 入口整行尾缀 `&` 剥离（G1 恒等契约四边缘的
//!   单源钉板——false 路径返回原文切片零漂）；
//! - [`alloc_id`]：job 表最低空位编号（1 起，满帽 None）；
//! - [`parse_job_ref`]：`kill %n` 单形引用解析（杂形全拒——形解析归此、
//!   在册与否归调用方表查）；
//! - [`spawn_line`]/[`running_line`]/[`done_line`]/[`terminated_line`]：
//!   登记回显/列示/通报四渲染行字节单源（黄金字节钉板）。

#![cfg_attr(not(test), no_std)]
extern crate alloc;

use alloc::vec::Vec;

/// job 表容量帽（G5 资源账单源：8 job hp + 1 前台 hp 远低 per-parent 24
/// 柄；全局 PROCESS_MAX=64 常驻约 20 无压力；≤8 使将来即时通报化可单批
/// wait_many（8+stdin+out ≤ 16）前瞻对齐）。表满拒在 spawn 前——不死 shell。
pub const JOBS_MAX: u32 = 8;

/// 行尾独立单 `&` 剥离（run_line 入口整行尾缀——操作符识别纪律：展开前
/// 原始文本）。返回 (剥离后文本, 是否后台)。
///
/// 规则：trim 后行尾是独立单 `&`（前字符非 `&`）即剥、剥后去尾空白循环
/// 往复；前字符为 `&` 即停（`&&` 尾形不剥——交 split_chains 现语义）；len 1
/// 裸 `&` 不剥。全边缘律：
/// - `a &` / `a&` → (`a`, true)——行尾独立单 `&` 剥离；
/// - `a &&` → 原文 false——次尾是 `&` 停剥（残链形非后台形）；
/// - `a& &` → (`a`, true)——循环剥（剥一个后暴露的下一个独立 `&` 再剥）；
/// - `a&&&b` → 原文 false——尾非 `&` 恒不剥（split_chains「单 `&` 非边界」
///   恒等契约零漂）；
/// - `P=a&b` / `run a&b` → 原文 false——尾非 `&` 零漂（赋值/名字值域保持；
///   赋值形后台的语义拒绝归 exec_bg，本函数只管语法层剥离）；
/// - `A=1 &` → (`A=1`, true)——语法层照剥，拒绝归调用方；
/// - 裸 `&` / 纯空白 / 空串 → 原文 false。
///
/// false 路径返回**原文切片**（G1 零漂——调用方现行路径逐字节不变）；true
/// 路径返回剥离后的 trim 口径文本（登记面 cmd 取本值），且结构性恒非空
/// （循环剥至余 len 1 裸 `&` 即停——「剥后空文本」由该律连带封死，调用方
/// 防御面保留；角落形 `& &` 剥一层后余裸 `&` 停：产出 ("&", true)——语义
/// 处置归调用方（exec_bg 落 127 not found 面），本函数只管语法层）。
pub fn strip_bg(line: &str) -> (&str, bool) {
    let mut rest = line.trim();
    let mut bg = false;
    loop {
        let b = rest.as_bytes();
        // len 1 裸 `&` 不剥；前字符为 `&` 即停（`&&` 尾形交 split_chains）。
        if b.len() >= 2 && b[b.len() - 1] == b'&' && b[b.len() - 2] != b'&' {
            // `&` 是 ASCII——字节切位即字符边界，安全。
            rest = rest[..b.len() - 1].trim_end();
            bg = true;
        } else {
            break;
        }
    }
    if bg {
        (rest, true)
    } else {
        (line, false)
    }
}

/// 最低空位编号：1..=JOBS_MAX 中首个不在册的号；满帽 None（表满拒在
/// spawn 前——不死 shell）。低号优先＝出表即让位（回收复用）；$! 恒粘
/// 由调用方记最后一次值实现（裁决 9），表号本身允许复用。
pub fn alloc_id(registered: &[u32]) -> Option<u32> {
    (1..=JOBS_MAX).find(|n| !registered.contains(n))
}

/// `kill %n` 作业引用解析：单形 `%` + 十进制数字串 → 号；杂形全拒（裸 `%`、
/// `%%`、`%a`、`%1a`、无 `%` 前缀、前后缀空白、溢出数字串 → None）。
/// 纯形解析——`%0`/前导零 `%01`/超帽 `%9` 形解析成功但在册与否归调用方
/// 表查（alloc_id 产号 ≥1 且 ≤JOBS_MAX，缺席走 no-such-job 诊断）。
pub fn parse_job_ref(s: &str) -> Option<u32> {
    let digits = s.strip_prefix('%')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// 登记回显行：`[n] cmd\n`（bash 形减 pid——jid 代位，裁决 12）。
pub fn spawn_line(out: &mut Vec<u8>, n: u32, cmd: &str) {
    out.extend_from_slice(b"[");
    push_num(out, n);
    out.extend_from_slice(b"] ");
    out.extend_from_slice(cmd.as_bytes());
    out.push(b'\n');
}

/// jobs 列示行：`[n] Running cmd\n`（返数据面——`jobs > f` 重定向自动成立）。
pub fn running_line(out: &mut Vec<u8>, n: u32, cmd: &str) {
    out.extend_from_slice(b"[");
    push_num(out, n);
    out.extend_from_slice(b"] Running ");
    out.extend_from_slice(cmd.as_bytes());
    out.push(b'\n');
}

/// 退出通报行：`[n]+ Done(code) cmd\n`（`+` ＝当前作业标记，bash 形；
/// code＝reap 真码首因保真——已退未收走同收割不覆写）。
pub fn done_line(out: &mut Vec<u8>, n: u32, code: i32, cmd: &str) {
    out.extend_from_slice(b"[");
    push_num(out, n);
    out.extend_from_slice(b"]+ Done(");
    push_num(out, code);
    out.extend_from_slice(b") ");
    out.extend_from_slice(cmd.as_bytes());
    out.push(b'\n');
}

/// kill 通报行：`[n]+ Terminated(code) cmd\n`（code＝terminate 传入码即
/// reap 码——143 惯例（128+15）与 Ctrl-C 的 130 先例同构异值）。
pub fn terminated_line(out: &mut Vec<u8>, n: u32, code: i32, cmd: &str) {
    out.extend_from_slice(b"[");
    push_num(out, n);
    out.extend_from_slice(b"]+ Terminated(");
    push_num(out, code);
    out.extend_from_slice(b") ");
    out.extend_from_slice(cmd.as_bytes());
    out.push(b'\n');
}

/// 数字入列（`to_string` 走 fmt 的 alloc 面——消费宿主 bsh 全量用
/// `format!`，fmt 机器随身无增量体积账）。
fn push_num<T: core::fmt::Display>(out: &mut Vec<u8>, v: T) {
    use alloc::string::ToString;
    out.extend_from_slice(v.to_string().as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    // —— strip_bg：独立尾 & 剥离正形 ——
    #[test]
    fn strip_basic_amp_tail() {
        assert_eq!(strip_bg("a &"), ("a", true));
        assert_eq!(strip_bg("a&"), ("a", true));
        assert_eq!(strip_bg("a && b &"), ("a && b", true));
        assert_eq!(strip_bg("A=1 &"), ("A=1", true));
        assert_eq!(strip_bg("run hello &"), ("run hello", true));
        // 检测基座＝trim 后文本：首尾空白不碍剥离，true 路径回 trim 口径。
        assert_eq!(strip_bg("  run hello &  "), ("run hello", true));
        assert_eq!(strip_bg("a\t&"), ("a", true));
    }

    // —— strip_bg：循环剥（`a& &` 形——剥一个后再剥暴露的下一个）——
    #[test]
    fn strip_loop_multiple_tails() {
        assert_eq!(strip_bg("a& &"), ("a", true));
        // 审查 note 钉板：剥后余裸 & 的角落形（循环剥至 len 1 即停）。
        assert_eq!(strip_bg("& &"), ("&", true));
        assert_eq!(strip_bg("a & &"), ("a", true));
        assert_eq!(strip_bg("run hello& &"), ("run hello", true));
    }

    // —— strip_bg：`&&` 尾停剥（残链形交 split_chains 现语义）——
    #[test]
    fn strip_double_amp_tail_stops() {
        assert_eq!(strip_bg("a &&"), ("a &&", false));
        assert_eq!(strip_bg("run hello &&"), ("run hello &&", false));
        assert_eq!(strip_bg("a && "), ("a && ", false));
        assert_eq!(strip_bg("&&"), ("&&", false));
    }

    // —— strip_bg：尾非 & 恒不剥（G1 四边缘之 `a&&&b`/`P=a&b`/`run a&b`）——
    #[test]
    fn strip_never_when_tail_not_amp() {
        assert_eq!(strip_bg("a&&&b"), ("a&&&b", false));
        assert_eq!(strip_bg("P=a&b"), ("P=a&b", false));
        assert_eq!(strip_bg("run a&b"), ("run a&b", false));
        assert_eq!(strip_bg("a & b"), ("a & b", false));
        assert_eq!(strip_bg("hello"), ("hello", false));
        assert_eq!(strip_bg(""), ("", false));
    }

    // —— strip_bg：裸 `&`（len 1）与纯空白不剥 ——
    #[test]
    fn strip_bare_amp_not_stripped() {
        assert_eq!(strip_bg("&"), ("&", false));
        assert_eq!(strip_bg("  &  "), ("  &  ", false));
        assert_eq!(strip_bg("   "), ("   ", false));
    }

    // —— strip_bg：false 路径返回原文切片（G1 零漂——指针同一性钉板）——
    #[test]
    fn strip_false_path_returns_original_slice() {
        for line in ["P=a&b", "a &&", "a&&&b", "&", "a & b", ""] {
            let (rest, bg) = strip_bg(line);
            assert!(!bg);
            assert!(std::ptr::eq(rest, line));
        }
    }

    // —— alloc_id：空表起步 1 ——
    #[test]
    fn alloc_starts_at_one_on_empty() {
        assert_eq!(alloc_id(&[]), Some(1));
    }

    // —— alloc_id：最低空位 ——
    #[test]
    fn alloc_picks_lowest_free_slot() {
        assert_eq!(alloc_id(&[1, 2, 4]), Some(3));
        assert_eq!(alloc_id(&[2, 3]), Some(1));
        assert_eq!(alloc_id(&[1]), Some(2));
    }

    // —— alloc_id：回收复用（低号让位）与满帽 None ——
    #[test]
    fn alloc_reuses_freed_lowest_and_caps() {
        let full: Vec<u32> = (1..=JOBS_MAX).collect();
        assert_eq!(alloc_id(&full), None);
        let after: Vec<u32> = full.iter().copied().filter(|n| *n != 3).collect();
        assert_eq!(alloc_id(&after), Some(3));
    }

    // —— parse_job_ref：正形（形解析不管帽/在册——归调用方表查）——
    #[test]
    fn parse_positive_forms() {
        assert_eq!(parse_job_ref("%1"), Some(1));
        assert_eq!(parse_job_ref("%8"), Some(8));
        assert_eq!(parse_job_ref("%9"), Some(9));
        assert_eq!(parse_job_ref("%01"), Some(1));
        assert_eq!(parse_job_ref("%0"), Some(0));
    }

    // —— parse_job_ref：杂形全拒 ——
    #[test]
    fn parse_rejects_misc_forms() {
        assert_eq!(parse_job_ref("%"), None);
        assert_eq!(parse_job_ref("%%"), None);
        assert_eq!(parse_job_ref("%a"), None);
        assert_eq!(parse_job_ref("%1a"), None);
        assert_eq!(parse_job_ref("1"), None);
        assert_eq!(parse_job_ref("% 1"), None);
        assert_eq!(parse_job_ref(" %1"), None);
        assert_eq!(parse_job_ref("%1 "), None);
        assert_eq!(parse_job_ref(""), None);
        assert_eq!(parse_job_ref("%4294967296"), None);
    }

    // —— 渲染黄金字节：登记回显/列示 ——
    #[test]
    fn render_spawn_and_running_golden() {
        let mut out = Vec::new();
        spawn_line(&mut out, 1, "run hello");
        assert_eq!(out, b"[1] run hello\n".to_vec());
        let mut out = Vec::new();
        running_line(&mut out, 12, "run hello");
        assert_eq!(out, b"[12] Running run hello\n".to_vec());
    }

    // —— 渲染黄金字节：Done/Terminated 通报（`+` 标记 + 真码保真）——
    #[test]
    fn render_done_and_terminated_golden() {
        let mut out = Vec::new();
        done_line(&mut out, 3, 0, "run hello");
        assert_eq!(out, b"[3]+ Done(0) run hello\n".to_vec());
        let mut out = Vec::new();
        terminated_line(&mut out, 1, 143, "run hello");
        assert_eq!(out, b"[1]+ Terminated(143) run hello\n".to_vec());
    }

    // —— B4 消费形态：jobs 列示数据＝running_line 逐行拼接 ——
    #[test]
    fn render_jobs_listing_composition() {
        let mut data = Vec::new();
        running_line(&mut data, 1, "run hello");
        running_line(&mut data, 2, "run talker");
        assert_eq!(
            data,
            b"[1] Running run hello\n[2] Running run talker\n".to_vec()
        );
    }
}
