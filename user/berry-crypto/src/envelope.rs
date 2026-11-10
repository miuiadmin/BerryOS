//! `.sig` 信封 v1 TLV 解析/序列化（Q3 未冻结——P0–P2 内野无消费者，
//! P3 合入前冻结；本模块是第一版实现，改版成本内野为零）。
//!
//! 分离式 `app.bap.sig`（不内嵌——.bap 载荷是冻结面 L1；v1.2 §三 :51）。
//! 布局照 24 号 §3.4 字段表 + P0-1 三条具体化（均**推断**标注，Q3 重议窗
//! 内随 P1/P2 实装免费评审）：
//!
//! ```text
//! 偏移   字段          型/长            说明
//! 0      magic         "BSIG" 4B        容器识别（.bap 容器头先例）
//! 4      ver           u8 = 1           信封版本（Q3 重议轨）
//! 5      alg_id        u8 = 1           1 = ed25519 + SHA-512/256（add-only）
//! 6      flags         u8               bit0 = pending 在场；bit1..=7 保留必 0
//! 7      lineage_len   u8 (1..=16)      + lineage 字节（[a-z0-9-]，帽同 SEC_RE）
//! 8+L    gen           u32 BE           单调代际（v1.2 §三 :58）
//! …      pk            32B              当前公钥
//! …      pending       0B 或 32B        后继钥 day-one 位（flags.bit0 执法）
//! …      chain_len     u8 (≤16)         轮换链节点数（线性单路径，禁分叉）
//! …      chain[i] ×N   每节点 96B       = new_pk 32B + rot_sig 64B
//! …      top_sig       64B              顶签名（覆盖域见 [`top_message`]）
//! 尾     ——            （无 trailer）   总长 = 8 + L + 4 + 32 + P + 1 + 96N + 64
//! ```
//!
//! **P0-1 具体化一（flags 位）**：24 号 §3.4 草案 pending「0B 或 32B」变长
//! 字段夹在 pk 与 chain_len 之间无在场位——同一字节流可两读（pk 首字节 vs
//! chain_len），正是 v1.2 §三 :65 禁的双解释器病灶（Janus CVE-2017-13156
//! 课）。本实现增设 1B flags（bit0 = pending 在场，保留位必 0——未知位保守
//! 拒）定死单一解释。
//!
//! **P0-1 具体化二（链节点带钥）**：24 号 §3.4「每节点 64B 签名」不存
//! new_pk 则覆盖域无法重组（Ed25519 签名不含消息——验证方须先知 new_pk
//! 才能重组 `BSIG-ROT` 覆盖域）。本实现每节点 = new_pk 32B + rot_sig 64B；
//! old_pk 由链序推出：node 0 的 old_pk = 账本锚钥（链外注入，见
//! [`ledger::LineageEntry`] anchors），node i 的 old_pk = node i-1 的
//! new_pk，末节点 new_pk 必 == 信封 pk（结构性执法 [`SigErr::ChainBroken`]）。
//!
//! **P0-1 勘误注三**：24 号 §3.4 尾行总长公式「12 + L + …」与其字段表固定
//! 头 7B 不符（字段表 gen 落位 7+L 自洽）——本实现按字段表推总长，加 flags
//! 位后固定头 = 8B（公式见 [`envelope_len`]）。
//!
//! **P0-2 验签合同**（本刀只解析/序列化，不验签）：①顶签名 = pk 对
//! [`top_message`]（`"BSIG-TOP"` ‖ 信封头〔magic 至 chain 末字节〕‖
//! bap_digest 32B〔SHA-512/256 全字节〕‖ manifest 关键域摘要 32B）的
//! ed25519 签名；②链节点 = old_pk 对 [`rot_message`] 的签名逐节点验；
//! ③manifest 关键域摘要**必须**由 berry-manifest 解析产物单源派生
//! （24 号 §3.3-1 单解释器纪律——禁止本 crate 内出现第二个 manifest
//! 语义判断）。拒因 BerryErr 化：结构化枚举 [`SigErr`]（add-only——
//! P0-2 增验签变体 `TopSigBad`/`RotSigBad` 等，只加不改义）。

/// 容器识别（对照 .bap 容器头先例，24 号 §3.4）。
pub const MAGIC: [u8; 4] = *b"BSIG";
/// 信封版本 v1（Q3 重议轨——`ver` 字段的存在价值正是给 P3 前重议窗留
/// 兼容轨，24 号 §1.3）。
pub const VER_V1: u8 = 1;
/// 算法枚举：1 = ed25519 + SHA-512/256（add-only；未知值保守拒）。
pub const ALG_ED25519_SHA512_256: u8 = 1;
/// flags bit0：pending 后继钥在场（P0-1 具体化一）。
pub const FLAG_PENDING: u8 = 1 << 0;
/// flags 已定义位掩码（其余位保留必 0——未知位保守拒，前向兼容轨）。
pub const FLAGS_KNOWN: u8 = FLAG_PENDING;
/// lineage 帽：1..=16B `[a-z0-9-]`——与 berry-manifest `SEC_VAL_MAX` /
/// `tools/mkapp.py` `SEC_RE` 同值同语义（三处单源同步义务，见 crate 头注）。
pub const LINEAGE_MAX: usize = 16;
/// ed25519 公钥长。
pub const PK_LEN: usize = 32;
/// ed25519 签名长。
pub const SIG_LEN: usize = 64;
/// SHA-512/256 摘要长（bap_digest / manifest 关键域摘要同长）。
pub const DIGEST_LEN: usize = 32;
/// 轮换链节点数帽（**推断**——24 号未设帽；零分配定长槽，berry-manifest
/// AGENT_ACTIONS_MAX 定长槽同思路）。
pub const CHAIN_NODES_MAX: usize = 16;
/// 域分隔标签：轮换链节点（防跨上下文签名重放，RFC 8032 context 精神）。
pub const ROT_LABEL: &[u8] = b"BSIG-ROT";
/// 域分隔标签：顶签名。
pub const TOP_LABEL: &[u8] = b"BSIG-TOP";

/// 最小信封长（lineage 1B、无 pending、零链）。
pub const ENVELOPE_MIN: usize = envelope_len(1, false, 0);
/// 最大信封长（lineage 帽、pending 在场、链帽）。
pub const ENVELOPE_MAX: usize = envelope_len(LINEAGE_MAX, true, CHAIN_NODES_MAX);

/// 信封总长公式（P0-1 勘误注三）：固定头 8B + L + 4 + 32 + P + 1 + 96N + 64。
pub const fn envelope_len(lineage_len: usize, pending: bool, chain_len: usize) -> usize {
    8 + lineage_len
        + 4
        + PK_LEN
        + if pending { PK_LEN } else { 0 }
        + 1
        + chain_len * (PK_LEN + SIG_LEN)
        + SIG_LEN
}

/// 拒因（结构化枚举，24 号 §3.3-2——人读文案渲染在后，不重蹈「一行中文
/// 拒因」旧态）。add-only：P0-2 增 `TopSigBad`/`RotSigBad` 等验签变体。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigErr {
    /// 输入短于 [`ENVELOPE_MIN`]。
    TooShort,
    /// magic ≠ "BSIG"。
    BadMagic,
    /// ver ≠ 1（未知版本保守拒——升版走 add-only 轨）。
    BadVer,
    /// alg_id 未在册（add-only 枚举外）。
    BadAlg,
    /// flags 保留位非 0（未知位保守拒）。
    BadFlags,
    /// lineage 越帽/空/字符集外 `[a-z0-9-]`。
    BadLineage,
    /// 总长与字段表不符（含 trailer——单一解释纪律：无尾随字节容差）。
    BadLen,
    /// chain_len 超 [`CHAIN_NODES_MAX`] 帽。
    BadChainLen,
    /// 链结构断：末节点 new_pk ≠ 信封 pk（线性单路径执法）。
    ChainBroken,
    /// 发射缓冲不足（[`SigEnvelopeV1::encode`]）。
    BadOutLen,
    /// 覆盖域构建器入参公钥长 ≠ 32B（[`rot_message`]）。
    BadPkLen,
}

/// 轮换链节点（P0-1 具体化二：new_pk 入存储，old_pk 由链序推出）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainNode {
    /// 本节点授权的新公钥。
    pub new_pk: [u8; PK_LEN],
    /// old_pk 对 [`rot_message`] 的 ed25519 签名（P0-2 验）。
    pub rot_sig: [u8; SIG_LEN],
}

/// `.sig` 信封 v1 解析产物（零分配定长——lineage/链均定长槽 + 在用数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SigEnvelopeV1 {
    /// alg_id（v1 唯一在册值 = 1；字段保留给 add-only 演化）。
    pub alg_id: u8,
    /// lineage 字节（`[a-z0-9-]`，1..=16B）。
    pub lineage: [u8; LINEAGE_MAX],
    /// lineage 在用长。
    pub lineage_len: usize,
    /// 单调代际（v1.2 §三 :58）。
    pub gen: u32,
    /// 当前公钥（链非空时 = 末节点 new_pk，结构执法）。
    pub pk: [u8; PK_LEN],
    /// 后继钥 day-one 位（v1.2 §三 :62 换钥纪律；None = 无 pending）。
    pub pending: Option<[u8; PK_LEN]>,
    /// 轮换链（线性单路径）。
    pub chain: [ChainNode; CHAIN_NODES_MAX],
    /// 链在用长。
    pub chain_len: usize,
    /// 顶签名（P0-2 对 [`top_message`] 验）。
    pub top_sig: [u8; SIG_LEN],
}

impl SigEnvelopeV1 {
    /// lineage 切片（在用段）。
    pub fn lineage(&self) -> &[u8] {
        &self.lineage[..self.lineage_len]
    }

    /// 链切片（在用段）。
    pub fn chain(&self) -> &[ChainNode] {
        &self.chain[..self.chain_len]
    }

    /// 顶签名覆盖域用的「信封头」长 = 总长 − top_sig 64B（magic 至 chain
    /// 末字节，24 号 §3.4 top_sig 覆盖域定义）。
    pub fn header_len(&self) -> usize {
        envelope_len(self.lineage_len, self.pending.is_some(), self.chain_len) - SIG_LEN
    }

    /// 单一解释解析：结构校验全量（magic/ver/alg/flags/lineage 值域/
    /// 精确总长/链帽/链尾续接）。永不 panic、零分配、输入只借用。
    pub fn parse(src: &[u8]) -> Result<Self, SigErr> {
        if src.len() < ENVELOPE_MIN {
            return Err(SigErr::TooShort);
        }
        if &src[0..4] != &MAGIC {
            return Err(SigErr::BadMagic);
        }
        if src[4] != VER_V1 {
            return Err(SigErr::BadVer);
        }
        let alg_id = src[5];
        if alg_id != ALG_ED25519_SHA512_256 {
            return Err(SigErr::BadAlg);
        }
        let flags = src[6];
        if flags & !FLAGS_KNOWN != 0 {
            return Err(SigErr::BadFlags);
        }
        let has_pending = flags & FLAG_PENDING != 0;
        let lineage_len = src[7] as usize;
        if lineage_len == 0 || lineage_len > LINEAGE_MAX || !valid_lineage(&src[8..8 + lineage_len]) {
            return Err(SigErr::BadLineage);
        }
        let mut off = 8 + lineage_len;
        // gen u32 BE + pk + (pending) + chain_len 字节的最小在场量先核。
        let fixed_tail = 4 + PK_LEN + if has_pending { PK_LEN } else { 0 } + 1 + SIG_LEN;
        if src.len() < off + fixed_tail {
            return Err(SigErr::BadLen);
        }
        let gen = u32::from_be_bytes([src[off], src[off + 1], src[off + 2], src[off + 3]]);
        off += 4;
        let mut pk = [0u8; PK_LEN];
        pk.copy_from_slice(&src[off..off + PK_LEN]);
        off += PK_LEN;
        let pending = if has_pending {
            let mut p = [0u8; PK_LEN];
            p.copy_from_slice(&src[off..off + PK_LEN]);
            off += PK_LEN;
            Some(p)
        } else {
            None
        };
        let chain_len = src[off] as usize;
        if chain_len > CHAIN_NODES_MAX {
            return Err(SigErr::BadChainLen);
        }
        off += 1;
        // 精确总长（无 trailer 容差——单一解释纪律）。
        let want = envelope_len(lineage_len, has_pending, chain_len);
        if src.len() != want {
            return Err(SigErr::BadLen);
        }
        let mut chain = [ChainNode { new_pk: [0; PK_LEN], rot_sig: [0; SIG_LEN] }; CHAIN_NODES_MAX];
        for node in chain.iter_mut().take(chain_len) {
            node.new_pk.copy_from_slice(&src[off..off + PK_LEN]);
            off += PK_LEN;
            node.rot_sig.copy_from_slice(&src[off..off + SIG_LEN]);
            off += SIG_LEN;
        }
        let mut top_sig = [0u8; SIG_LEN];
        top_sig.copy_from_slice(&src[off..off + SIG_LEN]);
        let mut lineage = [0u8; LINEAGE_MAX];
        lineage[..lineage_len].copy_from_slice(&src[8..8 + lineage_len]);
        // 链尾续接：链非空时末节点 new_pk 必 == pk（线性单路径终点执法）。
        if chain_len > 0 && chain[chain_len - 1].new_pk != pk {
            return Err(SigErr::ChainBroken);
        }
        Ok(SigEnvelopeV1 { alg_id, lineage, lineage_len, gen, pk, pending, chain, chain_len, top_sig })
    }

    /// 规范发射（字节级确定序 = 字段表序）。发射前复检结构不变量——
    /// 保证 [`parse`]∘[`encode`](Self::encode) 恒等即 canonical 形封闭。
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, SigErr> {
        if self.lineage_len == 0
            || self.lineage_len > LINEAGE_MAX
            || !valid_lineage(&self.lineage[..self.lineage_len])
        {
            return Err(SigErr::BadLineage);
        }
        if self.chain_len > CHAIN_NODES_MAX {
            return Err(SigErr::BadChainLen);
        }
        if self.chain_len > 0 && self.chain[self.chain_len - 1].new_pk != self.pk {
            return Err(SigErr::ChainBroken);
        }
        let need = envelope_len(self.lineage_len, self.pending.is_some(), self.chain_len);
        if out.len() < need {
            return Err(SigErr::BadOutLen);
        }
        out[0..4].copy_from_slice(&MAGIC);
        out[4] = VER_V1;
        out[5] = self.alg_id;
        out[6] = if self.pending.is_some() { FLAG_PENDING } else { 0 };
        out[7] = self.lineage_len as u8;
        let mut off = 8;
        out[off..off + self.lineage_len].copy_from_slice(self.lineage());
        off += self.lineage_len;
        out[off..off + 4].copy_from_slice(&self.gen.to_be_bytes());
        off += 4;
        out[off..off + PK_LEN].copy_from_slice(&self.pk);
        off += PK_LEN;
        if let Some(p) = self.pending {
            out[off..off + PK_LEN].copy_from_slice(&p);
            off += PK_LEN;
        }
        out[off] = self.chain_len as u8;
        off += 1;
        for node in self.chain() {
            out[off..off + PK_LEN].copy_from_slice(&node.new_pk);
            off += PK_LEN;
            out[off..off + SIG_LEN].copy_from_slice(&node.rot_sig);
            off += SIG_LEN;
        }
        out[off..off + SIG_LEN].copy_from_slice(&self.top_sig);
        debug_assert_eq!(off + SIG_LEN, need);
        Ok(need)
    }
}

/// lineage 值域：1..=16B `[a-z0-9-]`（帽同 SEC_RE——三处单源同步义务）。
fn valid_lineage(s: &[u8]) -> bool {
    s.iter().all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// `rot_message` 覆盖域最大长（`"BSIG-ROT"` 8 + 32 + 32 + 4 + lineage 帽 16）。
pub const ROT_MSG_MAX: usize = ROT_LABEL.len() + 2 * PK_LEN + 4 + LINEAGE_MAX;
/// `top_message` 覆盖域最大长（`"BSIG-TOP"` 8 + 信封头帽 + 两摘要 64）。
pub const TOP_MSG_MAX: usize = TOP_LABEL.len() + ENVELOPE_MAX - SIG_LEN + 2 * DIGEST_LEN;

/// 轮换链节点覆盖域：`"BSIG-ROT"` ‖ old_pk 32B ‖ new_pk 32B ‖ gen BE ‖
/// lineage 字节（24 号 §3.4——old_pk 入覆盖域防链节点嫁接）。
/// P0-2 验签消费；本刀供合同钉板。
pub fn rot_message(
    old_pk: &[u8],
    new_pk: &[u8],
    gen: u32,
    lineage: &[u8],
    out: &mut [u8],
) -> Result<usize, SigErr> {
    if old_pk.len() != PK_LEN || new_pk.len() != PK_LEN {
        return Err(SigErr::BadPkLen);
    }
    if lineage.len() > LINEAGE_MAX {
        return Err(SigErr::BadLineage);
    }
    let need = ROT_LABEL.len() + PK_LEN + PK_LEN + 4 + lineage.len();
    if out.len() < need {
        return Err(SigErr::BadOutLen);
    }
    let mut off = 0;
    out[..ROT_LABEL.len()].copy_from_slice(ROT_LABEL);
    off += ROT_LABEL.len();
    out[off..off + PK_LEN].copy_from_slice(old_pk);
    off += PK_LEN;
    out[off..off + PK_LEN].copy_from_slice(new_pk);
    off += PK_LEN;
    out[off..off + 4].copy_from_slice(&gen.to_be_bytes());
    off += 4;
    out[off..off + lineage.len()].copy_from_slice(lineage);
    Ok(need)
}

/// 顶签名覆盖域：`"BSIG-TOP"` ‖ 信封头（magic 至 chain 末字节 = 发射字节
/// 去 top_sig 尾段，长 = [`SigEnvelopeV1::header_len`]）‖ bap_digest 32B
/// （SHA-512/256 全字节）‖ manifest 关键域摘要 32B（单源 = berry-manifest
/// 解析产物派生，24 号 §3.3-1）。P0-2 验签消费；本刀供合同钉板。
pub fn top_message(
    envelope_header: &[u8],
    bap_digest: &[u8; DIGEST_LEN],
    manifest_digest: &[u8; DIGEST_LEN],
    out: &mut [u8],
) -> Result<usize, SigErr> {
    let need = TOP_LABEL.len() + envelope_header.len() + 2 * DIGEST_LEN;
    if out.len() < need {
        return Err(SigErr::BadOutLen);
    }
    let mut off = 0;
    out[..TOP_LABEL.len()].copy_from_slice(TOP_LABEL);
    off += TOP_LABEL.len();
    out[off..off + envelope_header.len()].copy_from_slice(envelope_header);
    off += envelope_header.len();
    out[off..off + DIGEST_LEN].copy_from_slice(bap_digest);
    off += DIGEST_LEN;
    out[off..off + DIGEST_LEN].copy_from_slice(manifest_digest);
    Ok(need)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合成信封（确定性填充：字节图案按字段长滚动）。
    fn sample(lineage_len: usize, pending: bool, chain_len: usize) -> SigEnvelopeV1 {
        let mut e = SigEnvelopeV1 {
            alg_id: ALG_ED25519_SHA512_256,
            lineage: [0; LINEAGE_MAX],
            lineage_len,
            gen: 0x0302_0100,
            pk: [0; PK_LEN],
            pending: None,
            chain: [ChainNode { new_pk: [0; PK_LEN], rot_sig: [0; SIG_LEN] }; CHAIN_NODES_MAX],
            chain_len,
            top_sig: [0; SIG_LEN],
        };
        for (i, b) in e.lineage.iter_mut().enumerate().take(lineage_len) {
            *b = match i % 3 {
                0 => b'o',
                1 => b'e',
                _ => b'-',
            };
        }
        // lineage 合法性：首字符必须字母/数字——用 "oem-system" 式图案。
        e.lineage[0] = b'o';
        let fill = |buf: &mut [u8], seed: u8| {
            for (i, b) in buf.iter_mut().enumerate() {
                *b = seed.wrapping_add(i as u8);
            }
        };
        fill(&mut e.pk, 0xA0);
        fill(&mut e.top_sig, 0xE0);
        if pending {
            let mut p = [0u8; PK_LEN];
            fill(&mut p, 0xB0);
            e.pending = Some(p);
        }
        // 链：node i new_pk 图案；末节点续接 pk。
        for i in 0..chain_len {
            fill(&mut e.chain[i].new_pk, 0xC0u8.wrapping_add(i as u8 * 8));
            fill(&mut e.chain[i].rot_sig, 0xD0u8.wrapping_add(i as u8 * 4));
        }
        if chain_len > 0 {
            e.chain[chain_len - 1].new_pk = e.pk;
        }
        e
    }

    // 1. round-trip 字节级对拍：解析(发射(x)) == x 且结构恒等——三档
    //    （最小/带 pending/满链帽）+ lineage 帽满档。
    #[test]
    fn roundtrip_byte_exact() {
        for (lin, pend, chain) in
            [(1, false, 0), (8, true, 0), (3, true, 3), (LINEAGE_MAX, true, CHAIN_NODES_MAX)]
        {
            let e = sample(lin, pend, chain);
            let mut buf = [0u8; ENVELOPE_MAX];
            let n = e.encode(&mut buf).expect("encode ok");
            assert_eq!(n, envelope_len(lin, pend, chain));
            let back = SigEnvelopeV1::parse(&buf[..n]).expect("parse ok");
            assert_eq!(back, e, "struct round-trip (lin={lin} pend={pend} chain={chain})");
            let mut buf2 = [0u8; ENVELOPE_MAX];
            let n2 = back.encode(&mut buf2).expect("re-encode ok");
            assert_eq!(n2, n);
            assert_eq!(&buf2[..n2], &buf[..n], "byte-exact round-trip");
        }
    }

    // 2. 拒面：magic / ver / alg / flags 保留位 / lineage 值域四型 /
    //    总长（截断与 trailer）/ 链帽 / 链断——全枚举咬合。
    #[test]
    fn reject_faces() {
        let e = sample(8, true, 2);
        let mut buf = [0u8; ENVELOPE_MAX];
        let n = e.encode(&mut buf).expect("encode ok");
        fn mangled(buf: &[u8], n: usize, f: &dyn Fn(&mut [u8])) -> Result<SigEnvelopeV1, SigErr> {
            let mut c = [0u8; ENVELOPE_MAX];
            c[..n].copy_from_slice(&buf[..n]);
            f(&mut c[..n]);
            SigEnvelopeV1::parse(&c[..n])
        }
        assert_eq!(mangled(&buf, n, &|c| c[0] = b'X'), Err(SigErr::BadMagic));
        assert_eq!(mangled(&buf, n, &|c| c[4] = 2), Err(SigErr::BadVer));
        assert_eq!(mangled(&buf, n, &|c| c[5] = 9), Err(SigErr::BadAlg));
        assert_eq!(mangled(&buf, n, &|c| c[6] = 0x02), Err(SigErr::BadFlags));
        assert_eq!(mangled(&buf, n, &|c| c[7] = 0), Err(SigErr::BadLineage));
        assert_eq!(mangled(&buf, n, &|c| c[7] = LINEAGE_MAX as u8 + 1), Err(SigErr::BadLineage));
        assert_eq!(mangled(&buf, n, &|c| c[8] = b'A'), Err(SigErr::BadLineage)); // 大写越值域
        assert_eq!(mangled(&buf, n, &|c| c[8] = b'_'), Err(SigErr::BadLineage)); // 字符集外
        assert_eq!(SigEnvelopeV1::parse(&buf[..n - 1]), Err(SigErr::BadLen)); // 截断
        assert_eq!(SigEnvelopeV1::parse(&buf[..n + 1]), Err(SigErr::BadLen)); // trailer
        assert_eq!(
            mangled(&buf, n, &|c| c[8 + 8 + 4 + PK_LEN + PK_LEN] = CHAIN_NODES_MAX as u8 + 1),
            Err(SigErr::BadChainLen)
        );
        // 链断：末节点（chain_len=2 → node 1，起点 = 链首 + 1×96B）new_pk 篡改。
        assert_eq!(
            mangled(&buf, n, &|c| {
                let off = 8 + 8 + 4 + PK_LEN + PK_LEN + 1 + (2 - 1) * (PK_LEN + SIG_LEN);
                c[off] ^= 0xFF;
            }),
            Err(SigErr::ChainBroken)
        );
        assert_eq!(SigEnvelopeV1::parse(&buf[..ENVELOPE_MIN - 1]), Err(SigErr::TooShort));
        // 发射缓冲不足。
        let mut small = [0u8; ENVELOPE_MAX];
        assert_eq!(e.encode(&mut small[..n - 1]), Err(SigErr::BadOutLen));
    }

    // 3. 长度公式与常量钉板：字段表自洽（P0-1 勘误注三的机械面）。
    #[test]
    fn length_formula_pins() {
        assert_eq!(ENVELOPE_MIN, 110); // 8+1+4+32+1+64
        assert_eq!(ENVELOPE_MAX, 1693); // 8+16+4+32+32+1+16*96+64
        assert_eq!(envelope_len(8, false, 0), 8 + 8 + 4 + 32 + 1 + 64);
        assert_eq!(envelope_len(1, true, 1), 8 + 1 + 4 + 32 + 32 + 1 + 96 + 64);
    }

    // 4. 覆盖域构建器钉板（P0-2 验签合同的字节面）：域分隔标签在头、
    //    字段序照 24 号 §3.4、长度自洽。
    #[test]
    fn message_builders_pin() {
        let e = sample(8, true, 2);
        let mut env = [0u8; ENVELOPE_MAX];
        let n = e.encode(&mut env).expect("encode ok");
        let header = &env[..e.header_len()];
        assert_eq!(e.header_len(), n - SIG_LEN);
        // rot_message。
        let old_pk = [0x11u8; PK_LEN];
        let mut rm = [0u8; ROT_MSG_MAX];
        let rn = rot_message(&old_pk, &e.pk, e.gen, e.lineage(), &mut rm).expect("rot ok");
        assert_eq!(rn, 8 + 32 + 32 + 4 + 8);
        assert_eq!(&rm[..8], ROT_LABEL);
        assert_eq!(&rm[8..40], &old_pk);
        assert_eq!(&rm[40..72], &e.pk);
        assert_eq!(&rm[72..76], &e.gen.to_be_bytes());
        assert_eq!(&rm[76..84], e.lineage());
        // top_message。
        let bd = [0x22u8; DIGEST_LEN];
        let md = [0x33u8; DIGEST_LEN];
        let mut tm = [0u8; TOP_MSG_MAX];
        let tn = top_message(header, &bd, &md, &mut tm).expect("top ok");
        assert_eq!(tn, 8 + header.len() + 64);
        assert_eq!(&tm[..8], TOP_LABEL);
        assert_eq!(&tm[8..8 + header.len()], header);
        assert_eq!(&tm[8 + header.len()..][..32], &bd);
        assert_eq!(&tm[tn - 32..tn], &md);
        // 拒面：缓冲不足 + 长度非法。
        let mut small = [0u8; ROT_MSG_MAX];
        assert_eq!(
            rot_message(&old_pk, &e.pk, e.gen, e.lineage(), &mut small[..rn - 1]),
            Err(SigErr::BadOutLen)
        );
        assert_eq!(rot_message(&old_pk, &[0u8; 31], e.gen, e.lineage(), &mut rm), Err(SigErr::BadPkLen));
        let mut small_top = [0u8; TOP_MSG_MAX];
        assert_eq!(top_message(header, &bd, &md, &mut small_top[..tn - 1]), Err(SigErr::BadOutLen));
    }

    // 5. flags 单一解释执法：pending 在场/缺席两态同构 round-trip——
    //    Janus 病灶（同字节流两读）在布局层不可表达。
    #[test]
    fn pending_disambiguation() {
        for pend in [false, true] {
            let e = sample(6, pend, 1);
            let mut buf = [0u8; ENVELOPE_MAX];
            let n = e.encode(&mut buf).expect("encode ok");
            let back = SigEnvelopeV1::parse(&buf[..n]).expect("parse ok");
            assert_eq!(back.pending.is_some(), pend);
            assert_eq!(back, e);
        }
        // 无 pending 时 flags 字节恰 0；在场时恰 FLAG_PENDING。
        let mut buf = [0u8; ENVELOPE_MAX];
        let n0 = sample(6, false, 0).encode(&mut buf).expect("ok");
        assert_eq!(buf[6], 0);
        let n1 = sample(6, true, 0).encode(&mut buf).expect("ok");
        assert_eq!(buf[6], FLAG_PENDING);
        assert_eq!(n1 - n0, PK_LEN); // 唯一差 = pending 32B
    }
}
