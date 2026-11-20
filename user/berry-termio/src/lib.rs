//! berry-termio —— berry-term 哑终端显示核纯函数面（T7，2026-11-17 命令行
//! 终端与shell完善刀——berry-term 运行时零机器执法 + 零 host 单测是 T 域
//! 最大回归面，本 crate 以脱离 QEMU 的 host 回归网对冲，berry-complete
//! 「纯面外迁」先例同款纪律）。
//!
//! **职责边界**：行环 / 尾段 / 回显字节解释器 / 软换行 / 视图计算 / 子
//! 监督分诊决策面（triage——只产决策不触 IO）——全部纯面（定长数组零
//! alloc，无 syscall 依赖）；ui.wake / child_send / wait_many 等通道与
//! 进程 IO 胶水（send/recv/wait/terminate/reap 调用）留在 berry-term
//! main.rs 不进本 crate。显示单源 = 子 bsh 回显字节——本模块只解释不
//! 生成。
//!
//! **字节解释器（feed）覆盖集 = bsh 回显词汇表**（bsh editor.rs 逐臂核实
//! 封闭）：纯文本＋`\n`（行定稿）＋BS-空格-BS 三连（退格/Ctrl-U/历史召回
//! 的 erase_line ×n——逐字节解释 `\b` 删尾、空格入尾、`\b` 再删，净删一格）
//! ＋`ESC[2J`/`ESC[H`（bsh clear 的 7B 序列）＋`^C\n`（Ctrl-C 作废行回显
//! ——`^`/`C` 照常可见字节入尾）＋PROMPT 重显（可见字节照常入尾段）。
//! 语义：
//! - `\n`：尾段整段成完成行入环（软换行切块）；
//! - `\b`（0x08）：删尾段末字节（空尾无操作）；
//! - `\r`：丢弃（bsh 回显词汇表无 `\r`，记面）；
//! - ESC 状态机跨 feed 批次持久（bsh editor.rs :84-110 同形，先于一切
//!   常规字节处理）：`ESC [ 2 J`＝清行环＋清尾段（参数恰为 `2` 才清——
//!   `ESC[J`/`ESC[3J` 等不认）；`ESC[H` 与其它 finals＝吞（无操作复位）；
//!   裸 ESC 后非 `[` 复位落下（本字节按常规字节处理）；参数字节
//!   0x30..=0x3f 留态等 final；非 final 乱序字节 unclosed CSI 复位落下；
//! - 其余未识别控制字节维持现状（进尾段渲染缺省字形面）；
//! - ESC 态中再收 ESC（esc=1/2 两态）＝复位＋丢字节（bsh editor 同形
//!   ——其 <0x20 控制码静默忽略；审查轮对齐，病态流防御面）；
//! - 尾段满 TAIL_MAX 丢后续字节（审查轮纠偏：bsh 常规词汇表上限
//!   prompt 5B＋LINE_CAP 256B＝261 ≪ 320，但 Tab 多候选列示的单行
//!   候选平铺（render_matches 一行内空格连排全部候选名）与超长完成
//!   输出行可超 320B——溢出是**可达的正常用户路径**，可见降级 = 该行
//!   尾段显示截断，下个 `\n` 后恢复）。
//!
//! **不做**通用 ANSI 屏模型（cell grid 重写已砍单——最小解释器覆盖词汇表
//! 全集即够）。
//!
//! **软换行（视图数学）**：完成行超 LINE_MAX 按 64B 切多物理行入环；尾段
//! 缓冲 TAIL_MAX=320 全容输入行回显（输入行不再截 64B）。视图 = 环行居上
//! ＋尾段前 K-1 块居下＋末块（K = ceil((tail_len+1)/64)，bsh 词汇表下
//! ≤5——prompt 5B＋行 256B；病态满容 320B 时 K=6，视图数学同律收缩），
//! 末块由 UI 层追加光标字节作输入行；环满时可见历史随 K 收缩（顶掉最老行
//! 保尾段可见——prompt 恒在屏）。

#![cfg_attr(not(test), no_std)]

/// 子监督分诊决策与键路仲裁纯面（2026-11-20 超时会流分诊刀 P1——死
/// 信号层级表／SendFeed 三态镜像／Backlog 键路背压状态机）。
pub mod triage;

/// 行环可见行数（berry-term 行环节点数单源）。
pub const TERM_ROWS: usize = 24;

/// 单物理行字节帽 = berry-ui `TEXT_INLINE` 现行值 64（TextBuf loud 拒面）。
pub const LINE_MAX: usize = 64;

/// 尾段收容帽：prompt 5B ＋ bsh LINE_CAP 256B 全容（＝320）——输入行
/// 回显不再截 64B；软换行块宽仍 LINE_MAX。
pub const TAIL_MAX: usize = 320;

/// 终端显示核：行环 + 尾段 + 回显字节解释器 + 视图计算（纯面状态机）。
///
/// 生命周期归 berry-term 帧循环独占；`new()` 全零初始化（const fn 可入
/// 静态段）。
pub struct TermCore {
    /// 行环内容（`head` 起逻辑序；满环后 head 推进顶掉最老行）。
    rows: [[u8; LINE_MAX]; TERM_ROWS],
    /// 各行有效长。
    lens: [usize; TERM_ROWS],
    /// 最老行物理下标。
    head: usize,
    /// 在册行数（0..=TERM_ROWS）。
    used: usize,
    /// 无 `\n` 尾段（活动行——子 prompt 与回显输入的驻留位，软换行多块
    /// 视图的末块即输入行）。
    tail: [u8; TAIL_MAX],
    tail_len: usize,
    /// ESC 序列状态机（跨 feed 批次持久——序列字节可能分批到达）：
    /// 0 = 常态 / 1 = 见 ESC / 2 = 见 ESC '['（收参数或 final）。
    esc: u8,
    /// CSI 参数累积判别：参数序列恰为 "2"（`ESC[2J` 消费面）。
    csi_two: bool,
    /// CSI 已见任意参数字节（"恰为首参 '2'" 的排他位）。
    csi_any: bool,
}

impl TermCore {
    /// 全零初始化。
    pub const fn new() -> TermCore {
        TermCore {
            rows: [[0u8; LINE_MAX]; TERM_ROWS],
            lens: [0usize; TERM_ROWS],
            head: 0,
            used: 0,
            tail: [0u8; TAIL_MAX],
            tail_len: 0,
            esc: 0,
            csi_two: false,
            csi_any: false,
        }
    }

    /// 子回显喂流：逐字节过解释器（ESC 状态机先于常规字节——序列字节
    /// 不进尾段）。
    pub fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.feed_byte(b);
        }
    }

    /// 单字节解释（feed 的核——独立于批量边界）。
    fn feed_byte(&mut self, b: u8) {
        // ESC 序列状态机（先于一切常规字节处理——ESC/参数/final 都不该
        // 进尾段；语义照 bsh editor.rs 同形）。
        match self.esc {
            1 => {
                if b == b'[' {
                    self.esc = 2;
                    self.csi_two = false;
                    self.csi_any = false;
                    return;
                }
                self.esc = 0; // 裸 ESC 后非 '['：复位，本字节按常规字节落下
                if b == 0x1b {
                    return; // 双 ESC：复位＋丢字节（bsh editor 同形——<0x20 静默忽略）
                }
            }
            2 => {
                if (0x30..=0x3f).contains(&b) {
                    // 参数字节：留态等 final；恰为首参 '2' 且无后续才记
                    // csi_two（"2" 之外任何参数形态失配）。
                    if !self.csi_any && b == b'2' {
                        self.csi_two = true;
                    } else {
                        self.csi_two = false;
                    }
                    self.csi_any = true;
                    return;
                }
                if (0x40..=0x7e).contains(&b) {
                    self.esc = 0;
                    if self.csi_two && b == b'J' {
                        // ESC[2J：清行环＋清尾段（bsh clear 消费面）。
                        self.used = 0;
                        self.head = 0;
                        self.tail_len = 0;
                    }
                    // ESC[H 与其它 finals：吞（无操作复位）。
                    return;
                }
                self.esc = 0; // unclosed CSI 复位，本字节按常规字节落下
                if b == 0x1b {
                    return; // 同双 ESC：复位＋丢字节（bsh editor 同形）
                }
            }
            _ => {
                if b == 0x1b {
                    self.esc = 1;
                    return;
                }
            }
        }
        // 常规字节解释。
        match b {
            b'\n' => {
                // 尾段整段成完成行（先拷出脱离借用——push_line 需 &mut self）。
                let n = self.tail_len;
                let mut line = [0u8; TAIL_MAX];
                line[..n].copy_from_slice(&self.tail[..n]);
                self.tail_len = 0;
                self.push_line(&line[..n]);
            }
            b'\r' => {} // 丢弃（bsh 回显词汇表无 \r）
            0x08 => {
                // \b：删尾段末字节（空尾无操作；BS-空格-BS 三连逐字节
                // 解释净删一格）。
                if self.tail_len > 0 {
                    self.tail_len -= 1;
                }
            }
            _ => {
                if self.tail_len < TAIL_MAX {
                    self.tail[self.tail_len] = b;
                    self.tail_len += 1;
                } // 满容：丢字节（截断——见模块头注）
            }
        }
    }

    /// 完成行入环（软换行）：超 LINE_MAX 按 64B 切多物理行；空行入一条
    /// 空行（bsh 空命令的回显行形态）。
    pub fn push_line(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            self.push_row(&[]);
            return;
        }
        for c in bytes.chunks(LINE_MAX) {
            self.push_row(c);
        }
    }

    /// 单物理行入环（≤LINE_MAX；满环覆写最老行并推进 head）。
    fn push_row(&mut self, bytes: &[u8]) {
        let n = bytes.len();
        if self.used < TERM_ROWS {
            let idx = (self.head + self.used) % TERM_ROWS;
            self.rows[idx][..n].copy_from_slice(bytes);
            self.lens[idx] = n;
            self.used += 1;
        } else {
            // 满环：head 位即被顶掉的最老行（覆写 + 推进）。
            self.rows[self.head][..n].copy_from_slice(bytes);
            self.lens[self.head] = n;
            self.head = (self.head + 1) % TERM_ROWS;
        }
    }

    /// 尾段只读视图（T6 探针面：prompt 收尾断言等）。
    pub fn tail(&self) -> &[u8] {
        &self.tail[..self.tail_len]
    }

    /// 视图输入行内容 = 尾段末块（UI 层追加光标字节——纯面不持光标字形）。
    /// 末块 + 光标 ≤ LINE_MAX 恒成立（K 的构造律）。
    pub fn input_block(&self) -> &[u8] {
        let k = self.tail_blocks();
        &self.tail[(k - 1) * LINE_MAX..self.tail_len]
    }

    /// 尾段块数 K（含光标位计：K = ceil((tail_len+1)/LINE_MAX)——末块给
    /// 输入行留光标位；tail_len=0 → K=1 空块）。
    fn tail_blocks(&self) -> usize {
        (self.tail_len + 1 + LINE_MAX - 1) / LINE_MAX
    }

    /// 可见行内容（屏顶起第 vi 行，0..TERM_ROWS）：逻辑行序 = 环行居上
    /// ＋尾段前 K-1 块居下（末块归输入行节点，不出现在本视图）；环满时
    /// 顶掉最老行保尾段可见（prompt 恒在屏）。越界/空位返回空切片。
    pub fn view_row(&self, vi: usize) -> &[u8] {
        if vi >= TERM_ROWS {
            return &[];
        }
        let k = self.tail_blocks();
        let ring_cap = TERM_ROWS - (k - 1);
        let skip = self.used.saturating_sub(ring_cap);
        let ring_vis = self.used - skip;
        if vi < ring_vis {
            let idx = (self.head + skip + vi) % TERM_ROWS;
            &self.rows[idx][..self.lens[idx]]
        } else if vi - ring_vis < k - 1 {
            // 尾段前 K-1 块（全满块——末块才是输入行）。
            let bi = vi - ring_vis;
            let s = bi * LINE_MAX;
            let e = (s + LINE_MAX).min(self.tail_len);
            &self.tail[s..e]
        } else {
            &[]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 行环推进序：三行按序入环、尾段清空。
    #[test]
    fn ring_advances_in_order() {
        let mut t = TermCore::new();
        t.feed(b"a\nb\nc\n");
        assert_eq!(t.view_row(0), b"a");
        assert_eq!(t.view_row(1), b"b");
        assert_eq!(t.view_row(2), b"c");
        assert_eq!(t.view_row(3), b"");
        assert_eq!(t.tail(), b"");
    }

    /// 满环覆写：30 行后可见 = 最新 24 行。
    #[test]
    fn ring_full_overwrites_oldest() {
        let mut t = TermCore::new();
        let mut s = Vec::new();
        for i in 0..30 {
            s.extend_from_slice(format!("L{i:02}\n").as_bytes());
        }
        t.feed(&s);
        assert_eq!(t.view_row(0), b"L06");
        assert_eq!(t.view_row(23), b"L29");
    }

    /// `\n` 前尾段驻留、`\n` 后成行。
    #[test]
    fn newline_pushes_tail_as_line() {
        let mut t = TermCore::new();
        t.feed(b"hello");
        assert_eq!(t.view_row(0), b"");
        assert_eq!(t.tail(), b"hello");
        t.feed(b"\n");
        assert_eq!(t.view_row(0), b"hello");
        assert_eq!(t.tail(), b"");
    }

    /// `\b` 删尾段末字节；空尾无操作。
    #[test]
    fn backspace_deletes_tail_byte() {
        let mut t = TermCore::new();
        t.feed(b"abc\x08");
        assert_eq!(t.tail(), b"ab");
        t.feed(b"\x08\x08\x08");
        assert_eq!(t.tail(), b"");
    }

    /// BS-空格-BS 三连逐字节解释净删一格（bsh erase_line 词汇）。
    #[test]
    fn bs_space_bs_net_deletes_one() {
        let mut t = TermCore::new();
        t.feed(b"abc");
        t.feed(b"\x08 \x08");
        assert_eq!(t.tail(), b"ab");
        // erase_line ×3（Ctrl-U 擦三字行）：
        t.feed(b"\x08 \x08\x08 \x08\x08 \x08");
        assert_eq!(t.tail(), b"");
    }

    /// ESC[2J 清行环＋清尾段（bsh clear 7B 的首半）。
    #[test]
    fn esc_2j_clears_screen() {
        let mut t = TermCore::new();
        t.feed(b"old line\n\x1b[2J");
        assert_eq!(t.view_row(0), b"");
        assert_eq!(t.tail(), b"");
        // clear 7B 全序列（含 ESC[H）后 prompt 驻尾段：
        t.feed(b"\x1b[2J\x1b[Hbsh> ");
        assert_eq!(t.view_row(0), b"");
        assert_eq!(t.tail(), b"bsh> ");
    }

    /// ESC[H 与其它 finals 吞（序列字节零入尾，尾段跨序列续写）；参数非
    /// 恰 "2" 的 J 序列不清屏。
    #[test]
    fn esc_h_and_unknown_csi_swallowed() {
        let mut t = TermCore::new();
        t.feed(b"x\x1b[Hy");
        assert_eq!(t.tail(), b"xy"); // 序列字节零入尾
        t.feed(b"keep\n");
        assert_eq!(t.view_row(0), b"xykeep"); // 尾段续写同行成线
        t.feed(b"\x1b[3J"); // 3 ≠ 2：吞且不清
        assert_eq!(t.view_row(0), b"xykeep");
        t.feed(b"\x1b[J"); // 空参数：吞且不清
        assert_eq!(t.view_row(0), b"xykeep");
        t.feed(b"\x1b[1;2H"); // 多参数：吞且不清
        assert_eq!(t.view_row(0), b"xykeep");
        t.feed(b"\x1b[Kz");
        assert_eq!(t.tail(), b"z");
    }

    /// 裸 ESC 后非 `[`：复位，本字节按常规字节落下。
    #[test]
    fn bare_esc_resets_and_falls_through() {
        let mut t = TermCore::new();
        t.feed(b"a\x1bx");
        assert_eq!(t.tail(), b"ax");
        // ESC 后 `\n`：落下成行定稿。
        t.feed(b"\x1b\n");
        assert_eq!(t.view_row(0), b"ax");
    }

    /// `\r` 丢弃。
    #[test]
    fn cr_discarded() {
        let mut t = TermCore::new();
        t.feed(b"a\rb\r");
        assert_eq!(t.tail(), b"ab");
    }

    /// unclosed CSI 乱序字节（非参数非 final）：复位，本字节按常规字节落下
    ///（未识别控制字节照常入尾段——缺省字形面）。
    #[test]
    fn unclosed_csi_falls_through() {
        let mut t = TermCore::new();
        t.feed(b"\x1b[\x01z"); // 0x01 非参数非 final：复位落下入尾，z 照常
        assert_eq!(t.tail(), b"\x01z");
    }

    /// ESC 态中再收 ESC：复位＋丢字节（bsh editor 同形对齐——审查轮补；
    /// esc=1 态与 unclosed CSI（esc=2）态两形）。
    #[test]
    fn esc_in_esc_state_dropped() {
        let mut t = TermCore::new();
        t.feed(b"a\x1b\x1bx"); // esc=1 态收 0x1b：丢，x 照常入尾
        assert_eq!(t.tail(), b"ax");
        let mut t2 = TermCore::new();
        t2.feed(b"b\x1b[1\x1by"); // esc=2 态（参数区）收 0x1b：复位丢弃，y 照常
        assert_eq!(t2.tail(), b"by");
    }

    /// 完成行超 64B 软换行：按 64B 切多物理行入环。
    #[test]
    fn long_line_soft_wraps() {
        let mut t = TermCore::new();
        let mut line = [b'A'; 150];
        line[0] = b'x';
        t.feed(&line);
        t.feed(b"\n");
        let r0 = t.view_row(0);
        assert_eq!(r0.len(), LINE_MAX);
        assert_eq!(&r0[..3], b"xAA");
        assert_eq!(t.view_row(1).len(), LINE_MAX);
        assert!(t.view_row(1).iter().all(|&b| b == b'A'));
        assert_eq!(t.view_row(2).len(), 150 - 2 * LINE_MAX);
        assert_eq!(t.view_row(3), b"");
    }

    /// 尾段多块视图：长尾段前 K-1 块居下、末块归输入行（+光标 ≤64B）。
    #[test]
    fn tail_multiblock_view() {
        let mut t = TermCore::new();
        t.feed(b"hist\n");
        t.feed(&[b'T'; 100]); // 无换行长尾段
        // K = ceil((100+1)/64) = 2：环 1 行 + 尾块 1 行 + 输入行。
        assert_eq!(t.view_row(0), b"hist");
        assert_eq!(t.view_row(1).len(), LINE_MAX);
        assert!(t.view_row(1).iter().all(|&b| b == b'T'));
        let ib = t.input_block();
        assert_eq!(ib.len(), 100 - LINE_MAX); // 末块 36B，UI 追加光标 37B ≤ 64
        assert!(ib.iter().all(|&b| b == b'T'));
        assert!(ib.len() + 1 <= LINE_MAX);
    }

    /// 可见历史随 K 收缩 + prompt 恒在屏：满环 + 长尾段时顶掉最老行。
    #[test]
    fn prompt_always_visible_when_full() {
        let mut t = TermCore::new();
        for i in 0..24 {
            t.feed(format!("row{i:02}\n").as_bytes());
        }
        t.feed(b"bsh> echo "); // 10B 尾段（K=1：零收缩）
        assert_eq!(t.view_row(0), b"row00");
        assert_eq!(t.view_row(23), b"row23");
        assert_eq!(t.input_block(), b"bsh> echo ");
        // 尾段过 64B（K=2）：可见历史收缩一行，尾块占末位行。
        t.feed(&[b'c'; 64]);
        assert_eq!(t.view_row(0), b"row01"); // 最老行被顶掉
        assert_eq!(t.view_row(22), b"row23");
        assert_eq!(t.view_row(23).len(), LINE_MAX); // 尾段前块
        assert!(t.input_block().len() + 1 <= LINE_MAX); // 末块 + 光标
    }

    /// 清屏后状态归零：视图/尾段/输入块全空。
    #[test]
    fn clear_resets_state() {
        let mut t = TermCore::new();
        t.feed(b"fill\nmore\x1b[2J");
        for vi in 0..TERM_ROWS {
            assert_eq!(t.view_row(vi), b"");
        }
        assert_eq!(t.tail(), b"");
        assert_eq!(t.input_block(), b"");
    }

    /// ESC 状态机跨 feed 批次持久（序列字节分批到达）。
    #[test]
    fn esc_state_persists_across_batches() {
        let mut t = TermCore::new();
        t.feed(b"gone\n");
        t.feed(b"\x1b");
        t.feed(b"[2");
        t.feed(b"J");
        assert_eq!(t.view_row(0), b"");
        // 分批的 ESC[H 也不漏字节进尾段：
        t.feed(b"\x1b[");
        t.feed(b"Hok");
        assert_eq!(t.tail(), b"ok");
    }

    /// `^C\n` 回显（Ctrl-C 作废行）：`^C` 追加在被中断行尾成行（真终端
    /// 同形——`bsh> draft^C` 一行）、prompt 重显驻尾段（喂流 = 子回显字节
    /// ——0x03 本身不被子回显，词汇表只有 "^C\n"）。
    #[test]
    fn caret_c_line_recorded() {
        let mut t = TermCore::new();
        t.feed(b"bsh> draft");
        // Interrupt 回显 "^C\n" + prompt 重显 "bsh> "。
        t.feed(b"^C\nbsh> ");
        assert_eq!(t.view_row(0), b"bsh> draft^C"); // ^C 收尾的中断行
        assert_eq!(t.view_row(1), b"");
        assert_eq!(t.tail(), b"bsh> ");
    }

    // —— triage 纯面（2026-11-20 超时会流分诊刀 P4 测试族：九键回归/
    // 溢出丢新/保序负证/忙段提示/排空全链/子死清零/死判收敛表）——

    use crate::triage::{
        Backlog, DeathEntry, DeathVerdict, KeyAct, Liveness, SendFeed, CHILD_BUSY_HINT_NS,
        KEYPEND_CAP,
    };

    /// ① 九键回归（病灶一钉板）：通道深 8——8 键 Sent 直达、第 9 键
    /// Full 转缓**不判死**；零 Dropped；busy_since 置位（hint_at 在席）；
    /// 排空毕 episode 回报零丢键、忙段复位。
    #[test]
    fn backlog_nine_keys_full_turns_to_buffer() {
        let mut bl = Backlog::new();
        let t = 1_000u64;
        for i in 0..8u8 {
            assert_eq!(bl.on_key(SendFeed::Sent, b'a' + i, t), KeyAct::Delivered);
        }
        assert_eq!(bl.pending_len(), 0);
        assert_eq!(bl.hint_at(), None); // 未开忙段
        // 第 9 键：满队（CHAN_DEPTH=8 已积压）→ 入缓开忙段——EBUSY 非
        // 死信号（G5），本状态机无任何死判出口。
        assert_eq!(bl.on_key(SendFeed::Full, b'z', t), KeyAct::Buffered);
        assert_eq!(bl.pending_len(), 1);
        assert_eq!(bl.hint_at(), Some(t + CHILD_BUSY_HINT_NS)); // busy_since 置位
        // 排空：单键送达、episode 回报零丢键、忙段复位。
        assert_eq!(bl.next_pending(), Some(b'z'));
        assert_eq!(bl.on_sent(), Some(0));
        assert_eq!(bl.next_pending(), None);
        assert_eq!(bl.hint_at(), None);
    }

    /// ② 溢出丢新不丢旧：CAP+5 键全 Full → 前 CAP 入缓、后 5
    /// DroppedNew；排空 FIFO 序断言（0..=255 逐字节＝入缓序）＋排空毕
    /// episode 回报 dropped=5（④ 的丢键>0 面）。
    #[test]
    fn backlog_overflow_drops_new_keeps_fifo() {
        let mut bl = Backlog::new();
        for i in 0..KEYPEND_CAP {
            assert_eq!(bl.on_key(SendFeed::Full, i as u8, 0), KeyAct::Buffered);
        }
        for k in 0..5u8 {
            assert_eq!(bl.on_key(SendFeed::Full, 0xF0 | k, 0), KeyAct::DroppedNew);
        }
        assert_eq!(bl.pending_len(), KEYPEND_CAP);
        // 排空全链：FIFO 序＝入缓序（丢新不丢旧——旧键 0..=255 全在）；
        // 末键送达后回报本忙段丢新总数 5。
        for i in 0..KEYPEND_CAP {
            assert_eq!(bl.next_pending(), Some(i as u8));
            let last = i + 1 == KEYPEND_CAP;
            assert_eq!(bl.on_sent(), if last { Some(5) } else { None });
        }
        assert_eq!(bl.next_pending(), None);
        assert_eq!(bl.hint_at(), None); // 忙段复位
    }

    /// ②′ 保序不变量负证：pending 非空时 Sent 不旁路（误喂 Sent 也入缓
    /// ——模型无旁路口）；LinkErr 不进 Backlog（防御＝丢新键、零状态
    /// 污染——不开忙段不动计数）。
    #[test]
    fn backlog_no_bypass_when_pending_and_linkerr_rejected() {
        let mut bl = Backlog::new();
        assert_eq!(bl.on_key(SendFeed::Full, b'x', 7), KeyAct::Buffered);
        // pending 非空时误喂 Sent（调用方违约形态）：不 Delivered、入缓
        // ——排空序 x 先 y 后（不靠时序假设的结构性保序）。
        assert_eq!(bl.on_key(SendFeed::Sent, b'y', 7), KeyAct::Buffered);
        assert_eq!(bl.pending_len(), 2);
        assert_eq!(bl.next_pending(), Some(b'x'));
        assert_eq!(bl.on_sent(), None);
        assert_eq!(bl.next_pending(), Some(b'y'));
        assert_eq!(bl.on_sent(), Some(0));
        // LinkErr 防御：丢新键、零状态污染（随后键流全新态起算）。
        let mut bl2 = Backlog::new();
        assert_eq!(bl2.on_key(SendFeed::LinkErr, b'e', 7), KeyAct::DroppedNew);
        assert_eq!(bl2.pending_len(), 0);
        assert_eq!(bl2.hint_at(), None);
        assert_eq!(bl2.on_key(SendFeed::Sent, b'k', 7), KeyAct::Delivered);
    }

    /// ③ 忙段提示恰一次：阈值前不出、到点出行附在缓键数、出行后不再出
    ///（hint_at 同步熄灭）；排空复位后再忙段再提示（新忙段新起点）。
    #[test]
    fn backlog_hint_once_per_episode_rearms_after_drain() {
        let mut bl = Backlog::new();
        let t0 = 100u64;
        assert_eq!(bl.on_key(SendFeed::Full, b'1', t0), KeyAct::Buffered);
        assert_eq!(bl.on_key(SendFeed::Full, b'2', t0), KeyAct::Buffered);
        assert_eq!(bl.hint_due(t0 + CHILD_BUSY_HINT_NS - 1), None);
        assert_eq!(bl.hint_due(t0 + CHILD_BUSY_HINT_NS), Some(2)); // 附 pending 数
        assert_eq!(bl.hint_due(t0 + CHILD_BUSY_HINT_NS + 1), None); // 恰一次
        assert_eq!(bl.hint_at(), None); // 已出行不再给 deadline
        // 排空复位（末键送达＝忙段毕）。
        assert_eq!(bl.next_pending(), Some(b'1'));
        assert_eq!(bl.on_sent(), None);
        assert_eq!(bl.next_pending(), Some(b'2'));
        assert_eq!(bl.on_sent(), Some(0));
        // 再忙段（新起点 t1）：再提示。
        let t1 = t0 + 60 * 1_000_000_000;
        assert_eq!(bl.on_key(SendFeed::Full, b'3', t1), KeyAct::Buffered);
        assert_eq!(bl.hint_due(t1 + CHILD_BUSY_HINT_NS - 1), None);
        assert_eq!(bl.hint_due(t1 + CHILD_BUSY_HINT_NS), Some(1));
    }

    /// ④ 排空全链（中段续键对账）：300 键溢出 44 后排空半程、中途续入
    /// 10 键（pending 非空一律入缓 FIFO 尾接——Sent 亦不旁路）、续排空
    /// ——episode 回报＝本忙段丢新总数 44，到达序＝入缓序全链一致。
    #[test]
    fn backlog_drain_chain_reports_episode_drops_with_mid_arrivals() {
        let mut bl = Backlog::new();
        for i in 0..300usize {
            let act = bl.on_key(SendFeed::Full, (i % 256) as u8, 0);
            if i < KEYPEND_CAP {
                assert_eq!(act, KeyAct::Buffered);
            } else {
                assert_eq!(act, KeyAct::DroppedNew);
            }
        }
        let mut received = Vec::new();
        for _ in 0..128 {
            received.push(bl.next_pending().unwrap());
            assert_eq!(bl.on_sent(), None); // 半程无 episode 回报
        }
        // 中途续键 10 枚：pending 非空——Sent 亦入缓（尾接不插队）。
        for k in 0..10u8 {
            assert_eq!(bl.on_key(SendFeed::Sent, b'0' + k, 0), KeyAct::Buffered);
        }
        while let Some(b) = bl.next_pending() {
            received.push(b);
            let report = bl.on_sent();
            if bl.pending_len() == 0 {
                assert_eq!(report, Some(44)); // 排空毕＝本忙段丢新总数
            } else {
                assert_eq!(report, None);
            }
        }
        let mut expect: Vec<u8> = (0..KEYPEND_CAP).map(|i| i as u8).collect();
        expect.extend((0..10u8).map(|k| b'0' + k));
        assert_eq!(received, expect); // 到达序＝入缓序（续键尾接）
        assert_eq!(bl.on_sent(), None); // 空缓无操作
    }

    /// ⑤ 子死清零：on_child_dead 回报在缓键数并全态复位（respawn 整体
    /// 替换的显式面——新会话不继承旧键）；清零后新键从零态起。
    #[test]
    fn backlog_child_dead_reports_pending_and_wipes() {
        let mut bl = Backlog::new();
        for k in 0..5u8 {
            assert_eq!(bl.on_key(SendFeed::Full, b'a' + k, 0), KeyAct::Buffered);
        }
        assert_eq!(bl.on_child_dead().0, 5);
        assert_eq!(bl.pending_len(), 0);
        assert_eq!(bl.next_pending(), None);
        assert_eq!(bl.hint_at(), None);
        assert_eq!(bl.hint_due(u64::MAX), None);
        // 清零后：Sent 直达（零残留）、新忙段全新起算。
        assert_eq!(bl.on_key(SendFeed::Sent, b'z', 0), KeyAct::Delivered);
        assert_eq!(bl.on_key(SendFeed::Full, b'w', 9), KeyAct::Buffered);
        assert_eq!(bl.hint_at(), Some(9 + CHILD_BUSY_HINT_NS));
    }

    /// ⑥ 死判收敛表（四入口全 TerminateThenReap 单路）：L1 EOF／L3
    /// stdout 臂／L3 键路臂×死询两标——Liveness 仅诊断不改变收敛
    ///（G5／裁决 5 表测执法）。
    #[test]
    fn death_entry_table_all_converge_terminate_then_reap() {
        let table = [
            DeathEntry::StdoutEof,
            DeathEntry::StdoutErr,
            DeathEntry::SendLinkErr(Liveness::Terminated),
            DeathEntry::SendLinkErr(Liveness::Alive),
        ];
        for e in &table {
            assert_eq!(e.converge(), DeathVerdict::TerminateThenReap);
        }
    }
}
