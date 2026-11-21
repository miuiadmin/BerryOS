//! berry-lex —— bsh 词法单源纯函数面（bsh v2.5 引号与统一分词器刀，刀谱
//! .agents/plans/2026-11-21-bsh引号与统一分词器v2.5刀实施设计.md 契约 L）。
//! 单遍零回溯扫描产扁平 Item 流：Word〔Piece 段列——字面/引号/变量
//! provenance 逐段携带，混合词 `"$V"x`/`a"b c"d 的 POSIX 逐段语义免二次
//! 改造〕＋Op〔`&&`/`||`/`|`/`>`/`>>`，**仅未引号区识别**〕＋Sep〔未引号
//! 空白原字节保留——render 保距的载体〕；配三视图纯函数与 strip_bg 引号
//! 感知版。bsh（musl 独立小工作区 path 依赖）与 3.0 bsh-syntax 共用的词法
//! 层单源——本扫描核即 3.0 Lexer 词法层子集（同核两层消费，禁平行第二套
//! 扫描器）。
//!
//! `cfg_attr(not(test), no_std)` + `extern crate alloc`（berry-jobs/
//! berry-complete/berry-poll 镜像形态）：库本体 no_std（workspace
//! none-target 全量构建可编），test 构建放开 std 仅为测试挂具；std 消费侧
//! （bsh，musl target）alloc 由 std 提供。零 syscall/零 IO/零 unsafe。
//! host 测试经 justfile test-host 显式行跑。
//!
//! 反注入纪律（脊，docs/shell设计 操作符识别纪律的引号外化保持法）：五族
//! 操作符只在未引号区成 token；展开产物永为数据——fields 的无引号展开只
//! 分裂字段、永不产 Op（`P=a|b` 后 `echo $P` 的 items 零 Op）。
//!
//! 五件原语：
//! - [`lex`]：单遍扫描 → Item 流（未闭合引号 `Err(UnclosedQuote)`）；
//! - [`strip_bg`]：行尾独立 `&` 剥离（引号感知——引号区内尾 `&` 不剥；
//!   返回契约与 berry-jobs 盲版逐字节同，良构输入盲/感知等价）；
//! - [`render`]：剥引号＋`$NAME`/`$?`/`$!` 展开**不分词**保距（bsh expand
//!   扫描律字节级移植——无引号输入下逐字节恒等，恒等契约执法点）；
//! - [`fields`]：argv 视图（无引号展开按空白分裂＋邻接粘连＋Op 融合字面
//!   并入——`a>|b` 段名 `a>`、`a | b > c` 段参 `>,c` 保距）；
//! - [`word_texts`]：剥引号不展开（Var/Status/Bang 还源文本 `$NAME`/`$?`/
//!   `$!` 字面——unset 名域/赋值名检测消费）。

#![cfg_attr(not(test), no_std)]
extern crate alloc;

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

/// 链/管道/重定向操作符（仅未引号区识别——引号内字节全是数据）。
/// 字面文本由 [`Op::text`] 派生（render/fields 融合的单源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `&&`：前段成功才执行。
    And,
    /// `||`：前段失败才执行。
    Or,
    /// `|`：管道。
    Pipe,
    /// `>`：重定向（覆盖写）。
    Redirect,
    /// `>>`：重定向（追加写）。
    Append,
}

impl Op {
    /// 源文本字面（maximal munch 识别的逆派生）。
    pub fn text(self) -> &'static str {
        match self {
            Op::And => "&&",
            Op::Or => "||",
            Op::Pipe => "|",
            Op::Redirect => ">",
            Op::Append => ">>",
        }
    }
}

/// 引号 provenance（词内逐段携带——`Piece` 的属主字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quote {
    /// 未引号（展开拆词、操作符/空白语法位生效）。
    None,
    /// 单引号（一切字面——含 `$` 与操作符字节）。
    Single,
    /// 双引号（`$NAME`/`$?`/`$!` 展开禁分词＋仅 `\$` `\"` `\\` 三转义）。
    Double,
}

/// 词内一段（provenance 逐段携带）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    /// 字面段。`text` 为剥引号后的字节（双引号转义已还原）；空引号 `""`/
    /// `''` 落 `text: ""`＋quote 非 None——fields 空字段保留的标记载体。
    Lit {
        /// 字面字节。
        text: String,
        /// 段的引号 provenance。
        quote: Quote,
    },
    /// `$NAME`（Single 区不产此形——那里 `$` 是字面）。
    Var {
        /// 变量名（`[A-Za-z_][A-Za-z0-9_]*` 贪名扫描产物）。
        name: String,
        /// 段的引号 provenance。
        quote: Quote,
    },
    /// `$?`（上一命令状态码）。
    Status {
        /// 段的引号 provenance。
        quote: Quote,
    },
    /// `$!`（最后一次后台作业号；未设＝<0 → 展开空串，镜像 bg_last 语义）。
    Bang {
        /// 段的引号 provenance。
        quote: Quote,
    },
}

/// 词（相邻无 Sep 的 Piece 并入同 Word——POSIX 相邻拼接）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    /// 段列（源序）。
    pub pieces: Vec<Piece>,
}

/// Item 流元素（扁平——嵌套结构属 3.0 Parser 域，本层不前渗）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// 词。
    Word(Word),
    /// 操作符（仅未引号区产出）。
    Op(Op),
    /// 未引号空白（原字节保留——render 保距的载体）。
    Sep(String),
}

/// 词法产物（扁平 Item 流）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lexed {
    /// Item 流（源序）。
    pub items: Vec<Item>,
}

/// 词法错误（单行拒——bsh 打 `bsh: unclosed quote`＋status 2；续行 3.0
/// berry-line）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexError {
    /// 行尾仍在引号区内（单/双引号未闭合）。
    UnclosedQuote,
}

/// 三态机内部态（Single/Double 内一切字节字面化或产展开段；Double 内
/// `\` 有转义旗语义）。
#[derive(Clone, Copy, PartialEq)]
enum State {
    Normal,
    Single,
    Double,
}

/// 词法输出积攒器。落序纪律：Sep 先于其后词内首段（源序）；Op/空白断词；
/// 字面游程按引号态切段（provenance 逐段）。引号区内容旗：空引号 `""`/`''`
/// 闭区时补 `Lit{"",q}` 空段标记（fields 空字段保留的载体），有内容则不补
/// ——`"$V"` 单 Var 段干净形态。
#[derive(Default)]
struct Out {
    items: Vec<Item>,
    pieces: Vec<Piece>,
    lit: String,
    lit_q: Option<Quote>,
    sep: String,
    region_content: bool,
}

impl Out {
    /// 物化当前字面游程为 Lit 段（空游程且开着也物化）。
    fn lit_flush(&mut self) {
        if let Some(q) = self.lit_q.take() {
            self.pieces.push(Piece::Lit {
                text: core::mem::take(&mut self.lit),
                quote: q,
            });
        }
    }
    /// 引号开区：落 pending Sep＋清内容旗（不开字面游程——首段由内容 lazy 开）。
    fn region_open(&mut self) {
        self.sep_flush();
        self.region_content = false;
    }
    /// 引号闭区：物化字面游程；区内零内容补 `Lit{"",q}` 空段（空引号语义）。
    fn region_close(&mut self, q: Quote) {
        self.lit_flush();
        if !self.region_content {
            self.pieces.push(Piece::Lit {
                text: String::new(),
                quote: q,
            });
        }
    }
    /// 开一段字面游程。词内容到达先落 pending Sep（源序：Sep 先于其后词内
    /// 首段）。
    fn lit_begin(&mut self, q: Quote) {
        self.sep_flush();
        self.region_content = true;
        if self.lit_q != Some(q) {
            self.lit_flush();
            self.lit_q = Some(q);
        }
    }
    /// 追加字面字节（同引号态续游程、异态/闭态切段——provenance 逐段）。
    fn lit_push(&mut self, s: &str, q: Quote) {
        self.lit_begin(q);
        self.lit.push_str(s);
    }
    fn sep_flush(&mut self) {
        if !self.sep.is_empty() {
            self.items
                .push(Item::Sep(core::mem::take(&mut self.sep)));
        }
    }
    /// 断词（物化字面游程＋落 Word；Sep 不动——空白游程续积）。
    fn word_end(&mut self) {
        self.lit_flush();
        if !self.pieces.is_empty() {
            self.items.push(Item::Word(Word {
                pieces: core::mem::take(&mut self.pieces),
            }));
        }
    }
    /// 空白（Normal 态）：断词＋积累 Sep 原字节。
    fn ws(&mut self, s: &str) {
        self.word_end();
        self.sep.push_str(s);
    }
    /// 操作符（Normal 态）：断词断 Sep，Op 自成 Item。
    fn op(&mut self, op: Op) {
        self.word_end();
        self.sep_flush();
        self.items.push(Item::Op(op));
    }
    /// 展开段（Var/Status/Bang）：断 Sep 断字面游程，Piece 入当前词；区内
    /// 产段计为引号区内容（`"$V"` 不补空段）。
    fn piece(&mut self, p: Piece) {
        self.sep_flush();
        self.lit_flush();
        self.region_content = true;
        self.pieces.push(p);
    }
    fn finish(mut self) -> Vec<Item> {
        self.word_end();
        self.sep_flush();
        self.items
    }
}

/// ASCII 空白字节集（`char::is_whitespace` 的 ASCII 子集；多字节空白走
/// 解 char 支路——Sep 集冻结 `char::is_whitespace`，与现 split_whitespace/
/// trim 同集，零漂优先）。
fn is_ascii_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | 0x0B | 0x0C | b'\r' | b'\n')
}

/// `$` 三形扫描（Normal/Double 共用——bsh expand 扫描律同形：`$NAME` 贪名、
/// `$?`/`$!` 单点、`$` 后非上述字符（或行尾）字面一字节）。返回消费后的
/// 下标。
fn scan_dollar(line: &str, b: &[u8], i: usize, quote: Quote, out: &mut Out) -> usize {
    if i + 1 < b.len() {
        let c = b[i + 1];
        if c == b'?' {
            out.piece(Piece::Status { quote });
            return i + 2;
        }
        if c == b'!' {
            out.piece(Piece::Bang { quote });
            return i + 2;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            // 贪名扫描：[A-Za-z_][A-Za-z0-9_]*（替换产物不重扫——纯数据）。
            let st = i + 1;
            let mut j = i + 2;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            out.piece(Piece::Var {
                name: line[st..j].to_string(),
                quote,
            });
            return j;
        }
    }
    out.lit_push("$", quote);
    i + 1
}

/// 单遍零回溯词法扫描：`line` → 扁平 Item 流。三态状态机：
/// - 单引号区内一切字节字面（含 `"`、`\`、`$` 与操作符字节——Op 永不在
///   区内产出）；
/// - 双引号区内 `$NAME`/`$?`/`$!` 三形产 Piece 携 Double（展开禁分词——
///   fields 视图执法）、仅 `\$` `\"` `\\` 三转义（其余 `\x` 双字节字面）、
///   `'` 与空白字面（引号内空白不是 Sep——`"a b"` 单词）；
/// - 未引号 `\` ＝字面字节（v2.5 裁决 4——恒等契约优先，今日 `echo a\b`
///   输出 `a\b` 零漂；行尾孤立 `\` 亦字面；POSIX 转义挂 3.0）；
/// - 空白积累 Sep；maximal munch 算符：`&&`→And、`||`→Or、`>>`→Append、
///   单 `>`→Redirect、单 `|`→Pipe；单 `&` 非尾缀＝词内字面字节（尾缀剥离
///   归 strip_bg 上游）；
/// - 多字节 UTF-8 解 char 归类（连续字节 ≥0x80 不撞 ASCII 语法位；Normal
///   态 Unicode 空白如 U+3000 同入 Sep）；
/// - 相邻无 Sep 的 Piece 并入同 Word（POSIX 相邻拼接）。
/// EOL 处 Single/Double 未闭 → `Err(UnclosedQuote)`。
pub fn lex(line: &str) -> Result<Lexed, LexError> {
    let b = line.as_bytes();
    let mut out = Out::default();
    let mut state = State::Normal;
    let mut i = 0usize;
    while i < b.len() {
        // 多字节 UTF-8：解 char 后按态归类。
        if b[i] >= 0x80 {
            let ch = line[i..].chars().next().unwrap();
            let end = i + ch.len_utf8();
            if state == State::Normal && ch.is_whitespace() {
                out.ws(&line[i..end]);
            } else {
                out.lit_push(&line[i..end], quote_of(state));
            }
            i = end;
            continue;
        }
        let c = b[i];
        match state {
            State::Single => {
                if c == b'\'' {
                    out.region_close(Quote::Single);
                    state = State::Normal;
                } else {
                    out.lit_push(&line[i..i + 1], Quote::Single);
                }
                i += 1;
            }
            State::Double => {
                if c == b'\\' {
                    // 仅 \$ \" \\ 三转义；其余 \x 双字节字面（本字节先行，
                    // 下一字节走常态并入同游程）。
                    if i + 1 < b.len() && matches!(b[i + 1], b'$' | b'"' | b'\\') {
                        out.lit_push(&line[i + 1..i + 2], Quote::Double);
                        i += 2;
                    } else {
                        out.lit_push("\\", Quote::Double);
                        i += 1;
                    }
                } else if c == b'"' {
                    out.region_close(Quote::Double);
                    state = State::Normal;
                    i += 1;
                } else if c == b'$' {
                    i = scan_dollar(line, b, i, Quote::Double, &mut out);
                } else {
                    // 其余一切（含 ' 与空白）字面。
                    out.lit_push(&line[i..i + 1], Quote::Double);
                    i += 1;
                }
            }
            State::Normal => {
                if is_ascii_ws(c) {
                    out.ws(&line[i..i + 1]);
                    i += 1;
                } else if c == b'\'' {
                    out.region_open();
                    state = State::Single;
                    i += 1;
                } else if c == b'"' {
                    out.region_open();
                    state = State::Double;
                    i += 1;
                } else if c == b'$' {
                    i = scan_dollar(line, b, i, Quote::None, &mut out);
                } else if c == b'&' {
                    if i + 1 < b.len() && b[i + 1] == b'&' {
                        out.op(Op::And);
                        i += 2;
                    } else {
                        // 单 & 词内字面（尾缀归 strip_bg 上游）。
                        out.lit_push("&", Quote::None);
                        i += 1;
                    }
                } else if c == b'|' {
                    if i + 1 < b.len() && b[i + 1] == b'|' {
                        out.op(Op::Or);
                        i += 2;
                    } else {
                        out.op(Op::Pipe);
                        i += 1;
                    }
                } else if c == b'>' {
                    if i + 1 < b.len() && b[i + 1] == b'>' {
                        out.op(Op::Append);
                        i += 2;
                    } else {
                        out.op(Op::Redirect);
                        i += 1;
                    }
                } else {
                    out.lit_push(&line[i..i + 1], Quote::None);
                    i += 1;
                }
            }
        }
    }
    if state != State::Normal {
        return Err(LexError::UnclosedQuote);
    }
    Ok(Lexed {
        items: out.finish(),
    })
}

fn quote_of(state: State) -> Quote {
    match state {
        State::Normal => Quote::None,
        State::Single => Quote::Single,
        State::Double => Quote::Double,
    }
}

/// 行尾独立单 `&` 剥离（引号感知版——berry-jobs 盲版语料整迁）。返回契约
/// 与 berry-jobs 现版逐字节同：false 返**原文切片**（零漂）、true 返剥后
/// trim 口径、循环剥、len 1 裸 `&` 停、前字符 `&` 停。差别仅在引号区——
/// 尾 `&` 在引号区内不剥。良构输入下盲/感知版等价（良构引号区尾恒是闭引号
/// 字节非 `&`；唯未闭合引号行尾 `&` 落区内——该行随后 lex 亦 `UnclosedQuote`
/// 拒，错误面零契约）。
///
/// 引号三形：`echo "a" &`→(`echo "a"`, true)、`echo "&"`→原文 false、
/// `echo "a&`→原文 false（未闭合区内尾 `&` 不剥）。
pub fn strip_bg(line: &str) -> (&str, bool) {
    let mut rest = line.trim();
    let mut bg = false;
    loop {
        let b = rest.as_bytes();
        // len 1 裸 `&` 不剥；前字符为 `&` 即停（`&&` 尾形交链分组）；尾 `&`
        // 须在引号区外（三态扫描——与 lex 状态机同律：未引号 `\` 是字面、
        // 不转义引号）。
        if b.len() >= 2
            && b[b.len() - 1] == b'&'
            && b[b.len() - 2] != b'&'
            && ends_outside_quotes(rest)
        {
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

/// 行末字节是否在引号区外（三态扫描；双引号内 `\` 跳下一字节；未闭合
/// 引号 → 末字节视为区内 → false）。
fn ends_outside_quotes(s: &str) -> bool {
    let b = s.as_bytes();
    let mut state = State::Normal;
    let mut i = 0usize;
    while i < b.len() {
        match state {
            State::Normal => match b[i] {
                b'\'' => state = State::Single,
                b'"' => state = State::Double,
                _ => {}
            },
            State::Single => {
                if b[i] == b'\'' {
                    state = State::Normal;
                }
            }
            State::Double => {
                if b[i] == b'\\' {
                    i += 1; // 转义吃下一字节
                } else if b[i] == b'"' {
                    state = State::Normal;
                }
            }
        }
        i += 1;
    }
    state == State::Normal
}

/// 三视图渲染注入：变量表（**后见者胜**——镜像 bsh get_var rev-find）、
/// `$?` 值、`$!` 值（<0＝未设 → 展开空串，镜像 bg_last 语义）。
pub struct Vars<'a> {
    /// 变量表（同名取最后一次出现）。
    pub table: &'a [(String, String)],
    /// `$?` 状态码。
    pub status: i32,
    /// `$!` 后台作业号（未设传负值）。
    pub bang: i32,
}

fn lookup<'a>(table: &'a [(String, String)], name: &str) -> Option<&'a str> {
    table.iter().rev().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

/// 视图一 render：剥引号＋`$NAME`/`$?`/`$!` 按 Piece 展开且**不分词**
/// 保距——Sep 原字节、Op 字面文本、引号剥除、双引号转义已还原。bsh
/// expand 扫描律字节级移植（未定义空串、贪名扫描、`$` 后非上述字符字面、
/// UTF-8 透传、替换产物不重扫）。**无引号输入下与 bsh expand 逐字节恒等**
/// （恒等契约执法点——echo/赋值/export 值域/not-found 消息消费）。
pub fn render(items: &[Item], vars: &Vars) -> String {
    let mut out = String::new();
    for item in items {
        match item {
            Item::Sep(s) => out.push_str(s),
            Item::Op(op) => out.push_str(op.text()),
            Item::Word(w) => {
                for p in &w.pieces {
                    match p {
                        Piece::Lit { text, .. } => out.push_str(text),
                        Piece::Var { name, .. } => {
                            if let Some(v) = lookup(vars.table, name) {
                                out.push_str(v);
                            }
                        }
                        Piece::Status { .. } => {
                            out.push_str(&vars.status.to_string());
                        }
                        Piece::Bang { .. } => {
                            // 未设（<0）展开空串——镜像 bg_last 语义。
                            if vars.bang >= 0 {
                                out.push_str(&vars.bang.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// 字段积攒器（fields/word_texts 共用骨架——Sep 断字段、Op 融合、无引号
/// 展开分裂、空字段保留四律同源；两视图只差贡献段）。
#[derive(Default)]
struct Fields {
    out: Vec<String>,
    cur: Option<String>,
    cur_quoted: bool,
}

impl Fields {
    /// 贡献一段文本：quoted 段必落字段（空引号保空字段）；未引号空段不
    /// 开字段（lex 不产未引号空 Lit，防御性保留）。
    fn seg(&mut self, s: &str, quoted: bool) {
        if quoted {
            self.cur_quoted = true;
        }
        if s.is_empty() && !quoted {
            return;
        }
        self.cur.get_or_insert_with(String::new).push_str(s);
    }
    /// 无引号展开值的字段分裂：空白游程断字段、非空白游程并入/起字段
    /// （内部分裂断字段、边界片与相邻字面段粘连——POSIX §2.6.5 邻接律）。
    fn split_val(&mut self, val: &str) {
        let mut run = String::new();
        for ch in val.chars() {
            if ch.is_whitespace() {
                if !run.is_empty() {
                    let r = core::mem::take(&mut run);
                    self.seg(&r, false);
                }
                self.close();
            } else {
                run.push(ch);
            }
        }
        if !run.is_empty() {
            let r = run;
            self.seg(&r, false);
        }
    }
    /// 展开段贡献（按 provenance 分道：quoted 不分词整段并入）。
    fn push_expansion(&mut self, val: &str, quote: Quote) {
        if quote == Quote::None {
            self.split_val(val);
        } else {
            self.seg(val, true);
        }
    }
    fn close(&mut self) {
        if let Some(f) = self.cur.take() {
            // 空字段保留：含 quoted 段的空字段保留（空引号语义）；纯无引号
            // 展开致空的字段丢弃（`$空` 消词）。
            if !f.is_empty() || self.cur_quoted {
                self.out.push(f);
            }
        }
        self.cur_quoted = false;
    }
    fn finish(mut self) -> Vec<String> {
        self.close();
        self.out
    }
}

/// 视图二 fields：argv 产位——Sep 断字段；字段内 Piece 串接（Lit 逐字、
/// Var/Status/Bang 展开值），仅 quote==None 的展开值按空白分裂；Op 融合
/// 为字面文本并入无 Sep 邻接字段（无开字段则起字段——`a>|b` 管道组一段名
/// `a>`、`a | b > c` 二段参 `[>,c]` 保距）。纯无引号展开致空的字段丢弃、
/// 含 quoted Piece 的空字段保留（空引号——argv 收空串）。
pub fn fields(items: &[Item], vars: &Vars) -> Vec<String> {
    let mut fb = Fields::default();
    for item in items {
        match item {
            Item::Sep(_) => fb.close(),
            Item::Op(op) => {
                let t = op.text();
                fb.seg(t, false);
            }
            Item::Word(w) => {
                for p in &w.pieces {
                    match p {
                        Piece::Lit { text, quote } => {
                            fb.seg(text, *quote != Quote::None);
                        }
                        Piece::Var { name, quote } => {
                            let v = lookup(vars.table, name).unwrap_or("");
                            fb.push_expansion(v, *quote);
                        }
                        Piece::Status { quote } => {
                            let v = vars.status.to_string();
                            fb.push_expansion(&v, *quote);
                        }
                        Piece::Bang { quote } => {
                            let v = if vars.bang >= 0 {
                                vars.bang.to_string()
                            } else {
                                String::new()
                            };
                            fb.push_expansion(&v, *quote);
                        }
                    }
                }
            }
        }
    }
    fb.finish()
}

/// 视图三 word_texts：剥引号、Var/Status/Bang **还原源文本**（`$NAME`/
/// `$?`/`$!` 字面）不展开。字段切分/Op 融合/空字段保留三律与 fields 同
/// 源（unset 名列/export 分类/赋值名检测消费——`unset $A` 字面名 `$A`
/// 零漂、`unset 'A'` 剥引号新增可用）。
pub fn word_texts(items: &[Item]) -> Vec<String> {
    let mut fb = Fields::default();
    for item in items {
        match item {
            Item::Sep(_) => fb.close(),
            Item::Op(op) => {
                let t = op.text();
                fb.seg(t, false);
            }
            Item::Word(w) => {
                for p in &w.pieces {
                    match p {
                        Piece::Lit { text, quote } => {
                            fb.seg(text, *quote != Quote::None);
                        }
                        Piece::Var { name, quote } => {
                            let mut s = String::from("$");
                            s.push_str(name);
                            fb.seg(&s, *quote != Quote::None);
                        }
                        Piece::Status { quote } => {
                            fb.seg("$?", *quote != Quote::None);
                        }
                        Piece::Bang { quote } => {
                            fb.seg("$!", *quote != Quote::None);
                        }
                    }
                }
            }
        }
    }
    fb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    // —— 期望值构造小件 ——
    fn lit(t: &str, q: Quote) -> Piece {
        Piece::Lit {
            text: t.into(),
            quote: q,
        }
    }
    fn lw(t: &str) -> Item {
        Item::Word(Word {
            pieces: vec![lit(t, Quote::None)],
        })
    }
    fn word(pieces: Vec<Piece>) -> Item {
        Item::Word(Word { pieces })
    }
    fn sep(t: &str) -> Item {
        Item::Sep(t.into())
    }
    fn op(o: Op) -> Item {
        Item::Op(o)
    }
    fn var(n: &str, q: Quote) -> Piece {
        Piece::Var {
            name: n.into(),
            quote: q,
        }
    }
    fn st(q: Quote) -> Piece {
        Piece::Status { quote: q }
    }
    fn bg(q: Quote) -> Piece {
        Piece::Bang { quote: q }
    }
    fn lx(s: &str) -> Vec<Item> {
        lex(s).expect("lex 不应报错").items
    }
    /// 按 Op 切组（链分组/管道分组的 bsh 消费面形状——测试侧镜像）。
    fn groups_at(items: &[Item], o: Op) -> Vec<&[Item]> {
        let mut out = Vec::new();
        let mut start = 0;
        for (i, it) in items.iter().enumerate() {
            if let Item::Op(x) = it {
                if *x == o {
                    out.push(&items[start..i]);
                    start = i + 1;
                }
            }
        }
        out.push(&items[start..]);
        out
    }
    /// 首个重定向族 Op 之前的组（do_redirect cmd 侧——空命令探针消费面）。
    fn before_redirect(items: &[Item]) -> &[Item] {
        let idx = items
            .iter()
            .position(|it| matches!(it, Item::Op(Op::Redirect) | Item::Op(Op::Append)))
            .expect("应含重定向族 Op");
        &items[..idx]
    }
    fn table() -> Vec<(String, String)> {
        vec![
            ("A".to_string(), "va".to_string()),
            ("V".to_string(), "a b".to_string()),
            ("W".to_string(), "  b  ".to_string()),
            ("EMPTY".to_string(), String::new()),
            ("PIPE".to_string(), "a|b".to_string()),
        ]
    }
    fn vs<'a>(t: &'a [(String, String)]) -> Vars<'a> {
        Vars {
            table: t,
            status: 7,
            bang: 3,
        }
    }

    /// bsh expand（main.rs :3500 扫描律）测试侧镜像——恒等律对拍基线（`$!`
    /// 参数化为 bang；get_var rev-find 同律）。无引号输入下 render 必须与之
    /// 逐字节同。
    fn expand_ref(s: &str, table: &[(String, String)], status: i32, bang: i32) -> String {
        let b = s.as_bytes();
        let mut out: Vec<u8> = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'$' && i + 1 < b.len() {
                let c = b[i + 1];
                if c == b'?' {
                    out.extend_from_slice(status.to_string().as_bytes());
                    i += 2;
                    continue;
                }
                if c == b'!' {
                    if bang >= 0 {
                        out.extend_from_slice(bang.to_string().as_bytes());
                    }
                    i += 2;
                    continue;
                }
                if c.is_ascii_alphabetic() || c == b'_' {
                    let (st, mut j) = (i + 1, i + 2);
                    while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                        j += 1;
                    }
                    if let Some((_, v)) = table.iter().rev().find(|(k, _)| k.as_str() == &s[st..j])
                    {
                        out.extend_from_slice(v.as_bytes());
                    }
                    i = j;
                    continue;
                }
            }
            out.push(b[i]);
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    // ======================== lex：三态机全边缘 ========================

    #[test]
    fn lex_two_words() {
        assert_eq!(lx("echo hi"), vec![lw("echo"), sep(" "), lw("hi")]);
        assert_eq!(lx("a"), vec![lw("a")]);
    }

    #[test]
    fn lex_ws_run_raw_preserved() {
        // Sep 持原空白字节——render 保距（`foo   bar` not-found 消息距前置）。
        assert_eq!(lx("foo   bar"), vec![lw("foo"), sep("   "), lw("bar")]);
        assert_eq!(lx("a\tb"), vec![lw("a"), sep("\t"), lw("b")]);
    }

    #[test]
    fn lex_leading_trailing_sep() {
        assert_eq!(lx(" a "), vec![sep(" "), lw("a"), sep(" ")]);
        assert_eq!(lx(""), Vec::<Item>::new());
        assert_eq!(lx("  "), vec![sep("  ")]);
    }

    #[test]
    fn lex_ops_chain() {
        assert_eq!(
            lx("a && b"),
            vec![lw("a"), sep(" "), op(Op::And), sep(" "), lw("b")]
        );
        assert_eq!(lx("a||b"), vec![lw("a"), op(Op::Or), lw("b")]);
    }

    #[test]
    fn lex_ops_redirect() {
        assert_eq!(
            lx("a | b"),
            vec![lw("a"), sep(" "), op(Op::Pipe), sep(" "), lw("b")]
        );
        assert_eq!(lx("a>b"), vec![lw("a"), op(Op::Redirect), lw("b")]);
        assert_eq!(lx("a>>b"), vec![lw("a"), op(Op::Append), lw("b")]);
        assert_eq!(
            lx("a >> b"),
            vec![lw("a"), sep(" "), op(Op::Append), sep(" "), lw("b")]
        );
    }

    #[test]
    fn lex_maximal_munch_triples() {
        // `>>>`＝Append+Redirect（消费层重定向族 ≥2 → usage 2 同观测）；
        // `a|||b` 余段 `|b`（今日 split_chains/split 同形）；`a&&&b` 余段 `&b`。
        assert_eq!(
            lx("a>>>b"),
            vec![lw("a"), op(Op::Append), op(Op::Redirect), lw("b")]
        );
        assert_eq!(
            lx("a|||b"),
            vec![lw("a"), op(Op::Or), op(Op::Pipe), lw("b")]
        );
        assert_eq!(
            lx("a&&&b"),
            vec![lw("a"), op(Op::And), word(vec![lit("&b", Quote::None)])]
        );
    }

    #[test]
    fn lex_single_amp_word_literal() {
        // 单 & 非尾缀＝词内字面（`a&b`/`run a&b` 零漂——strip_bg 上游不剥尾）。
        assert_eq!(lx("a&b"), vec![word(vec![lit("a&b", Quote::None)])]);
        assert_eq!(
            lx("run a&b"),
            vec![lw("run"), sep(" "), word(vec![lit("a&b", Quote::None)])]
        );
    }

    #[test]
    fn lex_bare_amp_between_words() {
        // `a & b` 今日 & 字面进裸名参数——保持（三词）。
        assert_eq!(
            lx("a & b"),
            vec![
                lw("a"),
                sep(" "),
                word(vec![lit("&", Quote::None)]),
                sep(" "),
                lw("b")
            ]
        );
    }

    #[test]
    fn lex_single_quote_literal_all() {
        // 单引号字面一切：操作符字节、$、双引号、反斜杠、空白全字面；零 Op 零 Sep。
        assert_eq!(
            lx(r#"'a|b$"\ &'"#),
            vec![word(vec![lit(r#"a|b$"\ &"#, Quote::Single)])]
        );
    }

    #[test]
    fn lex_single_quote_space_one_word() {
        // 引号内空白不是 Sep——`'a b'` 单词。
        assert_eq!(lx("'a b'"), vec![word(vec![lit("a b", Quote::Single)])]);
        assert_eq!(
            lx("echo 'a b'"),
            vec![lw("echo"), sep(" "), word(vec![lit("a b", Quote::Single)])]
        );
    }

    #[test]
    fn lex_double_quote_expansion_pieces() {
        // 双引号内 $NAME/$?/$! 三形产 Piece 携 Double；字面段与展开段分 Piece。
        assert_eq!(lx(r#""$V""#), vec![word(vec![var("V", Quote::Double)])]);
        assert_eq!(
            lx(r#""a$V b""#),
            vec![word(vec![
                lit("a", Quote::Double),
                var("V", Quote::Double),
                lit(" b", Quote::Double)
            ])]
        );
        assert_eq!(lx(r#""$?""#), vec![word(vec![st(Quote::Double)])]);
        assert_eq!(lx(r#""$!""#), vec![word(vec![bg(Quote::Double)])]);
        assert_eq!(
            lx(r#""RC=$? x""#),
            vec![word(vec![
                lit("RC=", Quote::Double),
                st(Quote::Double),
                lit(" x", Quote::Double)
            ])]
        );
    }

    #[test]
    fn lex_double_quote_dollar_literal() {
        // $ 后非名/$?/$! → 字面（expand 扫描律同形）；区内外一致。
        assert_eq!(lx(r#""$5 x""#), vec![word(vec![lit("$5 x", Quote::Double)])]);
        assert_eq!(lx(r#""$$""#), vec![word(vec![lit("$$", Quote::Double)])]);
        assert_eq!(lx(r#""end$""#), vec![word(vec![lit("end$", Quote::Double)])]);
        assert_eq!(lx("$5"), vec![word(vec![lit("$5", Quote::None)])]);
    }

    #[test]
    fn lex_double_quote_three_escapes() {
        // 仅 \$ \" \\ 三转义（还原为单字节、携 Double）。
        assert_eq!(lx(r#""\$""#), vec![word(vec![lit("$", Quote::Double)])]);
        assert_eq!(lx(r#""\"""#), vec![word(vec![lit("\"", Quote::Double)])]);
        assert_eq!(lx(r#""\\""#), vec![word(vec![lit("\\", Quote::Double)])]);
        assert_eq!(
            lx(r#""a\$b\"c\\d""#),
            vec![word(vec![lit("a$b\"c\\d", Quote::Double)])]
        );
    }

    #[test]
    fn lex_double_quote_other_backslash_two_bytes() {
        // 其余 \x 双字节字面（\n 非换行——\a/\n/\'/\空格 四形钉板）。
        assert_eq!(lx(r#""\a""#), vec![word(vec![lit("\\a", Quote::Double)])]);
        assert_eq!(lx(r#""\n""#), vec![word(vec![lit("\\n", Quote::Double)])]);
        assert_eq!(lx(r#""\'""#), vec![word(vec![lit("\\'", Quote::Double)])]);
        assert_eq!(
            lx(r#""\ ""#),
            vec![word(vec![lit("\\ ", Quote::Double)])]
        );
    }

    #[test]
    fn lex_mixed_word_piece_provenance() {
        // 相邻无 Sep 并入同 Word——混合词逐段 provenance（POSIX 相邻拼接）。
        assert_eq!(
            lx("a\"b c\"d"),
            vec![word(vec![
                lit("a", Quote::None),
                lit("b c", Quote::Double),
                lit("d", Quote::None)
            ])]
        );
        assert_eq!(
            lx("\"$V\"x"),
            vec![word(vec![var("V", Quote::Double), lit("x", Quote::None)])]
        );
        assert_eq!(
            lx("'a'$V"),
            vec![word(vec![lit("a", Quote::Single), var("V", Quote::None)])]
        );
    }

    #[test]
    fn lex_empty_quotes() {
        // 空引号产 Lit{"",q} 空段——fields 空字段保留的标记载体。
        assert_eq!(lx("\"\""), vec![word(vec![lit("", Quote::Double)])]);
        assert_eq!(lx("''"), vec![word(vec![lit("", Quote::Single)])]);
        assert_eq!(
            lx("a\"\"b"),
            vec![word(vec![
                lit("a", Quote::None),
                lit("", Quote::Double),
                lit("b", Quote::None)
            ])]
        );
        assert_eq!(
            lx("\"\" ''"),
            vec![
                word(vec![lit("", Quote::Double)]),
                sep(" "),
                word(vec![lit("", Quote::Single)])
            ]
        );
    }

    #[test]
    fn lex_unclosed_double() {
        assert_eq!(lex("echo \"abc"), Err(LexError::UnclosedQuote));
        assert_eq!(lex("\""), Err(LexError::UnclosedQuote));
        assert_eq!(lex("a\"b c"), Err(LexError::UnclosedQuote));
    }

    #[test]
    fn lex_unclosed_single() {
        assert_eq!(lex("echo 'abc"), Err(LexError::UnclosedQuote));
        assert_eq!(lex("'"), Err(LexError::UnclosedQuote));
    }

    #[test]
    fn lex_unclosed_escape_at_eol() {
        // 转义吃掉闭引号后仍未闭合；单引号内 \ 是字面、EOL 未闭照拒。
        assert_eq!(lex("\"a\\"), Err(LexError::UnclosedQuote));
        assert_eq!(lex("\"a\\\""), Err(LexError::UnclosedQuote));
        assert_eq!(lex("'a\\"), Err(LexError::UnclosedQuote));
    }

    #[test]
    fn lex_unquoted_backslash_literal() {
        // v2.5 裁决 4：未引号 \ ＝字面字节（恒等契约——`echo a\b` 输出 `a\b`
        // 零漂；行尾孤立 \ 亦字面、不报错）。
        assert_eq!(lx("a\\b"), vec![word(vec![lit("a\\b", Quote::None)])]);
        assert_eq!(lx("a\\"), vec![word(vec![lit("a\\", Quote::None)])]);
        assert_eq!(
            lx("echo a\\b"),
            vec![lw("echo"), sep(" "), word(vec![lit("a\\b", Quote::None)])]
        );
    }

    #[test]
    fn lex_dollar_greedy_name() {
        // 贪名扫描 [A-Za-z_][A-Za-z0-9_]*。
        assert_eq!(lx("$A_1b"), vec![word(vec![var("A_1b", Quote::None)])]);
        assert_eq!(lx("$_x9 y"), vec![word(vec![var("_x9", Quote::None)]), sep(" "), lw("y")]);
        assert_eq!(
            lx("$A x"),
            vec![word(vec![var("A", Quote::None)]), sep(" "), lw("x")]
        );
    }

    #[test]
    fn lex_dollar_non_name_literal() {
        assert_eq!(lx("$"), vec![word(vec![lit("$", Quote::None)])]);
        assert_eq!(lx("$$"), vec![word(vec![lit("$$", Quote::None)])]);
        // $ 后多字节字符：$ 字面（连续字节不撞 ASCII 语法位）。
        assert_eq!(lx("$中"), vec![word(vec![lit("$中", Quote::None)])]);
    }

    #[test]
    fn lex_utf8_words() {
        assert_eq!(
            lx("echo 中文 测试"),
            vec![lw("echo"), sep(" "), lw("中文"), sep(" "), lw("测试")]
        );
    }

    #[test]
    fn lex_utf8_ws_sep() {
        // Sep 集冻结 char::is_whitespace：Unicode 空白（U+3000/U+00A0）同入
        // Sep——与现 split_whitespace 同集零漂。
        assert_eq!(lx("a\u{3000}b"), vec![lw("a"), sep("\u{3000}"), lw("b")]);
        assert_eq!(lx("a\u{00a0}b"), vec![lw("a"), sep("\u{00a0}"), lw("b")]);
        // 引号区内 Unicode 空白是字面（禁分词）。
        assert_eq!(
            lx("\"a\u{3000}b\""),
            vec![word(vec![lit("a\u{3000}b", Quote::Double)])]
        );
    }

    #[test]
    fn lex_quote_interleave() {
        // 三区互嵌：双引号内 '、单引号内 "、多区相邻拼接。
        assert_eq!(lx("\"it's\""), vec![word(vec![lit("it's", Quote::Double)])]);
        assert_eq!(lx("'\"'"), vec![word(vec![lit("\"", Quote::Single)])]);
        assert_eq!(
            lx("'a'\"b\"c'd'"),
            vec![word(vec![
                lit("a", Quote::Single),
                lit("b", Quote::Double),
                lit("c", Quote::None),
                lit("d", Quote::Single)
            ])]
        );
    }

    // ==================== render：恒等律（对拍 expand 语义） ====================

    #[test]
    fn render_identity_plain() {
        let t = table();
        let v = vs(&t);
        for l in [
            "", " ", "a", "echo hi", "a | b > c", "a>>b", "a&&&b", "a & b", "a\tb",
            "foo   bar", "A=1 > f", "> f", ">> f", "run a&b",
        ] {
            assert_eq!(render(&lx(l), &v), expand_ref(l, &t, 7, 3), "恒等破:{l:?}");
        }
    }

    #[test]
    fn render_identity_expansions() {
        let t = table();
        let v = vs(&t);
        for l in [
            "$A", "$A x", "x$A", "$A$A", "$V", "x$V", "$V x", "$W", "a$V c$? d",
            "$A_", "$_x9 y", "x$A$V y", "RC=$?", "B=$!", "$EMPTY", "echo $EMPTY",
        ] {
            assert_eq!(render(&lx(l), &v), expand_ref(l, &t, 7, 3), "恒等破:{l:?}");
        }
    }

    #[test]
    fn render_identity_edge_dollars() {
        let t = table();
        let v = vs(&t);
        for l in ["$", "$$", "$5", "$5x", "$中", "a$b" /* 未定义 */, "echo a\\b", "a\\"] {
            assert_eq!(render(&lx(l), &v), expand_ref(l, &t, 7, 3), "恒等破:{l:?}");
        }
    }

    #[test]
    fn render_identity_bang_variants() {
        // $! 已设/未设（<0 → 空串——bg_last 镜像）与引用律。
        let t = table();
        for bang in [3, 0, -1] {
            for l in ["$!", "x$!y", "echo $!"] {
                let v = Vars {
                    table: &t,
                    status: 7,
                    bang,
                };
                assert_eq!(
                    render(&lx(l), &v),
                    expand_ref(l, &t, 7, bang),
                    "恒等破:{l:?} bang={bang}"
                );
            }
        }
    }

    #[test]
    fn render_identity_status_negative() {
        let t = table();
        for status in [0, 2, 127, -1] {
            let v = Vars {
                table: &t,
                status,
                bang: -1,
            };
            assert_eq!(render(&lx("$?"), &v), expand_ref("$?", &t, status, -1));
            assert_eq!(render(&lx("a$?b"), &v), expand_ref("a$?b", &t, status, -1));
        }
    }

    #[test]
    fn render_roundtrip_quote_free() {
        // 无 $ 无引号输入：render(lex(l)) == l 逐字节（Sep/Op/Lit 保距）。
        let t = table();
        let v = vs(&t);
        for l in ["a | b > c", "foo   bar", "a&&&b", "a & b", "A=1 > f", "> f", "a\tb", ""] {
            assert_eq!(render(&lx(l), &v), l, "回环破:{l:?}");
        }
    }

    #[test]
    fn render_strips_quotes() {
        let t = table();
        let v = vs(&t);
        assert_eq!(render(&lx("echo \"hello world\""), &v), "echo hello world");
        assert_eq!(render(&lx("echo 'a b'"), &v), "echo a b");
        assert_eq!(render(&lx("A=\"a b\""), &v), "A=a b");
        assert_eq!(render(&lx("'a'b\"c\""), &v), "abc");
    }

    #[test]
    fn render_escapes_resolved() {
        let t = table();
        let v = vs(&t);
        assert_eq!(render(&lx(r#""a\"b""#), &v), "a\"b");
        assert_eq!(render(&lx(r#""\$X""#), &v), "$X"); // 转义还原——不展开
        assert_eq!(render(&lx(r#""a\\b""#), &v), "a\\b");
        assert_eq!(render(&lx(r#""a\nb""#), &v), "a\\nb"); // \n 双字节字面
    }

    // ======================== fields：argv 视图三律 ========================

    #[test]
    fn fields_basic_words() {
        let t = table();
        let v = vs(&t);
        assert_eq!(fields(&lx("echo hi"), &v), vec!["echo", "hi"]);
        assert_eq!(fields(&lx("a   b"), &v), vec!["a", "b"]);
        assert_eq!(fields(&lx(" a"), &v), vec!["a"]); // 首 Sep 不产空字段
        assert_eq!(fields(&lx("a "), &v), vec!["a"]);
    }

    #[test]
    fn fields_var_splits() {
        // 无引号 $var 展开拆词（POSIX 同形——裁决 10 记面）。
        let t = table();
        let v = vs(&t);
        assert_eq!(fields(&lx("echo $V"), &v), vec!["echo", "a", "b"]);
        assert_eq!(fields(&lx("$V"), &v), vec!["a", "b"]);
    }

    #[test]
    fn fields_adjacent_glue() {
        // 邻接律：边界片与相邻字面段粘连、内部分裂断字段。
        let t = table();
        let v = vs(&t);
        assert_eq!(fields(&lx("x$V"), &v), vec!["xa", "b"]);
        assert_eq!(fields(&lx("$V\"x\""), &v), vec!["a", "bx"]);
        assert_eq!(fields(&lx("x\"$V\""), &v), vec!["xa b"]);
        assert_eq!(fields(&lx("\"$V\"x"), &v), vec!["a bx"]);
    }

    #[test]
    fn fields_value_ws_edges() {
        // 值含首尾空白：首尾空白片断字段不产空字段、内部片粘连（W="  b  "）。
        let t = table();
        let v = vs(&t);
        assert_eq!(fields(&lx("$W"), &v), vec!["b"]);
        assert_eq!(fields(&lx("a$W"), &v), vec!["a", "b"]);
        assert_eq!(fields(&lx("$W\"a\""), &v), vec!["b", "a"]);
    }

    #[test]
    fn fields_empty_expansion_swallow() {
        // 纯无引号展开致空字段丢弃（`$空` 消词——今日同观测）。
        let t = table();
        let v = vs(&t);
        assert_eq!(fields(&lx("$EMPTY"), &v), Vec::<String>::new());
        assert_eq!(fields(&lx("echo $EMPTY"), &v), vec!["echo"]);
        assert_eq!(fields(&lx("$EMPTY x"), &v), vec!["x"]);
        assert_eq!(fields(&lx("a$EMPTY"), &v), vec!["a"]);
        assert_eq!(fields(&lx("$UNDEF99"), &v), Vec::<String>::new());
    }

    #[test]
    fn fields_empty_quotes_kept() {
        // 空引号＝空字段保留（argv 收空串——裁决 12）。
        let t = table();
        let v = vs(&t);
        assert_eq!(fields(&lx("\"\""), &v), vec![""]);
        assert_eq!(fields(&lx("''"), &v), vec![""]);
        assert_eq!(fields(&lx("a \"\" b"), &v), vec!["a", "", "b"]);
        assert_eq!(fields(&lx("$EMPTY\"\""), &v), vec![""]);
        assert_eq!(fields(&lx("\"\"$EMPTY"), &v), vec![""]);
    }

    #[test]
    fn fields_quoted_undefined_kept() {
        // 双引号内未定义变量 → 空字段保留（quoted Piece 载体）。
        let t = table();
        let v = vs(&t);
        assert_eq!(fields(&lx("\"$UNDEF99\""), &v), vec![""]);
        assert_eq!(fields(&lx("\"$UNDEF99\"x"), &v), vec!["x"]);
        assert_eq!(fields(&lx("x\"$UNDEF99\""), &v), vec!["x"]);
    }

    #[test]
    fn fields_double_no_split() {
        // 双引号 $var 展开禁分词——owner 例 1 的词面基础。
        let t = table();
        let v = vs(&t);
        assert_eq!(fields(&lx("\"$V\""), &v), vec!["a b"]);
        assert_eq!(fields(&lx("echo \"a b\""), &v), vec!["echo", "a b"]);
        assert_eq!(fields(&lx("\"$W\""), &v), vec!["  b  "]); // 首尾空白保真
    }

    #[test]
    fn fields_status_bang() {
        let t = table();
        let v = Vars {
            table: &t,
            status: 7,
            bang: 3,
        };
        assert_eq!(fields(&lx("$?"), &v), vec!["7"]);
        assert_eq!(fields(&lx("x$?y"), &v), vec!["x7y"]);
        assert_eq!(fields(&lx("$!"), &v), vec!["3"]);
        let vn = Vars {
            table: &t,
            status: 0,
            bang: -1,
        };
        assert_eq!(fields(&lx("$!"), &vn), Vec::<String>::new()); // 未设消词
        assert_eq!(fields(&lx("\"$!\""), &vn), vec![""]); // quoted 保空
    }

    #[test]
    fn fields_op_fusion_adjacent() {
        // Op 融合字面并入无 Sep 邻接字段：`a>|b` 管道一段名 "a>"（B4 保距）。
        let t = table();
        let v = vs(&t);
        let items = lx("a>|b");
        let gs = groups_at(&items, Op::Pipe);
        assert_eq!(gs.len(), 2);
        assert_eq!(fields(gs[0], &v), vec!["a>"]);
        assert_eq!(fields(gs[1], &v), vec!["b"]);
        // 整流（链分组前）无 Sep 全融合。
        assert_eq!(fields(&items, &v), vec!["a>|b"]);
    }

    #[test]
    fn fields_op_fusion_spaced() {
        // `a | b > c`：段参 [>,c] 保距（今日 split_pipe_seg 同观测）。
        let t = table();
        let v = vs(&t);
        let items = lx("a | b > c");
        let gs = groups_at(&items, Op::Pipe);
        assert_eq!(gs.len(), 2);
        assert_eq!(fields(gs[0], &v), vec!["a"]);
        assert_eq!(fields(gs[1], &v), vec!["b", ">", "c"]);
    }

    #[test]
    fn fields_op_fusion_chain_group() {
        // `a|||b` 链二组 = [Op(Pipe), Word(b)]——fields 融合 "|b"（消费层
        // 管道臂段空校验 usage 2 同观测）。
        let t = table();
        let v = vs(&t);
        let items = lx("a|||b");
        let cg = groups_at(&items, Op::Or);
        assert_eq!(cg.len(), 2);
        assert_eq!(fields(cg[0], &v), vec!["a"]);
        assert_eq!(fields(cg[1], &v), vec!["|b"]);
    }

    // ======================== word_texts：剥引号不展开 ========================

    #[test]
    fn word_texts_basic() {
        assert_eq!(word_texts(&lx("unset A B_2")), vec!["unset", "A", "B_2"]);
        assert_eq!(word_texts(&lx("unset 'A'")), vec!["unset", "A"]);
        assert_eq!(word_texts(&lx("unset \"A B\"")), vec!["unset", "A B"]);
        // 引号内空白不是 Sep——单词。
        assert_eq!(word_texts(&lx("'a b'")), vec!["a b"]);
    }

    #[test]
    fn word_texts_dollar_restore() {
        // Var/Status/Bang 还源文本（不展开）——`unset $A` 字面名 `$A` 零漂。
        assert_eq!(word_texts(&lx("unset $A")), vec!["unset", "$A"]);
        assert_eq!(word_texts(&lx("x=$?")), vec!["x=$?"]);
        assert_eq!(word_texts(&lx("$!")), vec!["$!"]);
        assert_eq!(word_texts(&lx("\"$A\"")), vec!["$A"]);
    }

    #[test]
    fn word_texts_op_fusion() {
        // Op 融合与 fields 同律：无 Sep 邻接整流融合、Sep 断字段。
        assert_eq!(word_texts(&lx("a>|b")), vec!["a>|b"]);
        assert_eq!(word_texts(&lx("a | b")), vec!["a", "|", "b"]);
        assert_eq!(word_texts(&lx("a b")), vec!["a", "b"]);
    }

    // ============= strip_bg：berry-jobs 语料整迁＋引号感知三形 =============

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

    #[test]
    fn strip_loop_multiple_tails() {
        assert_eq!(strip_bg("a& &"), ("a", true));
        // 审查 note 钉板：剥后余裸 & 的角落形（循环剥至 len 1 即停）。
        assert_eq!(strip_bg("& &"), ("&", true));
        assert_eq!(strip_bg("a & &"), ("a", true));
        assert_eq!(strip_bg("run hello& &"), ("run hello", true));
    }

    #[test]
    fn strip_double_amp_tail_stops() {
        assert_eq!(strip_bg("a &&"), ("a &&", false));
        assert_eq!(strip_bg("run hello &&"), ("run hello &&", false));
        assert_eq!(strip_bg("a && "), ("a && ", false));
        assert_eq!(strip_bg("&&"), ("&&", false));
    }

    #[test]
    fn strip_never_when_tail_not_amp() {
        assert_eq!(strip_bg("a&&&b"), ("a&&&b", false));
        assert_eq!(strip_bg("P=a&b"), ("P=a&b", false));
        assert_eq!(strip_bg("run a&b"), ("run a&b", false));
        assert_eq!(strip_bg("a & b"), ("a & b", false));
        assert_eq!(strip_bg("hello"), ("hello", false));
        assert_eq!(strip_bg(""), ("", false));
    }

    #[test]
    fn strip_bare_amp_not_stripped() {
        assert_eq!(strip_bg("&"), ("&", false));
        assert_eq!(strip_bg("  &  "), ("  &  ", false));
        assert_eq!(strip_bg("   "), ("   ", false));
    }

    #[test]
    fn strip_false_path_returns_original_slice() {
        for line in ["P=a&b", "a &&", "a&&&b", "&", "a & b", ""] {
            let (rest, bg) = strip_bg(line);
            assert!(!bg);
            assert!(std::ptr::eq(rest, line));
        }
    }

    #[test]
    fn strip_quote_aware_forms() {
        // 引号三形（L4）：良构闭区尾是闭引号字节非 &——唯未闭合行尾 & 落区内。
        let (r, bg) = strip_bg("echo \"a\" &");
        assert!(bg);
        assert_eq!(r, "echo \"a\"");
        let (r, bg) = strip_bg("echo \"&\"");
        assert!(!bg);
        assert_eq!(r, "echo \"&\"");
        let (r, bg) = strip_bg("echo \"a&");
        assert!(!bg);
        assert_eq!(r, "echo \"a&");
        let (r, bg) = strip_bg("echo 'a&");
        assert!(!bg);
        assert_eq!(r, "echo 'a&");
        let (r, bg) = strip_bg("echo 'a' &");
        assert!(bg);
        assert_eq!(r, "echo 'a'");
        let (r, bg) = strip_bg("run \"a b\" &");
        assert!(bg);
        assert_eq!(r, "run \"a b\"");
        // 循环剥与引号感知共存。
        let (r, bg) = strip_bg("echo 'x' & &");
        assert!(bg);
        assert_eq!(r, "echo 'x'");
    }

    #[test]
    fn strip_quote_escapes_consistency() {
        // 双引号内 \" 不闭区——尾 & 在区内不剥（与 lex 状态机同律）。
        let line = "echo \"a\\\" &"; // echo "a\" &
        let (r, bg) = strip_bg(line);
        assert!(!bg);
        assert_eq!(r, line);
        // 未引号 \ 是字面（裁决 4）——不转义其后引号：\" & 开区后尾 & 区内不剥。
        let line = r#"echo \" &"#;
        let (r, bg) = strip_bg(line);
        assert!(!bg);
        assert_eq!(r, line);
        // 双引号内 \\ 转义——区闭，尾 & 区外照剥。
        let (r, bg) = strip_bg(r#"echo "a\\" &"#);
        assert!(bg);
        assert_eq!(r, r#"echo "a\\""#);
    }

    /// berry-jobs 盲版镜像（L4 等价证明的对拍基线——零引号字节语料逐条同果）。
    fn strip_bg_blind(line: &str) -> (&str, bool) {
        let mut rest = line.trim();
        let mut bg = false;
        loop {
            let b = rest.as_bytes();
            if b.len() >= 2 && b[b.len() - 1] == b'&' && b[b.len() - 2] != b'&' {
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

    #[test]
    fn strip_blind_aware_equivalence() {
        // 良构（零引号字节）输入盲/感知逐条对拍——L4 等价证明的语料执法。
        for line in [
            "a &", "a&", "a &&", "a& &", "a&&&b", "P=a&b", "run a&b", "&", "   ", "",
            "a & b", "  run hello &  ", "a\t&", "& &", "a && ", "run hello& &",
        ] {
            assert_eq!(strip_bg(line), strip_bg_blind(line), "盲/感知分歧:{line:?}");
        }
    }

    // ================ 恒等金样族（背景 12 全表逐条手算钉板） ================

    #[test]
    fn golden_pipe_redirect_args() {
        // `a | b > c` 二段参 [>,c]——今日 split_pipe_seg(" b > c ") 同观测。
        let t = table();
        let v = vs(&t);
        let items = lx("a | b > c");
        let gs = groups_at(&items, Op::Pipe);
        assert_eq!(fields(gs[1], &v), vec!["b", ">", "c"]);
    }

    #[test]
    fn golden_pipe_name_fusion() {
        // `a>|b` 段名 "a>"——今日段参同观测（B4 保距）。
        let t = table();
        let v = vs(&t);
        let items = lx("a>|b");
        let gs = groups_at(&items, Op::Pipe);
        assert_eq!(fields(gs[0], &v), vec!["a>"]);
        assert_eq!(fields(gs[1], &v), vec!["b"]);
    }

    #[test]
    fn golden_message_distance() {
        // `foo   bar` 消息距：not-found 文案用 render 全组文本保内距空格。
        let t = table();
        let v = vs(&t);
        assert_eq!(render(&lx("foo   bar"), &v), "foo   bar");
    }

    #[test]
    fn golden_chain_amp_triplex() {
        // `a&&&b` 今日＝段 ["a","&b"]——token 模型同形。
        let t = table();
        let v = vs(&t);
        let items = lx("a&&&b");
        let cg = groups_at(&items, Op::And);
        assert_eq!(cg.len(), 2);
        assert_eq!(fields(cg[0], &v), vec!["a"]);
        assert_eq!(fields(cg[1], &v), vec!["&b"]);
    }

    #[test]
    fn golden_assignment_value_domain() {
        // 赋值值域吃整段余文：`A=1 > f` 值 "1 > f" 不落重定向（保持）。
        let t = table();
        let v = vs(&t);
        let items = lx("A=1 > f");
        // 首词首段 unquoted Lit "A=1"（赋值形检测锚——bsh P3 消费）。
        match &items[0] {
            Item::Word(w) => assert_eq!(w.pieces[0], lit("A=1", Quote::None)),
            other => panic!("首 Item 应为 Word: {other:?}"),
        }
        assert_eq!(render(&items, &v), "A=1 > f");
        // `RC=$?` 进时 status 保形。
        assert_eq!(render(&lx("RC=$?"), &v), "RC=7");
        // `A="a b"` 值 a b（新增引号面）。
        assert_eq!(render(&lx("A=\"a b\""), &v), "A=a b");
    }

    #[test]
    fn golden_bare_amp_literal() {
        // `a & b` 今日 & 字面进裸名参数——fields 三词保持。
        let t = table();
        let v = vs(&t);
        assert_eq!(fields(&lx("a & b"), &v), vec!["a", "&", "b"]);
    }

    #[test]
    fn golden_empty_chain_segment() {
        // 空段跳过：`a &&` 尾组空、`&& a` 首组空（前导操作符承袭 status 闸）。
        let items = lx("a &&");
        let cg = groups_at(&items, Op::And);
        assert_eq!(cg.len(), 2);
        assert!(cg[1].is_empty());
        let items = lx("&& a");
        let cg = groups_at(&items, Op::And);
        assert_eq!(cg.len(), 2);
        assert!(cg[0].is_empty());
    }

    #[test]
    fn golden_chain_short_circuit_shape() {
        // 链短路（跳过段不求值不碰 status——bsh 消费层）的 Item 锚面。
        assert_eq!(
            lx("a && b || c"),
            vec![
                lw("a"),
                sep(" "),
                op(Op::And),
                sep(" "),
                lw("b"),
                sep(" "),
                op(Op::Or),
                sep(" "),
                lw("c")
            ]
        );
    }

    #[test]
    fn golden_empty_command_redirect() {
        // `> f` 空命令返 0 不建目标——探针空判（fields 空）的 berry-lex 侧锚。
        let t = table();
        let v = vs(&t);
        assert_eq!(fields(before_redirect(&lx("> f")), &v), Vec::<String>::new());
        assert_eq!(
            fields(before_redirect(&lx(">> f")), &v),
            Vec::<String>::new()
        );
    }

    #[test]
    fn golden_empty_expansion_redirect() {
        // `$空 > f` 同落空判（展开空吞词——do_redirect 探针勘误的观测等价面）。
        let t = table();
        let v = vs(&t);
        assert_eq!(
            fields(before_redirect(&lx("$EMPTY > f")), &v),
            Vec::<String>::new()
        );
    }

    #[test]
    fn golden_no_op_from_expansion() {
        // G2 反注入：`P=a|b` 后 `echo $P`——items 零 Op（展开产物永为数据）。
        let items = lx("echo $PIPE");
        assert!(items.iter().all(|it| !matches!(it, Item::Op(_))));
    }

    #[test]
    fn golden_pipe_name_from_var() {
        // `$P | cat`（P=a|b）首管道组 fields ["a|b"]——段名终值校验（拒 '|'）
        // 保留在消费层（⑨ 钉板前置锚）。
        let t = table();
        let v = vs(&t);
        let items = lx("$PIPE | cat");
        let gs = groups_at(&items, Op::Pipe);
        assert_eq!(gs.len(), 2);
        assert_eq!(fields(gs[0], &v), vec!["a|b"]);
        assert_eq!(fields(gs[1], &v), vec!["cat"]);
    }

    #[test]
    fn golden_owner_examples() {
        // owner 令三例的词面/argv/echo 面（路径端到端挂三镜墙专刀）。
        let t = table();
        let v = vs(&t);
        assert_eq!(render(&lx("echo \"hello world\""), &v), "echo hello world");
        assert_eq!(
            fields(&lx("echo \"hello world\""), &v),
            vec!["echo", "hello world"]
        );
        assert_eq!(fields(&lx("grep \"a|b\" f"), &v), vec!["grep", "a|b", "f"]);
        assert_eq!(
            fields(&lx("cat \"file with spaces\""), &v),
            vec!["cat", "file with spaces"]
        );
        assert_eq!(fields(&lx("echo \"x>y\""), &v), vec!["echo", "x>y"]);
        assert_eq!(fields(&lx("hexyl \"a b\""), &v), vec!["hexyl", "a b"]);
        // `echo 'a && b'` 无链切——引号内 && 是数据。
        assert_eq!(fields(&lx("echo 'a && b'"), &v), vec!["echo", "a && b"]);
    }

    #[test]
    fn golden_echo_backslash_literal() {
        // `echo a\b` 输出 `a\b`（今日观测）——恒等契约裁决 4 钉板。
        let t = table();
        let v = vs(&t);
        assert_eq!(render(&lx("echo a\\b"), &v), "echo a\\b");
    }

    #[test]
    fn golden_echo_double_space_distance() {
        // `echo a  b` 双空格保距——echo rest-of-line 原样（零漂前置）。
        let t = table();
        let v = vs(&t);
        assert_eq!(render(&lx("echo a  b"), &v), "echo a  b");
    }
}
