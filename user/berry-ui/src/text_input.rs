//! GUI-T1：IME 之前的文本输入基础（Plain Data 状态机）。
//!
//! 本模块不引入软键盘、IME Host、FormMetrics、Route/Back、富文本、剪贴板
//! 或 autofill。它只提供：
//!
//! - UTF-8 值与 codepoint selection/caret；
//! - Linux keycode 最小硬件编辑映射；
//! - change / selection change / submit / reject 的布尔事件面；
//! - P0 IME purpose 的 add-only 映射；
//! - R2 `insets::Ime` 消费用的键盘遮挡重叠纯函数。
//!
//! 状态由 app 持有；BerryUI 不注册回调、不自动请求软键盘、不改窗口焦点。
//! 焦点在 TextField 时，Enter/Space 先由本状态机消费；Tab/Shift+Tab 仍返回
//! C1 focus navigation，不由文本编辑吞掉。

use crate::desc::TextBuf;
use crate::utf8::{self, next};
use berry_guiproto::surface::{
    IME_PURPOSE_MULTILINE, IME_PURPOSE_NUMBER, IME_PURPOSE_PASSWORD, IME_PURPOSE_TEXT,
};
use berry_guiproto::{
    KEY_ACTION_DOWN, KEY_ACTION_REPEAT, KEY_ACTION_UP, KEY_MOD_ALT, KEY_MOD_CTRL,
    KEY_MOD_SHIFT, KEY_MOD_SUPER,
};

/// 文本值最大字节数（与 [`TextBuf`] / P0 `IME_TEXT_MAX` 同帽）。
pub const TEXT_INPUT_MAX: usize = crate::TEXT_INLINE;

/// P0 composition purpose 的 app 侧输入档。
///
/// v1 只开放单行 Text/Number/Password。`Multiline` 是 P0 purpose 的保留
/// 值，不作为 TextField 构造档；TextArea 属后续独立规格。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputKind {
    Text,
    Number,
    Password,
}

impl TextInputKind {
    /// P0 IME composition `purpose` u8 单源映射（不另造第二 wire 词表）。
    pub fn ime_purpose(self) -> u8 {
        match self {
            Self::Text => IME_PURPOSE_TEXT,
            Self::Number => IME_PURPOSE_NUMBER,
            Self::Password => IME_PURPOSE_PASSWORD,
        }
    }

    /// P0 保留 multiline purpose 的只读 accessor；v1 无 TextField 消费者。
    pub const IME_PURPOSE_MULTILINE: u8 = IME_PURPOSE_MULTILINE;
}

/// selection：anchor 与 caret 都是 codepoint offset（与 P0 `cursor_cp` 同口径）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSelection {
    pub anchor_cp: u16,
    pub caret_cp: u16,
}

impl TextSelection {
    pub const fn new(caret_cp: u16) -> Self {
        Self { anchor_cp: caret_cp, caret_cp }
    }

    pub fn is_caret(&self) -> bool {
        self.anchor_cp == self.caret_cp
    }

    pub fn ordered(self) -> (u16, u16) {
        if self.anchor_cp <= self.caret_cp {
            (self.anchor_cp, self.caret_cp)
        } else {
            (self.caret_cp, self.anchor_cp)
        }
    }
}

/// 一次编辑/提交操作的事件结果。布尔面足够 app 更新状态；不引入回调。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextInputOutcome {
    pub changed: bool,
    pub selection_changed: bool,
    pub submitted: bool,
    pub rejected: bool,
}

impl TextInputOutcome {
    fn selection_only() -> Self {
        Self { selection_changed: true, ..Self::default() }
    }

    fn rejected() -> Self {
        Self { rejected: true, ..Self::default() }
    }
}

/// 硬件编辑命令（IME composition 未来 commit 也收敛到同一编辑面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputEdit {
    Insert(char),
    Backspace,
    Delete,
    Left { extend: bool },
    Right { extend: bool },
    Home { extend: bool },
    End { extend: bool },
    SelectAll,
}

/// TextField 获得组件焦点时的键意图。Tab 永远交还 C1 focus navigation。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputKeyOp {
    Edit(TextInputEdit),
    Submit,
    FocusNext,
    FocusPrev,
}

/// 文本输入状态。非法 UTF-8 在构造/写入侧拒绝；渲染层仍可显示既有
/// replacement 语义，但本状态不把非法字节带进未来 IME composition。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextInputState {
    value: TextBuf,
    selection: TextSelection,
}

impl TextInputState {
    pub fn new(bytes: &[u8]) -> Result<Self, crate::UiError> {
        if !is_valid_utf8(bytes) {
            return Err(crate::UiError::NodeBad);
        }
        let value = TextBuf::new(bytes)?;
        let caret = utf8::count(value.as_bytes()) as u16;
        Ok(Self { value, selection: TextSelection::new(caret) })
    }

    pub fn value(&self) -> &TextBuf {
        &self.value
    }

    pub fn selection(&self) -> TextSelection {
        self.selection
    }

    pub fn caret_cp(&self) -> u16 {
        self.selection.caret_cp
    }

    /// P0 composition `cursor_cp` 当前值（codepoint offset）。
    pub fn ime_cursor_cp(&self) -> u16 {
        self.caret_cp()
    }

    pub fn len_cp(&self) -> u16 {
        utf8::count(self.value.as_bytes()) as u16
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    pub fn set_value(&mut self, bytes: &[u8]) -> Result<TextInputOutcome, crate::UiError> {
        let old = *self;
        let value = TextBuf::new(bytes)?;
        if !is_valid_utf8(value.as_bytes()) {
            return Err(crate::UiError::NodeBad);
        }
        let caret = utf8::count(value.as_bytes()) as u16;
        self.value = value;
        self.selection = TextSelection::new(caret);
        Ok(TextInputOutcome {
            changed: old.value != self.value,
            selection_changed: old.selection != self.selection,
            ..TextInputOutcome::default()
        })
    }

    pub fn select_all(&mut self) -> TextInputOutcome {
        let old = self.selection;
        self.selection = TextSelection { anchor_cp: 0, caret_cp: self.len_cp() };
        if old == self.selection {
            TextInputOutcome::default()
        } else {
            TextInputOutcome::selection_only()
        }
    }

    pub fn submit(&self) -> TextInputOutcome {
        TextInputOutcome { submitted: true, ..TextInputOutcome::default() }
    }

    pub fn apply_edit(&mut self, edit: TextInputEdit) -> TextInputOutcome {
        match edit {
            TextInputEdit::Insert(ch) => self.insert_char(ch),
            TextInputEdit::Backspace => self.delete_backward(),
            TextInputEdit::Delete => self.delete_forward(),
            TextInputEdit::Left { extend } => self.move_caret_signed(-1, extend),
            TextInputEdit::Right { extend } => self.move_caret_signed(1, extend),
            TextInputEdit::Home { extend } => {
                let old = self.selection;
                self.selection.caret_cp = 0;
                if !extend {
                    self.selection.anchor_cp = 0;
                }
                Self::selection_outcome(old, self.selection)
            }
            TextInputEdit::End { extend } => {
                let old = self.selection;
                let end = self.len_cp();
                self.selection.caret_cp = end;
                if !extend {
                    self.selection.anchor_cp = end;
                }
                Self::selection_outcome(old, self.selection)
            }
            TextInputEdit::SelectAll => self.select_all(),
        }
    }

    /// TextField 显示值：Text/Number 原样；Password 每个码点显示一个
    /// `*`（replacement 语义与 utf8 模块一致）。真实值仍归 app 状态。
    pub fn display_value(&self, kind: TextInputKind) -> Result<TextBuf, crate::UiError> {
        if kind != TextInputKind::Password {
            return Ok(self.value);
        }
        let bytes = self.value.as_bytes();
        let mut out = [0u8; TEXT_INPUT_MAX];
        let mut n = 0usize;
        let mut i = 0usize;
        while i < bytes.len() && n < TEXT_INPUT_MAX {
            let step = next(&bytes[i..]).map(|(_, n)| n).unwrap_or(1).max(1);
            out[n] = b'*';
            n += 1;
            i += step;
        }
        TextBuf::new(&out[..n])
    }

    /// 把 caret 前后的视觉字节拆成两段（调试/测试辅助；生产 TextField 使用
    /// [`Self::display_value`] + caret offset 组合）。
    pub fn split_visual(&self, kind: TextInputKind) -> Result<(TextBuf, TextBuf), crate::UiError> {
        let bytes = self.value.as_bytes();
        let caret = self.caret_cp() as usize;
        let (mut prefix, mut suffix) = ([0u8; TEXT_INPUT_MAX], [0u8; TEXT_INPUT_MAX]);
        let (mut pi, mut si, mut cp) = (0usize, 0usize, 0usize);
        let mut i = 0usize;
        while i < bytes.len() {
            let step = next(&bytes[i..]).map(|(_, n)| n).unwrap_or(1).max(1);
            let src = &bytes[i..i + step];
            let dst = if cp < caret { &mut prefix } else { &mut suffix };
            let di = if cp < caret { &mut pi } else { &mut si };
            if kind == TextInputKind::Password {
                if *di < TEXT_INPUT_MAX {
                    dst[*di] = b'*';
                    *di += 1;
                }
            } else if *di + src.len() <= TEXT_INPUT_MAX {
                dst[*di..*di + src.len()].copy_from_slice(src);
                *di += src.len();
            }
            i += step;
            cp += 1;
        }
        Ok((TextBuf::new(&prefix[..pi])?, TextBuf::new(&suffix[..si])?))
    }

    fn insert_char(&mut self, ch: char) -> TextInputOutcome {
        let mut buf = [0u8; 4];
        let bytes = ch.encode_utf8(&mut buf).as_bytes();
        let (start, end, start_cp) = self.selection_bytes();
        let tail_len = self.value.len as usize - end;
        let new_len = start + bytes.len() + tail_len;
        if new_len > TEXT_INPUT_MAX {
            return TextInputOutcome::rejected();
        }
        let mut tail = [0u8; TEXT_INPUT_MAX];
        tail[..tail_len].copy_from_slice(&self.value.as_bytes()[end..]);
        self.value.bytes[start..start + bytes.len()].copy_from_slice(bytes);
        self.value.bytes[start + bytes.len()..new_len].copy_from_slice(&tail[..tail_len]);
        self.value.len = new_len as u8;
        let caret = start_cp + utf8::count(bytes) as u16;
        self.selection = TextSelection::new(caret);
        TextInputOutcome { changed: true, selection_changed: true, ..TextInputOutcome::default() }
    }

    fn delete_selection(&mut self) -> Option<TextInputOutcome> {
        let (start, end, start_cp) = self.selection_bytes();
        if start == end {
            return None;
        }
        let tail_len = self.value.len as usize - end;
        let mut tail = [0u8; TEXT_INPUT_MAX];
        tail[..tail_len].copy_from_slice(&self.value.as_bytes()[end..]);
        self.value.bytes[start..start + tail_len].copy_from_slice(&tail[..tail_len]);
        self.value.len = (start + tail_len) as u8;
        self.selection = TextSelection::new(start_cp);
        Some(TextInputOutcome { changed: true, selection_changed: true, ..TextInputOutcome::default() })
    }

    fn delete_backward(&mut self) -> TextInputOutcome {
        if self.selection().is_caret() {
            if self.caret_cp() == 0 {
                return TextInputOutcome::rejected();
            }
            self.selection.anchor_cp = self.caret_cp() - 1;
            return self.delete_selection().unwrap_or_else(TextInputOutcome::rejected);
        }
        self.delete_selection().unwrap_or_else(TextInputOutcome::rejected)
    }

    fn delete_forward(&mut self) -> TextInputOutcome {
        if self.selection().is_caret() {
            if self.caret_cp() >= self.len_cp() {
                return TextInputOutcome::rejected();
            }
            self.selection.anchor_cp = self.caret_cp() + 1;
            return self.delete_selection().unwrap_or_else(TextInputOutcome::rejected);
        }
        self.delete_selection().unwrap_or_else(TextInputOutcome::rejected)
    }

    fn move_caret_signed(&mut self, delta: i8, extend: bool) -> TextInputOutcome {
        let old = self.selection;
        let target = if delta < 0 {
            self.caret_cp().saturating_sub(delta.unsigned_abs() as u16)
        } else {
            self.caret_cp().saturating_add(delta as u16).min(self.len_cp())
        };
        self.selection.caret_cp = target;
        if !extend {
            self.selection.anchor_cp = target;
        }
        Self::selection_outcome(old, self.selection)
    }

    fn selection_outcome(old: TextSelection, new: TextSelection) -> TextInputOutcome {
        if old == new {
            TextInputOutcome::default()
        } else {
            TextInputOutcome::selection_only()
        }
    }

    fn selection_bytes(&self) -> (usize, usize, u16) {
        let (start_cp, end_cp) = self.selection.ordered();
        let bytes = self.value.as_bytes();
        let mut offsets = [0usize; TEXT_INPUT_MAX + 1];
        let mut cp = 0usize;
        let mut i = 0usize;
        while i < bytes.len() && cp < offsets.len() {
            offsets[cp] = i;
            let step = next(&bytes[i..]).map(|(_, n)| n).unwrap_or(1).max(1);
            i += step;
            cp += 1;
        }
        offsets[cp.min(TEXT_INPUT_MAX)] = bytes.len();
        let s = start_cp as usize;
        let e = end_cp as usize;
        (offsets[s], offsets[e], start_cp)
    }
}

/// Linux keycode → TextField 键意图。UP 恒 None；Tab 仅 DOWN 且交还 C1；
/// Ctrl/Alt/Super 的非 Tab 组合保留给系统/应用快捷键。
pub fn text_key_intent(
    code: u8,
    action: u8,
    modifiers: u8,
    kind: TextInputKind,
) -> Option<TextInputKeyOp> {
    if action == KEY_ACTION_UP {
        return None;
    }
    if code == 15 && action == KEY_ACTION_DOWN {
        return Some(if modifiers & KEY_MOD_SHIFT != 0 {
            TextInputKeyOp::FocusPrev
        } else {
            TextInputKeyOp::FocusNext
        });
    }
    if modifiers & (KEY_MOD_CTRL | KEY_MOD_ALT | KEY_MOD_SUPER) != 0 {
        return None;
    }
    if code == 28 && (action == KEY_ACTION_DOWN || action == KEY_ACTION_REPEAT) {
        return Some(TextInputKeyOp::Submit);
    }
    let extend = modifiers & KEY_MOD_SHIFT != 0;
    let edit = match code {
        14 if action == KEY_ACTION_DOWN || action == KEY_ACTION_REPEAT => TextInputEdit::Backspace,
        111 if action == KEY_ACTION_DOWN || action == KEY_ACTION_REPEAT => TextInputEdit::Delete,
        105 if action == KEY_ACTION_DOWN || action == KEY_ACTION_REPEAT => {
            TextInputEdit::Left { extend }
        }
        106 if action == KEY_ACTION_DOWN || action == KEY_ACTION_REPEAT => {
            TextInputEdit::Right { extend }
        }
        102 if action == KEY_ACTION_DOWN => TextInputEdit::Home { extend },
        107 if action == KEY_ACTION_DOWN => TextInputEdit::End { extend },
        _ if action == KEY_ACTION_DOWN || action == KEY_ACTION_REPEAT => {
            let ch = printable_for_key(code, extend)?;
            if !char_allowed(kind, ch) {
                return None;
            }
            TextInputEdit::Insert(ch)
        }
        _ => return None,
    };
    Some(TextInputKeyOp::Edit(edit))
}

fn char_allowed(kind: TextInputKind, ch: char) -> bool {
    match kind {
        TextInputKind::Text | TextInputKind::Password => true,
        TextInputKind::Number => ch.is_ascii_digit(),
    }
}

fn printable_for_key(code: u8, shift: bool) -> Option<char> {
    const LETTERS: [(u8, char); 26] = [
        (30, 'a'), (48, 'b'), (46, 'c'), (32, 'd'), (18, 'e'), (33, 'f'),
        (34, 'g'), (35, 'h'), (23, 'i'), (36, 'j'), (37, 'k'), (38, 'l'),
        (50, 'm'), (49, 'n'), (24, 'o'), (25, 'p'), (16, 'q'), (19, 'r'),
        (31, 's'), (20, 't'), (22, 'u'), (47, 'v'), (17, 'w'), (45, 'x'),
        (21, 'y'), (44, 'z'),
    ];
    // US-QWERTY printables outside letters. Index = evdev code - 2.
    const PLAIN: [(u8, char); 12] = [
        (2, '1'), (3, '2'), (4, '3'), (5, '4'), (6, '5'), (7, '6'),
        (8, '7'), (9, '8'), (10, '9'), (11, '0'), (12, '-'), (13, '='),
    ];
    const SHIFT: [(u8, char); 12] = [
        (2, '!'), (3, '@'), (4, '#'), (5, '$'), (6, '%'), (7, '^'),
        (8, '&'), (9, '*'), (10, '('), (11, ')'), (12, '_'), (13, '+'),
    ];
    const PLAIN_EXTRA: [(u8, char); 10] = [
        (26, '['), (27, ']'), (39, ';'), (40, '\''), (41, '`'),
        (43, '\\'), (51, ','), (52, '.'), (53, '/'), (57, ' '),
    ];
    const SHIFT_EXTRA: [(u8, char); 10] = [
        (26, '{'), (27, '}'), (39, ':'), (40, '"'), (41, '~'),
        (43, '|'), (51, '<'), (52, '>'), (53, '?'), (57, ' '),
    ];
    if let Some((_, base)) = LETTERS.iter().find(|(key, _)| *key == code) {
        return Some(if shift { base.to_ascii_uppercase() } else { *base });
    }
    let table: &[(u8, char)] = if shift { &SHIFT } else { &PLAIN };
    if let Some((_, ch)) = table.iter().find(|(key, _)| *key == code) {
        return Some(*ch);
    }
    let extra: &[(u8, char)] = if shift { &SHIFT_EXTRA } else { &PLAIN_EXTRA };
    extra
        .iter()
        .find(|(key, _)| *key == code)
        .map(|(_, ch)| *ch)
}

/// R2 `insets::Ime` 消费面：由 field 屏幕底边、viewport 高与 IME bottom
/// inset 推导重叠像素。IME 未生产时 inset=0，恒返 0。
pub fn ime_overlap_px(field_bottom: i32, viewport_h: i32, ime_bottom_inset: i32) -> u32 {
    if ime_bottom_inset <= 0 || viewport_h <= 0 {
        return 0;
    }
    let ime_top = viewport_h.saturating_sub(ime_bottom_inset);
    let field_bottom = field_bottom.clamp(0, viewport_h);
    (field_bottom - ime_top).max(0) as u32
}

pub(crate) fn is_valid_utf8(bytes: &[u8]) -> bool {
    let mut i = 0usize;
    while i < bytes.len() {
        let (cp, step) = match next(&bytes[i..]) {
            Some(v) => v,
            None => return false,
        };
        if cp == utf8::REPLACEMENT && bytes[i..i + step] != *"\u{FFFD}".as_bytes() {
            return false;
        }
        i += step.max(1);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_printing_and_filters() {
        assert_eq!(
            text_key_intent(30, KEY_ACTION_DOWN, 0, TextInputKind::Text),
            Some(TextInputKeyOp::Edit(TextInputEdit::Insert('a')))
        );
        assert_eq!(
            text_key_intent(30, KEY_ACTION_DOWN, KEY_MOD_SHIFT, TextInputKind::Text),
            Some(TextInputKeyOp::Edit(TextInputEdit::Insert('A')))
        );
        assert_eq!(text_key_intent(30, KEY_ACTION_DOWN, 0, TextInputKind::Number), None);
        assert_eq!(text_key_intent(57, KEY_ACTION_DOWN, 0, TextInputKind::Text), Some(TextInputKeyOp::Edit(TextInputEdit::Insert(' '))));
    }

    #[test]
    fn utf8_split_and_edit() {
        let mut s = TextInputState::new("设a置".as_bytes()).unwrap();
        assert_eq!(s.len_cp(), 3);
        s.apply_edit(TextInputEdit::Left { extend: false });
        s.apply_edit(TextInputEdit::Left { extend: false });
        let (a, b) = s.split_visual(TextInputKind::Text).unwrap();
        assert_eq!(a.as_bytes(), "设".as_bytes());
        assert_eq!(b.as_bytes(), "a置".as_bytes());
        let out = s.apply_edit(TextInputEdit::Insert('X'));
        assert!(out.changed && out.selection_changed);
        assert_eq!(s.value().as_bytes(), "设Xa置".as_bytes());
    }
}
