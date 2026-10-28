//! editor —— C-4 bsh 行编辑器（std 版 mini-readline + Tab 补全）。
//!
//! **归属裁定兑现**（shell设计 §2.4 留档）：console 退化为逐字节透传（内核
//! 无 rootfs/fs 知识——微内核纪律），echo/退格/历史召回/Ctrl-U/**Tab 补全**
//! 全部归 bsh。语义逐字对齐内核时代 console mini-readline（P5 v0.5）：
//! 历史环 8 条 newest-first、非空且≠newest 才入环、`usize::MAX` = live
//! 游标、浏览态编辑已载入行、下翻到 live 清草稿、ESC 状态机三分支含非
//! final 复位落下（对抗审查回归面保持）。
//!
//! Tab 补全（本刀新增）：[`TabSource`] 注入候选源——编辑器状态机对源
//! 无假设（自验用 FakeSrc 确定性喂键；真源 = 内建 ∪ rootfs bin/ + libc
//! 目录视图，见 main.rs `RealSrc`）。切分/匹配/分类/列示全在 berry-complete
//! 纯函数面（host 10 测）——本模块只管喂键与回显字节拼装。
//!
//! 回显契约：`feed` 返回 `Flow::Continue(echo)` 的 echo 由调用方统一
//! `write_all` + `flush`（编辑器不持 stdout）；`Flow::Line(line)` 时调用方
//! 自行回显 `\n`（Enter 的 echo 归调用方——行定稿与执行同拍）。

use berry_complete::{complete_from, render_matches, token_at_eol, Completion, Token};

/// 提示符（多候选列示后的行重显同用）。
pub const PROMPT: &[u8] = b"bsh> ";
/// 历史环容量（内核时代同值 8）。
const HIST_CAP: usize = 8;
/// 行帽（内核 96 是定长数组约束；std 版放宽到 256——bsh 命令行量级）。
const LINE_CAP: usize = 256;

/// Tab 候选源（注入面——FakeSrc 可确定性自验编辑器状态机）。
pub trait TabSource {
    /// 命令位候选全集（**全名**；双源同名由 complete_from 内部去重——
    /// 调用方不必预归并）。
    fn commands(&mut self, out: &mut Vec<Vec<u8>>);
    /// 路径位候选：dir 下**直接子项名**（不含路径前缀——编辑器只补 base
    /// 段）。dir 形态：`/etc/`（含尾 `/`）或 `.`（cwd）。Err = 源不可用
    /// （无 fs）——Tab 静默无操作（编辑会话不死）。
    fn entries(&mut self, dir: &[u8]) -> Result<Vec<Vec<u8>>, ()>;
}

/// 每键产物。
pub enum Flow {
    /// 继续编辑；echo = 需回显的字节（可为空——控制字节/零命中 Tab）。
    Continue(Vec<u8>),
    /// 行定稿（Enter）——line 不含换行；`\n` 回显归调用方。
    Line(Vec<u8>),
    /// Ctrl-C（0x03，C-4b）：作废当前行（编辑器侧已清行 + 游标回 live；
    /// 作废草稿**不入历史**——hist_push 不调）。杀前台子进程归调用方
    /// （编辑器无子进程知识——feed 循环期恒无前台子进程，等待期的 0x03
    /// 走 collect_fg 杀链不经此处）。echo = "^C\n"；prompt 重显归调用方。
    Interrupt(Vec<u8>),
}

/// 行编辑器（单会话——bsh 交互循环独占）。
pub struct Editor {
    line: Vec<u8>,
    /// 历史环 newest-first（hist[0] = 最近行；帽 HIST_CAP 满环丢最旧）。
    hist: Vec<Vec<u8>>,
    /// 浏览游标：usize::MAX = live；0..hist.len() = 已载入的历史条。
    hi: usize,
    /// ESC 序列状态机（跨 read 批次持久——序列字节可能分批到达）：
    /// 0 = 常态 / 1 = 见 ESC / 2 = 见 ESC '['（收参数或 final）。
    esc: u8,
}

impl Editor {
    pub fn new() -> Self {
        Editor { line: Vec::new(), hist: Vec::new(), hi: usize::MAX, esc: 0 }
    }

    /// 当前编辑行只读视图（自验/诊断——行缓冲本身私有）。
    pub fn line(&self) -> &[u8] {
        &self.line
    }

    /// 喂一键（来自 stdin 的原始字节）。src 只在 Tab 键时被问。
    pub fn feed(&mut self, b: u8, src: &mut dyn TabSource) -> Flow {
        // ESC 序列状态机（先于一切常规字节处理——ESC/参数/final 都不该进
        // 行缓冲；语义照抄内核版）。
        if self.esc == 1 {
            if b == b'[' {
                self.esc = 2;
                return Flow::Continue(Vec::new());
            }
            self.esc = 0; // 裸 ESC 后非 '['：复位，本字节按普通字节落下
        } else if self.esc == 2 {
            if (0x30..=0x3f).contains(&b) {
                return Flow::Continue(Vec::new()); // 参数字节：留在 esc=2 等 final
            }
            if (0x40..=0x7e).contains(&b) {
                self.esc = 0;
                return match b {
                    b'A' => self.hist_up(),
                    b'B' => self.hist_down(),
                    // 其它 finals（左/右键、Home/End、Del 等）：吞——v1 无
                    // 光标移动语义，编辑点恒在行尾。
                    _ => Flow::Continue(Vec::new()),
                };
            }
            // 非 final（控制字节 0x7f/≥0x80 乱序等）：unclosed CSI 复位，
            // 本字节按普通字节落下（内核版对抗审查实抓面保持）。
            self.esc = 0;
        } else if b == 0x1b {
            self.esc = 1;
            return Flow::Continue(Vec::new());
        }
        match b {
            b'\n' | b'\r' => {
                self.hist_push();
                Flow::Line(core::mem::take(&mut self.line))
            }
            0x7f | 0x08 => {
                // 退格：有字符才回显 "\b \b" 三连（空行无操作——比内核版
                // 的无条件三连更正确，且无断言面依赖旧形态）。
                let mut echo = Vec::new();
                if self.line.pop().is_some() {
                    echo.extend_from_slice(b"\x08 \x08");
                }
                Flow::Continue(echo)
            }
            0x03 => {
                // Ctrl-C（C-4b）：作废当前行——清缓冲 + 游标回 live（草稿
                // 不入历史）。与 ESC 状态机共存逐一核对：三种 esc 态下
                // 0x03 都「复位落下」到本臂（esc=1 非 '[' 复位落下 /
                // esc=2 非参数 0x30..=0x3f、非 final 0x40..=0x7e 复位落下 /
                // esc=0 直达）——无一吞字节路径，CSI 中断语义与真终端
                // 一致（序列中断、行作废）。
                self.line.clear();
                self.hi = usize::MAX;
                Flow::Interrupt(b"^C\n".to_vec())
            }
            0x15 => {
                // Ctrl-U：行擦除（readline 惯例；浏览中 = 擦已载入历史行的
                // 编辑面，hi 不动）。
                let mut echo = Vec::new();
                erase_line(&mut echo, self.line.len());
                self.line.clear();
                Flow::Continue(echo)
            }
            b'\t' => self.tab(src),
            _ if b < 0x20 => Flow::Continue(Vec::new()), // 其它控制字节忽略
            _ => {
                let mut echo = Vec::new();
                if self.line.len() < LINE_CAP {
                    self.line.push(b);
                    echo.push(b);
                }
                // else：行溢出——丢字节不回显（行帽执法）。
                Flow::Continue(echo)
            }
        }
    }

    /// Enter 推历史：非空 && != newest（重复执行同一命令不灌环）；游标回
    /// live。空行同样回 live（否则上翻 ×N → Ctrl-U → 空行 Enter 后游标还
    /// 钉在 hist[hi+1]，再上翻召回旧条而非 newest——内核版对抗审查实抓面）。
    /// **入环拷贝不搬走**（内核版同语义）——feed 的 `Line(take)` 还要交出
    /// 本行去执行，搬走 = 交互模式每条命令都变空行（C-4 真机首跑实抓）。
    fn hist_push(&mut self) {
        self.hi = usize::MAX;
        if self.line.is_empty() {
            return;
        }
        if self.hist.first() == Some(&self.line) {
            return;
        }
        self.hist.insert(0, self.line.clone());
        self.hist.truncate(HIST_CAP);
    }

    /// 上翻：MAX→0（首翻 = newest）/ hi+1 < len → hi+1 / 否则停留（顶上
    /// 无操作）。
    fn hist_up(&mut self) -> Flow {
        if self.hist.is_empty() {
            return Flow::Continue(Vec::new());
        }
        let target = if self.hi == usize::MAX {
            0
        } else if self.hi + 1 < self.hist.len() {
            self.hi + 1
        } else {
            return Flow::Continue(Vec::new()); // 顶上停留
        };
        self.hist_load(target)
    }

    /// 下翻：hi > 0 → hi-1 / hi == 0 → 回 live（清空编辑行——草稿不保存，
    /// v1 极简）/ live 态无操作。
    fn hist_down(&mut self) -> Flow {
        if self.hi == usize::MAX {
            return Flow::Continue(Vec::new());
        }
        if self.hi == 0 {
            let mut echo = Vec::new();
            erase_line(&mut echo, self.line.len());
            self.line.clear();
            self.hi = usize::MAX;
            Flow::Continue(echo)
        } else {
            self.hist_load(self.hi - 1)
        }
    }

    /// 载入 hist[target]：擦当前行回显 + echo 新行（编辑点恒在行尾）。
    fn hist_load(&mut self, target: usize) -> Flow {
        let mut echo = Vec::new();
        erase_line(&mut echo, self.line.len());
        let l = self.hist[target].clone();
        echo.extend_from_slice(&l);
        self.line = l;
        self.hi = target;
        Flow::Continue(echo)
    }

    /// Tab：token 切分 → 命令位（内建 ∪ bin/，补后加空格）/ 路径位（dir
    /// 直接子项，不加空格）→ complete_from 三态消费。切分产物（prefix/
    /// dir/base）先拷出脱离行借用——apply 需 &mut self。
    fn tab(&mut self, src: &mut dyn TabSource) -> Flow {
        enum Pos {
            Cmd(Vec<u8>),
            Arg { dir: Vec<u8>, base: Vec<u8> },
        }
        let pos = match token_at_eol(&self.line) {
            Token::Command(prefix) => Pos::Cmd(prefix.to_vec()),
            Token::Arg(tok) => match tok.iter().rposition(|&c| c == b'/') {
                // 最后一个 '/' 切 dir（含尾 '/'）/ base（补全目标段）；
                // 无 '/' = cwd 相对名（dir = "."，真源折 getcwd）。
                Some(i) => Pos::Arg { dir: tok[..i + 1].to_vec(), base: tok[i + 1..].to_vec() },
                None => Pos::Arg { dir: b".".to_vec(), base: tok.to_vec() },
            },
        };
        let mut echo = Vec::new();
        match pos {
            Pos::Cmd(prefix) => {
                let mut cands = Vec::new();
                src.commands(&mut cands);
                let refs: Vec<&[u8]> = cands.iter().map(|c| c.as_slice()).collect();
                self.apply(&mut echo, &refs, &prefix, true);
            }
            Pos::Arg { dir, base } => {
                let names = match src.entries(&dir) {
                    Ok(n) => n,
                    Err(()) => return Flow::Continue(Vec::new()), // 无 fs：静默
                };
                let refs: Vec<&[u8]> = names.iter().map(|n| n.as_slice()).collect();
                self.apply(&mut echo, &refs, &base, false);
            }
        }
        Flow::Continue(echo)
    }

    /// 补全结果落行 + 回显：Unique 补后缀（+尾空格若 add_space）；Ambiguous
    /// 先补公共增量再列示 + 重显 prompt+当前行；None 无操作（零命中静默）。
    fn apply(&mut self, echo: &mut Vec<u8>, cands: &[&[u8]], prefix: &[u8], add_space: bool) {
        match complete_from(prefix, cands, add_space) {
            Completion::None => {}
            Completion::Unique { suffix, add_space } => {
                if !suffix.is_empty() && self.line.len() + suffix.len() <= LINE_CAP {
                    self.line.extend_from_slice(&suffix);
                    echo.extend_from_slice(&suffix);
                }
                if add_space && self.line.len() < LINE_CAP {
                    self.line.push(b' ');
                    echo.push(b' ');
                }
            }
            Completion::Ambiguous { common, matches } => {
                if !common.is_empty() && self.line.len() + common.len() <= LINE_CAP {
                    self.line.extend_from_slice(&common);
                    echo.extend_from_slice(&common);
                }
                render_matches(echo, &matches);
                echo.extend_from_slice(PROMPT);
                echo.extend_from_slice(&self.line);
            }
        }
    }
}

/// 擦除 n 格回显（"\b \b" ×n——退格三连同款；不触碰行缓冲本身）。
fn erase_line(echo: &mut Vec<u8>, n: usize) {
    for _ in 0..n {
        echo.extend_from_slice(b"\x08 \x08");
    }
}
