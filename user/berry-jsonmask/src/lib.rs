//! berry-jsonmask —— JSON 字节级约束解码掩蔽器（ai-runtime 总体设计契约
//! 三.2 首刀：`.agents/plans/ai-runtime/design.md`「零语法错误有限状态机
//! 约束（CFG / JSON Schema Masking）——在 Logits 采样阶段按编译后的
//! 字节级有限自动机即时掩蔽非法 Token，从数学上保证 100% 输出合法
//! 协议结构」）。本 crate 钉**自动机面**：[`Masker`] 是单源转移函数
//! [`Masker::step`]（吃一字节，接受即推进、拒绝即原态不动），合法集
//! [`Masker::mask`] 由对 256 字节逐一探测 `step` 派生——合法集与转移
//! 永不双写（掩蔽正确性 = 转移正确性，一事实一归属）。
//!
//! ## 首刀边界（深度 1，诚实注记）
//!
//! - 顶层恰一个值（标量或一层容器）——尾随内容含尾随空白即拒（流式
//!   多值/尾随逗号是非法 JSON，首刀连尾随空白一并从严）。
//! - 容器嵌套深度 1：一层数组/对象内的值只允许标量；`[`/`{` 在值位
//!   拒绝。嵌套需显式栈（PDA），随后续刀。
//! - JSON Schema 驱动的按 schema 编译（字段名/类型约束）随后续刀——
//! 本刀是语法层（RFC 8259 记法子集）。
//!
//! ## 语义律
//!
//! - **拒绝不改态**：`step` 拒绝时掩蔽器停在原态——采样器从 `mask()`
//!   重选合法字节即永不产生非法输出（100% 合法性的机制来源）。
//! - **数字无自终止符**：数字态在定界符（`,` `]` `}` 空白）处先定值
//!   再吃定界符（单次 step 内两段推进——数字读法与 RFC 一致）。
//! - **对象尾随逗号拒**：`{"k":1,}` 的 `,` 后是键位（不允许 `}`）——
//!   键位两态：首键位（空对象可闭）/后续键位（必 `"`）。

#![cfg_attr(not(test), no_std)]

/// 标量自动机态（容器上下文经 [`Masker`] 的 cont 槽单独持）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum V {
    /// 顶层/容器值位起点（深度 1：此处出现 `[`/`{` 仅当容器槽空/顶层）。
    Start,
    /// 字符串体内（`key` = 对象键位字符串）。
    Str { key: bool },
    /// 反斜杠转义后。
    StrEsc { key: bool },
    /// \u 转义的第 n 个十六进制位（n = 0..3 已消费数）。
    StrU { key: bool, n: u8 },
    LitT1,
    LitT2,
    LitT3,
    LitF1,
    LitF2,
    LitF3,
    LitF4,
    LitN1,
    LitN2,
    LitN3,
    NumMinus,
    NumZero,
    NumInt,
    NumDot,
    NumFrac,
    NumE,
    NumESign,
    NumExp,
    /// 对象键位（`allow_close` = 空对象首位）。
    ObjKey { allow_close: bool },
    /// 键字符串后等冒号。
    ObjColon,
    /// 冒号后的值位（深度 1：标量 only）。
    ObjVal,
    /// 数组首位（可闭）或逗号后值位（不可闭）。
    ArrVal { allow_close: bool },
    /// 容器内标量值完成：等 `,` 或匹配的闭括。
    ValDone,
    /// 顶层值完成（终态）。
    TopDone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cont {
    Arr,
    Obj,
}

fn is_ws(b: u8) -> bool {
    b == b' ' || b == b'\t' || b == b'\n' || b == b'\r'
}

fn is_digit(b: u8) -> bool {
    b.is_ascii_digit()
}

fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

/// JSON 字节级掩蔽器（深度 1 首刀）。零分配；`step`/`mask`/`finished`
/// 三口即全部面。
#[derive(Debug, Clone, Copy)]
pub struct Masker {
    v: V,
    cont: Option<Cont>,
}

impl Default for Masker {
    fn default() -> Self {
        Self::new()
    }
}

impl Masker {
    pub fn new() -> Self {
        Self { v: V::Start, cont: None }
    }

    /// 顶层值已完成为真（硬终态——字面量/字符串/容器闭括路径）。
    pub fn finished(&self) -> bool {
        self.v == V::TopDone
    }

    /// 此刻可完成为真（流式终判）：硬终态，或顶层裸数字态——数字无
    /// 自终止符，任意数字收尾处都可合法截止（采样停止判据）。
    pub fn can_finish(&self) -> bool {
        self.finished()
            || (self.cont.is_none()
                && matches!(self.v, V::NumZero | V::NumInt | V::NumFrac | V::NumExp))
    }

    /// 吃一字节：接受推进返 true；拒绝原态不动返 false（掩蔽语义的
    /// 机制核心——采样器据 mask 重选即恒合法）。
    pub fn step(&mut self, b: u8) -> bool {
        // 数字态的定界符先行定值（数字无自终止符）。
        if let V::NumZero | V::NumInt | V::NumFrac | V::NumExp = self.v {
            if is_ws(b) || b == b',' || b == b']' || b == b'}' {
                self.value_complete();
                return self.step(b);
            }
        }
        match self.v {
            V::Start => match b {
                _ if is_ws(b) => true,
                b'"' => { self.v = V::Str { key: false }; true },
                b'-' => { self.v = V::NumMinus; true },
                b'0' => { self.v = V::NumZero; true },
                b'1'..=b'9' => { self.v = V::NumInt; true },
                b't' => { self.v = V::LitT1; true },
                b'f' => { self.v = V::LitF1; true },
                b'n' => { self.v = V::LitN1; true },
                b'[' if self.cont.is_none() => {
                    self.cont = Some(Cont::Arr);
                    self.v = V::ArrVal { allow_close: true };
                    true
                }
                b'{' if self.cont.is_none() => {
                    self.cont = Some(Cont::Obj);
                    self.v = V::ObjKey { allow_close: true };
                    true
                }
                _ => false,
            },
            V::Str { key } => match b {
                b'"' => {
                    if key {
                        self.v = V::ObjColon;
                    } else {
                        self.value_complete();
                    }
                    true
                }
                b'\\' => { self.v = V::StrEsc { key }; true },
                _ if b < 0x20 => false, // 控制字符非法（RFC 8259）
                _ => true,
            },
            V::StrEsc { key } => match b {
                b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => {
                    self.v = V::Str { key };
                    true
                }
                b'u' => { self.v = V::StrU { key, n: 0 }; true },
                _ => false,
            },
            V::StrU { key, n } => {
                if is_hex(b) {
                    if n == 3 {
                        self.v = V::Str { key };
                    } else {
                        self.v = V::StrU { key, n: n + 1 };
                    }
                    true
                } else {
                    false
                }
            }
            V::LitT1 => self.lit(b, b'r', V::LitT2),
            V::LitT2 => self.lit(b, b'u', V::LitT3),
            V::LitT3 => {
                if b == b'e' {
                    self.value_complete();
                    true
                } else {
                    false
                }
            }
            V::LitF1 => self.lit(b, b'a', V::LitF2),
            V::LitF2 => self.lit(b, b'l', V::LitF3),
            V::LitF3 => self.lit(b, b's', V::LitF4),
            V::LitF4 => {
                if b == b'e' {
                    self.value_complete();
                    true
                } else {
                    false
                }
            }
            V::LitN1 => self.lit(b, b'u', V::LitN2),
            V::LitN2 => self.lit(b, b'l', V::LitN3),
            V::LitN3 => {
                if b == b'l' {
                    self.value_complete();
                    true
                } else {
                    false
                }
            }
            V::NumMinus => match b {
                b'0' => { self.v = V::NumZero; true },
                _ if is_digit(b) => { self.v = V::NumInt; true },
                _ => false,
            },
            V::NumZero => match b {
                b'.' => { self.v = V::NumDot; true },
                b'e' | b'E' => { self.v = V::NumE; true },
                _ => false, // 前导零禁（定界符路径已在函数头处理）
            },
            V::NumInt => match b {
                _ if is_digit(b) => true,
                b'.' => { self.v = V::NumDot; true },
                b'e' | b'E' => { self.v = V::NumE; true },
                _ => false,
            },
            V::NumDot => {
                if is_digit(b) {
                    self.v = V::NumFrac;
                    true
                } else {
                    false
                }
            }
            V::NumFrac => match b {
                _ if is_digit(b) => true,
                b'e' | b'E' => { self.v = V::NumE; true },
                _ => false,
            },
            V::NumE => match b {
                b'+' | b'-' => { self.v = V::NumESign; true },
                _ if is_digit(b) => { self.v = V::NumExp; true },
                _ => false,
            },
            V::NumESign => {
                if is_digit(b) {
                    self.v = V::NumExp;
                    true
                } else {
                    false
                }
            }
            V::NumExp => {
                is_digit(b) // 定界符路径已在函数头处理
            }
            V::ObjKey { allow_close } => match b {
                _ if is_ws(b) => true,
                b'"' => { self.v = V::Str { key: true }; true },
                b'}' if allow_close => self.close_container(),
                _ => false,
            },
            V::ObjColon => match b {
                _ if is_ws(b) => true,
                b':' => { self.v = V::ObjVal; true },
                _ => false,
            },
            V::ObjVal => match b {
                _ if is_ws(b) => true,
                b'"' => { self.v = V::Str { key: false }; true },
                b'-' => { self.v = V::NumMinus; true },
                b'0' => { self.v = V::NumZero; true },
                b'1'..=b'9' => { self.v = V::NumInt; true },
                b't' => { self.v = V::LitT1; true },
                b'f' => { self.v = V::LitF1; true },
                b'n' => { self.v = V::LitN1; true },
                // 深度 1：值位容器拒（嵌套随后续刀加栈）。
                _ => false,
            },
            V::ArrVal { allow_close } => match b {
                _ if is_ws(b) => true,
                b']' if allow_close => self.close_container(),
                b'"' => { self.v = V::Str { key: false }; true },
                b'-' => { self.v = V::NumMinus; true },
                b'0' => { self.v = V::NumZero; true },
                b'1'..=b'9' => { self.v = V::NumInt; true },
                b't' => { self.v = V::LitT1; true },
                b'f' => { self.v = V::LitF1; true },
                b'n' => { self.v = V::LitN1; true },
                _ => false,
            },
            V::ValDone => match (b, self.cont) {
                (b',', Some(Cont::Arr)) => {
                    self.v = V::ArrVal { allow_close: false };
                    true
                }
                (b',', Some(Cont::Obj)) => {
                    self.v = V::ObjKey { allow_close: false };
                    true
                }
                (b']', Some(Cont::Arr)) | (b'}', Some(Cont::Obj)) => self.close_container(),
                _ if is_ws(b) => true,
                _ => false,
            },
            V::TopDone => false, // 单值律：尾随一切拒（含尾随空白）
        }
    }

    fn lit(&mut self, b: u8, want: u8, next: V) -> bool {
        if b == want {
            self.v = next;
            true
        } else {
            false
        }
    }

    /// 标量值完成：容器内在 [`V::ValDone`] 等定界，顶层即终态。
    fn value_complete(&mut self) {
        self.v = if self.cont.is_some() { V::ValDone } else { V::TopDone };
    }

    /// 闭容器：深度 1 ——容器即顶层值，闭合即终态。
    fn close_container(&mut self) -> bool {
        self.cont = None;
        self.v = V::TopDone;
        true
    }

    /// 当前态合法字节集（对 256 字节逐一探测 step 派生——与转移函数
    /// 永不双写；每次调用 256 次纯探测，掩蔽面调用方可缓存）。
    pub fn mask(&self) -> [bool; 256] {
        let mut m = [false; 256];
        for b in 0..=255u8 {
            let mut probe = *self;
            m[b as usize] = probe.step(b);
        }
        m
    }

    /// 喂整串（逐字节 step）；中途拒绝即返 false 停在该位置（流式面
    /// 的批量便利口）。
    pub fn feed(&mut self, s: &[u8]) -> bool {
        for &b in s {
            if !self.step(b) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> (bool, bool) {
        // (喂串全过, 此刻可完)——前缀合法未完（如 "1e"）= (true, false)。
        let mut m = Masker::new();
        let ok = m.feed(s.as_bytes());
        (ok, m.can_finish())
    }

    #[test]
    fn scalars_accept() {
        assert_eq!(run("true"), (true, true));
        assert_eq!(run("false"), (true, true));
        assert_eq!(run("null"), (true, true));
        assert_eq!(run("\"abc\""), (true, true));
        assert_eq!(run("0"), (true, true));
        assert_eq!(run("-0.5"), (true, true));
        assert_eq!(run("12"), (true, true));
        assert_eq!(run("-12.5e+3"), (true, true));
        assert_eq!(run("1E9"), (true, true));
        assert_eq!(run("  42"), (true, true)); // 前导空白 ok
        assert_eq!(run("  42  ").0, false); // 尾随空白拒（单值从严律）
    }

    #[test]
    fn scalars_reject() {
        assert_eq!(run("truX").0, false);
        assert_eq!(run("01").0, false); // 前导零（字节级拒）
        assert_eq!(run("1."), (true, false)); // 前缀合法未完
        assert_eq!(run("1e"), (true, false));
        assert_eq!(run("1e+"), (true, false));
        assert_eq!(run("-"), (true, false)); // 前缀合法未完（负号后须数字）
        assert_eq!(run("nan").0, false);
        assert_eq!(run("\"a\u{1}b\"").0, false); // 控制字符
        assert_eq!(run("\"\\q\"").0, false); // 域外转义
        assert_eq!(run("\"\\u00G1\"").0, false); // 非十六进制
    }

    #[test]
    fn string_escapes_accept() {
        assert_eq!(run("\"a\\\"b\""), (true, true));
        assert_eq!(run("\"\\n\\t\\\\\""), (true, true));
        assert_eq!(run("\"\\u0041\""), (true, true));
    }

    #[test]
    fn arrays_depth1() {
        assert_eq!(run("[]"), (true, true));
        assert_eq!(run("[1, 2, 3]"), (true, true));
        assert_eq!(run("[ ]"), (true, true));
        assert_eq!(run("[1,\"a\",true,null]"), (true, true));
        assert_eq!(run("[,]").0, false); // 首位逗号
        assert_eq!(run("[1,]").0, false); // 尾随逗号（逗号后不可闭）
        assert_eq!(run("[1 2]").0, false); // 缺逗号
        assert_eq!(run("[[1]]").0, false); // 深度 1：嵌套拒（首刀边界）
    }

    #[test]
    fn objects_depth1() {
        assert_eq!(run("{}"), (true, true));
        assert_eq!(run("{\"k\":1}"), (true, true));
        assert_eq!(run("{\"k\":1,\"j\":2}"), (true, true));
        assert_eq!(run("{\"k\":true}"), (true, true));
        assert_eq!(run("{\"k\":}").0, false);
        assert_eq!(run("{1:2}").0, false); // 键必须字符串
        assert_eq!(run("{\"k\" 1}").0, false); // 缺冒号
        assert_eq!(run("{\"k\":1,}").0, false); // 尾随逗号
        assert_eq!(run("{\"k\":[]}").0, false); // 深度 1：值位容器拒
        assert_eq!(run("{\"k\":1]").0, false); // 闭括失配
    }

    #[test]
    fn number_delimiter_finalization() {
        // 数字定界两段推进：`[1,2]` 的 ',' 与 ']' 各自一次 step 消化。
        assert_eq!(run("[1,2]"), (true, true));
        assert_eq!(run("[12]"), (true, true));
        assert_eq!(run("{\"n\":9}"), (true, true));
        // 顶层裸数字无定界符：喂完即 finished（终态在 feed 尽时成立）。
        assert_eq!(run("7"), (true, true));
    }

    #[test]
    fn masks_derived_from_step() {
        let m = Masker::new();
        let mask = m.mask();
        for b in [b' ', b'"', b'-', b'0', b'9', b't', b'f', b'n', b'[', b'{'] {
            assert!(mask[b as usize], "Start 应允 {}", b as char);
        }
        for b in [b'a', b'x', b'+', b'.', b']', b'}', b',', b':'] {
            assert!(!mask[b as usize], "Start 应拒 {}", b as char);
        }
        // 字面量中段：只允下一字符。
        let mut m2 = Masker::new();
        m2.feed(b"tru");
        let mask2 = m2.mask();
        assert!(mask2[b'e' as usize] && mask2.iter().filter(|&&x| x).count() == 1);
        // 键后只允冒号（含空白）。
        let mut m3 = Masker::new();
        m3.feed(b"{\"k\"");
        let mask3 = m3.mask();
        assert!(mask3[b':' as usize]);
        assert!(!mask3[b',' as usize]);
        // 小数点后只数字。
        let mut m4 = Masker::new();
        m4.feed(b"1.");
        let mask4 = m4.mask();
        assert!(mask4[b'0' as usize] && mask4[b'9' as usize]);
        assert!(!mask4[b'.' as usize] && !mask4[b'e' as usize]);
    }

    #[test]
    fn reject_leaves_state_unchanged() {
        let mut m = Masker::new();
        assert!(!m.step(b'x'));
        // 拒绝后掩蔽器仍在 Start：喂合法串仍走通。
        assert_eq!(run_feed(&mut m, "true"), true);
        assert!(m.finished());
    }

    fn run_feed(m: &mut Masker, s: &str) -> bool {
        m.feed(s.as_bytes())
    }

    #[test]
    fn unfinished_states() {
        let mut m = Masker::new();
        m.feed(b"[1, ");
        assert!(!m.finished());
        m.feed(b"2");
        assert!(!m.finished());
        m.feed(b"]");
        assert!(m.finished());
        // 终态后一切拒（含空白——单值律）。
        assert!(!m.step(b' '));
    }
}
