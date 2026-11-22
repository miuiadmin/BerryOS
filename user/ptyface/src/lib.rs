//! berry-ptyface——终端计划 刀 5：Termios 行规纯核。
//!
//! Raw/Cooked/Echo/ISIG 四模式 + 行缓冲 + 特殊字符处理。
//!
//! 纯面纪律：行规状态机/输入变换 = 零 I/O 纯函数。
#![cfg_attr(not(test), no_std)]

/// 终端模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Termios {
    /// 输入模式旗。
    pub raw: bool,        // raw 模式：输入直通（无行缓冲/无回显/无信号）
    pub echo: bool,      // 回显
    pub isig: bool,      // 信号生成（Ctrl-C=SIGINT/Ctrl-Z=SIGTSTP）
    pub icanon: bool,    // 规范模式（行缓冲）
    pub icrnl: bool,     // CR→NL
}

impl Termios {
    pub fn cooked() -> Termios {
        Termios { raw: false, echo: true, isig: true, icanon: true, icrnl: true }
    }

    pub fn raw_mode() -> Termios {
        Termios { raw: true, echo: false, isig: false, icanon: false, icrnl: false }
    }
}

/// 特殊控制字符（cc 数组的语义位）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialChar {
    Intr,  // Ctrl-C (0x03)
    Quit,  // Ctrl-\ (0x1C)
    Erase, // Backspace/Del (0x7F)
    Kill,  // Ctrl-U (0x15)
    Eof,   // Ctrl-D (0x04)
    Start, // Ctrl-Q (0x11)
    Stop,  // Ctrl-S (0x13)
    Susp,  // Ctrl-Z (0x1A)
}

impl SpecialChar {
    pub fn from_byte(b: u8) -> Option<SpecialChar> {
        match b {
            0x03 => Some(SpecialChar::Intr),
            0x1C => Some(SpecialChar::Quit),
            0x7F | 0x08 => Some(SpecialChar::Erase),
            0x15 => Some(SpecialChar::Kill),
            0x04 => Some(SpecialChar::Eof),
            0x11 => Some(SpecialChar::Start),
            0x13 => Some(SpecialChar::Stop),
            0x1A => Some(SpecialChar::Susp),
            _ => None,
        }
    }

    /// 信号编号（如有）。
    pub fn signal(&self) -> Option<u8> {
        match self {
            SpecialChar::Intr => Some(2),  // SIGINT
            SpecialChar::Quit => Some(3),  // SIGQUIT
            SpecialChar::Susp => Some(20), // SIGTSTP
            _ => None,
        }
    }
}

/// 行缓冲状态。
pub struct LineBuffer {
    pub buf: [u8; 256],
    pub len: usize,
    pub term: Termios,
}

/// 输入处理结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputResult {
    /// 字节加入缓冲（未换行——尚不可读）。
    Buffered,
    /// 行完成（NL 或 EOF——缓冲内容可读）。
    Line,
    /// 行被擦除字符回退。
    Erased,
    /// 行被 Kill 清空。
    Killed,
    /// 信号生成（信号编号）。
    Signal(u8),
    /// 流停止（Ctrl-S）。
    Stopped,
    /// 流恢复（Ctrl-Q）。
    Started,
}

impl LineBuffer {
    pub fn new(term: Termios) -> LineBuffer {
        LineBuffer { buf: [0; 256], len: 0, term }
    }

    /// 处理单字节输入。
    pub fn input(&mut self, b: u8) -> InputResult {
        // CR→NL
        let b = if self.term.icrnl && b == b'\r' { b'\n' } else { b };

        if self.term.raw {
            // Raw 模式：一切直通。
            if self.len < self.buf.len() {
                self.buf[self.len] = b;
                self.len += 1;
                return InputResult::Line; // Raw 模式每字节即「完成」
            }
            return InputResult::Buffered;
        }

        // 非raw：先查特殊字符。
        if let Some(sc) = SpecialChar::from_byte(b) {
            if self.term.isig {
                if let Some(sig) = sc.signal() {
                    return InputResult::Signal(sig);
                }
            }
            match sc {
                SpecialChar::Erase => {
                    if self.len > 0 { self.len -= 1; }
                    return InputResult::Erased;
                }
                SpecialChar::Kill => {
                    self.len = 0;
                    return InputResult::Killed;
                }
                SpecialChar::Eof => return InputResult::Line,
                SpecialChar::Stop => return InputResult::Stopped,
                SpecialChar::Start => return InputResult::Started,
                _ => {}
            }
        }

        // 普通字节入缓冲。
        if self.len < self.buf.len() {
            self.buf[self.len] = b;
            self.len += 1;
        }

        // 规范模式：换行/EOF 即完成。
        if self.term.icanon && b == b'\n' {
            return InputResult::Line;
        }
        if !self.term.icanon {
            return InputResult::Line; // 非规范：即入即出
        }
        InputResult::Buffered
    }

    /// 取当前行（不清空）。
    pub fn line(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    /// 清空行。
    pub fn clear(&mut self) {
        self.len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooked_line_builds() {
        let mut lb = LineBuffer::new(Termios::cooked());
        assert_eq!(lb.input(b'H'), InputResult::Buffered);
        assert_eq!(lb.input(b'i'), InputResult::Buffered);
        assert_eq!(lb.input(b'\n'), InputResult::Line);
        assert_eq!(lb.line(), b"Hi\n");
        lb.clear();
        assert_eq!(lb.line(), b"");
    }

    #[test]
    fn erase_backspace() {
        let mut lb = LineBuffer::new(Termios::cooked());
        lb.input(b'A');
        lb.input(b'B');
        assert_eq!(lb.input(0x7F), InputResult::Erased);
        assert_eq!(lb.line(), b"A");
    }

    #[test]
    fn kill_clears() {
        let mut lb = LineBuffer::new(Termios::cooked());
        lb.input(b'A');
        lb.input(b'B');
        lb.input(b'C');
        assert_eq!(lb.input(0x15), InputResult::Killed);
        assert_eq!(lb.len, 0);
    }

    #[test]
    fn ctrl_c_signal() {
        let mut lb = LineBuffer::new(Termios::cooked());
        assert_eq!(lb.input(0x03), InputResult::Signal(2)); // SIGINT
    }

    #[test]
    fn raw_passthrough() {
        let mut lb = LineBuffer::new(Termios::raw_mode());
        assert_eq!(lb.input(b'X'), InputResult::Line); // Raw 即入即出
        assert_eq!(lb.input(0x03), InputResult::Line); // Ctrl-C 直通
    }

    #[test]
    fn isig_off_no_signal() {
        let mut t = Termios::cooked();
        t.isig = false;
        let mut lb = LineBuffer::new(t);
        assert_eq!(lb.input(0x03), InputResult::Buffered); // Ctrl-C 入缓冲
    }

    #[test]
    fn cr_to_nl() {
        let mut lb = LineBuffer::new(Termios::cooked());
        assert_eq!(lb.input(b'\r'), InputResult::Line); // CR→NL→行完成
        assert_eq!(lb.line(), b"\n");
    }
}
