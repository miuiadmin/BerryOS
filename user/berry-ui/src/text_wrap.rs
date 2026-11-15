//! LAYOUT-L5 固定容量文本换行计划。
//!
//! 输入仍是 `TextBuf` 的 64B 有效字节；输出只存行起点 byte offset，
//! 不复制文本、不分配堆。断行策略 v1 固定为贪心：
//!
//! ```text
//! U+0020 后可断
//! CJK 区间码点后可断
//! U+000A 显式换行：行终于 `\n` 前（`\n` 不计宽、不进行切片、不出字形）
//! 无合法断点且下一码点溢出行宽时紧急断前一个可容纳码点
//! 首码点自身超宽时独立成行（不丢字）
//! 空文本零行
//! ```
//!
//! 行宽全部经 `TextMetrics::measure` 供给缝计算，测量与绘制共用同一
//! [`plan`]，避免第二断行真相源。

use crate::metrics::TextMetrics;
use crate::utf8;

/// 文本行起点容量：64B 文本最坏每字节一行，加终哨共 65。
const START_CAP: usize = 65;

/// 换行计划：`starts[..lines]` 是每行在原文本中的起始 byte offset；
/// 最后一行延伸到文本末尾。
#[derive(Clone, Copy)]
pub struct WrapPlan {
    starts: [u8; START_CAP],
    ends: [u8; START_CAP],
    lines: usize,
}

impl WrapPlan {
    /// 空计划（零行/零尺寸）。
    pub const fn empty() -> Self {
        Self { starts: [0; START_CAP], ends: [0; START_CAP], lines: 0 }
    }

    /// 第 `line` 行起始 byte offset；越界返回 0。
    pub const fn start(&self, line: usize) -> usize {
        if line < self.lines {
            self.starts[line] as usize
        } else {
            0
        }
    }

    /// 行数。
    pub const fn len(&self) -> usize {
        self.lines
    }

    /// 是否零行。
    pub const fn is_empty(&self) -> bool {
        self.lines == 0
    }

    /// 第 `line` 行的可视终点 byte offset；越界返回 0。
    pub const fn end(&self, line: usize) -> usize {
        if line < self.lines {
            self.ends[line] as usize
        } else {
            0
        }
    }

    /// 第 `line` 行的原文切片；越界返回空切片。终点不含断行/行尾空格。
    pub fn line<'a>(&self, line: usize, text: &'a [u8]) -> &'a [u8] {
        if line >= self.lines {
            return &[];
        }
        let start = usize::from(self.starts[line]);
        let end = self.end(line).max(start);
        text.get(start..end).unwrap_or(&[])
    }
}

fn breakable(cp: u32) -> bool {
    cp == 0x20 || (0x2E80..=0x9FFF).contains(&cp) || (0xF900..=0xFAFF).contains(&cp)
}

/// 用供给缝生成贪心换行计划。`max_w` 是 incoming BoxConstraints 的宽上界；
/// 零/极小宽度下首码点独立成行，保证不丢字。
pub fn plan(text: &[u8], scale: u8, max_w: u32, tm: &dyn TextMetrics) -> WrapPlan {
    let mut out = WrapPlan::empty();
    if text.is_empty() {
        return out;
    }
    let mut start = 0usize;
    while start < text.len() {
        out.starts[out.lines] = start as u8;
        out.lines += 1;
        let line_index = out.lines - 1;
        if out.lines == START_CAP {
            // 理论不可达（64B 文本 + 终哨 = 65）；防御性停止，保留已可
            // 容纳的计划，不越界、不 panic。
            break;
        }

        let mut fit_end = start;
        let mut soft_break = None;
        let mut cursor = start;
        let mut overflow = false;
        while cursor < text.len() {
            let (cp, step) = utf8::next(&text[cursor..]).unwrap_or((utf8::REPLACEMENT, 1));
            let end = cursor.saturating_add(step.max(1)).min(text.len());
            if cp == 0x0A {
                // Hard newline closes this line before measuring it. Spaces
                // before `\n` are trailing spaces; spaces after it are kept as
                // intentional indentation on the next line.
                let mut line_end = cursor;
                while line_end > start && text[line_end - 1] == b' ' {
                    line_end -= 1;
                }
                out.ends[line_index] = line_end as u8;
                start = end;
                overflow = true;
                break;
            }
            let width = tm.measure(&text[start..end], scale).0;
            if width <= max_w {
                fit_end = end;
                if breakable(cp) {
                    soft_break = Some(end);
                }
                cursor = end;
            } else {
                overflow = true;
                let chosen = soft_break.filter(|&b| b > start).unwrap_or(fit_end);
                let mut line_end = if chosen > start { chosen } else { end };
                while line_end > start && text[line_end - 1] == b' ' {
                    line_end -= 1;
                }
                out.ends[line_index] = line_end as u8;
                start = line_end;
                // 断点后的 ASCII 空格不带入下一行；它不计入下一行可视宽。
                while start < text.len() && text[start] == b' ' {
                    start += 1;
                }
                break;
            }
        }
        if !overflow {
            let mut line_end = text.len();
            while line_end > start && text[line_end - 1] == b' ' {
                line_end -= 1;
            }
            out.ends[line_index] = line_end as u8;
            start = text.len();
        }
    }
    out
}

/// 按计划测量：宽 = 最大行宽，高 = 行数 × 单行高。空文本为 (0,0)。
pub fn measure(
    text: &[u8],
    scale: u8,
    planned: &WrapPlan,
    tm: &dyn TextMetrics,
) -> (u32, u32) {
    if planned.is_empty() {
        return (0, 0);
    }
    let mut w = 0u32;
    let mut h = 0u32;
    for line in 0..planned.len() {
        let (lw, lh) = tm.measure(planned.line(line, text), scale);
        w = w.max(lw);
        h = h.saturating_add(lh);
    }
    (w, h)
}
