//! 信任账本 TOML 源形态解析/发射/校验 + 单调水位比较器。
//!
//! **Q6 面（P0 合入 = 闸 B = 重议窗关闭，见 crate 头注）**：schema 照
//! `docs/SDK-2-分发与签名链设计-v1.2.md` §二 :38–45 + `research/厂商定制化
//! 深研/09 号` §2.4 :153–171 落形（24 号 §3.5 逐字段照抄，**零新增字段**）：
//!
//! ```toml
//! # trust-ledger v1（host 源形态；分发/设备形态 = TLV，随 P2/P3 转译）
//! schema = 1                             # u32，add-only
//!
//! [[lineage]]
//! id = "upstream"                        # 1..=16B [a-z0-9-]（帽同 SEC_VAL_MAX）
//! anchors = [
//!   { pk = "<64 hex>", type = "system" },# anchor_type ∈ tofu|system|ca|oem-system
//! ]
//! gen_max = 3                            # 单调水位（v1.2 §三 :63 护栏②）
//! ver_max = 0                            # per-app 水位归 per-app 记录账本（P3）
//! kind = ""                              # V4 占位：integrator|odm|developer，v1 恒空
//! issued_by = ""                         # V4 占位：v1 恒空（09 §2.4 :161）
//! cert_status = ""                       # V4 占位：事实源 = 签名索引（v1.2 §二 :43–44）
//! last_revision = 0                      # V4 占位：v1 恒空（09 §3.1 :213）
//! ```
//!
//! 解析纪律五条（24 号 §3.5）：①未知 `anchor_type` **保守拒绝**——09 号
//! §2.4 :169「不可评估」按硬拒落（不降级跳过；本 crate 读法：解析层硬拒
//! = loud 失败，防「账本作者写了新枚举、旧验证器静默无视」的两步发布纪律
//! 破坏面——新枚举进账本前全网最低验证器先升，v1.2 §二 :44–45）；②
//! 未知键硬拒（V12 product 清单同款）；③`schema` ≠ 1 硬拒（前向：升版走
//! add-only 轨）；④水位比较器独立纯函数（[`check_gen`]——供 P1 门禁与
//! P2/P3 复用，单源不两套）；⑤两步发布纪律注释位见 crate 头注（v1 单实现
//! 期空操作）。**V4 四占位字段 v1 值恒空**——非空值 = 旧验证器不可评估 →
//! 保守拒（[LedgerErr::V4NonEmpty]；v1.2 §二 :42「v1 值恒空/active」的
//! 「active」形随 Q6 终核三条主会话终裁，本刀按 24 号 §3.5 例样 `""` 落）。
//!
//! TOML 解析器落位：本模块**自持账本专用行解析器**（berry-manifest 逐行
//! 形态同款）——非「第二套」：解析的是不同 schema（「不长第二套」指同一
//! 格式/同一事实源不长两套解析器；berry-layout JSON 与 manifest TOML 并存
//! 即此格局，24 号 §3.5）。多行 `anchors = [...]` 内联表数组以括号深度
//! 收口跨行延伸（值窗 = 源字节连续窗——零分配；EOF 未闭合 =
//! [LedgerErr::BadAnchors]）。
//!
//! 发射（[`TrustLedger::emit`]）= 数据面 canonical 形（无注释、小写 hex、
//! 确定字段序）——parse∘emit 幂等钉板；拒因 BerryErr 化（[LedgerErr]
//! 结构化枚举，add-only）。

use crate::envelope::PK_LEN;

/// 账本 schema 版本（add-only；≠1 硬拒——前向升版走加轨不走改轨）。
pub const SCHEMA_V1: u32 = 1;
/// lineage id 帽：1..=16B `[a-z0-9-]`——与 [`crate::envelope::LINEAGE_MAX`] /
/// berry-manifest `SEC_VAL_MAX` / mkapp `SEC_RE` 同值同语义（单源同步义务）。
pub const LINEAGE_ID_MAX: usize = 16;
/// 公钥 hex 形长（32B 小写 hex——canonical 单形，大写拒）。
pub const PK_HEX_LEN: usize = 64;
/// 每 lineage 锚数帽（**推断**——24 号未设帽；零分配定长槽）。
pub const ANCHORS_MAX: usize = 8;
/// 账本 lineage 条目数帽（**推断**——同上）。
pub const LINEAGES_MAX: usize = 8;
/// 源文本帽（**推断**——防敌意巨值，berry-manifest `MANIFEST_MAX` 同思路；
/// 8 lineage × 8 anchor × 例样行宽量级上取整）。
pub const LEDGER_SRC_MAX: usize = 4096;

/// 锚类型（add-only 枚举；`ca` = 终态案 C 前向兼容口，`oem-system` = ADR
/// #82 V4 预埋 OEM 系统锚——闸 B 件，v1 值恒空位）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorType {
    Tofu,
    System,
    Ca,
    /// OEM 系统锚（v1 值恒空——条目四字段占位即终态，Q6 终核三条②）。
    OemSystem,
}

impl AnchorType {
    /// 词表（add-only 只加不改义；未知词保守拒）。
    pub const WORDS: &'static [&'static str] = &["tofu", "system", "ca", "oem-system"];

    fn parse(word: &[u8]) -> Option<Self> {
        match word {
            b"tofu" => Some(AnchorType::Tofu),
            b"system" => Some(AnchorType::System),
            b"ca" => Some(AnchorType::Ca),
            b"oem-system" => Some(AnchorType::OemSystem),
            _ => None,
        }
    }

    fn word(&self) -> &'static str {
        match self {
            AnchorType::Tofu => "tofu",
            AnchorType::System => "system",
            AnchorType::Ca => "ca",
            AnchorType::OemSystem => "oem-system",
        }
    }
}

/// 信任锚：公钥 + 锚类型（信任权重凭据——oem-system 是系统件特权通道准入，
/// tofu 不是；09 §2.4 纪律 4：kind/issued_by 是元数据不是委托）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    /// ed25519 公钥（hex 解码后 32B）。
    pub pk: [u8; PK_LEN],
    /// 锚类型。
    pub ty: AnchorType,
}

/// lineage 条目（零分配定长槽：锚定长 + 在用数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineageEntry {
    /// lineage id（`[a-z0-9-]`，1..=16B）。
    pub id: [u8; LINEAGE_ID_MAX],
    /// id 在用长。
    pub id_len: usize,
    /// 信任锚集。
    pub anchors: [Anchor; ANCHORS_MAX],
    /// 锚在用数（≥1——零锚条目无信任语义，拒）。
    pub anchors_len: usize,
    /// 单调水位：已见最高代际（v1.2 §三 :63 护栏②防回放）。
    pub gen_max: u32,
    /// per-app 水位归 per-app 记录账本（P3，v1.2 §五 :98）——本位恒 0 占位。
    pub ver_max: u32,
}

impl LineageEntry {
    /// id 切片（在用段）。
    pub fn id(&self) -> &[u8] {
        &self.id[..self.id_len]
    }

    /// 锚切片（在用段）。
    pub fn anchors(&self) -> &[Anchor] {
        &self.anchors[..self.anchors_len]
    }
}

/// 信任账本 v1（host 源形态解析产物）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustLedger {
    /// 条目定长槽 + 在用数。
    pub lineages: [LineageEntry; LINEAGES_MAX],
    /// 条目在用数。
    pub lineages_len: usize,
}

impl TrustLedger {
    /// 条目切片（在用段）。
    pub fn lineages(&self) -> &[LineageEntry] {
        &self.lineages[..self.lineages_len]
    }

    /// 按 id 查条目（P1 门禁/P2 boot 对账消费面）。
    pub fn find_lineage(&self, id: &[u8]) -> Option<&LineageEntry> {
        self.lineages().iter().find(|e| e.id() == id)
    }

    /// 单一解释解析（永不 panic、零分配、输入只借用）。
    pub fn parse(src: &[u8]) -> Result<Self, LedgerErr> {
        if src.len() > LEDGER_SRC_MAX {
            return Err(LedgerErr::TooLarge);
        }
        // 全文级拒 CR/tab（berry-manifest 同口径——行形解析器的静默面收口）。
        if src.contains(&b'\r') || src.contains(&b'\t') {
            return Err(LedgerErr::BadLine);
        }
        let empty = LineageEntry {
            id: [0; LINEAGE_ID_MAX],
            id_len: 0,
            anchors: [Anchor { pk: [0; PK_LEN], ty: AnchorType::Tofu }; ANCHORS_MAX],
            anchors_len: 0,
            gen_max: 0,
            ver_max: 0,
        };
        let mut ledger = TrustLedger { lineages: [empty; LINEAGES_MAX], lineages_len: 0 };
        let mut saw_schema = false;
        // 当前裸键归属：顶层（false）或最近 [[lineage]]（true）——TOML 表
        // 头后裸键归该表（berry-manifest 节追踪同款）。
        let mut in_lineage = false;
        let mut have = EntryFields::default();
        let mut lines = LineFeeder { src, pos: 0 };
        while let Some((raw, ls, le)) = lines.peek() {
            let t = trim(strip_comment(raw));
            if t.is_empty() {
                lines.advance(le);
                continue;
            }
            if let Some(rest) = cut_prefix(t, b"[[lineage]]") {
                if !rest.is_empty() {
                    return Err(LedgerErr::BadLine);
                }
                lines.advance(le);
                if in_lineage {
                    finish_entry(&mut ledger, &have)?;
                }
                if ledger.lineages_len >= LINEAGES_MAX {
                    return Err(LedgerErr::TooManyLineages);
                }
                ledger.lineages_len += 1;
                in_lineage = true;
                have = EntryFields::default();
                continue;
            }
            if t.starts_with(b"[[") || t.starts_with(b"[") {
                return Err(LedgerErr::UnknownSection);
            }
            let (key, val) = split_kv(t)?;
            if !in_lineage {
                if key != b"schema" {
                    return Err(LedgerErr::UnknownKey);
                }
                if saw_schema || parse_u32(val)? != SCHEMA_V1 {
                    return Err(LedgerErr::BadSchema);
                }
                saw_schema = true;
                lines.advance(le);
                continue;
            }
            match key {
                b"anchors" => {
                    // 多行内联表数组：值窗 = 源字节连续窗（零分配）。首行
                    // 值区 = '=' 后首非空格起、剥注释后行尾止；括号深度未
                    // 闭合则逐物理行延伸（注释行按剥后空行跳过——括号计数
                    // 不见 '#'，串外 '#' 语法上只可能是注释）。
                    let stripped = strip_comment(raw);
                    let eq = stripped.iter().position(|&b| b == b'=').ok_or(LedgerErr::BadLine)?;
                    let mut vstart = ls + eq + 1;
                    while vstart < ls + stripped.len() && src[vstart] == b' ' {
                        vstart += 1;
                    }
                    let mut vend = ls + stripped.len();
                    lines.advance(le);
                    while bracket_depth(&src[vstart..vend]) > 0 {
                        let (raw2, ls2, le2) = lines.peek().ok_or(LedgerErr::BadAnchors)?;
                        vend = ls2 + strip_comment(raw2).len();
                        lines.advance(le2);
                    }
                    let n = parse_anchors(&src[vstart..vend], &mut ledger.lineages
                        [ledger.lineages_len - 1]
                        .anchors)?;
                    ledger.lineages[ledger.lineages_len - 1].anchors_len = n;
                }
                _ => {
                    let slot = &mut ledger.lineages[ledger.lineages_len - 1];
                    match key {
                        b"id" => {
                            let v = parse_quoted(val)?;
                            if v.is_empty() || v.len() > LINEAGE_ID_MAX || !valid_lineage_id(v) {
                                return Err(LedgerErr::BadLineage);
                            }
                            slot.id_len = v.len();
                            slot.id[..v.len()].copy_from_slice(v);
                        }
                        b"gen_max" => slot.gen_max = parse_u32(val)?,
                        b"ver_max" => slot.ver_max = parse_u32(val)?,
                        b"kind" | b"issued_by" | b"cert_status" => {
                            if !parse_quoted(val)?.is_empty() {
                                return Err(LedgerErr::V4NonEmpty);
                            }
                        }
                        b"last_revision" => {
                            if parse_u32(val)? != 0 {
                                return Err(LedgerErr::V4NonEmpty);
                            }
                        }
                        _ => return Err(LedgerErr::UnknownKey),
                    }
                    lines.advance(le);
                }
            }
            have.mark(key);
        }
        if !saw_schema {
            return Err(LedgerErr::MissingKey);
        }
        if in_lineage {
            finish_entry(&mut ledger, &have)?;
        }
        if ledger.lineages_len == 0 {
            return Err(LedgerErr::NoLineage);
        }
        Ok(ledger)
    }

    /// canonical 发射（数据面无注释形、小写 hex、确定字段序）。缓冲不足 =
    /// [LedgerErr::BadOutLen]；parse∘emit 幂等钉板在 tests。
    pub fn emit(&self, out: &mut [u8]) -> Result<usize, LedgerErr> {
        let mut w = Writer { out, off: 0, overflow: false };
        w.put(b"schema = 1\n");
        for e in self.lineages() {
            w.put(b"\n[[lineage]]\nid = \"");
            w.put(e.id());
            w.put(b"\"\nanchors = [\n");
            for a in e.anchors() {
                w.put(b"  { pk = \"");
                w.put_hex(&a.pk);
                w.put(b"\", type = \"");
                w.put(a.ty.word().as_bytes());
                w.put(b"\" },\n");
            }
            w.put(b"]\ngen_max = ");
            w.put_dec(e.gen_max);
            w.put(b"\nver_max = ");
            w.put_dec(e.ver_max);
            w.put(b"\nkind = \"\"\nissued_by = \"\"\ncert_status = \"\"\nlast_revision = ");
            w.put_dec(0);
            w.put(b"\n");
        }
        if w.overflow {
            return Err(LedgerErr::BadOutLen);
        }
        Ok(w.off)
    }
}

/// gen 水位比较器（24 号 §3.5 纪律④：独立纯函数，单源不两套——P1 门禁
/// 与 P2/P3 设备侧共用）。**回降拒**：信封 gen < 账本 gen_max = 回放/回滚
/// → [LedgerErr::GenRollback]；gen ≥ gen_max 过（等值 = 同代际重放幂等面，
/// 水位不降）。
pub fn check_gen(gen: u32, gen_max: u32) -> Result<(), LedgerErr> {
    if gen < gen_max {
        Err(LedgerErr::GenRollback)
    } else {
        Ok(())
    }
}

/// 水位推进（接受信封后的新 gen_max = max 旧值, gen——P1/P2 更新面单源）。
pub const fn bump_gen(gen_max: u32, gen: u32) -> u32 {
    if gen > gen_max {
        gen
    } else {
        gen_max
    }
}

// ---- 内部：行供给/剥离/解析（berry-manifest 逐行形态同族）----

/// 条目字段齐备性槽（24 号 §3.5 例样九键全必填——缺任一 = MissingKey）。
#[derive(Default)]
struct EntryFields {
    id: bool,
    anchors: bool,
    gen_max: bool,
    ver_max: bool,
    kind: bool,
    issued_by: bool,
    cert_status: bool,
    last_revision: bool,
}

impl EntryFields {
    fn mark(&mut self, key: &[u8]) {
        match key {
            b"id" => self.id = true,
            b"anchors" => self.anchors = true,
            b"gen_max" => self.gen_max = true,
            b"ver_max" => self.ver_max = true,
            b"kind" => self.kind = true,
            b"issued_by" => self.issued_by = true,
            b"cert_status" => self.cert_status = true,
            b"last_revision" => self.last_revision = true,
            _ => {}
        }
    }

    fn all(&self) -> bool {
        self.id
            && self.anchors
            && self.gen_max
            && self.ver_max
            && self.kind
            && self.issued_by
            && self.cert_status
            && self.last_revision
    }
}

/// 条目收口：九键齐备 + id/锚非空 + 条目 id 不与在册撞。
fn finish_entry(ledger: &mut TrustLedger, have: &EntryFields) -> Result<(), LedgerErr> {
    if !have.all() {
        return Err(LedgerErr::MissingKey);
    }
    let i = ledger.lineages_len - 1;
    if ledger.lineages[i].id_len == 0 {
        return Err(LedgerErr::BadLineage);
    }
    if ledger.lineages[i].anchors_len == 0 {
        return Err(LedgerErr::BadAnchors);
    }
    let id = ledger.lineages[i].id();
    for prior in &ledger.lineages[..i] {
        if prior.id() == id {
            return Err(LedgerErr::DupLineage);
        }
    }
    Ok(())
}

/// 偏移式行供给（不消费式 peek + 显式 advance——anchors 值窗跨行延伸须
/// 持有行区间；line 切片与偏移皆自 `src` 派生，零分配）。
struct LineFeeder<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> LineFeeder<'a> {
    /// 下一物理行（不消费）：`(行切片, 行首偏移, 行尾偏移)`——行尾不含
    /// `\n`；文尽 = None。
    fn peek(&self) -> Option<(&'a [u8], usize, usize)> {
        if self.pos >= self.src.len() {
            return None;
        }
        let rest = &self.src[self.pos..];
        let end = match rest.iter().position(|&b| b == b'\n') {
            Some(i) => self.pos + i,
            None => self.src.len(),
        };
        Some((&self.src[self.pos..end], self.pos, end))
    }

    /// 消费到行尾之后（含换行）。
    fn advance(&mut self, line_end: usize) {
        self.pos = (line_end + 1).min(self.src.len());
    }
}

/// `#` 注释剥离：字符串外首个 `#` 起截断（berry-manifest 同款；anchors
/// 跨行窗按行剥——`#` 不进括号深度计数面）。
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

/// 两端去空格。
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

fn cut_prefix<'a>(s: &'a [u8], p: &[u8]) -> Option<&'a [u8]> {
    if s.len() >= p.len() && &s[..p.len()] == p {
        Some(&s[p.len()..])
    } else {
        None
    }
}

/// `key = value` 切分（无 `=` / 空键 / 键含空格 / 空值 = BadLine）。
fn split_kv(t: &[u8]) -> Result<(&[u8], &[u8]), LedgerErr> {
    let eq = t.iter().position(|&b| b == b'=').ok_or(LedgerErr::BadLine)?;
    let key = trim(&t[..eq]);
    let val = trim(&t[eq + 1..]);
    if key.is_empty() || val.is_empty() || key.contains(&b' ') {
        return Err(LedgerErr::BadLine);
    }
    Ok((key, val))
}

/// 带引号字符串值：`"..."`（无转义——值域全 ASCII 词表/hex，反斜杠/内
/// 嵌引号/内嵌 `#` 拒——canonical 单形）。
fn parse_quoted(val: &[u8]) -> Result<&[u8], LedgerErr> {
    if val.len() < 2 || val[0] != b'"' || val[val.len() - 1] != b'"' {
        return Err(LedgerErr::BadLine);
    }
    let inner = &val[1..val.len() - 1];
    if inner.contains(&b'"') || inner.contains(&b'\\') || inner.contains(&b'#') {
        return Err(LedgerErr::BadLine);
    }
    Ok(inner)
}

/// 纯十进制 u32（无前导零——"0" 本身合法；berry-manifest SdkVer 同口径）。
fn parse_u32(val: &[u8]) -> Result<u32, LedgerErr> {
    if val.is_empty() || val.len() > 10 || !val.iter().all(|b| b.is_ascii_digit()) {
        return Err(LedgerErr::BadNum);
    }
    if val.len() > 1 && val[0] == b'0' {
        return Err(LedgerErr::BadNum);
    }
    let mut n: u32 = 0;
    for &b in val {
        n = n
            .checked_mul(10)
            .and_then(|n| n.checked_add((b - b'0') as u32))
            .ok_or(LedgerErr::BadNum)?;
    }
    Ok(n)
}

fn valid_lineage_id(s: &[u8]) -> bool {
    s.iter().all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// 括号深度（字符串外计 `[`/`]`/`{`/`}`——多行 anchors 收口判据）。
fn bracket_depth(s: &[u8]) -> i32 {
    let mut d = 0i32;
    let mut in_str = false;
    for &b in s {
        match b {
            b'"' => in_str = !in_str,
            b'[' | b'{' if !in_str => d += 1,
            b']' | b'}' if !in_str => d -= 1,
            _ => {}
        }
    }
    d
}

/// anchors 值窗游标（token 化：空白/换行/注释跳过——值窗是源字节连续
/// 窗，中间行的行尾注释原文在窗内，`#` 至行尾按注释跳过；裸词/带引号
/// 串/单字符吃——语法位保证 skip 只发生在串外）。
struct Cur<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Cur<'a> {
    fn new(s: &'a [u8]) -> Self {
        Cur { s, i: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn eat(&mut self, b: u8) -> bool {
        if self.peek() == Some(b) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    /// 空白 + 注释跳过（`#` 至行尾）。
    fn skip_blank(&mut self) {
        loop {
            match self.peek() {
                Some(b' ') | Some(b'\n') => self.i += 1,
                Some(b'#') => {
                    while self.peek().is_some() && self.peek() != Some(b'\n') {
                        self.i += 1;
                    }
                }
                _ => return,
            }
        }
    }

    /// 窗尽（先跳空白/注释）。
    fn done(&mut self) -> bool {
        self.skip_blank();
        self.i >= self.s.len()
    }

    /// 裸词（`[a-z0-9_-]+`——键名面）。
    fn word(&mut self) -> Result<&'a [u8], LedgerErr> {
        let start = self.i;
        while matches!(self.peek(), Some(b'a'..=b'z') | Some(b'0'..=b'9') | Some(b'_') | Some(b'-')) {
            self.i += 1;
        }
        if self.i == start {
            return Err(LedgerErr::BadLine);
        }
        Ok(&self.s[start..self.i])
    }

    /// 带引号串（无转义——内嵌 `"`/`\`/`#` 拒，canonical 单形）。
    fn quoted(&mut self) -> Result<&'a [u8], LedgerErr> {
        if !self.eat(b'"') {
            return Err(LedgerErr::BadLine);
        }
        let start = self.i;
        loop {
            match self.peek() {
                None => return Err(LedgerErr::BadLine),
                Some(b'"') => {
                    let v = &self.s[start..self.i];
                    self.i += 1;
                    return Ok(v);
                }
                Some(b'\\') | Some(b'#') => return Err(LedgerErr::BadLine),
                Some(_) => self.i += 1,
            }
        }
    }
}

/// anchors 内联表数组解析：`[ { pk = "…", type = "…" }, … ]`（≥1 锚——
/// 零锚条目无信任语义，**推断**注；pk 小写 hex 64 恰满；type 词表内；
/// 键序两可；重复键/未知键拒；内联表内尾逗号拒（TOML 严格形）；数组尾
/// 逗号容差（例样在册）；同条目内 pk 重复拒——同钥双锚 = 信任权重歧义
/// 面，canonical 单形）。
fn parse_anchors(val: &[u8], out: &mut [Anchor; ANCHORS_MAX]) -> Result<usize, LedgerErr> {
    let mut c = Cur::new(val);
    c.skip_blank();
    if !c.eat(b'[') {
        return Err(LedgerErr::BadAnchors);
    }
    c.skip_blank();
    if c.eat(b']') {
        return Err(LedgerErr::BadAnchors); // 零锚
    }
    let mut n = 0;
    loop {
        // 一锚：{ pk = "..", type = ".." }（键序两可）。
        if !c.eat(b'{') {
            return Err(LedgerErr::BadAnchors);
        }
        let mut pk: Option<[u8; PK_LEN]> = None;
        let mut ty: Option<AnchorType> = None;
        loop {
            c.skip_blank();
            if c.eat(b'}') {
                break;
            }
            let key = c.word()?;
            c.skip_blank();
            if !c.eat(b'=') {
                return Err(LedgerErr::BadLine);
            }
            c.skip_blank();
            let v = c.quoted()?;
            match key {
                b"pk" => {
                    if pk.is_some() {
                        return Err(LedgerErr::DupKey);
                    }
                    if v.len() != PK_HEX_LEN {
                        return Err(LedgerErr::BadPk);
                    }
                    let mut buf = [0u8; PK_LEN];
                    hex_decode(v, &mut buf).ok_or(LedgerErr::BadPk)?;
                    pk = Some(buf);
                }
                b"type" => {
                    if ty.is_some() {
                        return Err(LedgerErr::DupKey);
                    }
                    ty = Some(AnchorType::parse(v).ok_or(LedgerErr::UnknownAnchorType)?);
                }
                _ => return Err(LedgerErr::UnknownKey),
            }
            c.skip_blank();
            if c.eat(b',') {
                // 严格形：表内尾逗号拒（',' 后必随键）。
                c.skip_blank();
                if c.peek() == Some(b'}') {
                    return Err(LedgerErr::BadAnchors);
                }
            }
        }
        let pk = pk.ok_or(LedgerErr::MissingKey)?;
        let ty = ty.ok_or(LedgerErr::MissingKey)?;
        if n >= ANCHORS_MAX {
            return Err(LedgerErr::BadAnchors);
        }
        for a in &out[..n] {
            if a.pk == pk {
                return Err(LedgerErr::DupAnchor);
            }
        }
        out[n] = Anchor { pk, ty };
        n += 1;
        // 项后：']' 收口 / ',' 续项（数组尾逗号容差）。
        c.skip_blank();
        if c.eat(b']') {
            break;
        }
        if !c.eat(b',') {
            return Err(LedgerErr::BadAnchors);
        }
        c.skip_blank();
        if c.peek() == Some(b']') {
            c.i += 1;
            break;
        }
    }
    if !c.done() {
        return Err(LedgerErr::BadAnchors); // 尾随垃圾拒
    }
    if n == 0 {
        return Err(LedgerErr::BadAnchors);
    }
    Ok(n)
}

/// 小写 hex 解码（canonical 单形：大写拒）。
fn hex_decode(s: &[u8], out: &mut [u8]) -> Option<()> {
    if s.len() != out.len() * 2 {
        return None;
    }
    for (i, pair) in s.chunks(2).enumerate() {
        let hi = hex_nibble(pair[0])?;
        let lo = hex_nibble(pair[1])?;
        out[i] = (hi << 4) | lo;
    }
    Some(())
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

/// 溢出容忍定长缓冲顺序写（零分配发射——过量 = overflow 旗，emit 收口
/// 转 BadOutLen；不 panic）。
struct Writer<'a> {
    out: &'a mut [u8],
    off: usize,
    overflow: bool,
}

impl<'a> Writer<'a> {
    fn put(&mut self, b: &[u8]) {
        if self.off + b.len() <= self.out.len() {
            self.out[self.off..self.off + b.len()].copy_from_slice(b);
            self.off += b.len();
        } else {
            self.overflow = true;
        }
    }

    fn put_hex(&mut self, b: &[u8]) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for &byte in b {
            self.put(&[HEX[(byte >> 4) as usize], HEX[(byte & 0xF) as usize]]);
        }
    }

    fn put_dec(&mut self, v: u32) {
        let mut buf = [0u8; 10];
        let mut n = 0;
        let mut x = v;
        loop {
            buf[n] = b'0' + (x % 10) as u8;
            n += 1;
            x /= 10;
            if x == 0 {
                break;
            }
        }
        for i in (0..n).rev() {
            self.put(&[buf[i]]);
        }
    }
}

/// 拒因（结构化枚举，add-only——BerryErr 化纪律同 [crate::envelope::SigErr]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerErr {
    /// 源文本超 [`LEDGER_SRC_MAX`] 帽。
    TooLarge,
    /// 行形坏（无 `=`/空键/裸 `#` 错位/CR/tab/引号错）。
    BadLine,
    /// 未知节头（仅 `[[lineage]]` 在册）。
    UnknownSection,
    /// 未知键（顶层仅 `schema`；条目九键闭集；内联表仅 pk/type）。
    UnknownKey,
    /// 重复键。
    DupKey,
    /// 必填键缺失（schema 或条目九键）。
    MissingKey,
    /// `schema` 非 1/重复出现。
    BadSchema,
    /// id 值域外（空/越帽/字符集外）。
    BadLineage,
    /// 数字非纯十进制/前导零/溢出。
    BadNum,
    /// anchors 形坏/零锚/越 [`ANCHORS_MAX`] 帽/未闭合。
    BadAnchors,
    /// pk 非 64 小写 hex。
    BadPk,
    /// 未知 anchor_type（保守拒——两步发布纪律执法面）。
    UnknownAnchorType,
    /// V4 占位字段 v1 非空（保守拒）。
    V4NonEmpty,
    /// 条目 id 撞已在册条目。
    DupLineage,
    /// 同条目内 pk 重复锚。
    DupAnchor,
    /// 条目数越 [`LINEAGES_MAX`] 帽。
    TooManyLineages,
    /// 零 `[[lineage]]` 条目（空账本无信任语义）。
    NoLineage,
    /// 发射缓冲不足。
    BadOutLen,
    /// gen 水位回降（[`check_gen`] 执法面——防回放，v1.2 §三 :63 护栏②）。
    GenRollback,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 24 号 §3.5 例样全文（注释含——源形态人读面；pk 用合成 64hex）。
    const EXEMPLAR: &str = r#"# trust-ledger v1（host 源形态；分发/设备形态 = TLV，随 P2/P3 转译）
schema = 1                             # u32，add-only（对照 V12 product 清单 schema=1 先例）

[[lineage]]
id = "upstream"                        # 1..=16B [a-z0-9-]（帽同 berry-manifest SEC_VAL_MAX）
anchors = [
  { pk = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", type = "system" },# anchor_type ∈ tofu|system|ca|oem-system（add-only）
]
gen_max = 3                            # 单调水位（v1.2 §三 :63 护栏②）
ver_max = 0                            # per-app 水位归 per-app 记录账本（P3，§五 :98）
kind = ""                              # V4 占位：integrator|odm|developer，v1 恒空
issued_by = ""                         # V4 占位：v1 恒空（09 §2.4 :161）
cert_status = ""                       # V4 占位：事实源 = 签名索引，账本不存或只读回显（v1.2 §二 :43–44）
last_revision = 0                      # V4 占位：v1 恒空（09 §3.1 :213）
"#;

    /// EXEMPLAR 的 canonical 发射形（数据面无注释；raw 串保缩进——`\` 续行
    /// 会吃行首空白，锚行两空格缩进正是发射形一部分）。
    const CANONICAL: &str = r#"schema = 1

[[lineage]]
id = "upstream"
anchors = [
  { pk = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", type = "system" },
]
gen_max = 3
ver_max = 0
kind = ""
issued_by = ""
cert_status = ""
last_revision = 0
"#;

    fn ok(s: &str) -> TrustLedger {
        TrustLedger::parse(s.as_bytes()).expect("parse ok")
    }

    fn bad(s: &str) -> LedgerErr {
        TrustLedger::parse(s.as_bytes()).expect_err("parse err")
    }

    // 1. 例样解析钉板：字段逐个（id/锚/pk 解码值/水位/V4 占位空）。
    #[test]
    fn exemplar_fields_pin() {
        let l = ok(EXEMPLAR);
        assert_eq!(l.lineages_len, 1);
        let e = &l.lineages[0];
        assert_eq!(e.id(), b"upstream");
        assert_eq!(e.anchors_len, 1);
        assert_eq!(e.anchors()[0].ty, AnchorType::System);
        assert_eq!(e.anchors()[0].pk[0], 0x01);
        assert_eq!(e.anchors()[0].pk[31], 0xef);
        assert_eq!(e.gen_max, 3);
        assert_eq!(e.ver_max, 0);
        assert_eq!(l.find_lineage(b"upstream"), Some(e));
        assert_eq!(l.find_lineage(b"oem"), None);
    }

    // 2. canonical round-trip：parse(例样含注释) → emit = CANONICAL 字节级；
    //    parse(CANONICAL) 结构恒等；emit 幂等（emit(parse(emit)) 字节同）。
    #[test]
    fn canonical_roundtrip() {
        let l = ok(EXEMPLAR);
        let mut out = [0u8; LEDGER_SRC_MAX];
        let n = l.emit(&mut out).expect("emit ok");
        assert_eq!(&out[..n], CANONICAL.as_bytes());
        assert_eq!(n, CANONICAL.len());
        let l2 = ok(core::str::from_utf8(&out[..n]).unwrap());
        assert_eq!(l2, l);
        let mut out2 = [0u8; LEDGER_SRC_MAX];
        let n2 = l2.emit(&mut out2).expect("emit2 ok");
        assert_eq!(&out2[..n2], &out[..n]);
        // 缓冲不足拒。
        let mut small = [0u8; LEDGER_SRC_MAX];
        assert_eq!(l.emit(&mut small[..n - 1]), Err(LedgerErr::BadOutLen));
    }

    // 3. 多行与单行 anchors 同构；四类锚词表全收（含 oem-system——Q6 终核
    //    条②的枚举面）；键序两可；锚集内注释行。
    #[test]
    fn anchors_forms_and_vocab() {
        let single = "schema = 1\n\
\n\
[[lineage]]\n\
id = \"oem\"\n\
anchors = [{ type = \"oem-system\", pk = \"00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff\" }]\n\
gen_max = 0\n\
ver_max = 0\n\
kind = \"\"\n\
issued_by = \"\"\n\
cert_status = \"\"\n\
last_revision = 0\n";
        let l = ok(single);
        assert_eq!(l.lineages()[0].anchors()[0].ty, AnchorType::OemSystem);
        // 三类剩余词表 + 多锚条目 + 锚集内注释行。
        let multi = "schema = 1
[[lineage]]
id = \"dev-1\"
anchors = [
  # 混合锚集
  { pk = \"0101010101010101010101010101010101010101010101010101010101010101\", type = \"tofu\" },
  { pk = \"0202020202020202020202020202020202020202020202020202020202020202\", type = \"ca\" },
  { pk = \"0303030303030303030303030303030303030303030303030303030303030303\", type = \"system\" },
]
gen_max = 4294967295
ver_max = 0
kind = \"\"
issued_by = \"\"
cert_status = \"\"
last_revision = 0
";
        let l = ok(multi);
        let e = &l.lineages()[0];
        assert_eq!(e.anchors_len, 3);
        assert_eq!(e.anchors()[0].ty, AnchorType::Tofu);
        assert_eq!(e.anchors()[1].ty, AnchorType::Ca);
        assert_eq!(e.anchors()[2].ty, AnchorType::System);
        assert_eq!(e.gen_max, u32::MAX);
        // 多条目（canonical + 单行形第二条）+ find_lineage 各命中。
        let two = format!("{CANONICAL}[[lineage]]\n{single_cut}", single_cut = {
            let s = single;
            &s[s.find("[[lineage]]\n").map(|i| i + "[[lineage]]\n".len()).unwrap_or(0)..]
        });
        let l2 = ok(&two);
        assert_eq!(l2.lineages_len, 2);
        assert!(l2.find_lineage(b"upstream").is_some());
        assert!(l2.find_lineage(b"oem").is_some());
    }

    // 4. 拒面（解析纪律五条 + 值域）：未知枚举保守拒/未知键三面/schema
    //    ≠1/schema 重复/schema 缺/V4 非空四型/pk 坏三型/零锚/未闭合/
    //    内联重复键/同条目 pk 重复/缺键/条目 id 撞/CR/无等号/id 值域/
    //    数字形/未知节/空账本/源帽。
    #[test]
    fn reject_faces() {
        // ① 未知 anchor_type 保守拒（两步发布纪律执法面）。
        assert_eq!(
            bad(&EXEMPLAR.replace("type = \"system\"", "type = \"vendor\"")),
            LedgerErr::UnknownAnchorType
        );
        // ② 未知键三面：顶层/条目/内联表。
        assert_eq!(bad(&format!("extra = 0\n{EXEMPLAR}")), LedgerErr::UnknownKey);
        assert_eq!(bad(&EXEMPLAR.replace("ver_max = 0", "extra = 0")), LedgerErr::UnknownKey);
        assert_eq!(
            bad(&EXEMPLAR.replace("type = \"system\" }", "type = \"system\", alg = \"1\" }")),
            LedgerErr::UnknownKey
        );
        // ③ schema 三面：≠1/缺/重。
        assert_eq!(bad(&EXEMPLAR.replace("schema = 1", "schema = 2")), LedgerErr::BadSchema);
        let no_schema = EXEMPLAR.replacen("schema = 1", "", 1);
        assert_eq!(bad(&no_schema), LedgerErr::MissingKey);
        assert_eq!(bad(&format!("schema = 1\n{EXEMPLAR}")), LedgerErr::BadSchema);
        // V4 非空四型（kind/issued_by/cert_status/last_revision）。
        assert_eq!(bad(&EXEMPLAR.replace("kind = \"\"", "kind = \"odm\"")), LedgerErr::V4NonEmpty);
        assert_eq!(bad(&EXEMPLAR.replace("issued_by = \"\"", "issued_by = \"upstream\"")), LedgerErr::V4NonEmpty);
        assert_eq!(bad(&EXEMPLAR.replace("cert_status = \"\"", "cert_status = \"active\"")), LedgerErr::V4NonEmpty);
        assert_eq!(bad(&EXEMPLAR.replace("last_revision = 0", "last_revision = 7")), LedgerErr::V4NonEmpty);
        // pk 坏三型：短/大写/非 hex。
        assert_eq!(bad(&EXEMPLAR.replacen("0123456789abcdef", "0123456789abcde", 1)), LedgerErr::BadPk);
        assert_eq!(bad(&EXEMPLAR.replacen("0123456789abcdef", "0123456789ABCDEF", 1)), LedgerErr::BadPk);
        assert_eq!(bad(&EXEMPLAR.replacen("0123456789abcdef", "0123456789abcdeg", 1)), LedgerErr::BadPk);
        // anchors：零锚（去锚行——例样锚行带行尾注释，整行滤除最稳）/
        // 未闭合（EOF）/内联表重复键/同条目 pk 重复。
        let zero_anchor: String =
            EXEMPLAR.lines().filter(|l| !l.contains("{ pk = ")).collect::<Vec<_>>().join("\n");
        assert_eq!(bad(&zero_anchor), LedgerErr::BadAnchors);
        assert_eq!(bad(&EXEMPLAR.replace("]\ngen_max", "\ngen_max")), LedgerErr::BadAnchors);
        assert_eq!(
            bad(&EXEMPLAR.replace("type = \"system\" }", "type = \"system\", type = \"tofu\" }")),
            LedgerErr::DupKey
        );
        assert_eq!(
            bad(&EXEMPLAR.replace(
                "]\ngen_max",
                "  { pk = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\", type = \"tofu\" },\n]\ngen_max"
            )),
            LedgerErr::DupAnchor
        );
        // 缺键（去 gen_max）/条目 id 撞/顶层键错挂。
        assert_eq!(bad(&EXEMPLAR.replace("gen_max = 3", "")), LedgerErr::MissingKey);
        let dup_id = format!(
            "{EXEMPLAR}[[lineage]]\nid = \"upstream\"\nanchors = [\n  {{ pk = \"0404040404040404040404040404040404040404040404040404040404040404\", type = \"tofu\" }},\n]\ngen_max = 0\nver_max = 0\nkind = \"\"\nissued_by = \"\"\ncert_status = \"\"\nlast_revision = 0\n"
        );
        assert_eq!(bad(&dup_id), LedgerErr::DupLineage);
        // 行形：CR/无等号/id 值域两型/数字形/未知节。
        assert_eq!(bad(&EXEMPLAR.replace("\n\n[[lineage]]", "\r\n[[lineage]]")), LedgerErr::BadLine);
        assert_eq!(bad(&EXEMPLAR.replace("id = \"upstream\"", "id upstream")), LedgerErr::BadLine);
        assert_eq!(bad(&EXEMPLAR.replace("id = \"upstream\"", "id = \"Upstream\"")), LedgerErr::BadLineage);
        assert_eq!(bad(&EXEMPLAR.replace("id = \"upstream\"", "id = \"a_b\"")), LedgerErr::BadLineage);
        assert_eq!(bad(&EXEMPLAR.replace("gen_max = 3", "gen_max = 03")), LedgerErr::BadNum);
        assert_eq!(bad(&EXEMPLAR.replace("gen_max = 3", "gen_max = -1")), LedgerErr::BadNum);
        assert_eq!(bad(&EXEMPLAR.replace("[[lineage]]", "[[vendor]]")), LedgerErr::UnknownSection);
        // 空账本（零 [[lineage]] 条目）/源帽。
        assert_eq!(bad("schema = 1\n"), LedgerErr::NoLineage);
        let huge = format!("schema = 1\n{}", "x".repeat(LEDGER_SRC_MAX));
        assert_eq!(bad(&huge), LedgerErr::TooLarge);
    }

    // 5. 水位比较器（纪律④：单源不两套）：回降拒边界 + 推进单调。
    #[test]
    fn water_level_comparator() {
        assert_eq!(check_gen(3, 3), Ok(())); // 等值 = 同代际幂等面
        assert_eq!(check_gen(4, 3), Ok(()));
        assert_eq!(check_gen(2, 3), Err(LedgerErr::GenRollback)); // 回放/回滚
        assert_eq!(check_gen(0, u32::MAX), Err(LedgerErr::GenRollback));
        assert_eq!(check_gen(u32::MAX, 0), Ok(()));
        assert_eq!(bump_gen(3, 5), 5);
        assert_eq!(bump_gen(5, 3), 5);
        assert_eq!(bump_gen(0, 0), 0);
    }

    // 6. 帽执法：条目帽 LINEAGES_MAX=8 / 锚帽 ANCHORS_MAX=8（**推断**帽，
    //    Q6 终核面之一——帽值收紧走 add-only 前的单窗口）。
    #[test]
    fn caps_enforced() {
        assert_eq!(LINEAGES_MAX, 8);
        assert_eq!(ANCHORS_MAX, 8);
        let entry = "\n[[lineage]]\nid = \"l-{n}\"\nanchors = [\n  { pk = \"{pk}\", type = \"tofu\" },\n]\ngen_max = 0\nver_max = 0\nkind = \"\"\nissued_by = \"\"\ncert_status = \"\"\nlast_revision = 0\n";
        let mut src = String::from("schema = 1\n");
        for i in 0..8 {
            src.push_str(&entry.replace("{n}", &i.to_string()).replace("{pk}", &format!("{i:02x}").repeat(32)));
        }
        assert_eq!(ok(&src).lineages_len, 8);
        // 第 9 条 → TooManyLineages。
        src.push_str(&entry.replace("{n}", "8").replace("{pk}", &"08".repeat(32)));
        assert_eq!(bad(&src), LedgerErr::TooManyLineages);
        // 第 9 锚 → BadAnchors。
        let nine = format!(
            "schema = 1\n\n[[lineage]]\nid = \"over\"\nanchors = [\n{}\n]\ngen_max = 0\nver_max = 0\nkind = \"\"\nissued_by = \"\"\ncert_status = \"\"\nlast_revision = 0\n",
            (0..9)
                .map(|i| format!("  {{ pk = \"{}\", type = \"tofu\" }},", format!("{i:02x}").repeat(32)))
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert_eq!(bad(&nine), LedgerErr::BadAnchors);
    }
}
