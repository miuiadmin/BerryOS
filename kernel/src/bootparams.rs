//! G5 内核半：`etc/boot-params.toml` 消费端（产品配置层 §1.6-补——设计
//! 预置的内核落体；工具侧投放 = `tools/berry_assemble.py` 计划面条目，
//! 生成件归全产线步⑥ mkcpio）。
//!
//! **通道裁决**（任务书 G5 步 1——侦察后 5 行记录）：
//! 1. 「内核读不了 rootfs」前提不成立：rootfs boot 期缓冲（main.rs 三源
//!    装载，:449-614）本就是内核可达字节流——cpio 定址查找是新原语但
//!    机制可行，设计 §1.6-补 原案直接落体。
//! 2. 采原案通道：rootfs cpio（= INITRD 角色）内定址文件
//!    `etc/boot-params.toml`（文本 TOML 子集）——locale 同构 + 内核解析
//!    面零新二进制格式（§1.6-补「与 §1.6-4 的关系」明文）。
//! 3. 候选 a（INITRD.BIN 段承载 params）不采：需给产链新增序列化段面，
//!    与「单一序列化器纪律」冲突且工具侧投放已定文本文件形。
//! 4. 候选 b（svcmgr 早期读 etc 回传内核）不采：svcmgr spawn 在
//!    timer::init/sched init 之后（main.rs :737/:782 vs :1318），回传无
//!    通道且时序倒挂——slice_hz 须在 timer::init 前定。
//! 5. 两键 v1 均内核侧可生效（无需降记「仅 intent_default 生效」）：
//!    sched_slice_hz → timer 换算注入；intent_default → Task 装配注入。
//!
//! **消费契约 v1**：文件缺席（含空 params 不投放的工具侧合同）= 正常
//! 缺省态**零输出**（与 §1.6-补投放面「缺省零扰动」配套——件缺打诊断
//! 行会让每个默认镜像 boot 都多一行，破 smoke 逐行零漂移，此处按机制
//! 修正设计表「件缺 → loud 诊断行」为「件缺 = 静默缺省」；诊断行只对
//! **坏值/坏格式/schema 不符**打，坏键回缺省、好键照常应用——boot 期
//! 无 panic 纪律）。解析有界：内容 ≤ 4KiB、行 ≤ 64（承 §1.6-4 count 帽
//! 32 的解析面定界）；schema 头 ≠ 1 = 未知格式全段弃（防 v2 语义被 v1
//! 内核误用——§1.6-4「超界即弃段用缺省」）。未知键跳过不弃段（v1
//! 内核跑 v2 镜像的向前兼容面——工具侧未知键硬拒是产线执法，两侧
//! 宽严分工非漂移）。
//!
//! eco 打点：`bootparams slice_hz=N slice=CYC intent=WORD`（QEMU 可观测
//! ——只打在位键；slice = 单源算式 [`berry_hal_aarch64::timer::
//! slice_cycles_for_hz`] 的换算直证）。诊断行 `bootparams: <cause>`
//!（首错一条，ASCII）。ktest 纯函数面 = `ktest.rs test_bootparams`
//!（静默执法——G3-1a f4 先例）。

use berry_hal_aarch64 as hal;

/// 定址文件名（§1.6-补投放面定形——`tools/berry_assemble.py`
/// BOOT_PARAMS_ENTRY 单源对读；升版同步义务在此注记）。
const NAME: &[u8] = b"etc/boot-params.toml";

/// 文件内容帽（解析面有界——§1.6-4 精神：32 键 × 词表值 + schema 头 +
/// 注释余量，4KiB 单页帽；超帽 = 坏件弃）。
const FILE_MAX: usize = 4096;

/// 行数帽（count 帽 32 的解析面定界同承 + schema/注释余量）。
const LINE_MAX: usize = 64;

/// 单行长度帽（防御面——词表值最宽 18B、键最宽 15B，128B 之外必坏件）。
const LINE_LEN_MAX: usize = 128;

/// newc 头长：6 魔数 + 13 × 8 hex（`user/berry-loader/src/lib.rs` CPIO_HDR
/// 对读）。
const HDR: usize = 110;

/// 条目名长帽（含 NUL；目标名 20B，归档内条目名最宽 ~30B——防御面）。
const NAME_MAX: usize = 300;

/// 词表 v1 两键的值域（`tools/berry_assemble.py` BOOT_PARAMS_SPEC 单源
/// 对读：sched_slice_hz u32 ∈ [16,1024]）。
const SLICE_HZ_MIN: u32 = 16;
const SLICE_HZ_MAX: u32 = 1024;

/// 六档 intent 词表（ADR #58——`berry_assemble` INTENTS / mkapp 同源镜像；
/// 词 ↔ 码双用：解析（词→码）与 eco 行（码→词）。升版同步义务在此注记）。
pub(crate) const INTENT_WORDS: [(&[u8], u8); 6] = [
    (b"unspecified", berry_sched::INTENT_UNSPECIFIED),
    (b"latency_critical", berry_sched::INTENT_LATENCY_CRITICAL),
    (b"interactive", berry_sched::INTENT_INTERACTIVE),
    (b"utility", berry_sched::INTENT_UTILITY),
    (b"background", berry_sched::INTENT_BACKGROUND),
    (b"maintenance", berry_sched::INTENT_MAINTENANCE),
];

/// rootfs 源视图（main.rs `RootfsBuf` 三形态归一：借片/堆整档字节直读，
/// fw_cfg 多卷拼装件按**逻辑长**（非页对齐容量）走 VMO 只读 read）。
pub enum Src<'a> {
    Bytes(&'a [u8]),
    Vmo(&'a crate::vmo::Vmo, usize),
}

impl Src<'_> {
    fn total(&self) -> usize {
        match self {
            Src::Bytes(b) => b.len(),
            Src::Vmo(_, n) => *n,
        }
    }

    /// 有界读：越界（几何违约）返 false——调用方按「未找到」降级。
    ///（`pub(crate)`：ktest 合成归档数据区对账用。）
    pub(crate) fn read_at(&self, off: usize, out: &mut [u8]) -> bool {
        if off > self.total() || out.len() > self.total() - off {
            return false;
        }
        match self {
            Src::Bytes(b) => {
                out.copy_from_slice(&b[off..off + out.len()]);
                true
            }
            Src::Vmo(v, _) => v.read(off, out).is_ok(),
        }
    }
}

/// 解析产物（全 owned——坏键回 None + 首错诊断；两键独立应用）。
#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    pub slice_hz: Option<u32>,
    pub intent: Option<u8>,
    /// 首错诊断（ASCII 静态串——eco 诊断行载荷；None = 干净）。
    pub diag: Option<&'static str>,
}

/// 全链扫描：cpio 定址查找 → 一次读取 → 有界解析。
/// `None` = 文件缺席（正常缺省态，调用方静默）；`Some` = 已解析件（含
/// 坏件——diag 在内，应用归 [`apply_and_report`]）。
pub fn scan(src: &Src<'_>) -> Option<Outcome> {
    let (off, len) = find(src, NAME)?;
    if len == 0 || len > FILE_MAX {
        return Some(Outcome { slice_hz: None, intent: None, diag: Some("size") });
    }
    let mut content = alloc::vec![0u8; len];
    if !src.read_at(off, &mut content) {
        return Some(Outcome { slice_hz: None, intent: None, diag: Some("read") });
    }
    Some(parse(&content))
}

/// 最小 newc 定址查找（berry-loader `cpio_walk` 同口径的内核侧窄化：
/// 不产迭代器、只找目标名——魔数 070701/070702、页垫 4096 几何、
/// TRAILER!!! 自终止、全 u64 checked 算术全对读；几何违约/截断 = 停走
/// 返 None——rootfs 整体合法性归用户态装载器，此处只作非致命查找）。
/// 找到返 `(data_off, filesize)`。
pub(crate) fn find(src: &Src<'_>, target: &[u8]) -> Option<(usize, usize)> {
    let total = src.total() as u64;
    let mut pos: u64 = 0;
    let mut hdr = [0u8; HDR];
    let mut name = [0u8; NAME_MAX];
    loop {
        // 尾部不足一头（含全零页垫尾/截断）= 干净结束（cpio_walk :94 同款）。
        if pos + HDR as u64 > total {
            return None;
        }
        if !src.read_at(pos as usize, &mut hdr) {
            return None;
        }
        // 魔数：070701 / 070702（cpio_walk :103 同款；check 字段 v1 不校验）。
        if &hdr[..4] != b"0707" || hdr[4] != b'0' || (hdr[5] != b'1' && hdr[5] != b'2') {
            return None;
        }
        let filesize = hex_field(&hdr, 6)?;
        let namesize = hex_field(&hdr, 11)?;
        if namesize == 0 || namesize > NAME_MAX as u64 {
            return None;
        }
        // 几何（mkcpio entry() 垫 4096 契约——「单编码器纪律」第五消费点，
        // 改垫粒度五点同步注记见 mkcpio.py CPIO_PAD）：header → 名（垫页）
        // → 数据（垫页）→ 下条。全 checked（恶意巨值兜住）。
        let name_off = pos + HDR as u64;
        let data_off = (name_off + namesize).checked_add(4095)? & !4095;
        let data_end = data_off.checked_add(filesize)?;
        let next = data_end.checked_add(4095)? & !4095;
        if next > total {
            return None;
        }
        let n = namesize as usize;
        if !src.read_at(name_off as usize, &mut name[..n]) {
            return None;
        }
        if name[n - 1] != 0 {
            return None;
        }
        let nm = &name[..n - 1];
        pos = next;
        if pos == 0 {
            return None; // 零长条目循环防护（垫几何下不可达，防御面）
        }
        if nm == b"TRAILER!!!" {
            return None;
        }
        if nm == target {
            // 目标件尺寸帽在 scan 兜（此处只还几何）。
            return Some((data_off as usize, filesize as usize));
        }
    }
}

/// hex8 字段折算（cpio_walk `field` 同款：大小写 hex 均收；非 hex = None）。
fn hex_field(h: &[u8; HDR], i: usize) -> Option<u64> {
    let s = &h[6 + i * 8..6 + i * 8 + 8];
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

/// TOML 子集解析（投放面格式 v1：`schema = 1` 头 + `key = value` 行，键序
/// 字典序——§1.6-补投放面；容忍空行/`#` 注释/两侧空白）。重复键后者胜
/// （TOML 语义本应拒重复——最小解析器不执法，投放面键序确定性已杜绝）。
pub(crate) fn parse(c: &[u8]) -> Outcome {
    let mut out = Outcome { slice_hz: None, intent: None, diag: None };
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
            _ => out.diag = out.diag.or(Some("key")),
        }
    }
    // schema 头 ≠ 1（缺失/坏值/未来版本）= 未知格式全段弃（§1.6-4 弃段
    // 用缺省；防 v2 语义被 v1 内核误用）——已拾键清空、诊断改 schema。
    if schema != Some(1) {
        return Outcome { slice_hz: None, intent: None, diag: Some("schema") };
    }
    out
}

/// 行内两端空白剥离（空格/Tab；TOML 子集最小面）。
fn trim(s: &[u8]) -> &[u8] {
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
/// 序列化器只产此形）。
fn parse_u32(s: &[u8]) -> Option<u32> {
    if s.is_empty() || s.len() > 9 {
        return None;
    }
    let mut v: u32 = 0;
    for &c in s {
        let d = c.checked_sub(b'0')?;
        v = v * 10 + d as u32;
    }
    Some(v)
}

/// intent 词 → 码（带引号形 `"word"`——投放面 TOML 字符串字面量）。
fn intent_code(val: &[u8]) -> Option<u8> {
    if val.len() < 2 || val[0] != b'"' || val[val.len() - 1] != b'"' {
        return None;
    }
    let word = &val[1..val.len() - 1];
    INTENT_WORDS
        .iter()
        .find(|(w, _)| *w == word)
        .map(|&(_, c)| c)
}

/// 应用 + eco 打点（调用点 = main.rs rootfs 装载段，ROOTFS ok 行之后）。
///
/// 注入点时序：`timer::set_slice_hz` 须在 `timer::init`（main.rs :737）前
/// ——SLICE 换算基准即改；`sched::set_intent_default` 须在首个 spawn
/// （:789 起）前——新任务装配即带。两处本函数调用点均满足。
pub fn apply_and_report(o: &Outcome) {
    if let Some(hz) = o.slice_hz {
        // SAFETY: boot 单线程、timer::init 之前（唯一调用点）。
        unsafe { hal::timer::set_slice_hz(hz as u64) };
    }
    if let Some(code) = o.intent {
        berry_sched::set_intent_default(code);
    }
    // 全段弃（schema 拒面：两键皆空）只打诊断行——eco 行只在有在位键时打。
    if o.slice_hz.is_some() || o.intent.is_some() {
        hal::uart_puts(b"bootparams");
        if let Some(hz) = o.slice_hz {
            hal::uart_puts(b" slice_hz=");
            hal::uart_put_dec(hz as u64);
            // 换算直证：单源算式同 timer::init 的 SLICE（62.5MHz/250 = 250000）。
            hal::uart_puts(b" slice=");
            hal::uart_put_dec(hal::timer::slice_cycles_for_hz(hz as u64));
        }
        if let Some(code) = o.intent {
            hal::uart_puts(b" intent=");
            // 码 → 词（解析面镜像反查；域外码不可能——parse 只出词表码）。
            if let Some((w, _)) = INTENT_WORDS.iter().find(|&(_, c)| *c == code) {
                hal::uart_puts(w);
            } else {
                hal::uart_put_dec(code as u64);
            }
        }
        hal::uart_puts(b"\n");
    }
    if let Some(d) = o.diag {
        hal::uart_puts(b"bootparams: ");
        hal::uart_puts(d.as_bytes());
        hal::uart_puts(b"\n");
    }
}
