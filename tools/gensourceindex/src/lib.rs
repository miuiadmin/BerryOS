//! BerryOS 官方源静态实例生成器 Rust 件（gensourceindex.py 首刀——彻底
//! Rust 化轨道 b 层 #5 刀序第五位）。本刀范围 = **TLV 索引编码/解析 +
//! 源钥清单冻结行格式 + 自持 SHA-512/256**：`Entry`/`Index` 视图 +
//! `build_index`（canonical 单形）/`parse_index`（单一解释全量执法）+
//! `build_keys_listing`/`parse_keys_listing` + `sha512_256`，与 Python
//! 现役件（tools/gensourceindex.py）黄金向量字节级对拍。产线零触碰——
//! Python 件现役不动，后续刀（berry-sign 面/product 派生/CLI）依
//! 彻底Rust化计划 §4 刀序推进后再换轨。
//!
//! 布局（模块头注推断标注，Python 件同源）：
//! 头 14B（magic "BSRC" + ver + alg + flags + reserved + index_ver u32 +
//! entry_count u16）+ source_pk 32B + 条目区（id 变长/三值版本 3×u16 BE/
//! size u64/digest 32B/lineage 变长/pk 0|32B/last_rev u32/cert 变长）+
//! 尾签名槽 64B。canonical 单形：条目 id 字节序升序；flags.signed 与
//! 槽零/非零一致性执法（谎报位拒）。

/// 容器识别（BSIG 先例——envelope.rs :51 同锚）。
pub const MAGIC: [u8; 4] = *b"BSRC";
/// 索引格式版本（Q3 重议轨）。
pub const VER_V1: u8 = 1;
/// 与信封同表（envelope.rs :56 add-only）。
pub const ALG_ED25519_SHA512_256: u8 = 1;
/// 尾签名槽在席（诚实降级位——unsigned 可见）。
pub const FLAG_SIGNED: u8 = 1 << 0;
/// bit1..=7 保留必 0（未知位保守拒）。
pub const FLAGS_KNOWN: u8 = FLAG_SIGNED;
/// ed25519 公钥长（envelope.rs PK_LEN 同源）。
pub const PK_LEN: usize = 32;
/// ed25519 签名长（envelope.rs SIG_LEN 同源）。
pub const SIG_LEN: usize = 64;
/// SHA-512/256 摘要长。
pub const DIGEST_LEN: usize = 32;
/// berry-manifest ID_MAX（lib.rs :136 同源）。
pub const ID_MAX: usize = 24;
/// mkapp SEC_RE 帽（berry_assemble VENDOR_MAX 同锚）。
pub const LINEAGE_MAX: usize = 16;
/// 三值版本段域（mkapp VER_RE 0..=999 同源）。
pub const VER_SEG_MAX: u32 = 999;
/// 条目数帽（推断——24 号未设帽；APP_SLOTS=10 基座 + OEM 余量上取整）。
pub const ENTRIES_MAX: usize = 64;
/// v1 恒值（v1.2 §二 :42-43；词表 add-only）。
pub const CERT_ACTIVE: &str = "active";
/// cert_status 长帽（推断——lineage 同量级）。
pub const CERT_MAX: usize = 16;
/// 签名覆盖域分隔标签（BSIG-TOP 同精神，推断）。
pub const SRC_LABEL: &[u8] = b"BSRC-TOP";
/// 头 46B + 空条目 + 签名槽 = 110B。
pub const INDEX_MIN: usize = 46 + SIG_LEN;
/// 源字节帽（推断——64×142B 条目 + 头尾余量）。
pub const INDEX_MAX: usize = 16384;
/// 两件套文件名（mkimage --source-instance 消费合同单源）。
pub const INDEX_NAME: &str = "source-index.tlv";
pub const KEYS_NAME: &str = "source-keys.txt";
/// 源清单冻结行格式（v1 冻结于 2026-11-11——改列宽/行序/字段 = 破冻须升 v2）。
pub const KEYS_FROZEN: &str = "2026-11-11";
/// 清单头行（`sources=NN` 前缀半）。
pub const KEYS_HEAD: &str = "berryos-source-keys v1 format_frozen=2026-11-11";
/// source_id 列宽（lineage 帽同宽——列宽即契约）。
pub const SID_W: usize = 16;

/// 拒因（loud die 的 Rust 面——`die!` 宏单点）。文案与 Python `die()`
/// 前缀口径对齐（`gensourceindex: FAIL: <msg>`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reject {
    pub message: String,
}

impl Reject {
    fn new(message: impl Into<String>) -> Self {
        Reject { message: message.into() }
    }
}

impl std::fmt::Display for Reject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "gensourceindex: FAIL: {}", self.message)
    }
}

impl std::error::Error for Reject {}

/// loud die（Python `die` 镜像——`Result` 面，调用方决定退出形）。
fn die(msg: impl Into<String>) -> Reject {
    Reject::new(msg)
}

type R<T> = Result<T, Reject>;

/// 条目八字段视图（id/三值版本/size/哈希/lineage/pk 见证/last_rev/cert）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// str（≤24B ID_RE 域：首字符字母数字、[A-Za-z0-9._-]）。
    pub id: String,
    /// (major, minor, patch) 各 0..=999。
    pub ver: (u32, u32, u32),
    /// u64。
    pub size: u64,
    /// 32B SHA-512/256。
    pub digest: [u8; DIGEST_LEN],
    /// str（1..=16B [a-z0-9-] 域）。
    pub lineage: String,
    /// 32B 或 None（无见证）。
    pub pk: Option<[u8; PK_LEN]>,
    /// u32（平台 level）。
    pub last_rev: u32,
    /// str（v1 恒 "active"）。
    pub cert: String,
}

impl Entry {
    /// id 的 ASCII 字节键（canonical 排序键——Python `key()` 镜像）。
    pub fn key(&self) -> Vec<u8> {
        self.id.as_bytes().to_vec()
    }

    /// 条目区编码（Python `Entry.encode` 逐域镜像）。
    pub fn encode(&self) -> R<Vec<u8>> {
        let mut out = Vec::with_capacity(1 + ID_MAX + 6 + 8 + DIGEST_LEN + 1 + LINEAGE_MAX + 1 + PK_LEN + 4 + 1 + CERT_MAX);
        let ib = self.id.as_bytes();
        if ib.is_empty() || ib.len() > u8::MAX as usize {
            return Err(die(format!("条目 id 长越域：{}", ib.len())));
        }
        out.push(ib.len() as u8);
        out.extend_from_slice(ib);
        for seg in [self.ver.0, self.ver.1, self.ver.2] {
            // u16 BE ×3（Python `to_bytes(2,"big")` 镜像——段域 ≤999 恒纳）。
            out.extend_from_slice(&(seg as u16).to_be_bytes());
        }
        out.extend_from_slice(&self.size.to_be_bytes());
        out.extend_from_slice(&self.digest);
        let lb = self.lineage.as_bytes();
        if lb.is_empty() || lb.len() > u8::MAX as usize {
            return Err(die(format!("条目 lineage 长越域：{}", lb.len())));
        }
        out.push(lb.len() as u8);
        out.extend_from_slice(lb);
        match self.pk {
            Some(_) => out.push(PK_LEN as u8),
            None => out.push(0),
        }
        if let Some(pk) = &self.pk {
            out.extend_from_slice(pk);
        }
        out.extend_from_slice(&self.last_rev.to_be_bytes());
        let cb = self.cert.as_bytes();
        if cb.is_empty() || cb.len() > u8::MAX as usize {
            return Err(die(format!("条目 cert 长越域：{}", cb.len())));
        }
        out.push(cb.len() as u8);
        out.extend_from_slice(cb);
        Ok(out)
    }
}

/// 索引视图（parse 产物——字段与布局一一对应）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    /// u32。
    pub index_ver: u32,
    /// 32B。
    pub source_pk: [u8; PK_LEN],
    /// id 升序。
    pub entries: Vec<Entry>,
    /// flags.bit0。
    pub signed: bool,
    /// 64B（unsigned = 全零）。
    pub src_sig: [u8; SIG_LEN],
}

/// id 域（1..=24B、首字符字母数字、[A-Za-z0-9._-]——berry-manifest ID 域
/// 同源；Python `ID_RE.fullmatch` 镜像）。
fn id_ok(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() || b.len() > ID_MAX {
        return false;
    }
    if !(b[0].is_ascii_alphanumeric()) {
        return false;
    }
    b[1..].iter().all(|&c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
}

/// lineage 域（1..=16B [a-z0-9-]——mkapp SEC_RE 同源；Python
/// `LINEAGE_RE.fullmatch` 镜像）。
fn lineage_ok(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() || b.len() > LINEAGE_MAX {
        return false;
    }
    b.iter().all(|&c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

/// 构造/解析共域执法（值域单点——encode/parse 同口径；Python
/// `_check_common` 镜像）。
fn check_common(entry: &Entry, what: &str) -> R<()> {
    if !id_ok(&entry.id) {
        return Err(die(format!(
            "{what}: id {:?} 越域（1..=24B、首字符字母数字、[A-Za-z0-9._-]——berry-manifest ID 域同源）",
            entry.id
        )));
    }
    if entry.ver.0 > VER_SEG_MAX || entry.ver.1 > VER_SEG_MAX || entry.ver.2 > VER_SEG_MAX {
        return Err(die(format!(
            "{what}: 三值版本 {:?} 越域（各 u16 且 ≤{VER_SEG_MAX}——mkapp VER_RE 同源）",
            entry.ver
        )));
    }
    if !entry.lineage.is_ascii() || !lineage_ok(&entry.lineage) {
        return Err(die(format!(
            "{what}: key_lineage {:?} 越域（1..=16B [a-z0-9-]——mkapp SEC_RE 同源）",
            entry.lineage
        )));
    }
    if entry.cert.is_empty() || entry.cert.len() > CERT_MAX {
        return Err(die(format!(
            "{what}: cert_status 长越域（1..={CERT_MAX}）"
        )));
    }
    if !entry.cert.is_ascii() {
        return Err(die(format!("{what}: cert_status 非 ASCII")));
    }
    Ok(())
}

/// 条目表 → 索引字节（canonical 单形：id 字节序升序 + 查重 + 全域执法）。
/// `src_sig = None` → 签名槽零占位（flags.signed=0）；`Some(64B)` → 在席。
/// Python `build_index` 逐域镜像。
pub fn build_index(
    entries: &[Entry],
    index_ver: u32,
    source_pk: &[u8; PK_LEN],
    src_sig: Option<&[u8; SIG_LEN]>,
) -> R<Vec<u8>> {
    if entries.len() > ENTRIES_MAX {
        return Err(die(format!(
            "条目数 {} 越帽 {ENTRIES_MAX}（推断帽——基座 9 + OEM 三包余量）",
            entries.len()
        )));
    }
    let mut ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    ids.sort_unstable();
    for w in ids.windows(2) {
        if w[0] == w[1] {
            return Err(die(format!(
                "条目 id 重复 ['{}']（索引身份单义——查重）",
                w[0]
            )));
        }
    }
    for e in entries {
        check_common(e, &format!("条目 {:?}", e.id))?;
    }
    let mut out = Vec::with_capacity(INDEX_MAX + 16);
    out.extend_from_slice(&MAGIC);
    out.push(VER_V1);
    out.push(ALG_ED25519_SHA512_256);
    out.push(if src_sig.is_some() { FLAG_SIGNED } else { 0 });
    out.push(0); // reserved
    out.extend_from_slice(&index_ver.to_be_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_be_bytes());
    out.extend_from_slice(source_pk);
    let mut sorted: Vec<&Entry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.key().cmp(&b.key()));
    for e in sorted {
        out.extend_from_slice(&e.encode()?);
    }
    match src_sig {
        Some(sig) => out.extend_from_slice(sig),
        None => out.extend_from_slice(&[0u8; SIG_LEN]),
    }
    if out.len() > INDEX_MAX {
        return Err(die(format!(
            "索引 {}B 越帽 {INDEX_MAX}（推断帽——防敌意巨值）",
            out.len()
        )));
    }
    Ok(out)
}

/// 单一解释解析：结构/值域/canonical 序/flags-slot 一致性全量执法（永不
/// 静默截断；未知 ver/alg/flags/reserved 保守拒——envelope.rs parse 同
/// 纪律）。Python `parse_index` 逐域镜像。
pub fn parse_index(data: &[u8]) -> R<Index> {
    if data.len() < INDEX_MIN {
        return Err(die(format!(
            "索引短于最小 {INDEX_MIN}B（got {}）",
            data.len()
        )));
    }
    if data[0..4] != MAGIC {
        return Err(die(format!(
            "magic 不符 {:?}（应 {:?}）",
            &data[0..4], MAGIC
        )));
    }
    if data[4] != VER_V1 {
        return Err(die(format!(
            "ver {} ≠ 1（未知版本保守拒——升版走 add-only 轨）",
            data[4]
        )));
    }
    if data[5] != ALG_ED25519_SHA512_256 {
        return Err(die(format!(
            "alg_id {} 未在册（1 = ed25519+SHA-512/256，add-only）",
            data[5]
        )));
    }
    let flags = data[6];
    if flags & !FLAGS_KNOWN != 0 {
        return Err(die(format!(
            "flags 保留位非 0：0x{flags:02x}（未知位保守拒）"
        )));
    }
    let signed = flags & FLAG_SIGNED != 0;
    if data[7] != 0 {
        return Err(die(format!("reserved {} ≠ 0（保守拒）", data[7])));
    }
    let index_ver = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
    let count = u16::from_be_bytes([data[12], data[13]]) as usize;
    if count > ENTRIES_MAX {
        return Err(die(format!("entry_count {count} 越帽 {ENTRIES_MAX}")));
    }
    let mut source_pk = [0u8; PK_LEN];
    source_pk.copy_from_slice(&data[14..46]);
    let mut off = 46usize;
    let mut entries = Vec::with_capacity(count);
    /// 变长域读取（len 前缀 + 域体——越界统一「条目#i 越界」拒）。
    fn take<'a>(data: &'a [u8], off: &mut usize, n: usize, i: usize) -> R<&'a [u8]> {
        let end = off.checked_add(n).ok_or_else(|| die(format!("条目#{i} 越界（索引截断——精确总长纪律）")))?;
        if end > data.len() {
            return Err(die(format!("条目#{i} 越界（索引截断——精确总长纪律）")));
        }
        let s = &data[*off..end];
        *off = end;
        Ok(s)
    }
    /// ASCII 变长域（非 ASCII = `条目#i 非 ASCII 字段` 拒——Python 同文案）。
    fn take_ascii(data: &[u8], off: &mut usize, n: usize, i: usize) -> R<String> {
        let b = take(data, off, n, i)?;
        match std::str::from_utf8(b) {
            // Python `decode("ascii", "strict")`：非 ASCII 拒（UTF-8 检查
            // 覆盖 ≤0x7F 同集——str::from_utf8 拒 >0x7F 字节）。
            Ok(s) if s.is_ascii() => Ok(s.to_string()),
            _ => Err(die(format!("条目#{i} 非 ASCII 字段"))),
        }
    }
    for i in 0..count {
        let id_len = *take(data, &mut off, 1, i)?.first().unwrap();
        let eid = take_ascii(data, &mut off, id_len as usize, i)?;
        let mut ver = [0u16; 3];
        for v in ver.iter_mut() {
            let b = take(data, &mut off, 2, i)?;
            *v = u16::from_be_bytes([b[0], b[1]]);
        }
        let b = take(data, &mut off, 8, i)?;
        let size = u64::from_be_bytes(b.try_into().unwrap());
        let mut digest = [0u8; DIGEST_LEN];
        digest.copy_from_slice(take(data, &mut off, DIGEST_LEN, i)?);
        let lin_len = *take(data, &mut off, 1, i)?.first().unwrap();
        let lineage = take_ascii(data, &mut off, lin_len as usize, i)?;
        let pk_len = *take(data, &mut off, 1, i)?.first().unwrap() as usize;
        if pk_len != 0 && pk_len != PK_LEN {
            return Err(die(format!(
                "条目#{i} pk_len {pk_len} 非 0/32（单一解释——无见证/见证两态外无第三读）"
            )));
        }
        let pk = if pk_len == PK_LEN {
            let mut p = [0u8; PK_LEN];
            p.copy_from_slice(take(data, &mut off, PK_LEN, i)?);
            Some(p)
        } else {
            None
        };
        let b = take(data, &mut off, 4, i)?;
        let last_rev = u32::from_be_bytes(b.try_into().unwrap());
        let cert_len = *take(data, &mut off, 1, i)?.first().unwrap();
        let cert = take_ascii(data, &mut off, cert_len as usize, i)?;
        let e = Entry {
            id: eid,
            ver: (ver[0] as u32, ver[1] as u32, ver[2] as u32),
            size,
            digest,
            lineage,
            pk,
            last_rev,
            cert,
        };
        check_common(&e, &format!("条目#{i}"))?;
        if e.cert != CERT_ACTIVE {
            return Err(die(format!(
                "条目#{i} cert_status {:?} 未在册（v1 词表 = {{'active'}}——add-only，未知值保守拒）",
                e.cert
            )));
        }
        entries.push(e);
    }
    if off + SIG_LEN != data.len() {
        return Err(die(format!(
            "总长不符：条目尾 {off} + 槽 {SIG_LEN} != {}（无尾随字节容差——单一解释纪律）",
            data.len()
        )));
    }
    let mut src_sig = [0u8; SIG_LEN];
    src_sig.copy_from_slice(&data[off..off + SIG_LEN]);
    if signed && src_sig == [0u8; SIG_LEN] {
        return Err(die("flags.signed=1 但签名槽全零（谎报位——拒）"));
    }
    if !signed && src_sig != [0u8; SIG_LEN] {
        return Err(die(
            "flags.signed=0 但签名槽非零（未签索引携带签名字节——单一解释拒；真签名走 --sign-key → flags.signed=1 同步翻位）",
        ));
    }
    let keys: Vec<Vec<u8>> = entries.iter().map(|e| e.key()).collect();
    let mut sorted_keys = keys.clone();
    sorted_keys.sort();
    let mut uniq = keys.clone();
    uniq.sort();
    uniq.dedup();
    if keys != sorted_keys || uniq.len() != keys.len() {
        return Err(die("条目非 id 升序或重复（canonical 单形——build 对拍前提）"));
    }
    Ok(Index { index_ver, source_pk, entries, signed, src_sig })
}

// ---- 源钥清单（冻结行格式 v1）----

/// 小写 hex（`pk.hex()` 镜像）。
fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for byte in b {
        s.push_str(&format!("{byte:02x}"));
    }
    s
}

/// 源钥清单字节（v1 冻结行格式——改列宽/行序/字段 = 破冻升 v2）：行 1 头
/// `berryos-source-keys v1 format_frozen=<日> sources=NN`；行 2 `key
/// source_id=<16 左对齐> pk=<64hex> fingerprint=<64hex>
/// sig_state=<signed|unsigned>`。fingerprint = SHA-512/256(pk)。ASCII、
/// LF、末行换行。Python `build_keys_listing` 逐字节镜像。
pub fn build_keys_listing(source_id: &str, pk: &[u8; PK_LEN], signed: bool) -> R<Vec<u8>> {
    if !source_id.is_ascii() || !lineage_ok(source_id) {
        return Err(die(format!(
            "source_id {:?} 越域（1..=16B [a-z0-9-]——lineage 同域口径）",
            source_id
        )));
    }
    let state = if signed { "signed" } else { "unsigned" };
    let mut sid = source_id.to_string();
    while sid.len() < SID_W {
        sid.push(' ');
    }
    let text = format!(
        "{KEYS_HEAD} sources=01\nkey source_id={sid} pk={} fingerprint={} sig_state={state}\n",
        hex(pk),
        hex(&sha512_256(pk))
    );
    Ok(text.into_bytes())
}

/// 清单回读（两件套对拍用）：→ (source_id, pk 32B, signed: bool)。冻结格式
/// 逐列执法（定宽正则整行钉——头行/列序/列宽/词表/小写 hex/fingerprint，
/// 漂移即拒；source_id 定宽 16 含右垫空格）。Python `parse_keys_listing`
/// 镜像。
pub fn parse_keys_listing(data: &[u8]) -> R<(String, [u8; PK_LEN], bool)> {
    let text = match std::str::from_utf8(data) {
        Ok(s) if s.is_ascii() => s,
        _ => return Err(die("source-keys 非 ASCII")),
    };
    let lines: Vec<&str> = text.split('\n').collect();
    // Python splitlines + 末行换行钉板：恰两逻辑行（尾 \n 后零内容）。
    if lines.len() != 3 || !lines[2].is_empty() {
        return Err(die(format!(
            "source-keys 行数 {} ≠ 2（v1 单源清单形）",
            lines.len().saturating_sub(1)
        )));
    }
    if lines[0] != format!("{KEYS_HEAD} sources=01") {
        return Err(die(format!(
            "source-keys 头行漂移：{:?}（v1 冻结格式）",
            lines[0]
        )));
    }
    let l1 = lines[1];
    let prefix = "key source_id=";
    let rest = match l1.strip_prefix(prefix) {
        Some(r) => r,
        None => {
            return Err(die(format!(
                "source-keys 条目行形漂移：{l1:?}（冻结列序/列宽/词表）"
            )))
        }
    };
    if rest.len() < SID_W {
        return Err(die(format!(
            "source-keys 条目行形漂移：{l1:?}（冻结列序/列宽/词表）"
        )));
    }
    let (sid_raw, tail) = rest.split_at(SID_W);
    // 列分隔恰一空格 + `pk=` 列名（sid 定宽 16 右垫空格 → tail 首必为
    // " pk="——Python regex `source_id=(.{16}) pk=` 同口径）。
    let tail = match tail.strip_prefix(" pk=") {
        Some(t) => t,
        None => {
            return Err(die(format!(
                "source-keys 条目行形漂移：{l1:?}（冻结列序/列宽/词表）"
            )))
        }
    };
    let mut parts = tail.split(' ');
    let pk_hex = parts.next().unwrap_or("");
    let fp_hex = match parts.next() {
        Some(f) if f.starts_with("fingerprint=") => &f["fingerprint=".len()..],
        _ => {
            return Err(die(format!(
                "source-keys 条目行形漂移：{l1:?}（冻结列序/列宽/词表）"
            )))
        }
    };
    let state = match parts.next() {
        Some(s) if s.starts_with("sig_state=") => &s["sig_state=".len()..],
        _ => {
            return Err(die(format!(
                "source-keys 条目行形漂移：{l1:?}（冻结列序/列宽/词表）"
            )))
        }
    };
    if parts.next().is_some() {
        return Err(die(format!(
            "source-keys 条目行形漂移：{l1:?}（冻结列序/列宽/词表）"
        )));
    }
    let sid = sid_raw.trim_end_matches(' ');
    if sid.is_empty() || !sid.is_ascii() || !lineage_ok(sid) {
        return Err(die(format!(
            "source_id 列漂移：{:?}（{SID_W} 定宽 + lineage 值域）",
            sid_raw
        )));
    }
    let unhex = |s: &str| -> Option<Vec<u8>> {
        if s.len() % 2 != 0 || s.is_empty() {
            return None;
        }
        let mut v = Vec::with_capacity(s.len() / 2);
        let b = s.as_bytes();
        let mut k = 0;
        while k < b.len() {
            let hi = (b[k] as char).to_digit(16)?;
            let lo = (b[k + 1] as char).to_digit(16)?;
            v.push((hi * 16 + lo) as u8);
            k += 2;
        }
        Some(v)
    };
    // 小写 hex 域执法（[0-9a-f]{64}——大写/非 hex 即形漂移拒）。
    if pk_hex.len() != PK_LEN * 2 || !pk_hex.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) {
        return Err(die(format!(
            "source-keys 条目行形漂移：{l1:?}（冻结列序/列宽/词表）"
        )));
    }
    if fp_hex.len() != DIGEST_LEN * 2 || !fp_hex.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) {
        return Err(die(format!(
            "source-keys 条目行形漂移：{l1:?}（冻结列序/列宽/词表）"
        )));
    }
    let pk_v = unhex(pk_hex).unwrap_or_default();
    let fp_v = unhex(fp_hex).unwrap_or_default();
    let mut pk = [0u8; PK_LEN];
    pk.copy_from_slice(&pk_v);
    if fp_v != sha512_256(&pk) {
        return Err(die("source-keys fingerprint 对拍不符（SHA-512/256(pk)）"));
    }
    let signed = match state {
        "signed" => true,
        "unsigned" => false,
        _ => {
            return Err(die(format!(
                "source-keys 条目行形漂移：{l1:?}（冻结列序/列宽/词表）"
            )))
        }
    };
    Ok((sid.to_string(), pk, signed))
}

// ---- 自持 SHA-512/256（FIPS 180-4——berry-crypto sha512::digest256 同源面）----

mod sha512;

/// 二刀（berry-sign 面）：发现序 + 子进程封装 + 索引签名/验签单点缝。
pub mod sign;

/// SHA-512/256 摘要（= BSIG 链 bap_digest 同算法——envelope.rs :68/
/// sha512::digest256）。自持实现零依赖（mkapp SHA-256 同款纪律）；NIST
/// 向量钉板见 tests。
pub fn sha512_256(data: &[u8]) -> [u8; 32] {
    sha512::digest512_256(data)
}

// ---- 三刀·CLI 纯函数面（--verify/--gen-keys 消费半边）----

/// 索引版本单调门（Python `check_index_version` 镜像——berry-crypto
/// ledger.rs `check_gen` 口径：new < seen = 拒（回滚/回放）；等值过
/// （同代幂等面）；升过。口径同步义务：Rust 侧改义随笔）。
pub fn check_index_version(new: u32, seen: u32) -> R<()> {
    if new < seen {
        return Err(die(format!(
            "索引版本回降拒：index_version {new} < 已见水位 {seen}（v1.2 §五 :91 索引级单调版本号防回滚——check_gen 同口径）"
        )));
    }
    Ok(())
}

/// --gen-keys 透传出口（Python `do_gen_keys` 镜像——berry-sign keygen
/// 子进程；不可得 loud 拒同文案）。返回打印行集（调用方负责 println）。
pub fn gen_keys_lines(bs_stdout: Option<&str>) -> R<Vec<String>> {
    let mut lines = vec![format!("{}", bs_stdout.unwrap_or("").trim())];
    lines.push(
        "gensourceindex: 注: dev 密钥对（berry-sign 头注「dev/test 面纪律」——永不用于真签，真签面随 #77/P3）"
            .to_string(),
    );
    Ok(lines)
}

#[cfg(test)]
mod cli_face_tests {
    use super::*;

    /// gen_keys_lines：透传行 trim 契约 + 注行逐字钉板（cmd_gen_keys
    /// 唯一打印源——Python do_gen_keys :876-878 文案镜像，双源漂移守卫）。
    #[test]
    fn gen_keys_lines_passthrough_and_note() {
        let ls = gen_keys_lines(Some("sk 写入 /tmp/a\npk 写入 /tmp/b\n\n")).unwrap();
        assert_eq!(
            ls,
            vec![
                "sk 写入 /tmp/a\npk 写入 /tmp/b".to_string(),
                "gensourceindex: 注: dev 密钥对（berry-sign 头注「dev/test 面纪律」——永不用于真签，真签面随 #77/P3）".to_string(),
            ]
        );
        // None = 空透传（防御形——子进程 stdout 缺席不 panic）
        let ls0 = gen_keys_lines(None).unwrap();
        assert_eq!(ls0.len(), 2);
        assert_eq!(ls0[0], "");
    }

    /// check_index_version：降拒/等值过/升过三态（Python check_index_version
    /// 镜像——check_gen 单调口径；拒文案回显双值）。
    #[test]
    fn check_index_version_monotone() {
        assert!(check_index_version(3, 2).is_ok()); // 升过
        assert!(check_index_version(2, 2).is_ok()); // 等值幂等过
        let e = check_index_version(1, 2).unwrap_err();
        let msg = format!("{e}");
        assert!(msg.contains("索引版本回降拒"), "{msg}");
        assert!(msg.contains('1') && msg.contains('2'), "{msg}");
    }
}
