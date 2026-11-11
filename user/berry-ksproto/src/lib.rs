//! berry-ksproto —— keystored 密钥服务 RPC 协议纯函数库（SEC-01 §4.5
//! 件 15+16 day-one 留缝，`berry-psproto` 先例：零分配、零 unsafe、零
//! 依赖，输入只借用）。
//!
//! **day-one 缝 = 冻结数据面，不是实现密钥库**（贪工与偷工同罪）：本库
//! 不含任何密码学算法；keyblob / 证明 token 对协议层一律**不透明**（版本
//! 域 + 长度前缀 + 负载原样搬运，内容不作语义解析）；tag 值语义
//! （purpose/algorithm 等 enum 取值）随 v2 keystored 软桩落地再冻结——
//! 本 crate 只冻「号 → kind」映射与帧布局。密钥材料永不回传调用方
//! （keystore2 形态：出参一律加密不透明 blob，原始密钥只在后端/岛内）。
//!
//! 拓扑（v2 keystored 软桩落地时）：keystored（用户态密钥服务，
//! `docs/安全与硬件隔离设计.md` §6 C1 拍板）持通道一端，client（app /
//! 引导链 rollback 面）持对端——**单对双向 + 严格串行 RPC**（一请求一
//! 应答，队列深 1 用量；[`Session`] 把该纪律做成可执行状态机）。v1 软桩
//! SecurityLevel=Software，真机岛到位后**同协议换 Enclave/StrongBox
//! 后端**——三档枚举 day-one 进 wire（学 Android 把等级做成 API 一等
//! 字段）。
//!
//! 布局（v0，全定宽，全数值 LE；帧帽 ≤ [`CHAN_MSG_MAX`]=256B 走 channel
//! inline——真实 RSA keyblob ~1-2KiB 超 inline 走 VMO 随行 handle 面，
//! v2 软桩落地时再裁，不在本 crate 冻结面）：
//!
//! - op 面最小化三 op：[`OP_KEY_GENERATE`](constant@OP_KEY_GENERATE)
//!   / [`OP_KEY_ATTEST`](constant@OP_ATTEST 之类) / [`OP_COUNTER`]。
//! - KEY_GENERATE 请求 `[op][sw_n u8]{TLV}[hw_n u8]{TLV}`，TLV =
//!   `[tag u16][vlen u8][value]`——AuthorizationList **软硬双域**
//!   （KeyMint attestation 扩展同形：software_enforced 在前、
//!   hardware_enforced 在后；sw = keystored 执法域，hw = 岛内执法域——
//!   v1 软桩两域同后端，域位 day-one 冻结不省）。
//! - KEY_GENERATE 应答 OK：`[st=0][level u8][keyblob]`；非 OK：1B `[st]`
//!   （**恒不携 level/blob——烘焙线**）。keyblob = `[ver u16][plen u8]
//!   [payload]`（不透明三件套：版本域 + 长度前缀 + 负载）。
//! - KEY_ATTEST 请求 `[op][keyblob][clen u8][challenge]`（**字段位定长
//!   上限** [`CHALLENGE_MAX`]，空/超长拒）；应答同 generate 形（token
//!   帽 [`ATTEST_TOKEN_MAX`]）。
//! - COUNTER 请求 READ/INCREMENT = `[op][sub][slot u32]` 6B、CAS 追加
//!   `[expected u64][new u64]` 共 22B；应答恒 9B `[st][value u64]`
//!   （**非 OK 恒全零 value——烘焙线**，失败重试路径 = 先 READ 再 CAS）。
//!
//! 严格解析：未知 tag（含留位段号）/ vlen 与 kind 宽度不符 / 域计数超帽 /
//! challenge 空/超长 / blob 空/超帽 / 截断 / 尾随字节 / 未知 st / 未知
//! level 一律 None（「编不出就拒绝，不截断」——截断的授权表是谎言表）。

#![cfg_attr(not(test), no_std)]

// ============================================================================
// op / 子命令 / 应答状态
// ============================================================================

/// 生成密钥（KEY_GENERATE）：请求携 AuthorizationList 软硬双域，应答携
/// 不透明 keyblob + 后端 SecurityLevel。
pub const OP_KEY_GENERATE: u8 = 1;
/// 密钥证明（KEY_ATTEST）：请求携 keyblob + attestationChallenge，应答携
/// 不透明证明 token + SecurityLevel（token 语义归 v3 EAT 面消费）。
pub const OP_KEY_ATTEST: u8 = 2;
/// 单调计数器（COUNTER）：[`CounterBackend`] 三方法的 RPC 面——L0 启动
/// 计数（ADR #47 回退检测）与 AVB 式 rollback index 统一走它。
pub const OP_COUNTER: u8 = 3;

/// COUNTER 子命令：读当前值。
pub const SUB_COUNTER_READ: u8 = 1;
/// COUNTER 子命令：单调 +1。
pub const SUB_COUNTER_INCREMENT: u8 = 2;
/// COUNTER 子命令：compare_and_set（expected 匹配才写 new；回卷拒）。
pub const SUB_COUNTER_CAS: u8 = 3;

/// 应答状态：成功。
pub const ST_OK: u8 = 0;
/// 应答状态：请求畸形（wire / 参数拒面——协议层 None 的服务端投影）。
pub const ST_BAD: u8 = 1;
/// 应答状态：对象不存在（keyblob 不可认 / counter slot 未分配）。
pub const ST_NOENT: u8 = 2;
/// 应答状态：回卷拒（counter CAS new < 当前值——单调语义安全执法）。
/// 仅 COUNTER 9B 应答形合法；1B 形（generate/attest 面）携带即拒。
pub const ST_ROLLBACK: u8 = 3;
/// 应答状态：CAS expected 不匹配（并发推进过——读改写重试）。
/// 仅 COUNTER 9B 应答形合法。
pub const ST_CAS_FAIL: u8 = 4;

// ============================================================================
// SecurityLevel 三档（day-one 进 wire——Android 把等级做成 API 一等字段）
// ============================================================================

/// 安全等级 wire 值：软件实现（v1 QEMU 软桩唯一档）。
pub const SECLEVEL_SOFTWARE: u8 = 0;
/// 安全等级 wire 值：可信执行环境（真机 TEE 后端）。
pub const SECLEVEL_TRUSTED_ENVIRONMENT: u8 = 1;
/// 安全等级 wire 值：独立安全元件（StrongBox 类第二实例——同协议不同档）。
pub const SECLEVEL_STRONG_BOX: u8 = 2;

/// 后端安全等级三档（wire 值见 `SECLEVEL_*`；KeyMint SecurityLevel 同序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityLevel {
    /// 软件实现（v1 软桩——进程级隔离 + 能力约束，无硬件边界）。
    Software,
    /// 可信执行环境（TrustZone 类 TEE——BerryOS v1–v3 不建岛，客户端定位）。
    TrustedEnvironment,
    /// 独立安全元件（StrongBox 类——SE 上的第二 keystored 实例）。
    StrongBox,
}

impl SecurityLevel {
    /// wire 字节。
    pub const fn to_byte(self) -> u8 {
        match self {
            SecurityLevel::Software => SECLEVEL_SOFTWARE,
            SecurityLevel::TrustedEnvironment => SECLEVEL_TRUSTED_ENVIRONMENT,
            SecurityLevel::StrongBox => SECLEVEL_STRONG_BOX,
        }
    }

    /// wire 字节 → 枚举；域外字节 None（未知等级拒——三档之外线上不存在）。
    pub const fn from_byte(b: u8) -> Option<Self> {
        match b {
            SECLEVEL_SOFTWARE => Some(SecurityLevel::Software),
            SECLEVEL_TRUSTED_ENVIRONMENT => Some(SecurityLevel::TrustedEnvironment),
            SECLEVEL_STRONG_BOX => Some(SecurityLevel::StrongBox),
            _ => None,
        }
    }
}

// ============================================================================
// tag 编号表（KeyMint 借号段，add-only 冻结）
// ============================================================================

/// tag 值类别（wire 宽度执法的唯一真值源；`vlen` 必须恰等于类别宽）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagKind {
    /// 布尔（presence 即真——vlen 恒 0）。
    Bool,
    /// u32 LE（4B）。
    U32,
    /// u64 LE（8B）。
    U64,
}

impl TagKind {
    /// wire 值宽（B）：Bool=0 / U32=4 / U64=8。
    pub const fn width(self) -> usize {
        match self {
            TagKind::Bool => 0,
            TagKind::U32 => 4,
            TagKind::U64 => 8,
        }
    }
}

// 域划分注释冻结（改号段前先读这段）：
//
//   号段借自 Android KeyMint（`android.hardware.security.keymint` Tag
//   编号，语义同名对应）。**add-only**：已用段只增不改不删；新增 tag =
//   加常量 + 入 `KNOWN_TAGS`/`KNOWN_KINDS` 两表（编译期断言护住升序与
//   配对）。tag 的**取值语义**（purpose/algorithm 等 enum 值）不在本表
//   冻结——归 v2 keystored。
//
//   已用段（kind 表在册，wire 可编解码）：
//     [1..=3, 5..=6]   算法/参数族：purpose / algorithm / keySize /
//                       digest / padding（U32）
//     [303, 305]       使用策略族：rollbackResistance / earlyBootOnly
//                       （Bool——删除后不可恢复→RPMB 执法 / 仅早启可用）
//     [502, 505]       认证绑定族：userSecureId（U64）/ authTimeout（U32）
//     [701..=702]      元数据族：creationDateTime（U64）/ origin（U32）
//     [705..=706]      版本棘轮族：osVersion / osPatchLevel（U32）
//
//   留位段（号段记名成常量但 kind 表未收——auth list TLV 内编码 v1 拒
//   （未知 tag 拒面），随需要 add-only 入表）：
//     [4]              blockMode（对称面 v2 随 AES 族入表）
//     [704]            rootOfTrust（KeyMint 语义「绑定但不可指定、永不返
//                      回」——attest 扩展输出面 v3）
//     [708..=709]      attestationChallenge / attestationApplicationId：
//                      challenge 走 KEY_ATTEST 请求**字段位**（定长上限
//                      + 超长拒，见 [`CHALLENGE_MAX`]）；appId 字段位留
//                      位；auth list 内 TLV 形留证明扩展 v3
//     [710..=723]      attestationId* ID 证明族（锁设备专用，v3+）

/// 用途（签名/加密/验签/解密/议密——KeyMint KeyPurpose 值域归 v2）。
pub const TAG_PURPOSE: u16 = 1;
/// 算法（RSA/EC/ML_DSA…）。
pub const TAG_ALGORITHM: u16 = 2;
/// 密钥位宽。
pub const TAG_KEY_SIZE: u16 = 3;
/// 摘要算法。
pub const TAG_DIGEST: u16 = 5;
/// 填充模式。
pub const TAG_PADDING: u16 = 6;
/// 删除后不可恢复（→ RPMB 执法）。
pub const TAG_ROLLBACK_RESISTANCE: u16 = 303;
/// 仅早启阶段可用。
pub const TAG_EARLY_BOOT_ONLY: u16 = 305;
/// 绑定用户认证 SID。
pub const TAG_USER_SECURE_ID: u16 = 502;
/// 认证时效（秒）。
pub const TAG_AUTH_TIMEOUT: u16 = 505;
/// 生成时间。
pub const TAG_CREATION_DATE_TIME: u16 = 701;
/// 来源（GENERATED/IMPORTED/SECURELY_IMPORTED——值域归 v2）。
pub const TAG_ORIGIN: u16 = 702;
/// 版本绑定棘轮：OS 版本。
pub const TAG_OS_VERSION: u16 = 705;
/// 版本绑定棘轮：OS 补丁级。
pub const TAG_OS_PATCH_LEVEL: u16 = 706;

// —— 留位段记名常量（kind 表未收——auth list TLV 内编码 = 未知 tag 拒）——

/// 块模式（留位——对称面 v2 入表）。
pub const TAG_BLOCK_MODE: u16 = 4;
/// 信任根（留位——attest 输出面 v3；「绑定不指定、永不返回」）。
pub const TAG_ROOT_OF_TRUST: u16 = 704;
/// 证明挑战值（留位 TLV 形——KEY_ATTEST 请求字段位承接，见
/// [`CHALLENGE_MAX`]）。
pub const TAG_ATTESTATION_CHALLENGE: u16 = 708;
/// 证明申请方身份（留位——字段位 + TLV 形均 v3）。
pub const TAG_ATTESTATION_APPLICATION_ID: u16 = 709;

/// 已用段 tag 表（与 [`KNOWN_KINDS`] 逐位配对；严格升序——编译期断言）。
pub const KNOWN_TAGS: [u16; 13] = [
    TAG_PURPOSE,
    TAG_ALGORITHM,
    TAG_KEY_SIZE,
    TAG_DIGEST,
    TAG_PADDING,
    TAG_ROLLBACK_RESISTANCE,
    TAG_EARLY_BOOT_ONLY,
    TAG_USER_SECURE_ID,
    TAG_AUTH_TIMEOUT,
    TAG_CREATION_DATE_TIME,
    TAG_ORIGIN,
    TAG_OS_VERSION,
    TAG_OS_PATCH_LEVEL,
];

/// 已用段 kind 表（索引与 [`KNOWN_TAGS`] 配对——号 → 宽度的单一事实源）。
pub const KNOWN_KINDS: [TagKind; 13] = [
    TagKind::U32, // [1]   purpose
    TagKind::U32, // [2]   algorithm
    TagKind::U32, // [3]   keySize
    TagKind::U32, // [5]   digest
    TagKind::U32, // [6]   padding
    TagKind::Bool, // [303] rollbackResistance
    TagKind::Bool, // [305] earlyBootOnly
    TagKind::U64, // [502] userSecureId
    TagKind::U32, // [505] authTimeout
    TagKind::U64, // [701] creationDateTime
    TagKind::U32, // [702] origin
    TagKind::U32, // [705] osVersion
    TagKind::U32, // [706] osPatchLevel
];

const fn tags_strictly_ascending(tags: &[u16]) -> bool {
    let mut i = 1;
    while i < tags.len() {
        if tags[i - 1] >= tags[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// 号 → kind 查询（const：表驱动零分支查找）；留位段/域外号 None。
pub const fn tag_kind(tag: u16) -> Option<TagKind> {
    let mut i = 0;
    while i < KNOWN_TAGS.len() {
        if KNOWN_TAGS[i] == tag {
            return Some(KNOWN_KINDS[i]);
        }
        i += 1;
    }
    None
}

/// 编译期冻结断言（防号段漂移）：借 KeyMint 的编号逐颗钉死——改任何
/// `TAG_*` 常量值即编译失败；表升序 + 与 kind 表逐位配对。
const _: () = {
    assert!(TAG_PURPOSE == 1, "KeyMint 借号段冻结");
    assert!(TAG_ALGORITHM == 2, "KeyMint 借号段冻结");
    assert!(TAG_KEY_SIZE == 3, "KeyMint 借号段冻结");
    assert!(TAG_DIGEST == 5, "KeyMint 借号段冻结");
    assert!(TAG_PADDING == 6, "KeyMint 借号段冻结");
    assert!(TAG_ROLLBACK_RESISTANCE == 303, "KeyMint 借号段冻结");
    assert!(TAG_EARLY_BOOT_ONLY == 305, "KeyMint 借号段冻结");
    assert!(TAG_USER_SECURE_ID == 502, "KeyMint 借号段冻结");
    assert!(TAG_AUTH_TIMEOUT == 505, "KeyMint 借号段冻结");
    assert!(TAG_CREATION_DATE_TIME == 701, "KeyMint 借号段冻结");
    assert!(TAG_ORIGIN == 702, "KeyMint 借号段冻结");
    assert!(TAG_OS_VERSION == 705, "KeyMint 借号段冻结");
    assert!(TAG_OS_PATCH_LEVEL == 706, "KeyMint 借号段冻结");
    assert!(TAG_BLOCK_MODE == 4, "留位段记名同样钉死");
    assert!(TAG_ROOT_OF_TRUST == 704, "留位段记名同样钉死");
    assert!(TAG_ATTESTATION_CHALLENGE == 708, "留位段记名同样钉死");
    assert!(TAG_ATTESTATION_APPLICATION_ID == 709, "留位段记名同样钉死");
    assert!(
        tags_strictly_ascending(&KNOWN_TAGS),
        "tag 号段表必须严格升序（add-only 冻结）"
    );
    assert!(KNOWN_TAGS.len() == KNOWN_KINDS.len(), "tag/kind 表逐位配对");
};

// ============================================================================
// 帧帽（channel inline 面预算——编译期断言护住全部 ≤ CHAN_MSG_MAX）
// ============================================================================

/// channel inline 帧帽（kernel CHAN_MSG_MAX 同值——协议家族共用常量）。
pub const CHAN_MSG_MAX: usize = 256;
/// 单域 tag 条目数帽。
pub const AUTH_TAGS_MAX: usize = 8;
/// 单条目 wire 最宽（tag 2B + vlen 1B + U64 值 8B）。
pub const AUTH_ENTRY_WIRE_MAX: usize = 2 + 1 + 8;
/// 单域最宽（count 1B + 8 × 最宽条目）。
pub const AUTH_LIST_WIRE_MAX: usize = 1 + AUTH_TAGS_MAX * AUTH_ENTRY_WIRE_MAX;
/// KEY_GENERATE 请求帧帽（op 1B + 软硬双域）。
pub const GEN_REQ_MAX: usize = 1 + 2 * AUTH_LIST_WIRE_MAX;
/// keyblob 负载帽（v1 软桩 blob 远小于此；真 RSA blob 走 VMO 面 v2 裁）。
pub const KEYBLOB_MAX: usize = 160;
/// KEY_GENERATE OK 应答帧帽（st + level + blob 三件套）。
pub const GEN_REP_MAX: usize = 2 + 3 + KEYBLOB_MAX;
/// attestationChallenge 字段位定长上限（远程验证方 nonce 规模 32–64B）。
pub const CHALLENGE_MAX: usize = 64;
/// KEY_ATTEST 请求帧帽（op + keyblob 三件套 + challenge 字段位）。
pub const ATTEST_REQ_MAX: usize = 1 + (3 + KEYBLOB_MAX) + (1 + CHALLENGE_MAX);
/// 证明 token 负载帽（token 语义 v3 EAT 面消费——协议层不透明）。
pub const ATTEST_TOKEN_MAX: usize = 160;
/// KEY_ATTEST OK 应答帧帽。
pub const ATTEST_REP_MAX: usize = 2 + 3 + ATTEST_TOKEN_MAX;
/// COUNTER READ/INCREMENT 请求恰长（op + sub + slot）。
pub const COUNTER_REQ_SHORT_LEN: usize = 1 + 1 + 4;
/// COUNTER CAS 请求恰长（短形 + expected + new）。
pub const COUNTER_REQ_CAS_LEN: usize = COUNTER_REQ_SHORT_LEN + 8 + 8;
/// COUNTER 请求帧帽。
pub const COUNTER_REQ_MAX: usize = COUNTER_REQ_CAS_LEN;
/// COUNTER 应答恰长（st + value）。
pub const COUNTER_REP_LEN: usize = 1 + 8;

/// 帧帽预算断言：全部消息形 ≤ channel inline 帽（超帽 = 须切 VMO 面的
/// 结构性变化，编译期炸响）。
const _: () = {
    assert!(CHAN_MSG_MAX == 256, "inline 帽与 kernel CHAN_MSG_MAX 同值");
    assert!(GEN_REQ_MAX <= CHAN_MSG_MAX, "GEN_REQ_MAX 超 inline 帽");
    assert!(GEN_REP_MAX <= CHAN_MSG_MAX, "GEN_REP_MAX 超 inline 帽");
    assert!(ATTEST_REQ_MAX <= CHAN_MSG_MAX, "ATTEST_REQ_MAX 超 inline 帽");
    assert!(ATTEST_REP_MAX <= CHAN_MSG_MAX, "ATTEST_REP_MAX 超 inline 帽");
    assert!(COUNTER_REQ_MAX <= CHAN_MSG_MAX, "COUNTER_REQ_MAX 超 inline 帽");
    assert!(COUNTER_REP_LEN <= CHAN_MSG_MAX, "COUNTER_REP_LEN 超 inline 帽");
};

fn valid_op(op: u8) -> bool {
    op == OP_KEY_GENERATE || op == OP_KEY_ATTEST || op == OP_COUNTER
}

/// generate/attest 面 1B 非_OK 形的合法状态域（ROLLBACK/CAS_FAIL 是
/// COUNTER 9B 形专属——状态族按形状钉死，1B 形携带即拒）。
fn valid_status_st(st: u8) -> bool {
    st == ST_BAD || st == ST_NOENT
}

/// COUNTER 9B 应答形的合法状态域。
fn valid_counter_st(st: u8) -> bool {
    st == ST_OK || valid_status_st(st) || st == ST_ROLLBACK || st == ST_CAS_FAIL
}

// ============================================================================
// AuthorizationList 软硬双域编解码
// ============================================================================

/// 授权表条目（编码侧输入；value 宽度由 [`tag_kind`] 执法）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthEntry<'a> {
    pub tag: u16,
    pub value: &'a [u8],
}

/// 授权表双域：sw = keystored 软件执法域 / hw = 岛内硬件执法域（KeyMint
/// software_enforced / hardware_enforced 同形——v1 软桩两域同后端，域位
/// day-one 冻结不省：换真件后端时 hw 域的执法者跟着换，wire 不动）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthDomain {
    Software,
    Hardware,
}

/// 编码授权表（裸面——无 op 前缀）：`[sw_n][TLV…][hw_n][TLV…]`。
/// 拒绝面（条目数超 [`AUTH_TAGS_MAX`] / 未知 tag——含留位段号 / 值宽与
/// kind 不符 / buf 不足）一律 None，不落半帧。**重复 tag 不拒**：
/// purpose/digest 族 KeyMint 语义允许重复，wire 是 TLV 袋——唯一性/重复
/// 语义归 keystored。
pub fn put_auth_list(out: &mut [u8], sw: &[AuthEntry<'_>], hw: &[AuthEntry<'_>]) -> Option<usize> {
    if sw.len() > AUTH_TAGS_MAX || hw.len() > AUTH_TAGS_MAX {
        return None;
    }
    // 校验先行（宽度/未知 tag），再一次性定长写——中途失败不落半帧。
    let mut total = 2usize;
    for e in sw.iter().chain(hw.iter()) {
        let kind = tag_kind(e.tag)?;
        if e.value.len() != kind.width() {
            return None;
        }
        total += 3 + e.value.len();
    }
    let m = out.get_mut(..total)?;
    m[0] = sw.len() as u8;
    let mut pos = 1usize;
    for e in sw {
        pos = put_entry(m, pos, e);
    }
    m[pos] = hw.len() as u8;
    pos += 1;
    for e in hw {
        pos = put_entry(m, pos, e);
    }
    debug_assert_eq!(pos, total);
    Some(total)
}

/// 条目落帧（前置校验已过——纯写入，返回新游标）。
fn put_entry(m: &mut [u8], pos: usize, e: &AuthEntry<'_>) -> usize {
    m[pos..pos + 2].copy_from_slice(&e.tag.to_le_bytes());
    m[pos + 2] = e.value.len() as u8;
    m[pos + 3..pos + 3 + e.value.len()].copy_from_slice(e.value);
    pos + 3 + e.value.len()
}

/// 解析授权表（裸面）：逐条回调 `(domain, tag, value)`。畸形（截断 /
/// 域计数超帽 / 未知 tag / vlen 与 kind 宽度不符 / 尾随字节）整体 None
/// ——半张授权表不进消费面。
pub fn parse_auth_list(buf: &[u8], mut on_entry: impl FnMut(AuthDomain, u16, &[u8])) -> Option<()> {
    let mut rest = buf;
    for domain in [AuthDomain::Software, AuthDomain::Hardware] {
        let n = *rest.first()? as usize;
        if n > AUTH_TAGS_MAX {
            return None;
        }
        rest = &rest[1..];
        for _ in 0..n {
            if rest.len() < 3 {
                return None;
            }
            let tag = u16::from_le_bytes([rest[0], rest[1]]);
            let vlen = rest[2] as usize;
            let kind = tag_kind(tag)?; // 未知 tag（含留位段）拒
            if vlen != kind.width() {
                return None;
            }
            if rest.len() < 3 + vlen {
                return None;
            }
            on_entry(domain, tag, &rest[3..3 + vlen]);
            rest = &rest[3 + vlen..];
        }
    }
    if !rest.is_empty() {
        return None; // 恰长执法——尾随字节 = 畸形
    }
    Some(())
}

// ============================================================================
// 不透明 blob 三件套（版本域 + 长度前缀 + 不透明负载）
// ============================================================================

/// 编码不透明 blob：`[ver u16 LE][plen u8][payload]`。拒绝面（payload
/// 空 / 超 `cap` / buf 不足）None；**负载原样搬运，内容不作语义解析**
/// ——协议层对 keyblob/证明 token 零知识。
pub fn put_blob(out: &mut [u8], version: u16, payload: &[u8], cap: usize) -> Option<usize> {
    if payload.is_empty() || payload.len() > cap || cap > 255 {
        return None;
    }
    let total = 3 + payload.len();
    let m = out.get_mut(..total)?;
    m[0..2].copy_from_slice(&version.to_le_bytes());
    m[2] = payload.len() as u8;
    m[3..].copy_from_slice(payload);
    Some(total)
}

/// 解析不透明 blob（恰长执法——尾随字节拒；payload 空 / 超 `cap` 拒）。
/// 返回 `(版本域, 负载切片)`；截断任何一处 None。
pub fn parse_blob(buf: &[u8], cap: usize) -> Option<(u16, &[u8])> {
    if buf.len() < 4 {
        return None;
    }
    let ver = u16::from_le_bytes([buf[0], buf[1]]);
    let plen = buf[2] as usize;
    if plen == 0 || plen > cap || buf.len() != 3 + plen {
        return None;
    }
    Some((ver, &buf[3..]))
}

// ============================================================================
// KEY_GENERATE
// ============================================================================

/// 编码 KEY_GENERATE 请求：`[op][授权表双域]`。拒绝面同 [`put_auth_list`]
/// + buf 不足。
pub fn encode_generate_request(
    out: &mut [u8],
    sw: &[AuthEntry<'_>],
    hw: &[AuthEntry<'_>],
) -> Option<usize> {
    let n = put_auth_list(out.get_mut(1..)?, sw, hw)?;
    out[0] = OP_KEY_GENERATE;
    Some(1 + n)
}

/// 解析 KEY_GENERATE 请求（服务端）：op 字节先行校验，授权表逐条回调。
/// 非 KEY_GENERATE op / 授权表畸形 → None。
pub fn parse_generate_request(
    buf: &[u8],
    on_entry: impl FnMut(AuthDomain, u16, &[u8]),
) -> Option<()> {
    if buf.first()? != &OP_KEY_GENERATE {
        return None;
    }
    parse_auth_list(buf.get(1..)?, on_entry)
}

/// KEY_GENERATE 应答（解析产物；st=ST_OK 时 level/blob 有意义，非 OK 形
/// 恒 Software/0/空占位——**不消费**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenReply<'a> {
    pub st: u8,
    /// 后端安全等级（仅 OK 形携带）。
    pub level: SecurityLevel,
    /// keyblob 版本域（仅 OK 形携带）。
    pub blob_ver: u16,
    /// 不透明 keyblob 负载（仅 OK 形非空）。
    pub blob: &'a [u8],
}

/// 编码 KEY_GENERATE **成功**应答：`[ST_OK][level][keyblob 三件套]`。
/// 烘焙线：本函数只编 OK 形（非 OK 走 [`encode_status_reply`] 1B 形——
/// 非 OK 恒不携 level/blob）；blob 空/超 [`KEYBLOB_MAX`] / buf 不足 None。
pub fn encode_generate_reply(
    out: &mut [u8],
    level: SecurityLevel,
    blob_ver: u16,
    blob: &[u8],
) -> Option<usize> {
    let n = put_blob(out.get_mut(2..)?, blob_ver, blob, KEYBLOB_MAX)?;
    out[0] = ST_OK;
    out[1] = level.to_byte();
    Some(2 + n)
}

/// 编码 generate/attest 面**非 OK** 应答：1B `[st]`。烘焙线：st=ST_OK 拒
/// （成功必携 blob——1B OK 形线上不存在）；counter 专属状态
/// （ROLLBACK/CAS_FAIL）在 1B 形拒（状态族按形状钉死）。
pub fn encode_status_reply(out: &mut [u8], st: u8) -> Option<usize> {
    if !valid_status_st(st) {
        return None;
    }
    out.get_mut(..1)?[0] = st;
    Some(1)
}

/// 解析 KEY_GENERATE 应答（客户端）。两形：
/// - 1B `[st]`（st ∈ {BAD, NOENT}）→ blob 空占位；
/// - `[ST_OK][level][blob 三件套]`（恰长）。
///
/// 烘焙线双侧执法：1B 形携 OK / 长形携非 OK / 未知 level / blob 空/超帽/
/// 截断/尾随 → None。
pub fn parse_generate_reply(buf: &[u8]) -> Option<GenReply<'_>> {
    let st = *buf.first()?;
    if buf.len() == 1 {
        if !valid_status_st(st) {
            return None; // 1B OK 形不存在；counter 专属状态 1B 形拒
        }
        return Some(GenReply {
            st,
            level: SecurityLevel::Software,
            blob_ver: 0,
            blob: &[],
        });
    }
    if st != ST_OK {
        return None; // 非 OK 恒不携 level/blob——长形非 OK 线上不存在
    }
    let level = SecurityLevel::from_byte(*buf.get(1)?)?;
    let (blob_ver, blob) = parse_blob(buf.get(2..)?, KEYBLOB_MAX)?;
    Some(GenReply {
        st,
        level,
        blob_ver,
        blob,
    })
}

// ============================================================================
// KEY_ATTEST
// ============================================================================

/// KEY_ATTEST 请求（解析产物；blob 与 challenge 均借用原缓冲）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttestReq<'a> {
    /// 待证明 keyblob 版本域。
    pub blob_ver: u16,
    /// 待证明 keyblob 负载（不透明）。
    pub blob: &'a [u8],
    /// attestationChallenge（远程验证方 nonce，1..=[`CHALLENGE_MAX`]）。
    pub challenge: &'a [u8],
}

/// KEY_ATTEST 应答（解析产物；烘焙线与 [`GenReply`] 同形——非 OK 恒
/// Software/0/空占位）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttestReply<'a> {
    pub st: u8,
    /// 出证明的后端等级（仅 OK 形携带）。
    pub level: SecurityLevel,
    /// 证明 token 版本域（仅 OK 形携带）。
    pub token_ver: u16,
    /// 不透明证明 token（仅 OK 形非空；语义归 v3 EAT 消费面）。
    pub token: &'a [u8],
}

/// 编码 KEY_ATTEST 请求：`[op][keyblob 三件套][clen][challenge]`。
/// **字段位定长上限**：challenge 空 / 超 [`CHALLENGE_MAX`] 拒（非法定义
/// 在协议层，不是调用方约定）；blob 面 [`put_blob`] 同执法。
pub fn encode_attest_request(
    out: &mut [u8],
    blob_ver: u16,
    blob: &[u8],
    challenge: &[u8],
) -> Option<usize> {
    if challenge.is_empty() || challenge.len() > CHALLENGE_MAX {
        return None;
    }
    let n = put_blob(out.get_mut(1..)?, blob_ver, blob, KEYBLOB_MAX)?;
    let total = 1 + n + 1 + challenge.len();
    let m = out.get_mut(..total)?;
    m[0] = OP_KEY_ATTEST;
    m[1 + n] = challenge.len() as u8;
    m[2 + n..].copy_from_slice(challenge);
    Some(total)
}

/// 解析 KEY_ATTEST 请求（服务端）：op / blob 三件套 / challenge 字段位
/// 全严格；challenge 空/超帽 / blob 空/超帽 / 截断 / 尾随 → None。
///
/// blob 在帧**中段**（后随 challenge 字段位）——不走 [`parse_blob`]（其
/// 恰长执法只适用帧尾），就地取 ver/plen 头 + 域内切片，恰长由整帧总长
/// 收口。
pub fn parse_attest_request(buf: &[u8]) -> Option<AttestReq<'_>> {
    if buf.first()? != &OP_KEY_ATTEST {
        return None;
    }
    let rest = buf.get(1..)?;
    if rest.len() < 4 {
        return None; // ver 2B + plen 1B + 至少 1B 负载 + clen 1B
    }
    let blob_ver = u16::from_le_bytes([rest[0], rest[1]]);
    let plen = rest[2] as usize;
    if plen == 0 || plen > KEYBLOB_MAX || rest.len() < 3 + plen + 1 {
        return None;
    }
    let blob = &rest[3..3 + plen];
    let chsec = &rest[3 + plen..];
    let clen = chsec[0] as usize;
    if clen == 0 || clen > CHALLENGE_MAX || chsec.len() != 1 + clen {
        return None; // 空/超帽 + 恰长执法（截断/尾随同面）
    }
    Some(AttestReq {
        blob_ver,
        blob,
        challenge: &chsec[1..],
    })
}

/// 编码 KEY_ATTEST **成功**应答：`[ST_OK][level][token 三件套]`（token
/// 帽 [`ATTEST_TOKEN_MAX`]；非 OK 走 [`encode_status_reply`]）。
pub fn encode_attest_reply(
    out: &mut [u8],
    level: SecurityLevel,
    token_ver: u16,
    token: &[u8],
) -> Option<usize> {
    let n = put_blob(out.get_mut(2..)?, token_ver, token, ATTEST_TOKEN_MAX)?;
    out[0] = ST_OK;
    out[1] = level.to_byte();
    Some(2 + n)
}

/// 解析 KEY_ATTEST 应答（烘焙线与 [`parse_generate_reply`] 同形）。
pub fn parse_attest_reply(buf: &[u8]) -> Option<AttestReply<'_>> {
    let st = *buf.first()?;
    if buf.len() == 1 {
        if !valid_status_st(st) {
            return None;
        }
        return Some(AttestReply {
            st,
            level: SecurityLevel::Software,
            token_ver: 0,
            token: &[],
        });
    }
    if st != ST_OK {
        return None;
    }
    let level = SecurityLevel::from_byte(*buf.get(1)?)?;
    let (token_ver, token) = parse_blob(buf.get(2..)?, ATTEST_TOKEN_MAX)?;
    Some(AttestReply {
        st,
        level,
        token_ver,
        token,
    })
}

// ============================================================================
// COUNTER（件 16——L0 启动计数 + rollback index 统一面）
// ============================================================================

/// counter slot 域上界（0..=32——AVB 式 rollback index 槽规模；L0 启动
/// 计数（ADR #47）同住此域，见 [`SLOT_L0_BOOT`]）。
pub const COUNTER_SLOTS_MAX: u32 = 32;

/// slot 0 记名：L0 启动计数（ADR #47 启动回退检测）——rollback index
/// 槽从 1..= 起。
pub const SLOT_L0_BOOT: u32 = 0;

/// slot 句柄域执法（wire u32 定宽，域外拒）。
pub const fn slot_valid(slot: u32) -> bool {
    slot <= COUNTER_SLOTS_MAX
}

/// COUNTER 请求三形（slot u32 定宽；CAS 携 expected/new 全宽）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterReq {
    /// 读当前值（对应 [`CounterBackend::read`]）。
    Read { slot: u32 },
    /// 单调 +1（对应 [`CounterBackend::increment`]）。
    Increment { slot: u32 },
    /// compare_and_set（对应 [`CounterBackend::compare_and_set`]）。
    CompareAndSet { slot: u32, expected: u64, new: u64 },
}

/// 编码 COUNTER 请求：READ/INCREMENT 6B / CAS 22B。slot 域外（>32）/
/// buf 不足 → None。
pub fn encode_counter_request(out: &mut [u8], req: &CounterReq) -> Option<usize> {
    let (sub, slot, expected, new) = match *req {
        CounterReq::Read { slot } => (SUB_COUNTER_READ, slot, 0, 0),
        CounterReq::Increment { slot } => (SUB_COUNTER_INCREMENT, slot, 0, 0),
        CounterReq::CompareAndSet {
            slot,
            expected,
            new,
        } => (SUB_COUNTER_CAS, slot, expected, new),
    };
    if !slot_valid(slot) {
        return None;
    }
    let total = if sub == SUB_COUNTER_CAS {
        COUNTER_REQ_CAS_LEN
    } else {
        COUNTER_REQ_SHORT_LEN
    };
    let m = out.get_mut(..total)?;
    m[0] = OP_COUNTER;
    m[1] = sub;
    m[2..6].copy_from_slice(&slot.to_le_bytes());
    if sub == SUB_COUNTER_CAS {
        m[6..14].copy_from_slice(&expected.to_le_bytes());
        m[14..22].copy_from_slice(&new.to_le_bytes());
    }
    Some(total)
}

/// 解析 COUNTER 请求：op/sub/形状三重校验（READ/INCREMENT 恰 6B、CAS 恰
/// 22B——尾随/截断拒）；slot 域外 / 未知 sub（含 0）→ None。
pub fn parse_counter_request(buf: &[u8]) -> Option<CounterReq> {
    if buf.first()? != &OP_COUNTER {
        return None;
    }
    let sub = *buf.get(1)?;
    let slot = u32::from_le_bytes(buf.get(2..6)?.try_into().ok()?);
    if !slot_valid(slot) {
        return None;
    }
    match sub {
        SUB_COUNTER_READ if buf.len() == COUNTER_REQ_SHORT_LEN => Some(CounterReq::Read { slot }),
        SUB_COUNTER_INCREMENT if buf.len() == COUNTER_REQ_SHORT_LEN => {
            Some(CounterReq::Increment { slot })
        }
        SUB_COUNTER_CAS if buf.len() == COUNTER_REQ_CAS_LEN => {
            let expected = u64::from_le_bytes(buf[6..14].try_into().ok()?);
            let new = u64::from_le_bytes(buf[14..22].try_into().ok()?);
            Some(CounterReq::CompareAndSet {
                slot,
                expected,
                new,
            })
        }
        _ => None, // 未知 sub + 形状不符（截断/尾随）
    }
}

/// COUNTER 应答（解析产物）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CounterReply {
    pub st: u8,
    /// 当前/新值（OK 形携带；**非 OK 恒全零**——烘焙线，失败重试 = 先
    /// READ 再 CAS，不从错误应答里读值）。
    pub value: u64,
}

/// 编码 COUNTER 应答：恒 [`COUNTER_REP_LEN`] 9B。烘焙线双侧执法：未知
/// st → None；**非 OK 携非零 value → None**（调用方传 0 即可，库执法
/// 不靠自觉——netdproto「非 OK 恒全零 addr」同族）。
pub fn encode_counter_reply(out: &mut [u8], st: u8, value: u64) -> Option<usize> {
    if !valid_counter_st(st) {
        return None;
    }
    if st != ST_OK && value != 0 {
        return None; // 烘焙线：非 OK 恒全零 value
    }
    let m = out.get_mut(..COUNTER_REP_LEN)?;
    m[0] = st;
    m[1..9].copy_from_slice(&value.to_le_bytes());
    Some(COUNTER_REP_LEN)
}

/// 解析 COUNTER 应答：恰 9B + st 在域 + 非 OK 恒全零 value（与编码侧
/// 对称——线上不变量双侧钉死）。
pub fn parse_counter_reply(buf: &[u8]) -> Option<CounterReply> {
    if buf.len() != COUNTER_REP_LEN {
        return None;
    }
    let st = buf[0];
    let value = u64::from_le_bytes(buf[1..9].try_into().ok()?);
    if !valid_counter_st(st) {
        return None;
    }
    if st != ST_OK && value != 0 {
        return None; // 烘焙线对称执法
    }
    Some(CounterReply { st, value })
}

// ============================================================================
// 单调计数器后端抽象（件 16 本体）
// ============================================================================

/// [`CounterBackend`] 值错误：slot 域外 / 未分配（非法句柄面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterError {
    NoSuchSlot,
    /// u64 顶格——单调语义下无更高值可取（回卷拒同族执法：顶格即终态，
    /// 不允许绕道回卷）。
    Saturated,
}

/// [`CounterBackend::compare_and_set`] 错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CasError {
    NoSuchSlot,
    /// expected ≠ 当前值（并发推进过——读改写重试）。
    Mismatch { current: u64 },
    /// **new < 当前值——回卷拒**（安全执法，检查序先于 Mismatch）。
    Rollback { current: u64 },
}

/// 单调计数器后端抽象（SEC-4 §5.2：L0 启动计数回退检测（ADR #47）与
/// AVB 式 rollback index **统一走它**——day-one 不走单调接口，v4 接
/// RPMB 时引导链要重写一遍）。
///
/// 后端拓扑：v1 QEMU = 宿主侧文件（smoke 确定性）；真机 = RPMB/eFuse。
/// RPMB 帧驱动可放 devmgr（vblk 同族持证应用），但 **auth key 持有者与
/// HMAC 计算在 keystored**（Linux「内核只透传帧、密钥在 TEE」同拓扑，
/// BerryOS 的「安全侧最小近似」= keystored）——内核零 RPMB 知识。
///
/// 语义契约（一切实现必须遵守；本 crate `#[cfg(test)]` 的
/// `InMemoryCounters` 为参考实现）：
/// - **单调不减**：slot 值生存期内只能前进（严格增或幂等重写），任何
///   路径不得使其回退——掉电/崩溃/并发下同样成立（真后端靠 RPMB
///   「写前计数」/ eFuse 不可逆性保证）。
/// - **回卷拒**：`compare_and_set(new < 当前值)` 一律
///   [`Err(CasError::Rollback)`]，**即使 expected 恰好匹配**——
///   expected 是并发提示不是授权；回卷拒是安全执法，检查序先于
///   Mismatch（expected 不匹配且 new 回卷 → 仍报 Rollback）。
/// - `new == 当前值` 幂等重写合法（非回卷）。
/// - slot 域 0..=[`COUNTER_SLOTS_MAX`]；域外/未分配 → NoSuchSlot。
pub trait CounterBackend {
    /// 读当前值；slot 域外/未分配 → [`Err(CounterError::NoSuchSlot)`]。
    fn read(&self, slot: u32) -> Result<u64, CounterError>;

    /// 单调 +1，返回新值；域外/未分配 → NoSuchSlot；u64 顶格 →
    /// [`Err(CounterError::Saturated)`]（顶格即终态——不允许绕道回卷）。
    fn increment(&mut self, slot: u32) -> Result<u64, CounterError>;

    /// expected 匹配当前值才写 new，返回写入值。检查序（安全执法优先）：
    /// ① slot 面NoSuchSlot → ② **new < 当前值 → Rollback** →
    /// ③ expected ≠ 当前值 → Mismatch → ④ 写入返 new。
    fn compare_and_set(&mut self, slot: u32, expected: u64, new: u64) -> Result<u64, CasError>;
}

// ============================================================================
// 严格串行 RPC 会话机
// ============================================================================

/// 严格串行 RPC 会话机（psproto 头注「一请求一应答，队列深 1 用量」纪律
/// 的可执行形态——两端共用同一状态机执法畸形序）。
///
/// 相位两档：请求侧（可发/可收请求）↔ 应答侧（在途一请求，只可收应答）。
/// **畸形会话序** = 应答相位再收请求（上一请求未收口）/ 请求相位收应答
/// （无在途请求）→ None；拒收**不改相位**（在途请求不因畸形输入丢失）。
/// op ↔ 应答形配对由消息级恰长/形状执法承接（1B 形 vs 9B 形 vs blob 形
/// 的 st 域各自钉死），本机只管相位序。
#[derive(Debug)]
pub struct Session {
    pending: bool,
}

impl Session {
    /// 新会话（请求相位起步）。
    pub const fn new() -> Self {
        Session { pending: false }
    }

    /// 是否有在途请求（应答相位）。
    pub const fn awaiting_reply(&self) -> bool {
        self.pending
    }

    /// 请求相位转移（发请求前/服务端收请求时调用）。畸形序（在途未收口
    /// 再收请求）或野 op（三 op 之外）→ None。
    pub fn on_request(&mut self, op: u8) -> Option<()> {
        if self.pending || !valid_op(op) {
            return None;
        }
        self.pending = true;
        Some(())
    }

    /// 应答相位收口（收应答时调用）。无在途请求 → None；收口后相位回
    /// 请求侧。
    pub fn on_reply(&mut self) -> Option<()> {
        if !self.pending {
            return None;
        }
        self.pending = false;
        Some(())
    }
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

// ============================================================================
// 测试（host std 挂具；InMemoryCounters = 件 16 参考实现）
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 纯内存测试后端（件 16——**仅 #[cfg(test)]**；slot 全 None 起步、
    /// `init` 显式落初值，单调/回卷拒语义按 trait 契约执法——未来
    /// keystored 的 QEMU 软桩后端同形）。
    struct InMemoryCounters {
        slots: [Option<u64>; COUNTER_SLOTS_MAX as usize + 1],
    }

    impl InMemoryCounters {
        fn new() -> Self {
            InMemoryCounters {
                slots: [None; COUNTER_SLOTS_MAX as usize + 1],
            }
        }

        /// 首次落初值（已分配 → false；域外 → false）。
        fn init(&mut self, slot: u32, value: u64) -> bool {
            if !slot_valid(slot) {
                return false;
            }
            match self.slots.get_mut(slot as usize) {
                Some(c @ None) => {
                    *c = Some(value);
                    true
                }
                _ => false,
            }
        }

        fn cell(&self, slot: u32) -> Result<u64, ()> {
            match self.slots.get(slot as usize) {
                Some(Some(v)) => Ok(*v),
                _ => Err(()),
            }
        }
    }

    impl CounterBackend for InMemoryCounters {
        fn read(&self, slot: u32) -> Result<u64, CounterError> {
            self.cell(slot).map_err(|_| CounterError::NoSuchSlot)
        }

        fn increment(&mut self, slot: u32) -> Result<u64, CounterError> {
            let cur = self.cell(slot).map_err(|_| CounterError::NoSuchSlot)?;
            let nv = cur.checked_add(1).ok_or(CounterError::Saturated)?;
            self.slots[slot as usize] = Some(nv);
            Ok(nv)
        }

        fn compare_and_set(&mut self, slot: u32, expected: u64, new: u64) -> Result<u64, CasError> {
            let cur = self.cell(slot).map_err(|_| CasError::NoSuchSlot)?;
            if new < cur {
                return Err(CasError::Rollback { current: cur });
            }
            if expected != cur {
                return Err(CasError::Mismatch { current: cur });
            }
            self.slots[slot as usize] = Some(new);
            Ok(new)
        }
    }

    // —— tag 号段冻结 ————————————————————————————————————————————

    /// 运行时镜像编译期断言：借号逐颗对表（KeyMint 权威值）+ 升序 +
    /// kind 配对 + kind 宽度。
    #[test]
    fn tag_number_space_frozen() {
        assert_eq!(TAG_PURPOSE, 1);
        assert_eq!(TAG_ALGORITHM, 2);
        assert_eq!(TAG_KEY_SIZE, 3);
        assert_eq!(TAG_DIGEST, 5);
        assert_eq!(TAG_PADDING, 6);
        assert_eq!(TAG_ROLLBACK_RESISTANCE, 303);
        assert_eq!(TAG_EARLY_BOOT_ONLY, 305);
        assert_eq!(TAG_USER_SECURE_ID, 502);
        assert_eq!(TAG_AUTH_TIMEOUT, 505);
        assert_eq!(TAG_CREATION_DATE_TIME, 701);
        assert_eq!(TAG_ORIGIN, 702);
        assert_eq!(TAG_OS_VERSION, 705);
        assert_eq!(TAG_OS_PATCH_LEVEL, 706);
        // 留位段记名（kind 表未收）。
        assert_eq!(TAG_BLOCK_MODE, 4);
        assert_eq!(TAG_ROOT_OF_TRUST, 704);
        assert_eq!(TAG_ATTESTATION_CHALLENGE, 708);
        assert_eq!(TAG_ATTESTATION_APPLICATION_ID, 709);
        // 表升序 + 配对（编译期断言的运行时对读面）。
        let mut prev = 0u16;
        for (i, &t) in KNOWN_TAGS.iter().enumerate() {
            assert!(t > prev, "KNOWN_TAGS 升序违约 @{}", i);
            prev = t;
            assert_eq!(KNOWN_KINDS.len(), KNOWN_TAGS.len());
            assert_eq!(tag_kind(t), Some(KNOWN_KINDS[i]));
        }
        // kind 宽度钉板。
        assert_eq!(TagKind::Bool.width(), 0);
        assert_eq!(TagKind::U32.width(), 4);
        assert_eq!(TagKind::U64.width(), 8);
    }

    /// 未知 tag 拒：域外号（0/7/100/723/9999）与**留位段记名号**
    /// （4/704/708/709——kind 表未收即线上不存在）。
    #[test]
    fn tag_kind_unknown_and_reserved_reject() {
        for t in [0u16, 7, 100, 304, 710, 723, 9999] {
            assert_eq!(tag_kind(t), None, "域外号 {} 应拒", t);
        }
        // 留位段：记名常量存在但 kind 表未收——auth list TLV 内编码拒。
        for t in [TAG_BLOCK_MODE, TAG_ROOT_OF_TRUST, TAG_ATTESTATION_CHALLENGE,
                  TAG_ATTESTATION_APPLICATION_ID] {
            assert_eq!(tag_kind(t), None, "留位段 {} 未入 kind 表应拒", t);
        }
        // 已用段抽查。
        assert_eq!(tag_kind(TAG_PURPOSE), Some(TagKind::U32));
        assert_eq!(tag_kind(TAG_ROLLBACK_RESISTANCE), Some(TagKind::Bool));
        assert_eq!(tag_kind(TAG_USER_SECURE_ID), Some(TagKind::U64));
    }

    // —— SecurityLevel ————————————————————————————————————————————

    #[test]
    fn security_level_wire_domain() {
        assert_eq!(
            SecurityLevel::from_byte(SECLEVEL_SOFTWARE),
            Some(SecurityLevel::Software)
        );
        assert_eq!(
            SecurityLevel::from_byte(SECLEVEL_TRUSTED_ENVIRONMENT),
            Some(SecurityLevel::TrustedEnvironment)
        );
        assert_eq!(
            SecurityLevel::from_byte(SECLEVEL_STRONG_BOX),
            Some(SecurityLevel::StrongBox)
        );
        for b in 3..=u8::MAX {
            assert_eq!(SecurityLevel::from_byte(b), None, "level {} 域外应拒", b);
        }
        for lv in [
            SecurityLevel::Software,
            SecurityLevel::TrustedEnvironment,
            SecurityLevel::StrongBox,
        ] {
            assert_eq!(SecurityLevel::from_byte(lv.to_byte()), Some(lv));
        }
        // 三档 wire 值钉板（0/1/2——KeyMint SecurityLevel 同序）。
        assert_eq!((SECLEVEL_SOFTWARE, SECLEVEL_TRUSTED_ENVIRONMENT, SECLEVEL_STRONG_BOX),
                   (0, 1, 2));
    }

    // —— AuthorizationList ————————————————————————————————————————

    fn u32v(v: u32) -> [u8; 4] {
        v.to_le_bytes()
    }

    fn u64v(v: u64) -> [u8; 8] {
        v.to_le_bytes()
    }

    fn roundtrip_auth(sw: &[AuthEntry<'_>], hw: &[AuthEntry<'_>]) -> Vec<(AuthDomain, u16, Vec<u8>)> {
        let mut buf = [0u8; CHAN_MSG_MAX];
        let n = put_auth_list(&mut buf, sw, hw).expect("合法面应编出");
        let mut out = Vec::new();
        parse_auth_list(&buf[..n], |d, t, v| out.push((d, t, v.to_vec())))
            .expect("合法面应解出");
        assert_eq!(out.len(), sw.len() + hw.len());
        out
    }

    #[test]
    fn auth_list_roundtrip_multi_domain() {
        let p = u32v(2); // KM_PURPOSE_SIGN 借值（值语义 v2 冻结）
        let ks = u32v(256);
        let sid = u64v(0x0102_0304_0506_0708);
        let dt = u64v(1_770_000_000_000);
        let ov = u32v(1);
        let sw = [
            AuthEntry { tag: TAG_PURPOSE, value: &p },
            AuthEntry { tag: TAG_KEY_SIZE, value: &ks },
        ];
        let hw = [
            AuthEntry { tag: TAG_USER_SECURE_ID, value: &sid },
            AuthEntry { tag: TAG_ROLLBACK_RESISTANCE, value: &[] },
            AuthEntry { tag: TAG_CREATION_DATE_TIME, value: &dt },
            AuthEntry { tag: TAG_OS_VERSION, value: &ov },
        ];
        let got = roundtrip_auth(&sw, &hw);
        let want: Vec<(AuthDomain, u16, Vec<u8>)> = sw
            .iter()
            .map(|e| (AuthDomain::Software, e.tag, e.value.to_vec()))
            .chain(
                hw.iter()
                    .map(|e| (AuthDomain::Hardware, e.tag, e.value.to_vec())),
            )
            .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn auth_list_empty_golden() {
        // 双空域：`[sw_n=0][hw_n=0]` 2B——结构合法（keyblob 域外授权全后端定）。
        let mut buf = [0u8; 8];
        let n = put_auth_list(&mut buf, &[], &[]).unwrap();
        assert_eq!(n, 2);
        assert_eq!(&buf[..n], &[0, 0]);
        assert_eq!(parse_auth_list(&buf[..n], |_, _, _| panic!("无条目")), Some(()));
    }

    #[test]
    fn auth_list_encode_rejects() {
        let p = u32v(2);
        let mut buf = [0u8; CHAN_MSG_MAX];
        // 未知 tag（域外 7 + 留位段 708——kind 表未收）。
        for bad in [7u16, TAG_BLOCK_MODE, TAG_ROOT_OF_TRUST, TAG_ATTESTATION_CHALLENGE] {
            assert_eq!(
                put_auth_list(&mut buf, &[AuthEntry { tag: bad, value: &p }], &[]),
                None,
                "未知 tag {} 编码应拒",
                bad
            );
        }
        // 值宽与 kind 不符：purpose 收 8B / rollbackResistance 收 4B /
        // userSecureId 收 4B。
        assert_eq!(
            put_auth_list(
                &mut buf,
                &[AuthEntry { tag: TAG_PURPOSE, value: &u64v(2) }],
                &[]
            ),
            None
        );
        assert_eq!(
            put_auth_list(
                &mut buf,
                &[AuthEntry { tag: TAG_ROLLBACK_RESISTANCE, value: &p }],
                &[]
            ),
            None
        );
        assert_eq!(
            put_auth_list(
                &mut buf,
                &[AuthEntry { tag: TAG_USER_SECURE_ID, value: &p }],
                &[]
            ),
            None
        );
        // 条目数超帽：9 条（单域帽 8）。
        let many: Vec<AuthEntry<'_>> = (0..=AUTH_TAGS_MAX)
            .map(|_| AuthEntry { tag: TAG_DIGEST, value: &p })
            .collect();
        assert_eq!(put_auth_list(&mut buf, &many, &[]), None);
        assert_eq!(put_auth_list(&mut buf, &[], &many), None);
        // 恰帽 8 条可编（帽边界）。
        let cap8: Vec<AuthEntry<'_>> = (0..AUTH_TAGS_MAX)
            .map(|_| AuthEntry { tag: TAG_DIGEST, value: &p })
            .collect();
        assert!(put_auth_list(&mut buf, &cap8, &[]).is_some());
        // buf 不足：恰帽最宽双域 > 小 buf。
        let mut small = [0u8; 16];
        assert_eq!(put_auth_list(&mut small, &cap8, &[]), None);
        // 空 buf。
        assert_eq!(put_auth_list(&mut [], &[], &[]), None);
    }

    #[test]
    fn auth_list_parse_rejects() {
        let p = u32v(2);
        // 基准帧：sw=[purpose] hw=[rollbackResistance]。
        let mut base = Vec::new();
        base.extend_from_slice(&[1]);
        base.extend_from_slice(&TAG_PURPOSE.to_le_bytes());
        base.push(4);
        base.extend_from_slice(&p);
        base.extend_from_slice(&[1]);
        base.extend_from_slice(&TAG_ROLLBACK_RESISTANCE.to_le_bytes());
        base.push(0);
        assert!(parse_auth_list(&base, |_, _, _| ()).is_some());
        // 截断：每切点全拒。
        for cut in 0..base.len() {
            assert_eq!(
                parse_auth_list(&base[..cut], |_, _, _| ()),
                None,
                "截断 @{} 应拒",
                cut
            );
        }
        // 尾随字节。
        let mut tail = base.clone();
        tail.push(0xAA);
        assert_eq!(parse_auth_list(&tail, |_, _, _| ()), None);
        // 未知 tag（域外 + 留位段）。
        for bad in [7u16, TAG_BLOCK_MODE, 704, 708] {
            let mut m = vec![1u8];
            m.extend_from_slice(&bad.to_le_bytes());
            m.push(4);
            m.extend_from_slice(&p);
            m.push(0);
            assert_eq!(parse_auth_list(&m, |_, _, _| ()), None, "未知 tag {} 应拒", bad);
        }
        // vlen 与 kind 宽度不符（purpose 声 8B 但只给 4B 截断面 + 声 0）。
        let mut w0 = vec![1u8];
        w0.extend_from_slice(&TAG_PURPOSE.to_le_bytes());
        w0.push(0);
        w0.push(0);
        assert_eq!(parse_auth_list(&w0, |_, _, _| ()), None);
        // 域计数超帽（sw_n=9）。
        let mut over = vec![9u8];
        over.extend_from_slice(&TAG_PURPOSE.to_le_bytes());
        over.push(4);
        over.extend_from_slice(&p);
        assert_eq!(parse_auth_list(&over, |_, _, _| ()), None);
        // 缺第二域 count 字节。
        let mut one = vec![0u8];
        one.extend_from_slice(&TAG_PURPOSE.to_le_bytes());
        one.push(4);
        one.extend_from_slice(&p);
        assert_eq!(parse_auth_list(&one, |_, _, _| ()), None);
    }

    // —— KEY_GENERATE ——————————————————————————————————————————————

    #[test]
    fn generate_request_roundtrip_and_golden() {
        let p = u32v(2);
        let ks = u32v(256);
        let sw = [
            AuthEntry { tag: TAG_PURPOSE, value: &p },
            AuthEntry { tag: TAG_KEY_SIZE, value: &ks },
        ];
        let hw = [AuthEntry { tag: TAG_ROLLBACK_RESISTANCE, value: &[] }];
        let mut buf = [0u8; GEN_REQ_MAX];
        let n = encode_generate_request(&mut buf, &sw, &hw).unwrap();
        // 黄金字节钉板（改布局 = 此测必炸）。
        let mut want = vec![OP_KEY_GENERATE, 2];
        want.extend_from_slice(&TAG_PURPOSE.to_le_bytes());
        want.push(4);
        want.extend_from_slice(&p);
        want.extend_from_slice(&TAG_KEY_SIZE.to_le_bytes());
        want.push(4);
        want.extend_from_slice(&ks);
        want.push(1);
        want.extend_from_slice(&TAG_ROLLBACK_RESISTANCE.to_le_bytes());
        want.push(0);
        assert_eq!(n, want.len());
        assert_eq!(&buf[..n], want.as_slice());
        // 往返。
        let mut got: Vec<(AuthDomain, u16, Vec<u8>)> = Vec::new();
        parse_generate_request(&buf[..n], |d, t, v| {
            got.push((d, t, v.to_vec()))
        })
        .unwrap();
        assert_eq!(
            got,
            vec![
                (AuthDomain::Software, TAG_PURPOSE, p.to_vec()),
                (AuthDomain::Software, TAG_KEY_SIZE, ks.to_vec()),
                (AuthDomain::Hardware, TAG_ROLLBACK_RESISTANCE, vec![]),
            ]
        );
        // 帧帽常量钉板（预算数学防漂移）。
        assert_eq!(AUTH_ENTRY_WIRE_MAX, 11);
        assert_eq!(AUTH_LIST_WIRE_MAX, 89);
        assert_eq!(GEN_REQ_MAX, 179);
    }

    #[test]
    fn generate_request_rejects_wrong_op_and_malformed() {
        let p = u32v(2);
        let sw = [AuthEntry { tag: TAG_PURPOSE, value: &p }];
        let mut buf = [0u8; GEN_REQ_MAX];
        let n = encode_generate_request(&mut buf, &sw, &[]).unwrap();
        // op 字节改 2/3 → parse 拒（op 分派面）。
        for op in [OP_KEY_ATTEST, OP_COUNTER, 0, 9] {
            let mut m = buf;
            m[0] = op;
            assert_eq!(
                parse_generate_request(&m, |_, _, _| ()),
                None,
                "op {} 应拒",
                op
            );
        }
        // 授权表畸形（截断）逐切点拒。
        for cut in 0..n {
            assert_eq!(
                parse_generate_request(&buf[..cut], |_, _, _| ()),
                None,
                "截断 @{} 应拒",
                cut
            );
        }
        // 空 buf。
        assert_eq!(parse_generate_request(&[], |_, _, _| ()), None);
    }

    #[test]
    fn generate_reply_roundtrip_and_golden() {
        // OK 形黄金字节：st + level + ver u16 LE + plen + 负载。
        let mut buf = [0u8; GEN_REP_MAX];
        let n = encode_generate_reply(
            &mut buf,
            SecurityLevel::TrustedEnvironment,
            0x0BEE,
            &[0xAA, 0xBB, 0xCC],
        )
        .unwrap();
        assert_eq!(n, 8);
        assert_eq!(
            &buf[..n],
            &[ST_OK, SECLEVEL_TRUSTED_ENVIRONMENT, 0xEE, 0x0B, 3, 0xAA, 0xBB, 0xCC][..]
        );
        let r = parse_generate_reply(&buf[..n]).unwrap();
        assert_eq!(r.st, ST_OK);
        assert_eq!(r.level, SecurityLevel::TrustedEnvironment);
        assert_eq!(r.blob_ver, 0x0BEE);
        assert_eq!(r.blob, &[0xAA, 0xBB, 0xCC]);
        // 三档 level 全往返（StrongBox 同协议第二实例的档位面）。
        for lv in [
            SecurityLevel::Software,
            SecurityLevel::TrustedEnvironment,
            SecurityLevel::StrongBox,
        ] {
            let n = encode_generate_reply(&mut buf, lv, 0, &[0x01]).unwrap();
            assert_eq!(parse_generate_reply(&buf[..n]).unwrap().level, lv);
        }
        // 最宽 blob 边界恰收。
        let wide = [0x5Au8; KEYBLOB_MAX];
        let n = encode_generate_reply(&mut buf, SecurityLevel::Software, 7, &wide).unwrap();
        assert_eq!(n, GEN_REP_MAX);
        assert_eq!(parse_generate_reply(&buf[..n]).unwrap().blob, &wide[..]);
    }

    #[test]
    fn generate_reply_baked_line_and_rejects() {
        let mut buf = [0u8; GEN_REP_MAX];
        let n = encode_generate_reply(&mut buf, SecurityLevel::Software, 0, &[1, 2]).unwrap();
        // 截断每切点拒（含 1B 切——1B OK 形线上不存在）。
        for cut in 0..n {
            assert_eq!(parse_generate_reply(&buf[..cut]), None, "截断 @{} 应拒", cut);
        }
        // 尾随字节。
        let mut tail = buf;
        tail[n] = 0x99;
        assert_eq!(parse_generate_reply(&tail[..n + 1]), None);
        // 未知 level 字节。
        let mut bad = buf;
        bad[1] = 3;
        assert_eq!(parse_generate_reply(&bad[..n]), None);
        // blob plen 超/空：plen 声 0 / 声 > 帽（手造帧——编码器编不出）。
        let z = [ST_OK, SECLEVEL_SOFTWARE, 0, 0, 0];
        assert_eq!(parse_generate_reply(&z), None); // plen=0
        let mut over = [0u8; 32];
        over[0] = ST_OK;
        over[1] = SECLEVEL_SOFTWARE;
        over[2] = 0;
        over[3] = 161; // KEYBLOB_MAX+1
        assert_eq!(parse_generate_reply(&over), None);
        // 长 5B 但声明 plen=1 只给 4B 区 → 恰长违约（3+1≠5-2）拒。
        let lie = [ST_OK, SECLEVEL_SOFTWARE, 0, 1, 0xAA, 0xBB];
        assert_eq!(parse_generate_reply(&lie), None);
        // 编码侧：blob 空 / 超帽 / buf 不足。
        let mut small = [0u8; 6];
        assert_eq!(
            encode_generate_reply(&mut small, SecurityLevel::Software, 0, &[1, 2, 3, 4]),
            None
        );
        let mut b2 = [0u8; GEN_REP_MAX];
        assert_eq!(
            encode_generate_reply(&mut b2, SecurityLevel::Software, 0, &[]),
            None
        );
        let over_blob = [0u8; KEYBLOB_MAX + 1];
        assert_eq!(
            encode_generate_reply(&mut b2, SecurityLevel::Software, 0, &over_blob),
            None
        );
        // 1B 非 OK 形：合法域 {BAD, NOENT}；OK/ROLLBACK/CAS_FAIL 1B 形拒
        // （状态族按形状钉死——烘焙线）。
        for st in [ST_BAD, ST_NOENT] {
            let mut m = [0u8; 2];
            assert_eq!(encode_status_reply(&mut m, st), Some(1));
            assert_eq!(m[0], st);
            let r = parse_generate_reply(&m[..1]).unwrap();
            assert_eq!(r.st, st);
            assert_eq!(r.blob, &[]); // 非 OK 恒不携 blob
        }
        for st in [ST_OK, ST_ROLLBACK, ST_CAS_FAIL, 5, 255] {
            let mut m = [0u8; 2];
            assert_eq!(encode_status_reply(&mut m, st), None, "1B 形 st={} 应拒", st);
            assert_eq!(parse_generate_reply(&[st]), None);
        }
        // 长形非 OK：线上不存在。
        let mut lnb = buf;
        lnb[0] = ST_BAD;
        assert_eq!(parse_generate_reply(&lnb[..n]), None);
    }

    // —— 不透明 blob 三件套（keyblob/证明 token 共用编码） ————————————

    #[test]
    fn blob_codec_shared_roundtrip_and_rejects() {
        let mut buf = [0u8; 256];
        // 版本域 LE 钉板：ver=0x0102 → 字节 02 01。
        let n = put_blob(&mut buf, 0x0102, &[0xFF], 8).unwrap();
        assert_eq!(&buf[..n], &[0x02, 0x01, 1, 0xFF]);
        assert_eq!(parse_blob(&buf[..n], 8), Some((0x0102, &[0xFF][..])));
        // 两帽共用：keyblob 帽与 token 帽各自执法。
        let wide = [0x11u8; 160];
        assert_eq!(put_blob(&mut buf, 0, &wide, KEYBLOB_MAX).unwrap(), 163);
        assert_eq!(put_blob(&mut buf, 0, &wide, ATTEST_TOKEN_MAX).unwrap(), 163);
        let over = [0u8; 161];
        assert_eq!(put_blob(&mut buf, 0, &over, KEYBLOB_MAX), None);
        assert_eq!(put_blob(&mut buf, 0, &over, ATTEST_TOKEN_MAX), None);
        // 空/零帽。
        assert_eq!(put_blob(&mut buf, 0, &[], 8), None);
        assert_eq!(put_blob(&mut buf, 0, &[1], 0), None);
        // 解析拒绝面：<4B / plen=0 / plen>cap / 尾随。
        assert_eq!(parse_blob(&[], 8), None);
        assert_eq!(parse_blob(&[0, 0, 1], 8), None);
        assert_eq!(parse_blob(&[0, 0, 0, 1], 8), None); // plen=0
        assert_eq!(parse_blob(&[0, 0, 2, 1, 2], 1), None); // plen>cap
        assert_eq!(parse_blob(&[0, 0, 1, 1, 9], 8), None); // 尾随
        // 截断每切点。
        let n = put_blob(&mut buf, 0x1234, &[1, 2, 3, 4], 8).unwrap();
        for cut in 0..n {
            assert_eq!(parse_blob(&buf[..cut], 8), None, "截断 @{} 应拒", cut);
        }
    }

    // —— KEY_ATTEST ————————————————————————————————————————————————

    #[test]
    fn attest_request_roundtrip_and_challenge_cap() {
        let blob = [0xDE, 0xAD];
        let ch = b"berry";
        let mut buf = [0u8; ATTEST_REQ_MAX];
        let n = encode_attest_request(&mut buf, 1, &blob, ch).unwrap();
        // 黄金字节：op + ver LE + plen + blob + clen + challenge。
        assert_eq!(
            &buf[..n],
            &[OP_KEY_ATTEST, 0x01, 0x00, 2, 0xDE, 0xAD, 5, b'b', b'e', b'r', b'r', b'y'][..]
        );
        let r = parse_attest_request(&buf[..n]).unwrap();
        assert_eq!(r.blob_ver, 1);
        assert_eq!(r.blob, &blob);
        assert_eq!(r.challenge, ch);
        // 挑战帽边界：1B 恰收 / 64B 恰收 / 0 拒 / 65 拒。
        let mut b2 = [0u8; ATTEST_REQ_MAX];
        let c1 = [0x7Au8; 1];
        let n1 = encode_attest_request(&mut b2, 0, &blob, &c1).unwrap();
        assert_eq!(parse_attest_request(&b2[..n1]).unwrap().challenge, &c1[..]);
        let c64 = [0x7Bu8; CHALLENGE_MAX];
        let n64 = encode_attest_request(&mut b2, 0, &blob, &c64).unwrap();
        assert_eq!(parse_attest_request(&b2[..n64]).unwrap().challenge, &c64[..]);
        assert_eq!(encode_attest_request(&mut b2, 0, &blob, &[]), None);
        let c65 = [0x7Cu8; CHALLENGE_MAX + 1];
        assert_eq!(encode_attest_request(&mut b2, 0, &blob, &c65), None);
        // 解析侧同执法（手造帧）：clen=0 / clen=65 / 截断 / 尾随。
        let mut m = vec![OP_KEY_ATTEST, 1, 0, 2, 0xDE, 0xAD, 0];
        assert_eq!(parse_attest_request(&m), None);
        *m.last_mut().unwrap() = 65;
        assert_eq!(parse_attest_request(&m), None);
        for cut in 0..n {
            assert_eq!(parse_attest_request(&buf[..cut]), None, "截断 @{} 应拒", cut);
        }
        let mut tail = buf;
        tail[n] = 0x55;
        assert_eq!(parse_attest_request(&tail[..n + 1]), None);
        // keyblob 面：空 blob / 超帽 blob 拒（put_blob 单源执法）。
        assert_eq!(encode_attest_request(&mut b2, 0, &[], ch), None);
        let over = [0u8; KEYBLOB_MAX + 1];
        assert_eq!(encode_attest_request(&mut b2, 0, &over, ch), None);
        // op 字节错值拒。
        let mut mop = buf;
        mop[0] = OP_KEY_GENERATE;
        assert_eq!(parse_attest_request(&mop[..n]), None);
    }

    #[test]
    fn attest_reply_roundtrip_and_baked_line() {
        let mut buf = [0u8; ATTEST_REP_MAX];
        let tok = [0xC0, 0xFF, 0xEE];
        let n = encode_attest_reply(&mut buf, SecurityLevel::StrongBox, 2, &tok).unwrap();
        assert_eq!(n, 8);
        let r = parse_attest_reply(&buf[..n]).unwrap();
        assert_eq!(r.st, ST_OK);
        assert_eq!(r.level, SecurityLevel::StrongBox);
        assert_eq!(r.token_ver, 2);
        assert_eq!(r.token, &tok);
        // 最宽 token 边界。
        let wide = [0x09u8; ATTEST_TOKEN_MAX];
        let n = encode_attest_reply(&mut buf, SecurityLevel::Software, 9, &wide).unwrap();
        assert_eq!(n, ATTEST_REP_MAX);
        assert_eq!(parse_attest_reply(&buf[..n]).unwrap().token, &wide[..]);
        // 截断/尾随/未知 level/1B OK 形。
        for cut in 0..n {
            assert_eq!(parse_attest_reply(&buf[..cut]), None, "截断 @{} 应拒", cut);
        }
        let mut tail = [0u8; ATTEST_REP_MAX + 1];
        tail[..n].copy_from_slice(&buf[..n]);
        tail[n] = 1;
        assert_eq!(parse_attest_reply(&tail[..n + 1]), None);
        let mut bad = buf;
        bad[1] = 9;
        assert_eq!(parse_attest_reply(&bad[..n]), None);
        assert_eq!(parse_attest_reply(&[ST_OK]), None);
        // 非 OK 1B 形（encode_status_reply 共用）。
        let r = parse_attest_reply(&[ST_NOENT]).unwrap();
        assert_eq!(r.st, ST_NOENT);
        assert_eq!(r.token, &[]);
        // 超帽 token 编码拒。
        let over = [0u8; ATTEST_TOKEN_MAX + 1];
        assert_eq!(encode_attest_reply(&mut buf, SecurityLevel::Software, 0, &over), None);
    }

    // —— COUNTER ———————————————————————————————————————————————————

    #[test]
    fn counter_request_shapes_and_golden() {
        // 三形恰长：READ/INCREMENT 6B、CAS 22B。
        let mut buf = [0u8; COUNTER_REQ_MAX];
        let n = encode_counter_request(&mut buf, &CounterReq::Read { slot: SLOT_L0_BOOT }).unwrap();
        assert_eq!(n, COUNTER_REQ_SHORT_LEN);
        assert_eq!(&buf[..n], &[OP_COUNTER, SUB_COUNTER_READ, 0, 0, 0, 0][..]);
        let r = parse_counter_request(&buf[..n]).unwrap();
        assert_eq!(r, CounterReq::Read { slot: 0 });
        let n = encode_counter_request(&mut buf, &CounterReq::Increment { slot: 5 }).unwrap();
        assert_eq!(&buf[..n], &[OP_COUNTER, SUB_COUNTER_INCREMENT, 5, 0, 0, 0][..]);
        assert_eq!(parse_counter_request(&buf[..n]), Some(CounterReq::Increment { slot: 5 }));
        // CAS 黄金字节：slot=5 expected=7 new=8 全 LE。
        let n = encode_counter_request(
            &mut buf,
            &CounterReq::CompareAndSet {
                slot: 5,
                expected: 7,
                new: 8,
            },
        )
        .unwrap();
        assert_eq!(n, COUNTER_REQ_CAS_LEN);
        assert_eq!(
            &buf[..n],
            &[
                OP_COUNTER, SUB_COUNTER_CAS, 5, 0, 0, 0, //
                7, 0, 0, 0, 0, 0, 0, 0, //
                8, 0, 0, 0, 0, 0, 0, 0,
            ][..]
        );
        assert_eq!(
            parse_counter_request(&buf[..n]),
            Some(CounterReq::CompareAndSet { slot: 5, expected: 7, new: 8 })
        );
        // slot 定宽域边界：32 恰收 / 33 拒（编码 + 解析双侧）。
        let n = encode_counter_request(&mut buf, &CounterReq::Read { slot: COUNTER_SLOTS_MAX })
            .unwrap();
        assert_eq!(parse_counter_request(&buf[..n]), Some(CounterReq::Read { slot: 32 }));
        assert_eq!(
            encode_counter_request(&mut buf, &CounterReq::Read { slot: COUNTER_SLOTS_MAX + 1 }),
            None
        );
        let m = [OP_COUNTER, SUB_COUNTER_READ, 33, 0, 0, 0];
        assert_eq!(parse_counter_request(&m), None);
        // u32::MAX slot 拒。
        let mm = [OP_COUNTER, SUB_COUNTER_READ, 0xFF, 0xFF, 0xFF, 0xFF];
        assert_eq!(parse_counter_request(&mm), None);
        // 未知 sub（0/4）拒。
        for sub in [0u8, 4, 255] {
            let m = [OP_COUNTER, sub, 0, 0, 0, 0];
            assert_eq!(parse_counter_request(&m), None, "sub {} 应拒", sub);
        }
        // op 字节错值拒。
        let mo = [OP_KEY_GENERATE, SUB_COUNTER_READ, 0, 0, 0, 0];
        assert_eq!(parse_counter_request(&mo), None);
        // 形状执法：READ 7B（尾随）拒 / CAS 截断（21B）拒 / CAS 每切点拒。
        let m7 = [OP_COUNTER, SUB_COUNTER_READ, 0, 0, 0, 0, 0];
        assert_eq!(parse_counter_request(&m7), None);
        for cut in 0..n {
            assert_eq!(parse_counter_request(&buf[..cut]), None, "CAS 截断 @{} 应拒", cut);
        }
        // buf 不足拒。
        let mut tiny = [0u8; 5];
        assert_eq!(encode_counter_request(&mut tiny, &CounterReq::Read { slot: 0 }), None);
    }

    #[test]
    fn counter_reply_roundtrip_baked_line_and_golden() {
        let mut buf = [0u8; COUNTER_REP_LEN];
        // OK 全值域往返 + LE 黄金。
        for v in [0u64, 1, 0x0102_0304_0506_0708, u64::MAX] {
            let n = encode_counter_reply(&mut buf, ST_OK, v).unwrap();
            assert_eq!(n, COUNTER_REP_LEN);
            assert_eq!(parse_counter_reply(&buf[..n]), Some(CounterReply { st: ST_OK, value: v }));
        }
        assert_eq!(
            &buf[..],
            &[ST_OK, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF][..]
        );
        // 非 OK 全状态域：恒全零 value（烘焙线双侧）。
        for st in [ST_BAD, ST_NOENT, ST_ROLLBACK, ST_CAS_FAIL] {
            let n = encode_counter_reply(&mut buf, st, 0).unwrap();
            assert_eq!(parse_counter_reply(&buf[..n]), Some(CounterReply { st, value: 0 }));
            assert_eq!(encode_counter_reply(&mut buf, st, 1), None, "非 OK 携值编码应拒");
            let m = [st, 1, 0, 0, 0, 0, 0, 0, 0];
            assert_eq!(parse_counter_reply(&m), None, "非 OK 携值解析应拒");
        }
        // 未知 st 拒（0 域外已含 OK 在内另测；5/255 域外）。
        for st in [5u8, 255] {
            assert_eq!(encode_counter_reply(&mut buf, st, 0), None);
            let m = [st, 0, 0, 0, 0, 0, 0, 0, 0];
            assert_eq!(parse_counter_reply(&m), None);
        }
        // 恰长执法：8B/10B 拒。
        let m8 = [0u8; 8];
        assert_eq!(parse_counter_reply(&m8), None);
        let m10 = [0u8; 10];
        assert_eq!(parse_counter_reply(&m10), None);
        // buf 不足拒。
        let mut tiny = [0u8; 8];
        assert_eq!(encode_counter_reply(&mut tiny, ST_OK, 0), None);
    }

    // —— 会话机（畸形会话序拒面） ———————————————————————————————————

    #[test]
    fn session_serial_discipline() {
        let mut s = Session::new();
        assert!(!s.awaiting_reply());
        // 合法序：请求 → 应答 × 2 轮。
        assert_eq!(s.on_request(OP_KEY_GENERATE), Some(()));
        assert!(s.awaiting_reply());
        assert_eq!(s.on_reply(), Some(()));
        assert!(!s.awaiting_reply());
        assert_eq!(s.on_request(OP_COUNTER), Some(()));
        assert_eq!(s.on_reply(), Some(()));
        // 畸形序①：在途未收口再收请求 → None，且相位不变（在途不丢）。
        assert_eq!(s.on_request(OP_KEY_ATTEST), Some(()));
        assert_eq!(s.on_request(OP_KEY_GENERATE), None, "在途再请求应拒");
        assert!(s.awaiting_reply(), "拒收不改相位");
        assert_eq!(s.on_reply(), Some(())); // 原在途请求仍可收口
        // 畸形序②：无在途请求收应答 → None。
        assert_eq!(s.on_reply(), None, "无在途收应答应拒");
        assert!(!s.awaiting_reply());
        // 野 op：三 op 之外拒（0/4/255）。
        for op in [0u8, 4, 255] {
            assert_eq!(s.on_request(op), None, "野 op {} 应拒", op);
        }
        assert!(!s.awaiting_reply(), "野 op 不落相位");
        // 三 op 全合法入相位。
        for op in [OP_KEY_GENERATE, OP_KEY_ATTEST, OP_COUNTER] {
            assert_eq!(s.on_request(op), Some(()));
            assert_eq!(s.on_reply(), Some(()));
        }
    }

    // —— CounterBackend 参考实现语义（单调/回卷拒） ———————————————————

    #[test]
    fn counter_backend_inmemory_semantics() {
        let mut c = InMemoryCounters::new();
        // 未分配/域外 → NoSuchSlot。
        assert_eq!(c.read(SLOT_L0_BOOT), Err(CounterError::NoSuchSlot));
        assert_eq!(c.read(33), Err(CounterError::NoSuchSlot));
        assert_eq!(c.read(u32::MAX), Err(CounterError::NoSuchSlot));
        // init：首次 true / 二次 false / 域外 false。
        assert!(c.init(SLOT_L0_BOOT, 41));
        assert!(!c.init(SLOT_L0_BOOT, 0));
        assert!(!c.init(33, 0));
        assert_eq!(c.read(SLOT_L0_BOOT), Ok(41));
        // 单调 +1。
        assert_eq!(c.increment(SLOT_L0_BOOT), Ok(42));
        assert_eq!(c.read(SLOT_L0_BOOT), Ok(42));
        // CAS 前进。
        assert_eq!(
            c.compare_and_set(SLOT_L0_BOOT, 42, 100),
            Ok(100)
        );
        // 回卷拒：expected 恰匹配仍拒（安全执法优先于并发提示）。
        assert_eq!(
            c.compare_and_set(SLOT_L0_BOOT, 100, 99),
            Err(CasError::Rollback { current: 100 })
        );
        // 回卷拒支配 Mismatch：expected 错 + new 回卷 → 仍 Rollback。
        assert_eq!(
            c.compare_and_set(SLOT_L0_BOOT, 0, 50),
            Err(CasError::Rollback { current: 100 })
        );
        // expected 不匹配（new 前进）→ Mismatch。
        assert_eq!(
            c.compare_and_set(SLOT_L0_BOOT, 0, 101),
            Err(CasError::Mismatch { current: 100 })
        );
        // 幂等重写（new == 当前）合法。
        assert_eq!(c.compare_and_set(SLOT_L0_BOOT, 100, 100), Ok(100));
        // 域外 CAS。
        assert_eq!(
            c.compare_and_set(99, 0, 1),
            Err(CasError::NoSuchSlot)
        );
        // 顶格终态：init u64::MAX 后 increment → Saturated（回卷拒同族）。
        assert!(c.init(1, u64::MAX));
        assert_eq!(c.increment(1), Err(CounterError::Saturated));
        assert_eq!(c.read(1), Ok(u64::MAX)); // 顶格值不丢
        assert_eq!(
            c.compare_and_set(1, u64::MAX, u64::MAX),
            Ok(u64::MAX) // 幂等重写在顶格仍合法
        );
    }

    /// counter 三方法 ↔ wire 三 sub 的对拍（trait 语义 ↔ RPC 形一一对应）。
    #[test]
    fn counter_wire_maps_backend_ops() {
        let mut c = InMemoryCounters::new();
        assert!(c.init(7, 10));
        // READ。
        let mut buf = [0u8; COUNTER_REQ_MAX];
        let n = encode_counter_request(&mut buf, &CounterReq::Read { slot: 7 }).unwrap();
        match parse_counter_request(&buf[..n]).unwrap() {
            CounterReq::Read { slot } => {
                let v = c.read(slot).unwrap();
                let mut rb = [0u8; COUNTER_REP_LEN];
                let rn = encode_counter_reply(&mut rb, ST_OK, v).unwrap();
                assert_eq!(parse_counter_reply(&rb[..rn]).unwrap().value, 10);
            }
            _ => panic!("READ 形错配"),
        }
        // INCREMENT。
        let n = encode_counter_request(&mut buf, &CounterReq::Increment { slot: 7 }).unwrap();
        match parse_counter_request(&buf[..n]).unwrap() {
            CounterReq::Increment { slot } => {
                let v = c.increment(slot).unwrap();
                assert_eq!(v, 11);
            }
            _ => panic!("INCREMENT 形错配"),
        }
        // CAS 回卷 → wire ST_ROLLBACK 恒全零 value（烘焙线）。
        let n = encode_counter_request(
            &mut buf,
            &CounterReq::CompareAndSet { slot: 7, expected: 11, new: 3 },
        )
        .unwrap();
        match parse_counter_request(&buf[..n]).unwrap() {
            CounterReq::CompareAndSet { slot, expected, new } => {
                let st = match c.compare_and_set(slot, expected, new) {
                    Ok(v) => {
                        assert_eq!(v, new);
                        ST_OK
                    }
                    Err(CasError::Rollback { .. }) => ST_ROLLBACK,
                    Err(CasError::Mismatch { .. }) => ST_CAS_FAIL,
                    Err(CasError::NoSuchSlot) => ST_NOENT,
                };
                let mut rb = [0u8; COUNTER_REP_LEN];
                let rn = encode_counter_reply(&mut rb, st, 0).unwrap();
                let got = parse_counter_reply(&rb[..rn]).unwrap();
                assert_eq!(got.st, ST_ROLLBACK);
                assert_eq!(got.value, 0); // 烘焙线：非 OK 恒全零
            }
            _ => panic!("CAS 形错配"),
        }
    }

    /// 帧帽预算钉板（编译期断言的运行时对读面——预算数学防漂移）。
    #[test]
    fn frame_budget_consts() {
        assert_eq!(CHAN_MSG_MAX, 256);
        assert_eq!(AUTH_TAGS_MAX, 8);
        assert_eq!(KEYBLOB_MAX, 160);
        assert_eq!(CHALLENGE_MAX, 64);
        assert_eq!(ATTEST_TOKEN_MAX, 160);
        assert_eq!(GEN_REQ_MAX, 179);
        assert_eq!(GEN_REP_MAX, 165);
        assert_eq!(ATTEST_REQ_MAX, 229);
        assert_eq!(ATTEST_REP_MAX, 165);
        assert_eq!(COUNTER_REQ_SHORT_LEN, 6);
        assert_eq!(COUNTER_REQ_CAS_LEN, 22);
        assert_eq!(COUNTER_REQ_MAX, 22);
        assert_eq!(COUNTER_REP_LEN, 9);
        for cap in [
            GEN_REQ_MAX,
            GEN_REP_MAX,
            ATTEST_REQ_MAX,
            ATTEST_REP_MAX,
            COUNTER_REQ_MAX,
            COUNTER_REP_LEN,
        ] {
            assert!(cap <= CHAN_MSG_MAX, "帧帽 {} 超 inline", cap);
        }
        // 状态/op/sub 钉板。
        assert_eq!(
            (OP_KEY_GENERATE, OP_KEY_ATTEST, OP_COUNTER),
            (1, 2, 3)
        );
        assert_eq!(
            (SUB_COUNTER_READ, SUB_COUNTER_INCREMENT, SUB_COUNTER_CAS),
            (1, 2, 3)
        );
        assert_eq!((ST_OK, ST_BAD, ST_NOENT, ST_ROLLBACK, ST_CAS_FAIL), (0, 1, 2, 3, 4));
        assert_eq!(COUNTER_SLOTS_MAX, 32);
        assert_eq!(SLOT_L0_BOOT, 0);
    }
}
