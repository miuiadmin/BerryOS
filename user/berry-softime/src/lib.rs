#![cfg_attr(not(test), no_std)]
//! berry-softime —— softimed 软键盘引擎 v1（QWERTY-lite ASCII）。
//!
//! 三件：①键位模型（3 字母行 + 动作行，Shift 大小写档）；②按键 →
//! 注入载荷（可打印 ASCII 走 DEV_TEXT 单字节串、Backspace/Enter 走
//! DEV_KEYBOARD Linux input 码——gui-inputd EV_KEY feed 泵同词汇）；
//! ③显隐状态机（ime_wanted 位形——SERVICE 底窗显隐单源）。
//! 编码**不自带帧装**：`inject_frame` 调 [`berry_guiproto::
//! encode_input_inject_req`] 单源（值域拒/帽执法在彼处）。
//! SERVICE 底窗渲染与 OP_INPUT_INJECT 通道接线随波 3 并主（模块级挂账，
//! 见 Cargo.toml 头注）。

use berry_guiproto::{encode_input_inject_req, parse_input_inject_req, DEV_KEYBOARD, DEV_TEXT, INPUT_INJECT_REQ_MAX};

/// Linux input 事件码（gui-inputd EV_KEY feed 泵同词汇——非 BerryOS 私码）。
pub const KEY_BACKSPACE: u8 = 14;
/// Enter 键码。
pub const KEY_ENTER: u8 = 28;

/// 键位（lite 集：可打印 ASCII + Backspace/Enter/Shift/空格）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// 可打印 ASCII（a-z0-9 与标点——Shift 档由 [`Keyboard::shift`] 决定大小写映射）。
    Char(u8),
    /// 空格（DEV_TEXT ' '）。
    Space,
    /// 退格（DEV_KEYBOARD KEY_BACKSPACE）。
    Backspace,
    /// 回车（DEV_KEYBOARD KEY_ENTER）。
    Enter,
    /// Shift 档切换（不注入——纯引擎态）。
    Shift,
}

/// QWERTY-lite 三字母行（小写档源表——Shift 档经 ASCII 位运算）。
pub const ROWS: [&[Key]; 4] = [
    &[Key::Char(b'q'), Key::Char(b'w'), Key::Char(b'e'), Key::Char(b'r'), Key::Char(b't'), Key::Char(b'y'), Key::Char(b'u'), Key::Char(b'i'), Key::Char(b'o'), Key::Char(b'p')],
    &[Key::Char(b'a'), Key::Char(b's'), Key::Char(b'd'), Key::Char(b'f'), Key::Char(b'g'), Key::Char(b'h'), Key::Char(b'j'), Key::Char(b'k'), Key::Char(b'l')],
    &[Key::Char(b'z'), Key::Char(b'x'), Key::Char(b'c'), Key::Char(b'v'), Key::Char(b'b'), Key::Char(b'n'), Key::Char(b'm')],
    &[Key::Shift, Key::Space, Key::Backspace, Key::Enter],
];

/// 键显示名（渲染面消费；动作键用 ASCII 助记——文案帽约束）。
/// 定长 4B：首字节有效、其余零（空格 = ' '，Shift = '^'，退格 = 'B'，回车 = '\n'）。
pub fn key_label(k: Key, shift: bool) -> [u8; 4] {
    let mut out = [0u8; 4];
    match k {
        Key::Char(c) => {
            let up = if shift && c.is_ascii_lowercase() { c - 32 } else { c };
            out[0] = up;
        }
        Key::Space => out[0] = b' ',
        Key::Backspace => out[0] = b'B',
        Key::Enter => out[0] = b'\n',
        Key::Shift => out[0] = b'^',
    }
    out
}

/// 软键盘引擎态（Shift 档 + 显隐）。
pub struct Keyboard {
    shift: bool,
    face: Face,
}

/// 显隐状态机：焦点窗 ime_wanted 是唯一真源，`visible` 是其影子。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Face {
    pub visible: bool,
    pub ime_wanted_by_focus: bool,
}

impl Face {
    /// 新建（隐）。
    pub const fn new() -> Self {
        Face { visible: false, ime_wanted_by_focus: false }
    }

    /// 焦点窗 ime_wanted 变更 → 返回显隐是否翻转（SERVICE 窗建/撤单源驱动）。
    pub fn focus_wants(&mut self, wants: bool) -> bool {
        self.ime_wanted_by_focus = wants;
        let next = wants;
        if next != self.visible {
            self.visible = next;
            true
        } else {
            false
        }
    }
}

impl Keyboard {
    /// 新建（隐 + 小写档）。
    pub const fn new() -> Self {
        Keyboard { shift: false, face: Face::new() }
    }

    /// 当前 Shift 档。
    pub fn shift_on(&self) -> bool {
        self.shift
    }

    /// 显隐面引用（SERVICE 窗接线消费）。
    pub fn face(&mut self) -> &mut Face {
        &mut self.face
    }

    /// 按键 → 注入帧（Shift 按键翻档不注入返回 0；其余经 guiproto 单源编码）。
    /// 返回写入长度（0 = 无注入）。
    pub fn inject_frame(&mut self, k: Key, out: &mut [u8]) -> usize {
        match k {
            Key::Shift => {
                self.shift = !self.shift;
                0
            }
            Key::Char(c) => {
                let ch = if self.shift && c.is_ascii_lowercase() { c - 32 } else { c };
                // Shift 单发档：字母注入后回落（OH/IOS 软键盘单发大写惯例）。
                if self.shift {
                    self.shift = false;
                }
                encode_input_inject_req(out, DEV_TEXT, &[ch]).unwrap_or(0)
            }
            Key::Space => encode_input_inject_req(out, DEV_TEXT, &[b' ']).unwrap_or(0),
            Key::Backspace => encode_input_inject_req(out, DEV_KEYBOARD, &[KEY_BACKSPACE]).unwrap_or(0),
            Key::Enter => encode_input_inject_req(out, DEV_KEYBOARD, &[KEY_ENTER]).unwrap_or(0),
        }
    }
}

/// 帧长帽（宿主缓冲尺寸单源引出）。
pub const FRAME_MAX: usize = INPUT_INJECT_REQ_MAX;

/// 解码回看（测试/诊断用——guiproto parse 单源引回）。
pub fn peek(buf: &[u8]) -> Option<(u8, &[u8])> {
    parse_input_inject_req(buf)
}
