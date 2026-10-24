//! # berry-manifest —— BerryOS 应用清单 v0 纯函数解析器
//!
//! `docs/app-0实施设计草案.md` §2/§3（ADR #43/#44：应用 = 纯用户态协议
//! 约定 + cpio 容器 + TOML manifest）。「解析器即产品」家族第 8 例
//! （loader/fsproto/virtio/blkproto/fatfs/netproto/netdproto/tcp 之后）：**零分配、零
//! unsafe、零依赖、输入只借用、no_std**——宿主 svcmgr 是 none target 平像
//! 程序（aarch64-unknown-none，无 std 无 alloc），带 std/alloc 依赖即
//! workspace 构建链编译失败。
//!
//! ## schema v0（§2 冻结）
//!
//! `[app]` 五键必填：id / version / entry / type / intent；
//! `[capabilities]` 一键必填：console（v0 唯一能力词汇）；`[runtime]`
//! 整节可省：stack_pages（缺省 8，1..=64）。type 恒 `"app"`
//! （`"driver"` 随 devmgr v2 add-only）。
//!
//! ## 语法子集（冻结到字节级——「编不出就拒绝，不截断」）
//!
//! - 换行 **LF-only**：任何 CR 出现即拒（git autocrlf 检出即触发，build
//!   门卫早炸优于运行期降级）；制表符一律拒；
//! - 整数 = 纯 `[0-9]+`：拒 hex/八进制/二进制前缀、下划线、正负号；
//! - 字符串内**反斜杠一律拒**（v0 无转义语义——`"a\nb"` 解出真实换行属
//!   静默错值类）；字符串限可打印 ASCII 0x20..=0x7E；
//! - `#` 注释：整行与行尾两形态都收（字符串内 `#` 不算注释），注释内容
//!   任意字节（多字节 UTF-8 亦然）——剥离后零语义面；
//! - 间距容忍集：等号两侧空格可省、行首缩进仅空格、节头内名前后各 ≤1
//!   空格；
//! - 空 `[runtime]` 合法（= 全缺省）；节序无关。
//!
//! ## 严格拒面（v0 无宽容忽略档）
//!
//! 未知节 / 未知键 / 重复节 / 重复键 / 必填缺失 / 类型错 / 值域外。未知键
//! loud 失败优于半工作——新键包在旧解析器上炸响 = 显式版本不匹配信号
//! （协商机制随 v2 版本化）。
//!
//! ## 帽常量双侧同步义务
//!
//! §1 尺寸帽表在 tools/mkapp.py（build 期门卫）与本 crate（运行期执法）
//! 各硬编码一份——mkapp 无法 import Rust，其门卫注释以本 crate 常量定义
//! 处为对账锚点；**改帽值须双侧同步**（mkfat → berry-fatfs 头注义务同
//! 款）。entry 必 `bin/` 前缀（完整包内路径）；消费端按整串比对 cpio
//! 条目名，无路径语义 = 无遍历面。

#![cfg_attr(not(test), no_std)]

use core::str::from_utf8;

// ---- 帽常量（§1 尺寸帽表；双侧同步义务见模块头注）----

/// manifest.toml ≤ 1024B（字段表绰绰有余；防敌意巨值）。
pub const MANIFEST_MAX: usize = 1024;
/// `.bap` 整包 ≤ 4 MiB（appd 走查几何界，防几何炸弹）。
pub const BAP_MAX: usize = 4 * 1024 * 1024;
/// id ≤ 24B，`[A-Za-z0-9._-]`，首字符字母数字。
pub const ID_MAX: usize = 24;
/// version ≤ 16B（记录不消费）。
pub const VERSION_MAX: usize = 16;
/// entry ≤ 32B，必 `bin/` 前缀（与包内文件一一对应）。
pub const ENTRY_MAX: usize = 32;
/// rootfs 内 app 条目 ≤ 4（appd 静态收集槽）。
pub const APP_SLOTS: usize = 4;
/// stack_pages 缺省 8（std 小程序口径）。
pub const STACK_PAGES_DEFAULT: u32 = 8;
/// stack_pages 下界（0 页栈非法）。
pub const STACK_PAGES_MIN: u32 = 1;
/// stack_pages 上界（hexyl 32 页 = musl 128KiB 惯例之倍）。
pub const STACK_PAGES_MAX: u32 = 64;

/// `ManifestErr` 理由帽——appd 降级行 `eco: app <id> <理由>` 的发射窗按
/// 24B 设计（§5）；超长理由自身截断后接省略号（「截断不 panic」）。
const REASON_MAX: usize = 24;

// ---- Intent：六档冻结词表 ----

/// 应用声明的调度 intent——**词表与判别值镜像 `sched/src/intent.rs`
/// 常量块（INTENT_UNSPECIFIED..INTENT_MAINTENANCE，单一事实源在彼处）**。
/// v0 只记录不消费；运行期 u8 映射表不在此落（避免与 intent.rs 成双表）。
/// 判别值 day-one 定型：v2 消费点 `as u8` 即得正确编码（TPIDR 偏移 /
/// vblk SECTOR_SZ const assert 同款「形状钉子」——无钉子则声明顺序一改
/// = 六档全错、静默错优先级带）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Intent {
    Unspecified = 0x00,
    LatencyCritical = 0x01,
    Interactive = 0x02,
    Utility = 0x03,
    Background = 0x04,
    Maintenance = 0x05,
}

// 编译期钉（首/尾；六变体全量断言在 host 测试 intent_discriminants_pinned）。
const _: () = assert!(Intent::Unspecified as u8 == 0x00);
const _: () = assert!(Intent::Maintenance as u8 == 0x05);

impl Intent {
    /// manifest 小写蛇形词 → 枚举（大小写敏感；野词 None）。
    pub fn from_word(w: &str) -> Option<Self> {
        Some(match w {
            "unspecified" => Intent::Unspecified,
            "latency_critical" => Intent::LatencyCritical,
            "interactive" => Intent::Interactive,
            "utility" => Intent::Utility,
            "background" => Intent::Background,
            "maintenance" => Intent::Maintenance,
            _ => return None,
        })
    }

    /// 枚举 → manifest 小写蛇形词（canonical 重序列化 / 诊断行用）。
    pub fn word(self) -> &'static str {
        match self {
            Intent::Unspecified => "unspecified",
            Intent::LatencyCritical => "latency_critical",
            Intent::Interactive => "interactive",
            Intent::Utility => "utility",
            Intent::Background => "background",
            Intent::Maintenance => "maintenance",
        }
    }
}

// ---- 解析产物 ----

/// 解析成功的 manifest（全借用——零拷贝切进输入缓冲）。type 恒 `"app"`
/// （§2 唯一值），不入结构；stack_pages 已合并缺省。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Manifest<'a> {
    pub id: &'a str,
    pub version: &'a str,
    /// 完整包内路径，含 `bin/` 前缀（如 `bin/appdemo`）。
    pub entry: &'a str,
    pub console: bool,
    pub stack_pages: u32,
    pub intent: Intent,
}

/// 解析失败理由（定长内联，零分配）——appd 降级行按 `reason_str()` 打印。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManifestErr {
    reason: [u8; REASON_MAX],
    len: usize,
}

impl ManifestErr {
    /// 理由 ≤24B 直存；超长截断到 21B + "…"（UTF-8 三字节）= 恰 24B。
    /// 现役理由全 ASCII，21B 处截断不会劈开多字节字符（防御面：万一
    /// 劈开，`reason_str()` 兜底空串，绝不 panic）。
    fn new(reason: &str) -> Self {
        let b = reason.as_bytes();
        if b.len() <= REASON_MAX {
            let mut r = [0u8; REASON_MAX];
            r[..b.len()].copy_from_slice(b);
            ManifestErr { reason: r, len: b.len() }
        } else {
            let mut r = [0u8; REASON_MAX];
            r[..21].copy_from_slice(&b[..21]);
            r[21] = 0xE2; // U+2026 HORIZONTAL ELLIPSIS
            r[22] = 0x80;
            r[23] = 0xA6;
            ManifestErr { reason: r, len: REASON_MAX }
        }
    }

    pub fn reason_str(&self) -> &str {
        from_utf8(&self.reason[..self.len]).unwrap_or("")
    }
}

fn err(reason: &str) -> ManifestErr {
    ManifestErr::new(reason)
}

// ---- 内部解析器 ----

#[derive(Clone, Copy)]
enum Sec {
    App,
    Caps,
    Run,
}

struct Fields<'a> {
    id: Option<&'a [u8]>,
    version: Option<&'a [u8]>,
    entry: Option<&'a [u8]>,
    ty: Option<&'a [u8]>,
    intent: Option<Intent>,
    console: Option<bool>,
    stack_pages: Option<u32>,
}

/// 两端去空格（空格是唯一合法空白——CR/tab 已在全文级拒）。
fn trim(s: &[u8]) -> &[u8] {
    let mut a = 0;
    let mut b = s.len();
    while a < b && s[a] == b' ' {
        a += 1;
    }
    while b > a && s[b - 1] == b' ' {
        b -= 1;
    }
    &s[a..b]
}

/// `#` 注释剥离：字符串外首个 `#` 起整段截断（行尾注释；整行注释由此
/// 自然得出——剥离后为空行跳过）。扫描维护引号开关；字符串内 `#` 不算
/// 注释。反斜杠在值层另拒，此处不假设转义。
fn strip_comment(line: &[u8]) -> &[u8] {
    let mut in_str = false;
    for (i, &b) in line.iter().enumerate() {
        match b {
            b'"' => in_str = !in_str,
            b'#' if !in_str => return &line[..i],
            _ => {}
        }
    }
    line
}

/// 双引号字符串：内容非空集、无内嵌引号/反斜杠、可打印 ASCII 0x20..=0x7E。
/// 空串语法合法（语义层再按字段拒）。
fn parse_string(v: &[u8]) -> Option<&[u8]> {
    if v.len() < 2 {
        return None;
    }
    if v[0] != b'"' || v[v.len() - 1] != b'"' {
        return None;
    }
    let inner = &v[1..v.len() - 1];
    for &b in inner {
        if b == b'"' || b == b'\\' {
            return None;
        }
        if !(0x20..=0x7E).contains(&b) {
            return None;
        }
    }
    Some(inner)
}

fn parse_bool(v: &[u8]) -> Option<bool> {
    match v {
        b"true" => Some(true),
        b"false" => Some(false),
        _ => None,
    }
}

/// 纯 `[0-9]+` 十进制 u32：非数字（含 hex 前缀/下划线/正负号/小数点）或
/// 溢出即 None。
fn parse_u32(v: &[u8]) -> Option<u32> {
    if v.is_empty() {
        return None;
    }
    let mut n: u32 = 0;
    for &b in v {
        if !b.is_ascii_digit() {
            return None;
        }
        n = n.checked_mul(10)?.checked_add((b - b'0') as u32)?;
    }
    Some(n)
}

/// id 合法性：1..=ID_MAX、首字符字母数字、全串 `[A-Za-z0-9._-]`。
fn valid_id(id: &[u8]) -> bool {
    if id.is_empty() || id.len() > ID_MAX {
        return false;
    }
    if !id[0].is_ascii_alphanumeric() {
        return false;
    }
    id.iter()
        .all(|&b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
}

fn set_str<'a>(slot: &mut Option<&'a [u8]>, val: &'a [u8]) -> Result<(), ManifestErr> {
    if slot.is_some() {
        return Err(err("dup key"));
    }
    match parse_string(val) {
        Some(s) => {
            *slot = Some(s);
            Ok(())
        }
        None => Err(err("bad value")),
    }
}

/// manifest v0 解析主入口。永不 panic；一切拒绝经 `ManifestErr` 带理由。
pub fn parse(bytes: &[u8]) -> Result<Manifest<'_>, ManifestErr> {
    if bytes.len() > MANIFEST_MAX {
        return Err(err("too big"));
    }
    if bytes.contains(&b'\r') {
        return Err(err("CRLF"));
    }
    if bytes.contains(&b'\t') {
        return Err(err("tab"));
    }

    let mut f = Fields {
        id: None,
        version: None,
        entry: None,
        ty: None,
        intent: None,
        console: None,
        stack_pages: None,
    };
    let (mut saw_app, mut saw_caps, mut saw_run) = (false, false, false);
    let mut cur: Option<Sec> = None;

    for raw in bytes.split(|&b| b == b'\n') {
        let line = trim(strip_comment(raw));
        if line.is_empty() {
            continue;
        }

        // 节头：[name]，名前后各 ≤1 可选空格；多余空格折出未知节。
        if line[0] == b'[' {
            if line.len() < 2 || line[line.len() - 1] != b']' {
                return Err(err("bad section"));
            }
            let mut inner = &line[1..line.len() - 1];
            if inner.first() == Some(&b' ') {
                inner = &inner[1..];
            }
            if inner.last() == Some(&b' ') {
                inner = &inner[..inner.len() - 1];
            }
            let sec = match inner {
                b"app" => Sec::App,
                b"capabilities" => Sec::Caps,
                b"runtime" => Sec::Run,
                _ => return Err(err("unknown section")),
            };
            let dup = match sec {
                Sec::App => saw_app,
                Sec::Caps => saw_caps,
                Sec::Run => saw_run,
            };
            if dup {
                return Err(err("dup section"));
            }
            match sec {
                Sec::App => saw_app = true,
                Sec::Caps => saw_caps = true,
                Sec::Run => saw_run = true,
            }
            cur = Some(sec);
            continue;
        }

        // key = value
        let eq = match line.iter().position(|&b| b == b'=') {
            Some(i) => i,
            None => return Err(err("bad line")),
        };
        let key = trim(&line[..eq]);
        let val = trim(&line[eq + 1..]);
        if key.is_empty() {
            return Err(err("bad line"));
        }
        let sec = match cur {
            Some(s) => s,
            None => return Err(err("key before section")),
        };
        match (sec, key) {
            (Sec::App, b"id") => set_str(&mut f.id, val)?,
            (Sec::App, b"version") => set_str(&mut f.version, val)?,
            (Sec::App, b"entry") => set_str(&mut f.entry, val)?,
            (Sec::App, b"type") => set_str(&mut f.ty, val)?,
            (Sec::App, b"intent") => {
                if f.intent.is_some() {
                    return Err(err("dup key"));
                }
                let w = parse_string(val).ok_or_else(|| err("bad value"))?;
                // parse_string 已验可打印 ASCII，from_utf8 必然成功——防御
                // unwrap_or 兜底。
                let word = from_utf8(w).unwrap_or("");
                f.intent = Some(Intent::from_word(word).ok_or_else(|| err("bad intent"))?);
            }
            (Sec::Caps, b"console") => {
                if f.console.is_some() {
                    return Err(err("dup key"));
                }
                f.console = Some(parse_bool(val).ok_or_else(|| err("bad value"))?);
            }
            (Sec::Run, b"stack_pages") => {
                if f.stack_pages.is_some() {
                    return Err(err("dup key"));
                }
                let n = parse_u32(val).ok_or_else(|| err("bad int"))?;
                if !(STACK_PAGES_MIN..=STACK_PAGES_MAX).contains(&n) {
                    return Err(err("range"));
                }
                f.stack_pages = Some(n);
            }
            _ => return Err(err("unknown key")),
        }
    }

    // ---- 收尾：节在场 → 必填键在场 → 语义校验（§2 冻结面）----
    if !saw_app {
        return Err(err("missing [app]"));
    }
    if !saw_caps {
        return Err(err("missing [capabilities]"));
    }
    let id = f.id.ok_or_else(|| err("missing id"))?;
    if !valid_id(id) {
        return Err(err("bad id"));
    }
    let version = f.version.ok_or_else(|| err("missing version"))?;
    if version.is_empty() || version.len() > VERSION_MAX {
        return Err(err("bad version"));
    }
    let entry = f.entry.ok_or_else(|| err("missing entry"))?;
    // entry = 恰 `bin/` 前缀的完整包内路径：短于/等于前缀、超长、无前缀
    // 皆拒（`bin/` 裸前缀 = 空文件名，拒）。
    if entry.len() <= b"bin/".len() || entry.len() > ENTRY_MAX || !entry.starts_with(b"bin/") {
        return Err(err("bad entry"));
    }
    let ty = f.ty.ok_or_else(|| err("missing type"))?;
    if ty != b"app" {
        return Err(err("bad type"));
    }
    let intent = f.intent.ok_or_else(|| err("missing intent"))?;
    let console = f.console.ok_or_else(|| err("missing console"))?;

    // 字节集已由 valid_id/parse_string 验为 ASCII——from_utf8 必然成功，
    // unwrap_or 仅防御（永不 panic 承诺）。
    Ok(Manifest {
        id: from_utf8(id).unwrap_or(""),
        version: from_utf8(version).unwrap_or(""),
        entry: from_utf8(entry).unwrap_or(""),
        console,
        stack_pages: f.stack_pages.unwrap_or(STACK_PAGES_DEFAULT),
        intent,
    })
}

// ---- host 测试面（§3 清单：完整钉板 / 缺省合并 / 坏输入族 + 恰满边界
// 正例 / 真 manifest fixture 共识 / intent 判别值钉板）----

#[cfg(test)]
mod tests {
    use super::*;

    /// §2 样例同形（stack_pages = 16 以区分显式值与缺省 8）。
    const BASE: &str = r#"[app]
id = "APPDEMO"
version = "0.1.0"
entry = "bin/appdemo"
type = "app"
intent = "utility"

[capabilities]
console = true

[runtime]
stack_pages = 16
"#;

    fn ok(s: &str) -> Manifest<'_> {
        parse(s.as_bytes()).expect("parse ok")
    }

    fn bad(s: &str) -> ManifestErr {
        parse(s.as_bytes()).expect_err("parse err")
    }

    // 1. 完整钉板：§2 样例逐字段断言。
    #[test]
    fn full_pin_canonical() {
        let m = ok(BASE);
        assert_eq!(m.id, "APPDEMO");
        assert_eq!(m.version, "0.1.0");
        assert_eq!(m.entry, "bin/appdemo");
        assert!(m.console);
        assert_eq!(m.stack_pages, 16);
        assert_eq!(m.intent, Intent::Utility);
    }

    // 2. 缺省合并：省 [runtime] → stack_pages == 8。
    #[test]
    fn defaults_when_runtime_omitted() {
        let s = BASE.replace("\n[runtime]\nstack_pages = 16\n", "\n");
        assert_eq!(ok(&s).stack_pages, STACK_PAGES_DEFAULT);
    }

    // 2b. 空 [runtime] 节合法（= 全缺省，与省节同义）。
    #[test]
    fn empty_runtime_section_legal() {
        let s = BASE.replace("stack_pages = 16\n", "");
        assert!(s.trim_end().ends_with("[runtime]"));
        assert_eq!(ok(&s).stack_pages, STACK_PAGES_DEFAULT);
    }

    // 3. 恰满边界正例（fsproto out_exact_fit 同族——帽值 `<` vs `<=` 漂移
    // 时任一侧静默分叉、测试照绿的防线）。
    #[test]
    fn exact_fit_boundaries() {
        let id24 = "A".repeat(ID_MAX);
        assert_eq!(ok(&BASE.replace("APPDEMO", &id24)).id, id24);

        let v16 = "0".repeat(VERSION_MAX);
        assert_eq!(ok(&BASE.replace("0.1.0", &v16)).version, v16);

        let e32 = format!("bin/{}", "a".repeat(ENTRY_MAX - 4));
        assert_eq!(e32.len(), ENTRY_MAX);
        assert_eq!(ok(&BASE.replace("bin/appdemo", &e32)).entry, e32);

        assert_eq!(ok(&BASE.replace("16", "1")).stack_pages, STACK_PAGES_MIN);
        assert_eq!(ok(&BASE.replace("16", "64")).stack_pages, STACK_PAGES_MAX);
    }

    // 4. manifest 恰 1024B（注释补零——注释内容零语义面）；1025B 拒。
    #[test]
    fn manifest_exactly_1024_bytes() {
        let mut s = BASE.to_string();
        s.push('\n');
        s.push('#');
        s.push_str(&"a".repeat(MANIFEST_MAX - BASE.len() - 2));
        assert_eq!(s.len(), MANIFEST_MAX);
        assert!(parse(s.as_bytes()).is_ok());

        let mut t = s.clone();
        t.push('a');
        assert_eq!(bad(&t).reason_str(), "too big");
    }

    // 5. 间距容忍集全形：整行/行尾注释（多字节 UTF-8 注释内容）、
    // `[ app ]`、`id="X"` 无空格、行首缩进、空行。
    #[test]
    fn syntax_tolerance_set() {
        let s = "\n# 整行注释——内容任意字节（多字节 UTF-8 亦然）\n[ app ]\nid=\"APPQUIET\"\n  version = \"0.2.0\"\nentry = \"bin/appquiet\"   # 行尾注释\n\ntype = \"app\"\nintent = \"background\"\n\n[capabilities]\nconsole = false   # false = appd 私有采集对\n\n[runtime]\nstack_pages = 8\n";
        let m = ok(s);
        assert_eq!(m.id, "APPQUIET");
        assert_eq!(m.version, "0.2.0");
        assert_eq!(m.entry, "bin/appquiet");
        assert!(!m.console);
        assert_eq!(m.stack_pages, 8);
        assert_eq!(m.intent, Intent::Background);
    }

    // 6. intent 词表往返（大小写敏感；野词 None）。
    #[test]
    fn intent_words_roundtrip() {
        let pairs = [
            ("unspecified", Intent::Unspecified),
            ("latency_critical", Intent::LatencyCritical),
            ("interactive", Intent::Interactive),
            ("utility", Intent::Utility),
            ("background", Intent::Background),
            ("maintenance", Intent::Maintenance),
        ];
        for (w, v) in pairs {
            assert_eq!(Intent::from_word(w), Some(v));
            assert_eq!(v.word(), w);
        }
        assert_eq!(Intent::from_word("urgent"), None);
        assert_eq!(Intent::from_word("Utility"), None);
    }

    // 7. intent 判别值钉板（§3 item 5：跨 crate 词表对账的唯一机器锚）。
    #[test]
    fn intent_discriminants_pinned() {
        assert_eq!(Intent::Unspecified as u8, 0x00);
        assert_eq!(Intent::LatencyCritical as u8, 0x01);
        assert_eq!(Intent::Interactive as u8, 0x02);
        assert_eq!(Intent::Utility as u8, 0x03);
        assert_eq!(Intent::Background as u8, 0x04);
        assert_eq!(Intent::Maintenance as u8, 0x05);
    }

    // 8. 空输入 / 裸输入。
    #[test]
    fn reject_empty_and_bare() {
        assert!(parse(b"").is_err());
        assert!(parse(b"\n\n").is_err());
        // 只有节头无键 → missing 面
        assert!(parse(b"[app]\nid = \"X\"\n").is_err());
    }

    // 9. CR（CRLF 与裸 CR 同拒）与 tab。
    #[test]
    fn reject_cr_and_tab() {
        let s = BASE.replace('\n', "\r\n");
        assert_eq!(bad(&s).reason_str(), "CRLF");
        let s2 = format!("{BASE}\r");
        assert_eq!(bad(&s2).reason_str(), "CRLF");
        let s3 = BASE.replace("id = ", "\tid = ");
        assert_eq!(bad(&s3).reason_str(), "tab");
    }

    // 10. 未知节 / 未知键 / 节前键 / 坏节头 / 越节键。
    #[test]
    fn reject_unknown_and_misplaced() {
        assert_eq!(bad(&BASE.replace("[runtime]", "[ui]")).reason_str(), "unknown section");
        // [ app  ]（双空格）折出 "unknown section"——容忍集外
        assert_eq!(bad(&BASE.replace("[runtime]", "[  runtime ]")).reason_str(), "unknown section");
        assert_eq!(bad(&BASE.replace("[runtime]", "[runtime")).reason_str(), "bad section");
        assert_eq!(bad(&BASE.replace("id = ", "uid = ")).reason_str(), "unknown key");
        // console 出现在 [app] 节 = 越节未知键
        assert_eq!(
            bad(&BASE.replace("type = \"app\"", "type = \"app\"\nconsole = true")).reason_str(),
            "unknown key"
        );
        // 任何节头之前的键
        assert_eq!(
            bad(&BASE.replace("[app]\n", "")).reason_str(),
            "key before section"
        );
        // 缺 [app] 节（无 app 键越节干扰的纯形态）
        assert_eq!(
            bad("[capabilities]\nconsole = true\n").reason_str(),
            "missing [app]"
        );
    }

    // 11. 重复节 / 重复键。
    #[test]
    fn reject_duplicates() {
        assert_eq!(bad(&format!("{BASE}[runtime]\nstack_pages = 4\n")).reason_str(), "dup section");
        assert_eq!(bad(&format!("{BASE}[app]\n")).reason_str(), "dup section");
        assert_eq!(
            bad(&BASE.replace("[runtime]\n", "[runtime]\nstack_pages = 8\n")).reason_str(),
            "dup key"
        );
        assert_eq!(bad(&BASE.replace("[app]\n", "[app]\nid = \"OTHER\"\n")).reason_str(), "dup key");
        assert_eq!(
            bad(&BASE.replace("[capabilities]\n", "[capabilities]\nconsole = false\n"))
                .reason_str(),
            "dup key"
        );
    }

    // 12. 必填缺失六路 + 缺 [capabilities]。
    #[test]
    fn reject_missing_fields() {
        for (line, reason) in [
            ("id = \"APPDEMO\"\n", "missing id"),
            ("version = \"0.1.0\"\n", "missing version"),
            ("entry = \"bin/appdemo\"\n", "missing entry"),
            ("type = \"app\"\n", "missing type"),
            ("intent = \"utility\"\n", "missing intent"),
            ("console = true\n", "missing console"),
        ] {
            assert_eq!(bad(&BASE.replace(line, "")).reason_str(), reason);
        }
        assert_eq!(
            bad(&BASE.replace("[capabilities]\nconsole = true\n", "")).reason_str(),
            "missing [capabilities]"
        );
    }

    // 13. type / intent 词面。
    #[test]
    fn reject_bad_type_and_intent() {
        assert_eq!(bad(&BASE.replace("\"app\"", "\"driver\"")).reason_str(), "bad type");
        assert_eq!(bad(&BASE.replace("\"utility\"", "\"urgent\"")).reason_str(), "bad intent");
        // 裸词（无引号）
        assert_eq!(bad(&BASE.replace("\"utility\"", "utility")).reason_str(), "bad value");
    }

    // 14. 布尔严格面。
    #[test]
    fn reject_bad_bool() {
        assert_eq!(bad(&BASE.replace("true", "yes")).reason_str(), "bad value");
        assert_eq!(bad(&BASE.replace("console = true", "console = \"true\"")).reason_str(), "bad value");
        assert_eq!(bad(&BASE.replace("console = true", "console = 1")).reason_str(), "bad value");
    }

    // 15. 整数族：非数字 / 前缀 / 下划线 / 符号 / 小数 / 溢出 / 空 / 越界。
    #[test]
    fn reject_bad_ints() {
        for (v, reason) in [
            ("1.5", "bad int"),
            ("0x10", "bad int"),
            ("1_0", "bad int"),
            ("+1", "bad int"),
            ("-1", "bad int"),
            ("99999999999", "bad int"), // u32 溢出
            ("\"8\"", "bad int"),       // 引号裹的数字不是整数
            ("", "bad int"),            // 空值
            ("0", "range"),
            ("65", "range"),
        ] {
            let s = BASE.replace("stack_pages = 16", &format!("stack_pages = {v}"));
            assert_eq!(bad(&s).reason_str(), reason, "stack_pages = {v:?}");
        }
    }

    // 16. 字符串族：反斜杠 / 未闭合 / 内嵌引号 / 非可打印。
    #[test]
    fn reject_bad_strings() {
        assert_eq!(bad(&BASE.replace("APPDEMO", "A\\B")).reason_str(), "bad value");
        assert_eq!(bad(&BASE.replace("\"APPDEMO\"", "\"APPDEMO")).reason_str(), "bad value");
        assert_eq!(bad(&BASE.replace("APPDEMO", "A\"B")).reason_str(), "bad value");
        // \x7f（DEL）非可打印 ASCII
        assert_eq!(bad(&BASE.replace("APPDEMO", "A\u{7f}B")).reason_str(), "bad value");
    }

    // 17. id 字符集与长度（含首字符非字母数字、空 id）。
    #[test]
    fn reject_bad_id() {
        assert_eq!(bad(&BASE.replace("APPDEMO", "-APPDEMO")).reason_str(), "bad id");
        assert_eq!(bad(&BASE.replace("APPDEMO", "APP DEMO")).reason_str(), "bad id");
        assert_eq!(bad(&BASE.replace("APPDEMO", "APP/DEMO")).reason_str(), "bad id");
        assert_eq!(bad(&BASE.replace("APPDEMO", "A".repeat(ID_MAX + 1).as_str())).reason_str(), "bad id");
        assert_eq!(bad(&BASE.replace("\"APPDEMO\"", "\"\"")).reason_str(), "bad id");
    }

    // 18. version / entry 形状。
    #[test]
    fn reject_bad_version_entry() {
        assert_eq!(
            bad(&BASE.replace("0.1.0", "0".repeat(VERSION_MAX + 1).as_str())).reason_str(),
            "bad version"
        );
        assert_eq!(bad(&BASE.replace("\"0.1.0\"", "\"\"")).reason_str(), "bad version");
        // 无 bin/ 前缀 / 裸前缀 / 超长
        assert_eq!(bad(&BASE.replace("bin/appdemo", "sbin/appdemo")).reason_str(), "bad entry");
        assert_eq!(bad(&BASE.replace("bin/appdemo", "bin/")).reason_str(), "bad entry");
        let long_entry = format!("bin/{}", "a".repeat(ENTRY_MAX - 3));
        assert!(long_entry.len() > ENTRY_MAX);
        assert_eq!(bad(&BASE.replace("bin/appdemo", &long_entry)).reason_str(), "bad entry");
    }

    // 19. 超集语法逐项（tomllib 收而子集拒——mkapp 门卫自测的运行期对照面）。
    #[test]
    fn reject_superset_syntax() {
        // 点键
        assert_eq!(bad(&BASE.replace("id = ", "app.id = ")).reason_str(), "unknown key");
        // 数组
        assert!(parse(BASE.replace("[capabilities]\nconsole = true", "[capabilities]\nconsole = [1]").as_bytes()).is_err());
        // 内联表
        assert!(parse(BASE.replace("console = true", "console = { on = 1 }").as_bytes()).is_err());
        // 多行字符串（"""……"""——首尾引号剥离后内嵌引号即拒）
        assert!(parse(b"[app]\nid = \"\"\"X\"\"\"\n[capabilities]\nconsole = true\n").is_err());
        // 浮点 / 负 stack_pages 已在 reject_bad_ints 覆盖
    }

    // 20. 真 manifest fixture 共识（§3 item 4 正例半——真源文件不落入
    // 「tomllib 收而子集拒」缝里的单边钉板；负例半在 mkapp 门卫自测）。
    #[test]
    fn fixture_appdemo() {
        const M: &str = include_str!("../../bins/appdemo/manifest.toml");
        let m = ok(M);
        assert_eq!(m.id, "APPDEMO");
        assert_eq!(m.version, "0.1.0");
        assert_eq!(m.entry, "bin/appdemo");
        assert!(m.console);
        assert_eq!(m.stack_pages, STACK_PAGES_DEFAULT); // 省 [runtime]
        assert_eq!(m.intent, Intent::Utility);
    }

    // 21. appquiet：显式 stack_pages = 8 + console = false + background 词。
    #[test]
    fn fixture_appquiet() {
        const M: &str = include_str!("../../bins/appquiet/manifest.toml");
        let m = ok(M);
        assert_eq!(m.id, "APPQUIET");
        assert_eq!(m.version, "0.1.0");
        assert_eq!(m.entry, "bin/appquiet");
        assert!(!m.console);
        assert_eq!(m.stack_pages, 8); // 显式路径
        assert_eq!(m.intent, Intent::Background);
    }

    // 22. 理由截断（「编不出就拒绝」对诊断行放宽为「截断不 panic」）。
    #[test]
    fn err_reason_truncation() {
        let e = ManifestErr::new("abcdefghijklmnopqrstuvwxyz"); // 26B > 24B
        assert_eq!(e.reason_str().len(), REASON_MAX);
        assert!(e.reason_str().ends_with('…'));
        let e2 = ManifestErr::new("short");
        assert_eq!(e2.reason_str(), "short");
        // 现役全部理由 ≤24B 的自证（发射窗契约面）
        for r in [
            "too big", "CRLF", "tab", "bad section", "unknown section", "dup section",
            "bad line", "key before section", "unknown key", "dup key", "bad value",
            "bad intent", "bad int", "range", "missing [app]", "missing [capabilities]",
            "missing id", "bad id", "missing version", "bad version", "missing entry",
            "bad entry", "missing type", "bad type", "missing intent", "missing console",
        ] {
            assert!(r.len() <= REASON_MAX, "{r}");
            assert_eq!(ManifestErr::new(r).reason_str(), r);
        }
    }
}
