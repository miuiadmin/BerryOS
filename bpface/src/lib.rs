//! berry-bpface —— boot-params 消费端纯函数面（W3-Q140，自
//! `kernel/src/bootparams.rs` 外迁）。
//!
//! 单源纪律（同 berry-wsrcface / berry-dtbface）：**判定律唯一事实源在
//! 此**，kernel `bootparams.rs` 薄壳化消费（re-export + Vmo 物理臂留
//! kernel 侧——`Src` 归一视图的 VMO read 不迁）。
//!
//! 为什么 host 钉板此前不可达：`kernel/src/bootparams.rs` 全模块在
//! kernel crate（no_std + HAL/alloc 环境），host 测试面不链接——cpio
//! newc 定址查找的几何推进律（魔数/页垫/TRAILER 自终止/checked 防回卷）
//! 与 TOML 子集解析的执法面（schema 门/三键值域/未知键宽严分工/行数帽）
//! 此前只有 ktest `test_bootparams`（QEMU boot 相）+ 真镜像 eco 行两路
//! 间接覆盖。本 crate 把两面按纯字节面抽成纯函数 + 表驱动测试——语义
//! （判序/边界/诊断码）逐字对齐 kernel 侧，改这里的语义 = 改 boot 行为。
//!
//! 抽离面（与 kernel 侧逐字对齐）：
//! - 常量族：[`FILE_MAX`]/[`LINE_MAX`]/[`LINE_LEN_MAX`]/[`HDR`]/
//!   [`NAME_MAX`]/[`SLICE_HZ_MIN`]/[`SLICE_HZ_MAX`] + [`INTENT_WORDS`]
//!   六档词表（码值单源 = berry-intface `INTENT_*`）。
//! - [`hex8`]: newc hex8 字段折算（大小写 hex 均收，非 hex = None）。
//! - [`entry_geometry`]: 单条目几何推进（header → 名 → 4096 垫 → 数据
//!   → 4096 垫 → 下条；全 checked 防敌意巨值回卷）。
//! - [`find_in`]: cpio newc 定址查找状态机（魔数 070701/070702、
//!   namesize 域 [1,NAME_MAX]、NUL 终止名、TRAILER!!! 自终止、零长条目
//!   循环防护）——消费 [`ByteSource`] 纯读取 trait。
//! - [`parse`]: TOML 子集解析（schema = 1 头 + `key = value` 行；容忍
//!   空行/`#` 注释/两侧空白/CR；重复键后者胜；未知键跳过不弃段；坏键
//!   回缺省 + 首错诊断 [`Outcome::diag`]）。
//! - [`parse_u32`]/[`intent_code`]: 纯数字执法 + 引号词表查找。
//!
//! 不在此 crate（留 kernel 壳）：`Src::Vmo` 物理臂（VMO 只读 read 走
//! kernel vmo.rs）、`alloc::vec![0u8; len]` 内容缓冲、`apply_and_report`
//! HAL 注入与 eco 打点（timer/sched 静态面 + uart）——本面只钉「字节流
//! 怎么走查、键值怎么解析」，不钉「内存怎么借、参数怎么生效」。
//!
//! 纪律：纯函数、零 unsafe、零 panic——敌意几何（巨 namesize/巨
//! filesize/截断头）全 None 兜，不越界不回卷。

#![cfg_attr(not(test), no_std)]

use berry_intface::{
    INTENT_BACKGROUND, INTENT_INTERACTIVE, INTENT_LATENCY_CRITICAL, INTENT_MAINTENANCE,
    INTENT_UNSPECIFIED, INTENT_UTILITY,
};

/// 定址文件名（§1.6-补投放面定形——`tools/berry_assemble.py`
/// BOOT_PARAMS_ENTRY 单源对读；升版同步义务在此注记）。
pub const NAME: &[u8] = b"etc/boot-params.toml";

/// 文件内容帽（解析面有界——§1.6-4 精神：32 键 × 词表值 + schema 头 +
/// 注释余量，4KiB 单页帽；超帽 = 坏件弃）。
pub const FILE_MAX: usize = 4096;

/// 行数帽（count 帽 32 的解析面定界同承 + schema/注释余量）。
pub const LINE_MAX: usize = 64;

/// 单行长度帽（防御面——词表值最宽 18B、键最宽 15B，128B 之外必坏件）。
pub const LINE_LEN_MAX: usize = 128;

/// newc 头长：6 魔数 + 13 × 8 hex。
pub const HDR: usize = 110;

/// 条目名长帽（含 NUL；目标名 20B，归档内条目名最宽 ~30B——防御面）。
pub const NAME_MAX: usize = 300;

/// 词表 v1 三键的值域（`tools/berry_assemble.py` BOOT_PARAMS_SPEC 单源
/// 对读）。
pub const SLICE_HZ_MIN: u32 = 16;
pub const SLICE_HZ_MAX: u32 = 1024;

/// 六档 intent 词表（ADR #58——`berry_assemble` INTENTS / mkapp 同源
/// 镜像；词 ↔ 码双用：解析（词→码）与 eco 行（码→词）。升版同步义务
/// 在此注记）。
pub const INTENT_WORDS: [(&[u8], u8); 6] = [
    (b"unspecified", INTENT_UNSPECIFIED),
    (b"latency_critical", INTENT_LATENCY_CRITICAL),
    (b"interactive", INTENT_INTERACTIVE),
    (b"utility", INTENT_UTILITY),
    (b"background", INTENT_BACKGROUND),
    (b"maintenance", INTENT_MAINTENANCE),
];

/// 解析产物（全 Option——坏键回 None + 首错诊断；三键独立应用）。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Outcome {
    pub slice_hz: Option<u32>,
    pub intent: Option<u8>,
    /// CP-11 第三键：系统缺省 timer slack（纳秒）。
    pub timer_slack: Option<u32>,
    /// 首错诊断（ASCII 静态串——eco 诊断行载荷；None = 干净）。
    pub diag: Option<&'static str>,
}

impl Outcome {
    /// 干净缺省态（件缺/全弃 = 调用方静默）。
    pub const CLEAN: Outcome = Outcome { slice_hz: None, intent: None, timer_slack: None, diag: None };

    /// 全段弃（schema 拒面：三键皆空 + 诊断 schema——防 v2 语义被 v1
    /// 内核误用，§1.6-4「超界即弃段用缺省」）。
    pub const DISCARD_SCHEMA: Outcome =
        Outcome { slice_hz: None, intent: None, timer_slack: None, diag: Some("schema") };

    /// 坏件诊断形（size/read 几何面——scan 消费）。
    pub const fn bad(cause: &'static str) -> Outcome {
        Outcome { slice_hz: None, intent: None, timer_slack: None, diag: Some(cause) }
    }
}

/// cpio 字节流读取抽象（kernel 侧 `Src` 归一视图实现本 trait——Bytes
/// 直切/Vmo 走 vmo.read；host 测试用 `&[u8]`）。
pub trait ByteSource {
    /// 总长（逻辑长——多卷拼装件非页对齐容量）。
    fn total(&self) -> usize;
    /// 有界读：越界（几何违约）返 false——调用方按「未找到」降级。
    fn read_at(&self, off: usize, out: &mut [u8]) -> bool;
}

/// `&[u8]` 直读臂（host 测试 + kernel `Src::Bytes` 同构——`S = [u8]`，
/// `find_in(bytes, ..)` 直接传切片引用）。
impl ByteSource for [u8] {
    fn total(&self) -> usize {
        self.len()
    }
    fn read_at(&self, off: usize, out: &mut [u8]) -> bool {
        if off > self.len() || out.len() > self.len() - off {
            return false;
        }
        out.copy_from_slice(&self[off..off + out.len()]);
        true
    }
}

/// hex8 字段折算（cpio_walk `field` 同款：大小写 hex 均收；非 hex =
/// None）。
pub fn hex8(s: &[u8]) -> Option<u64> {
    if s.len() != 8 {
        return None;
    }
    let mut v: u64 = 0;
    for &c in s {
        let d = match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => return None,
        };
        v = v * 16 + d as u64;
    }
    Some(v)
}

/// 单条目几何推进：header → 名（NUL 终止）→ 4096 垫 → 数据 → 4096 垫
/// → 下条（mkcpio entry() 垫 4096 契约——「单编码器纪律」第五消费点，
/// 改垫粒度五点同步注记见 mkcpio.py CPIO_PAD）。全 checked（敌意巨值
/// 回卷拒）。
///
/// 输入 = 头字段（filesize/namesize）与当前头偏移；输出 = (name_off,
/// data_off, data_end, next)。总长界检归 [`find_in`]（此处只算几何）。
pub fn entry_geometry(pos: u64, filesize: u64, namesize: u64) -> Option<EntryGeometry> {
    let name_off = pos + HDR as u64;
    let data_off = (name_off + namesize).checked_add(4095)? & !4095;
    let data_end = data_off.checked_add(filesize)?;
    let next = data_end.checked_add(4095)? & !4095;
    Some(EntryGeometry { name_off, data_off, data_end, next })
}

/// 单条目几何四元组。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct EntryGeometry {
    pub name_off: u64,
    pub data_off: u64,
    pub data_end: u64,
    pub next: u64,
}

/// cpio newc 定址查找状态机（berry-loader `cpio_walk` 同口径的窄化：
/// 不产迭代器、只找目标名——魔数 070701/070702、页垫 4096 几何、
/// TRAILER!!! 自终止、全 checked；几何违约/截断 = 停走返 None——rootfs
/// 整体合法性归用户态装载器，此处只作非致命查找）。找到返
/// `(data_off, filesize)`。
pub fn find_in<S: ByteSource + ?Sized>(src: &S, target: &[u8]) -> Option<(usize, usize)> {
    let total = src.total() as u64;
    let mut pos: u64 = 0;
    let mut hdr = [0u8; HDR];
    let mut name = [0u8; NAME_MAX];
    loop {
        // 尾部不足一头（含全零页垫尾/截断）= 干净结束（cpio_walk 同款）。
        if pos + HDR as u64 > total {
            return None;
        }
        if !src.read_at(pos as usize, &mut hdr) {
            return None;
        }
        // 魔数：070701 / 070702（check 字段 v1 不校验）。
        if &hdr[..4] != b"0707" || hdr[4] != b'0' || (hdr[5] != b'1' && hdr[5] != b'2') {
            return None;
        }
        // 字段序：magic 后 ino=0,mode=1,uid=2,gid=3,nlink=4,mtime=5,
        // filesize=6,…,namesize=11,check=12（各 8 hex）。
        let filesize = hex8(&hdr[6 + 8 * 6..6 + 8 * 6 + 8])?;
        let namesize = hex8(&hdr[6 + 8 * 11..6 + 8 * 11 + 8])?;
        if namesize == 0 || namesize > NAME_MAX as u64 {
            return None;
        }
        let g = entry_geometry(pos, filesize, namesize)?;
        if g.next > total {
            return None;
        }
        let n = namesize as usize;
        if !src.read_at(g.name_off as usize, &mut name[..n]) {
            return None;
        }
        if name[n - 1] != 0 {
            return None;
        }
        let nm = &name[..n - 1];
        pos = g.next;
        if pos == 0 {
            return None; // 零长条目循环防护（垫几何下不可达，防御面）
        }
        if nm == b"TRAILER!!!" {
            return None;
        }
        if nm == target {
            return Some((g.data_off as usize, filesize as usize));
        }
    }
}

/// TOML 子集解析（投放面格式 v1：`schema = 1` 头 + `key = value` 行，键
/// 序字典序——§1.6-补投放面；容忍空行/`#` 注释/两侧空白）。重复键后者
/// 胜（TOML 语义本应拒重复——最小解析器不执法，投放面键序确定性已杜
/// 绝）。
pub fn parse(c: &[u8]) -> Outcome {
    let mut out = Outcome { slice_hz: None, intent: None, timer_slack: None, diag: None };
    let mut schema: Option<u32> = None;
    let mut lines = 0;
    for line in c.split(|&b| b == b'\n') {
        if lines >= LINE_MAX {
            out.diag = out.diag.or(Some("lines"));
            break;
        }
        lines += 1;
        if line.len() > LINE_LEN_MAX {
            out.diag = out.diag.or(Some("syntax"));
            continue;
        }
        let l = trim(line);
        if l.is_empty() || l[0] == b'#' {
            continue;
        }
        let Some(eq) = l.iter().position(|&b| b == b'=') else {
            out.diag = out.diag.or(Some("syntax"));
            continue;
        };
        let (key, val) = (trim(&l[..eq]), trim(&l[eq + 1..]));
        match key {
            b"schema" => match parse_u32(val) {
                Some(v) => schema = Some(v),
                None => out.diag = out.diag.or(Some("schema")),
            },
            b"sched_slice_hz" => match parse_u32(val) {
                Some(v) if (SLICE_HZ_MIN..=SLICE_HZ_MAX).contains(&v) => {
                    out.slice_hz = Some(v);
                }
                _ => out.diag = out.diag.or(Some("range")),
            },
            b"intent_default" => match intent_code(val) {
                Some(code) => out.intent = Some(code),
                None => out.diag = out.diag.or(Some("intent")),
            },
            // CP-11 第三键：u32 纳秒全收（域 [0,999_999_999] = parse_u32
            // 九位帽自然执法——越九位 None 落「range」诊断，与词表上界
            // 999_999_999 恰合，零再裁面）。
            b"timer_slack" => match parse_u32(val) {
                Some(v) => out.timer_slack = Some(v),
                None => out.diag = out.diag.or(Some("range")),
            },
            _ => out.diag = out.diag.or(Some("key")),
        }
    }
    // schema 头 ≠ 1（缺失/坏值/未来版本）= 未知格式全段弃（§1.6-4 弃段
    // 用缺省；防 v2 语义被 v1 内核误用）——已拾键清空、诊断改 schema。
    if schema != Some(1) {
        return Outcome::DISCARD_SCHEMA;
    }
    out
}

/// 行内两端空白剥离（空格/Tab；TOML 子集最小面）。
pub fn trim(s: &[u8]) -> &[u8] {
    let mut a = 0;
    let mut b = s.len();
    while a < b && (s[a] == b' ' || s[a] == b'\t') {
        a += 1;
    }
    while b > a && (s[b - 1] == b' ' || s[b - 1] == b'\t') {
        b -= 1;
    }
    // CR 容忍（CRLF 产线防御面——现役产链 LF-only）。
    while b > a && s[b - 1] == b'\r' {
        b -= 1;
    }
    &s[a..b]
}

/// 十进制 u32（纯数字、≤9 位防溢；无符号/无引号/无下划线——投放面
/// 序列化器只产此形）。**纯数字执法**（b'0'..=b'9' 闭域）：原
/// `checked_sub(b'0')` 只拦 <'0' 下溢、字母串可走私为巨值（"abc"→5451）
/// ——两键时代由各键值域兜住的隐式合同，CP-11 timer_slack 全 u32 域键
/// 落地时暴露（ktest 实测钉板），显式化为解析面执法。
pub fn parse_u32(s: &[u8]) -> Option<u32> {
    if s.is_empty() || s.len() > 9 {
        return None;
    }
    let mut v: u32 = 0;
    for &c in s {
        let d = match c {
            b'0'..=b'9' => c - b'0',
            _ => return None,
        };
        v = v * 10 + d as u32;
    }
    Some(v)
}

/// intent 词 → 码（带引号形 `"word"`——投放面 TOML 字符串字面量）。
pub fn intent_code(val: &[u8]) -> Option<u8> {
    if val.len() < 2 || val[0] != b'"' || val[val.len() - 1] != b'"' {
        return None;
    }
    let word = &val[1..val.len() - 1];
    INTENT_WORDS
        .iter()
        .find(|(w, _)| *w == word)
        .map(|&(_, c)| c)
}
