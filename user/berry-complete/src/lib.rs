//! berry-complete —— bsh Tab 补全纯函数面（C-4，shell设计 §2.4「Tab 补全
//! defer v2」留档的兑现——编辑会话归属随本刀下沉 bsh，补全候选的切分/
//! 匹配/分类/列示抽成可 host 测试的纯函数，编辑器状态机只管喂键）。
//!
//! 字节域（&[u8]）不进 str：编辑器行缓冲本就是字节（内核时代同款），
//! str 化只在行定稿执行时发生；补全面对任意字节宽容（非 UTF-8 序列
//! 最多少补，不炸）。
//!
//! 三个原语：
//! - [`token_at_eol`]：行尾 token 切分（命令位 vs 参数位——补全域的判别）；
//! - [`complete_from`]：前缀匹配 → 唯一（补后缀）/ 多候选（公共增量 +
//!   排序去重候选全集）/ 零命中；
//! - [`render_matches`]：多候选列示字节格式化。
//!
//! 语义注记：
//! - 多候选的 `common` = 全候选公共前缀**超出已键前缀**的增量——编辑器
//!   先补公共增量再列示（readline 惯例：`ec<Tab>` 双候选 ech/echo 先补
//!   `h` 再列）；
//! - 候选全集**排序 + 相邻去重**后返回（列示确定性 + 命令位「内建 ∪
//!   rootfs bin/」双源同名去重——调用方不必预归并）；
//! - `add_space` 是调用方策略（命令位补全后追加空格、路径位不加），
//!   本 crate 只透传不解释。

#![cfg_attr(not(test), no_std)]
extern crate alloc;

use alloc::vec::Vec;

/// 行尾 token 切分结果。
#[derive(Debug, PartialEq, Eq)]
pub enum Token<'a> {
    /// 命令位（token 之前全空白——含空行/纯空白行：Tab 从头补命令名）。
    Command(&'a [u8]),
    /// 参数位（前面已有词——Tab 补路径/文件名；行尾空白 = 下一参空键）。
    Arg(&'a [u8]),
}

/// 行尾 token 切分：从行尾起找最后一个词。
///
/// - 行尾无空白：token = 行尾词（`cat OUT.` → `OUT.`）；
/// - 行尾有空白：token = **空**（`cat ` → 空——Tab 补下一个尚未开键的词；
///   纯空白行 → Command(空)，命令位从头补）；
/// - token 之前全空白 → Command，否则 Arg。
pub fn token_at_eol(line: &[u8]) -> Token<'_> {
    let is_ws = |b: u8| b == b' ' || b == b'\t';
    let end = line.len();
    // 跳过行尾空白（若有），找到最后一个词的词尾。
    let mut s = end;
    while s > 0 && is_ws(line[s - 1]) {
        s -= 1;
    }
    let word_end = s;
    // 词尾往左到词首。
    while s > 0 && !is_ws(line[s - 1]) {
        s -= 1;
    }
    // 行尾空白 = 下一词空键（token 起点 = 行尾）；否则 token = 行尾词。
    let (tok, tok_start) = if word_end < end {
        (&line[end..], end)
    } else {
        (&line[s..word_end], s)
    };
    if line[..tok_start].iter().all(|&b| is_ws(b)) {
        Token::Command(tok)
    } else {
        Token::Arg(tok)
    }
}

/// 补全结果（纯函数产物——编辑器消费）。
#[derive(Debug, PartialEq, Eq)]
pub enum Completion {
    /// 唯一命中：`suffix` = 需补入行尾的字节（候选减已键前缀，可为空——
    /// 恰完整键入时只补 `add_space` 的尾空格）；`add_space` 调用方策略透传。
    Unique {
        suffix: Vec<u8>,
        add_space: bool,
    },
    /// 多命中：`common` = 公共前缀超出已键部分的增量（可为空）；`matches`
    /// = 排序 + 去重后的候选全集（列示面）。
    Ambiguous {
        common: Vec<u8>,
        matches: Vec<Vec<u8>>,
    },
    /// 零命中：无操作。
    None,
}

/// 前缀匹配 + 结果分类。candidates 顺序任意、可含重复（内部排序去重）。
/// prefix 空 → 全命中（Tab 在空 token 上列全域）。
pub fn complete_from(prefix: &[u8], candidates: &[&[u8]], add_space: bool) -> Completion {
    let mut hits: Vec<&[u8]> = candidates
        .iter()
        .copied()
        .filter(|c| c.starts_with(prefix))
        .collect();
    if hits.is_empty() {
        return Completion::None;
    }
    hits.sort_unstable(); // 列示确定性（字节序）
    hits.dedup(); // 双源同名（内建 ∪ bin/ 的 echo 族）
    if hits.len() == 1 {
        let c = hits[0];
        // 前缀过滤保证 c.len() >= prefix.len()。
        return Completion::Unique {
            suffix: c[prefix.len()..].to_vec(),
            add_space,
        };
    }
    // 全候选公共前缀（都含 prefix，lcp 长 ≥ prefix 长）。
    let mut lcp = prefix.len();
    'outer: while lcp < hits[0].len() {
        let b = hits[0][lcp];
        for c in &hits[1..] {
            if lcp >= c.len() || c[lcp] != b {
                break 'outer;
            }
        }
        lcp += 1;
    }
    Completion::Ambiguous {
        common: hits[0][prefix.len()..lcp].to_vec(),
        matches: hits.iter().map(|c| c.to_vec()).collect(),
    }
}

/// 多候选列示格式化：`\n` + 候选空格分隔 + `\n`（提示符/当前行重显由
/// 调用方接续——编辑器持有 prompt 常量）。
pub fn render_matches(out: &mut Vec<u8>, matches: &[Vec<u8>]) {
    out.push(b'\n');
    for (i, m) in matches.iter().enumerate() {
        if i > 0 {
            out.push(b' ');
        }
        out.extend_from_slice(m);
    }
    out.push(b'\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    // —— token_at_eol：四形 + 边界 ——
    #[test]
    fn token_empty_line_is_command() {
        assert_eq!(token_at_eol(b""), Token::Command(b""));
        assert_eq!(token_at_eol(b"   "), Token::Command(b""));
    }

    #[test]
    fn token_first_word_is_command() {
        assert_eq!(token_at_eol(b"ec"), Token::Command(b"ec"));
        assert_eq!(token_at_eol(b"  ec"), Token::Command(b"ec"));
    }

    #[test]
    fn token_later_word_is_arg() {
        assert_eq!(token_at_eol(b"cat OUT."), Token::Arg(b"OUT."));
        assert_eq!(token_at_eol(b"cat /etc/m"), Token::Arg(b"/etc/m"));
    }

    #[test]
    fn token_trailing_ws_is_empty_arg() {
        // 行尾空白 = 下一词空键：`cat ` 补参（空），不回补 `cat` 本身。
        assert_eq!(token_at_eol(b"cat "), Token::Arg(b""));
        assert_eq!(token_at_eol(b"cat  x "), Token::Arg(b""));
    }

    // —— complete_from：唯一/多候选/零命中 ——
    #[test]
    fn unique_match_gives_suffix() {
        assert_eq!(
            complete_from(b"ec", &[b"echo", b"env"], true),
            Completion::Unique {
                suffix: b"ho".to_vec(),
                add_space: true,
            }
        );
        // 路径位 add_space=false 透传。
        assert_eq!(
            complete_from(b"OUT.T", &[b"OUT.TXT"], false),
            Completion::Unique {
                suffix: b"XT".to_vec(),
                add_space: false,
            }
        );
    }

    #[test]
    fn unique_exact_prefix_empty_suffix() {
        // 恰完整键入：后缀空（编辑器只补尾空格）。
        assert_eq!(
            complete_from(b"echo", &[b"echo"], true),
            Completion::Unique {
                suffix: Vec::new(),
                add_space: true,
            }
        );
    }

    #[test]
    fn empty_prefix_matches_all() {
        assert_eq!(
            complete_from(b"", &[b"echo", b"env"], true),
            Completion::Ambiguous {
                common: b"e".to_vec(), // lcp("echo","env") = "e"，prefix 空 → 增量即 "e"
                matches: vec![b"echo".to_vec(), b"env".to_vec()],
            }
        );
    }

    #[test]
    fn ambiguous_common_and_sorted_dedup() {
        // 乱序入 + 双源同名（env ×2）→ 排序去重出；公共增量 = lcp 超
        // prefix 部分（ech/echo → "h"）。
        assert_eq!(
            complete_from(b"ec", &[b"env", b"echo", b"ech", b"env"], false),
            Completion::Ambiguous {
                common: b"h".to_vec(),
                matches: vec![b"ech".to_vec(), b"echo".to_vec()],
            }
        );
    }

    #[test]
    fn no_match_is_none() {
        assert_eq!(complete_from(b"zz", &[b"echo", b"env"], true), Completion::None);
        assert_eq!(complete_from(b"x", &[], true), Completion::None);
    }

    // —— render_matches 字节格式 ——
    #[test]
    fn render_matches_bytes() {
        let mut out = Vec::new();
        render_matches(
            &mut out,
            &[b"ech".to_vec(), b"echo".to_vec()],
        );
        assert_eq!(out, b"\nech echo\n".to_vec());
    }
}
