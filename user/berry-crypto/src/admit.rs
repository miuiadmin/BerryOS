//! P3-0 件一：装载 admit 决策模块（`docs/设备侧验签与账本落盘设计-v1.0.md`
//! §二 2.1 形态裁决①②——决策函数进 lib、tools 只编排）。
//!
//! - **单解释器纪律**：账本 TOML（[`crate::ledger`]）/信封 TLV
//!   （[`crate::envelope`]）/策略档（本模块 [`parse_policy`]）的语义判断
//!   全在 Rust 单源，不落 Python 第二份；装载决策这种**组合语义**不可
//!   镜像（对照反面：gensourceindex `check_index_version` 单函数可镜像）。
//! - **设备侧可逐字消费**：berry-sign 验签组合逻辑上移本模块
//!   （[`verify_signed`]——lib.rs :34-36 既定义务的兑现；berry-sign bin
//!   verify 单源委托本函数）。
//! - **manifest 语义不进 lib**：[`LoadFace`] 只收摘要面（bap 全字节摘要 +
//!   manifest 关键域摘要 + lineage 回声字节）——关键域摘要输入由
//!   berry-manifest 解析产物单源派生（24 号 §3.3-1 方案 A，派生点 =
//!   berry-sign bin；本 crate lib 面零 manifest 语义判断）。
//!
//! 决策规格（设计 §1.3.2 策略表 + §1.3.3 验签序 + §1.4 账本）：
//!
//! | 档 | sideload | require_sig | 无 .sig | 未知 lineage |
//! |---|---|---|---|---|
//! | `dev` | on | off | 降级装载 + loud 行 | TOFU 首见记录后装载 |
//! | `product-open` | off | off | 降级装载 + loud 行 | 拒载 `unknown-src` |
//! | `product-locked` | off | on | 拒载 `nosig` | 拒载（同左） |
//!
//! **恒执法（不随档位）**：①防剥离——per-app 记录在案的已签 id 再见
//! 无 .sig = 拒载（全档同拒，剥离是攻击不是兼容态）；②水位——gen 回降
//! 拒（[`crate::ledger::check_gen`] 单源）+ ver 回降拒
//! （[`crate::ledger::check_ver`]）；③lineage↔pk 绑定（G9，2026-11-12）
//! ——在册 lineage 的零链信封 pk ∉ 该 lineage 锚集 = 同 lineage 换钥未
//! 走轮换链仪式，拒（`key-swap`，全档同拒——「换钥走仪式不走 TOFU」，
//! SDK-2 §二 :36-37 + §1.4「首见即锚，此后同 lineage 换钥走轮换链机器
//! 裁决」；带链信封 = 仪式面，锚走链机器裁决不受影响）。**账本不可用
//! （增量缺席）期间**：验签照做，水位跳检 + 「回放防护未生效」诚实声明
//! （`replay_guard = false`，§1.3.2/§1.4 RAM-only 降级——回放窗口重启
//! 后重开，诚实降级接受）。

use crate::ed25519;
use crate::envelope::{
    rot_message, top_message, SigEnvelopeV1, SigErr, DIGEST_LEN, ENVELOPE_MAX, LINEAGE_MAX,
    ROT_MSG_MAX, SIG_LEN, TOP_MSG_MAX,
};
use crate::ledger::{
    bump_gen, bump_ver, check_gen, check_ver, Anchor, AnchorType, AppRecord, AppSource,
    LedgerErr, LineageEntry, TrustLedger, ANCHORS_MAX, APP_ID_MAX, APPS_MAX, LINEAGES_MAX,
};

// ---- 策略档（§1.3.2 三档——常量单源，CLI/p3sim 只持词表镜像）----

/// 装载策略三档（`etc/load-policy.toml` 暂名的档值——Q4 落位随 P3 刀裁）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Dev,
    ProductOpen,
    ProductLocked,
}

impl Policy {
    /// 档词表（add-only；未知档保守拒）。
    pub const WORDS: &'static [&'static str] = &["dev", "product-open", "product-locked"];

    /// 档词 → 档（未知 None——保守拒由 [`parse_policy`] 落）。
    pub fn parse_word(word: &[u8]) -> Option<Policy> {
        match word {
            b"dev" => Some(Policy::Dev),
            b"product-open" => Some(Policy::ProductOpen),
            b"product-locked" => Some(Policy::ProductLocked),
            _ => None,
        }
    }

    /// 档词（canonical 单形）。
    pub fn word(&self) -> &'static str {
        match self {
            Policy::Dev => "dev",
            Policy::ProductOpen => "product-open",
            Policy::ProductLocked => "product-locked",
        }
    }

    /// sideload 开关（§1.3.2 表：dev on，产品两档 off）。
    pub const fn sideload(&self) -> bool {
        matches!(self, Policy::Dev)
    }

    /// require_sig 开关（§1.3.2 表：仅 product-locked on——「关 = 拒装 +
    /// 显式错误」SDK-2 §九 :133）。
    pub const fn require_sig(&self) -> bool {
        matches!(self, Policy::ProductLocked)
    }

    /// TOFU 准入（§1.3.2 表：仅 dev——首见即锚）。
    pub const fn tofu(&self) -> bool {
        matches!(self, Policy::Dev)
    }
}

/// 策略件源帽（**推断**——三行量级 + 注释余量）。
pub const POLICY_SRC_MAX: usize = 256;
/// 策略件 schema 版本（add-only；≠1 硬拒）。
pub const POLICY_SCHEMA_V1: u32 = 1;

/// 策略件解析拒因（结构化，add-only）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyFileErr {
    /// 源文本超 [`POLICY_SRC_MAX`] 帽。
    TooLarge,
    /// 行形坏（CR/tab/无 `=`/空键/引号错）。
    BadLine,
    /// 未知键（仅 schema/tier 闭集）。
    UnknownKey,
    /// schema 缺/≠1/重复。
    BadSchema,
    /// tier 缺（必填）。
    MissingTier,
    /// tier 词未知（保守拒）。
    UnknownTier,
}

/// 策略件解析（`schema = 1` + `tier = "<档>"` 两键闭集；`#` 注释容许——
/// ledger.rs 行形同款；语义判断单源在本，Python 不持第二份）。
pub fn parse_policy(src: &[u8]) -> Result<Policy, PolicyFileErr> {
    use PolicyFileErr::*;
    if src.len() > POLICY_SRC_MAX {
        return Err(TooLarge);
    }
    if src.contains(&b'\r') || src.contains(&b'\t') {
        return Err(BadLine);
    }
    let mut saw_schema = false;
    let mut tier: Option<Policy> = None;
    for raw in src.split(|&b| b == b'\n') {
        // 注释剥离 + 两端去空格（ledger.rs 同形，局部小件不复用私有 fn）。
        let mut line = raw;
        let mut in_str = false;
        let mut cut = line.len();
        for (i, &b) in line.iter().enumerate() {
            match b {
                b'"' => in_str = !in_str,
                b'#' if !in_str => {
                    cut = i;
                    break;
                }
                _ => {}
            }
        }
        line = &line[..cut];
        let t = trim_spaces(line);
        if t.is_empty() {
            continue;
        }
        let eq = t.iter().position(|&b| b == b'=').ok_or(BadLine)?;
        let key = trim_spaces(&t[..eq]);
        let val = trim_spaces(&t[eq + 1..]);
        if key.is_empty() || val.is_empty() || key.contains(&b' ') {
            return Err(BadLine);
        }
        match key {
            b"schema" => {
                if saw_schema || val != b"1" {
                    return Err(BadSchema);
                }
                saw_schema = true;
            }
            b"tier" => {
                if val.len() < 2 || val[0] != b'"' || val[val.len() - 1] != b'"' {
                    return Err(BadLine);
                }
                let word = &val[1..val.len() - 1];
                if word.contains(&b'"') || word.contains(&b'\\') {
                    return Err(BadLine);
                }
                tier = Some(Policy::parse_word(word).ok_or(UnknownTier)?);
            }
            _ => return Err(UnknownKey),
        }
    }
    if !saw_schema {
        return Err(BadSchema);
    }
    tier.ok_or(MissingTier)
}

fn trim_spaces(s: &[u8]) -> &[u8] {
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

// ---- 装载面输入（摘要面——manifest 语义单源 = berry-manifest，bin 派生）----

/// 单件装载面（纯数据，零分配借用形态）。
///
/// 契约：`id` ≤ [`APP_ID_MAX`]、`lineage` ≤ [`LINEAGE_MAX`]（值域由
/// berry-manifest 上游执法；越域输入查不中任何在册记录/条目 = fail-safe
/// 拒，不 panic）。
#[derive(Debug, Clone, Copy)]
pub struct LoadFace<'a> {
    /// app id（manifest `[app].id`——防剥离对账键）。
    pub id: &'a [u8],
    /// manifest `[security].key_lineage`（信封回声对账源；无 [security]
    /// 的 .bap 由调用方传空切片——空 ≠ 任何合法信封 lineage，恒回声拒）。
    pub lineage: &'a [u8],
    /// 版本水位可比较值（三值 `x.y.z` 各段 0..=999 → u32；freshness 面）。
    pub ver: u32,
    /// .bap 全字节 SHA-512/256 摘要。
    pub bap_digest: [u8; DIGEST_LEN],
    /// manifest 关键域摘要（berry-manifest 解析产物单源派生——调用方）。
    pub manifest_digest: [u8; DIGEST_LEN],
    /// 伴生 `.sig` 信封字节（None = 无伴生件——防剥离/档位函数消费）。
    pub sig: Option<&'a [u8]>,
    /// 装载来源（boot 批 builtin | SPAWN sideload——§1.3.1 三路径）。
    pub source: AppSource,
}

/// 装载裁决上下文（不变信任态）。
#[derive(Debug, Clone, Copy)]
pub struct AdmitCtx {
    /// 策略档。
    pub policy: Policy,
    /// 回放防护在位（false = 增量账本缺席——水位跳检 + 「回放防护未
    /// 生效」诚实声明面由 CLI 渲染；§1.3.2/§1.4）。
    pub replay_guard: bool,
}

// ---- 决策输出 ----

/// 允许注记（loud/静默的语义面——eco 行渲染在 CLI，lib 只出结构）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Note {
    /// 验签过装载（重叠窗/锚定面——水位代际随件）。
    Verified { gen: u32 },
    /// 降级装载 + loud 行（无 .sig，dev/product-open 档——失败隔离家族，
    /// `eco: app <id> unsigned`）。
    DegradeUnsigned,
    /// TOFU 首见记录后装载（dev 档——首见即锚，§1.4）。
    TofuFirstSeen { gen: u32 },
}

/// 策略族拒因（结构化，add-only；eco 词见 [`PolicyErr::word`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyErr {
    /// 无 .sig 且 require_sig on（product-locked——`nosig`）。
    NoSig,
    /// 未知 lineage 且档位无 TOFU 准入（product-open/locked——
    /// `unknown-src`）。
    UnknownSrc,
    /// sideload 关（产品档——`sideload`，SDK-2 §九 :133 字面）。
    SideloadOff,
    /// 防剥离：per-app 记录在案的已签 id 再见无 .sig（全档恒拒——
    /// `stripped`）。
    Stripped,
    /// 同 lineage 换钥未走仪式：在册 lineage 的零链信封 pk ∉ 该 lineage
    /// 锚集（全档恒拒——`key-swap`，G9 2026-11-12；「换钥走仪式不走
    /// TOFU」，仪式 = 信封轮换链〔BSIG-ROT 锚走链，SDK-2 §二 :36-37〕，
    /// 拒因行带 lineage + 两 pk 前缀由 CLI 渲染——结构单源在本）。
    KeySwap,
}

impl PolicyErr {
    /// eco 拒因词（kebab 单形——CLI 决策行消费；add-only 同步）。
    pub fn word(&self) -> &'static str {
        match self {
            PolicyErr::NoSig => "nosig",
            PolicyErr::UnknownSrc => "unknown-src",
            PolicyErr::SideloadOff => "sideload",
            PolicyErr::Stripped => "stripped",
            PolicyErr::KeySwap => "key-swap",
        }
    }
}

/// 拒因三族（设计 §二：`Reject{SigErr/LedgerErr/PolicyErr}` 字面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// 信封结构/验签序族（parse/回声/锚/顶签名/链）。
    Sig(SigErr),
    /// 水位族（gen/ver 回降、帽溢出）。
    Ledger(LedgerErr),
    /// 策略族（档位表 + 恒执法护栏）。
    Policy(PolicyErr),
}

impl Reason {
    /// eco 拒因词（三族分派——词表单源在各枚举 `word()`）。
    pub fn word(&self) -> &'static str {
        match self {
            Reason::Sig(e) => e.word(),
            Reason::Ledger(e) => e.word(),
            Reason::Policy(e) => e.word(),
        }
    }
}

/// 装载裁决（设计 §二：`Verdict ∈ {Allow{注记}, Reject{…}, Mutate{…}}`
/// 字面三态；`Mutate ⊃ Allow`——装载且账本有更新时出 Mutate，records'
/// 随 ledger' 同库携带（[[app]] 同库不同键，§1.4））。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// 允许装载（注记面）——无账本突变（降级 loud / 原地重放）。
    Allow(Note),
    /// 允许装载 + 账本突变（TOFU 首见 / 水位推进 / 锚学习 / per-app
    /// 记录 upsert）——单写者折叠消费（CLI 逐件 fold，设备面 = appd）。
    Mutate { note: Note, ledger: TrustLedger },
    /// 拒载 + 显式错误（拒载不 trap——失败隔离，单 app 坏不连坐）。
    Reject(Reason),
}

// ---- 验签序五步（§1.3.3——berry-sign verify_envelope 逐字上移单源）----

/// 验签产物（装载面字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SigOk {
    /// lineage 字节（= manifest key_lineage——回声已对账）。
    pub lineage: [u8; LINEAGE_MAX],
    /// lineage 在用长。
    pub lineage_len: usize,
    /// 单调代际。
    pub gen: u32,
    /// 轮换链长（0 = 零链）。
    pub chain_len: usize,
    /// 当前公钥（top_sig 验签钥；链非空 = 链尾续接钥）。
    pub pk: [u8; 32],
}

impl SigOk {
    /// lineage 切片（在用段）。
    pub fn lineage(&self) -> &[u8] {
        &self.lineage[..self.lineage_len]
    }
}

/// 验签序五步（单一解释单源——berry-sign bin verify 委托消费）：
///
/// 1. 信封 parse（[`SigEnvelopeV1::parse`] 单解释——结构/值域/链尾续接）；
/// 2. lineage 回声对账（信封 lineage == manifest key_lineage——跨 lineage
///    拒面 [`SigErr::LineageEcho`]）；
/// 3. 锚走链（**重叠双签窗：任一锚起链全程过即过**——零链 = 锚 pk 须 ==
///    信封 pk〔`KeyMismatch`〕；带链 = node 0 old_pk = 锚、链序推进逐节点
///    BSIG-ROT 验〔`RotSigBad`〕，SDK-2 §三 :61-62）；
/// 4. 顶签名验（当前钥对 BSIG-TOP——[`SigErr::TopSigBad`]）。
///
/// （第 5 步链验并入第 3 步锚走链——链从锚起验，无独立相位。）
pub fn verify_signed(
    anchors: &[Anchor],
    lineage_echo: &[u8],
    bap_digest: &[u8; DIGEST_LEN],
    manifest_digest: &[u8; DIGEST_LEN],
    sig: &[u8],
) -> Result<SigOk, Reason> {
    let env = SigEnvelopeV1::parse(sig).map_err(Reason::Sig)?;
    echo_check(&env, lineage_echo)?;
    // 锚走链：链尾续接（parse 结构执法 last.new_pk == pk）保证带链走完
    // old == env.pk；零链要求锚 pk == env.pk。
    let mut witnessed = false;
    for a in anchors {
        let mut old = a.pk;
        let mut ok = true;
        for node in env.chain() {
            let mut rm = [0u8; ROT_MSG_MAX];
            let rn = match rot_message(&old, &node.new_pk, env.gen, env.lineage(), &mut rm) {
                Ok(n) => n,
                Err(e) => return Err(Reason::Sig(e)), // BadPkLen 理论不可达（定长）
            };
            if !ed25519::verify(&old, &node.rot_sig, &rm[..rn]) {
                ok = false;
                break;
            }
            old = node.new_pk;
        }
        if ok && old == env.pk {
            witnessed = true;
            break;
        }
    }
    if !witnessed {
        return Err(Reason::Sig(if env.chain_len == 0 {
            SigErr::KeyMismatch
        } else {
            SigErr::RotSigBad
        }));
    }
    top_verify(&env, bap_digest, manifest_digest)?;
    Ok(SigOk {
        lineage: env.lineage,
        lineage_len: env.lineage_len,
        gen: env.gen,
        chain_len: env.chain_len,
        pk: env.pk,
    })
}

/// 回声对账（验签序第 2 步——TOFU 路与锚定路共用单源）。
fn echo_check(env: &SigEnvelopeV1, lineage_echo: &[u8]) -> Result<(), Reason> {
    if env.lineage() != lineage_echo {
        return Err(Reason::Sig(SigErr::LineageEcho));
    }
    Ok(())
}

/// 顶签名验（验签序第 4 步——TOFU 路与锚定路共用单源）。
fn top_verify(
    env: &SigEnvelopeV1,
    bap_digest: &[u8; DIGEST_LEN],
    manifest_digest: &[u8; DIGEST_LEN],
) -> Result<(), Reason> {
    // 头重组：parse∘encode canonical 形封闭（envelope.rs encode 发射前
    // 复检不变量）——重组头与签发头逐字节同。
    let mut buf = [0u8; ENVELOPE_MAX];
    let n = env.encode(&mut buf).map_err(Reason::Sig)?;
    let mut msg = [0u8; TOP_MSG_MAX];
    let mn = top_message(&buf[..n - SIG_LEN], bap_digest, manifest_digest, &mut msg)
        .map_err(Reason::Sig)?;
    if !ed25519::verify(&env.pk, &env.top_sig, &msg[..mn]) {
        return Err(Reason::Sig(SigErr::TopSigBad));
    }
    Ok(())
}

// ---- admit 决策（纯函数——§1.3.2 档位表 + 恒执法护栏 + §1.4 TOFU）----

/// 装载裁决主函数。输入 = 装载面（摘要面 + sig 伴生）+ 策略档 + 信任态
/// （账本：lineage 锚集/水位 + per-app 记录）→ 输出 = [`Verdict`]。
///
/// 次序（护栏先于档位让步）：sideload 门 → 防剥离（恒）→ 无 .sig 档位
/// 函数 → 验签序（在册分支带 lineage↔pk 绑定执法：零链异 pk = 换钥未走
/// 仪式拒，恒——G9）→ 未知 lineage 档位函数（TOFU/拒）→ 水位（恒，
/// `replay_guard` 关 = 跳检诚实降级）→ 突变折叠（水位推进/锚学习/
/// per-app upsert）。
pub fn admit(ctx: &AdmitCtx, ledger: &TrustLedger, face: &LoadFace<'_>) -> Verdict {
    // ① sideload 门（§1.3.2 表 sideload 列；§九 :133「关 = 拒装 + 显式错误」）。
    if face.source == AppSource::Sideload && !ctx.policy.sideload() {
        return Verdict::Reject(Reason::Policy(PolicyErr::SideloadOff));
    }
    let Some(sig) = face.sig else {
        // ② 防剥离护栏（恒执法全档——SDK-2 §三 :63 ①；剥离是攻击不是
        //    兼容态）。记录在案 = 已签 id 在案（记录仅由验签过装载创建）。
        if ledger.find_app(face.id).is_some() {
            return Verdict::Reject(Reason::Policy(PolicyErr::Stripped));
        }
        // ③ 无 .sig 处理 = 档位函数（§1.3.2 表——dev/product-open 降级
        //    + loud，product-locked 恒拒载 SDK-2 §九 :133 字面）。
        return if ctx.policy.require_sig() {
            Verdict::Reject(Reason::Policy(PolicyErr::NoSig))
        } else {
            Verdict::Allow(Note::DegradeUnsigned)
        };
    };
    // ④ lineage 在册分支 vs TOFU 分支（§1.3.3 锚来源分档）。
    match ledger.find_lineage(face.lineage) {
        Some(entry) => {
            let ok = match verify_signed(entry.anchors(), face.lineage, &face.bap_digest, &face.manifest_digest, sig)
            {
                Ok(o) => o,
                // ④b G9 lineage↔pk 绑定执法（恒执法 ③，2026-11-12）：
                // `KeyMismatch` 仅产自「零链 ∧ 信封 pk ∉ 任一锚」
                // （verify_signed 单源分派——带链不见证走 RotSigBad）＝
                // 同 lineage 换钥未走轮换链仪式 → 策略族拒（全档）。
                // **推断落位**：任务书「验签过后查 pk ∈ 锚」按零链面收口
                // ——带链到达钥是仪式本体（SDK-2 §二 :36「同 lineage 验链
                // 自动通过」+ §三 :61-62 重叠双签窗 + ⑥ 锚学习），逐字
                // 「pk ∈ 锚才过」会连仪式一起拒，与母法矛盾故不取。
                Err(Reason::Sig(SigErr::KeyMismatch)) => {
                    return Verdict::Reject(Reason::Policy(PolicyErr::KeySwap));
                }
                Err(r) => return Verdict::Reject(r),
            };
            // ⑤ 水位恒执法（§三 :63 ②；增量缺席 = 跳检 + 诚实声明面）。
            if ctx.replay_guard {
                if let Err(e) = check_gen(ok.gen, entry.gen_max) {
                    return Verdict::Reject(Reason::Ledger(e));
                }
                if let Some(rec) = ledger.find_app(face.id) {
                    if let Err(e) = check_ver(face.ver, rec.ver_max) {
                        return Verdict::Reject(Reason::Ledger(e));
                    }
                }
            }
            // ⑥ 突变：水位推进（不降）+ 锚学习（链到达钥入锚集——重叠双签
            //    窗的设备侧形成面；帽满不追加，水位仍推进）+ per-app upsert。
            let idx = ledger.lineages.iter().position(|e| e.id() == face.lineage);
            let Some(idx) = idx else { unreachable!("find_lineage Some 分支") };
            if ledger.find_app(face.id).is_none() && ledger.apps_len >= APPS_MAX {
                return Verdict::Reject(Reason::Ledger(LedgerErr::TooManyApps));
            }
            let mut nl = *ledger;
            {
                let ent = &mut nl.lineages[idx];
                ent.gen_max = bump_gen(ent.gen_max, ok.gen);
                if ent.anchors_len < ANCHORS_MAX && ent.anchors().iter().all(|a| a.pk != ok.pk) {
                    ent.anchors[ent.anchors_len] = Anchor { pk: ok.pk, ty: AnchorType::Tofu };
                    ent.anchors_len += 1;
                }
            }
            upsert_app(&mut nl, face, ok.pk);
            Verdict::Mutate { note: Note::Verified { gen: ok.gen }, ledger: nl }
        }
        None => {
            // 未知 lineage：TOFU 仅 dev（首见即锚——§1.4；product 两档
            // unknown-src 拒）。
            if !ctx.policy.tofu() {
                return Verdict::Reject(Reason::Policy(PolicyErr::UnknownSrc));
            }
            let env = match SigEnvelopeV1::parse(sig) {
                Ok(e) => e,
                Err(e) => return Verdict::Reject(Reason::Sig(e)),
            };
            if echo_check(&env, face.lineage).is_err() {
                return Verdict::Reject(Reason::Sig(SigErr::LineageEcho));
            }
            // 首见验签 = parse + 回声 + 顶签名（锚 = 当前钥 env.pk；首见
            // 链 = 死历史——信任不取自链，此后换钥走链从锚起机器裁决，
            // §1.4「首见即锚」）。
            if top_verify(&env, &face.bap_digest, &face.manifest_digest).is_err() {
                return Verdict::Reject(Reason::Sig(SigErr::TopSigBad));
            }
            // 帽前检（纯函数不产半突变态）。
            if ledger.lineages_len >= LINEAGES_MAX {
                return Verdict::Reject(Reason::Ledger(LedgerErr::TooManyLineages));
            }
            if ledger.find_app(face.id).is_none() && ledger.apps_len >= APPS_MAX {
                return Verdict::Reject(Reason::Ledger(LedgerErr::TooManyApps));
            }
            let mut nl = *ledger;
            {
                let mut ent = LineageEntry {
                    id: [0; crate::ledger::LINEAGE_ID_MAX],
                    id_len: face.lineage.len().min(crate::ledger::LINEAGE_ID_MAX),
                    anchors: [Anchor { pk: [0; 32], ty: AnchorType::Tofu }; ANCHORS_MAX],
                    anchors_len: 0,
                    gen_max: env.gen,
                    ver_max: 0,
                };
                ent.id[..ent.id_len].copy_from_slice(&face.lineage[..ent.id_len]);
                ent.anchors[0] = Anchor { pk: env.pk, ty: AnchorType::Tofu };
                ent.anchors_len = 1;
                nl.lineages[nl.lineages_len] = ent;
                nl.lineages_len += 1;
            }
            upsert_app(&mut nl, face, env.pk);
            Verdict::Mutate { note: Note::TofuFirstSeen { gen: env.gen }, ledger: nl }
        }
    }
}

/// per-app 记录 upsert（新键 = 四字段首记，旧键 = pk/来源刷新 + boot 计数
/// +1 + ver 水位推进不降——§1.4 字段面单源）。调用方保证帽有余量。
fn upsert_app(nl: &mut TrustLedger, face: &LoadFace<'_>, pk: [u8; 32]) {
    if let Some(i) = nl.apps[..nl.apps_len].iter().position(|r| r.id() == face.id) {
        let r = &mut nl.apps[i];
        r.pk = pk;
        r.source = face.source;
        r.boots = r.boots.saturating_add(1);
        r.ver_max = bump_ver(r.ver_max, face.ver);
        return;
    }
    let n = face.id.len().min(APP_ID_MAX);
    let mut rec = AppRecord {
        id: [0; APP_ID_MAX],
        id_len: n,
        pk,
        source: face.source,
        boots: 1,
        ver_max: face.ver,
    };
    rec.id[..n].copy_from_slice(&face.id[..n]);
    nl.apps[nl.apps_len] = rec;
    nl.apps_len += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::{ChainNode, ALG_ED25519_SHA512_256, CHAIN_NODES_MAX, LINEAGE_MAX as ENV_LIN_MAX};

    // ---- 测试基件：确定性钥 + 信封构造器（bin sign_envelope 的测试镜像
    //      ——签发单源仍 = berry-sign bin；此处仅产夹具）----

    fn tkey(seed: u8) -> [u8; 32] {
        core::array::from_fn(|i| seed.wrapping_add(i as u8))
    }

    fn pk_of(sk: &[u8; 32]) -> [u8; 32] {
        ed25519::public_key(sk)
    }

    /// 构造信封：锚钥链 `sks`（sks[0] = 起始锚钥 … 末 = 当前钥）——
    /// 零链 = 单钥；带链 = 逐节点 BSIG-ROT（node i = sks[i] 对
    /// (pk_i → pk_{i+1}) 的签名）。
    fn envelope(
        sks: &[[u8; 32]],
        lineage: &[u8],
        gen: u32,
        bap_digest: &[u8; DIGEST_LEN],
        manifest_digest: &[u8; DIGEST_LEN],
    ) -> Vec<u8> {
        assert!(!sks.is_empty() && sks.len() <= CHAIN_NODES_MAX + 1);
        let cur_sk = sks[sks.len() - 1];
        let cur_pk = pk_of(&cur_sk);
        let mut env = SigEnvelopeV1 {
            alg_id: ALG_ED25519_SHA512_256,
            lineage: [0; ENV_LIN_MAX],
            lineage_len: lineage.len(),
            gen,
            pk: cur_pk,
            pending: None,
            chain: [ChainNode { new_pk: [0; 32], rot_sig: [0; SIG_LEN] }; CHAIN_NODES_MAX],
            chain_len: sks.len() - 1,
            top_sig: [0; SIG_LEN],
        };
        env.lineage[..lineage.len()].copy_from_slice(lineage);
        for i in 0..sks.len() - 1 {
            env.chain[i].new_pk = pk_of(&sks[i + 1]);
            let mut rm = [0u8; ROT_MSG_MAX];
            let rn = rot_message(&pk_of(&sks[i]), &env.chain[i].new_pk, gen, lineage, &mut rm).unwrap();
            env.chain[i].rot_sig = ed25519::sign(&sks[i], &rm[..rn]);
        }
        let mut buf = [0u8; ENVELOPE_MAX];
        let n = env.encode(&mut buf).unwrap();
        let mut msg = [0u8; TOP_MSG_MAX];
        let mn = top_message(&buf[..n - SIG_LEN], bap_digest, manifest_digest, &mut msg).unwrap();
        env.top_sig = ed25519::sign(&cur_sk, &msg[..mn]);
        let n2 = env.encode(&mut buf).unwrap();
        buf[..n2].to_vec()
    }

    fn anchor(pk: [u8; 32], ty: AnchorType) -> Anchor {
        Anchor { pk, ty }
    }

    fn lin(id: &str, anchors: &[Anchor], gen_max: u32) -> LineageEntry {
        let mut e = LineageEntry {
            id: [0; crate::ledger::LINEAGE_ID_MAX],
            id_len: id.len(),
            anchors: [anchor([0; 32], AnchorType::Tofu); ANCHORS_MAX],
            anchors_len: anchors.len(),
            gen_max,
            ver_max: 0,
        };
        e.id[..id.len()].copy_from_slice(id.as_bytes());
        e.anchors[..anchors.len()].copy_from_slice(anchors);
        e
    }

    fn app_rec(id: &str, pk: [u8; 32], source: AppSource, boots: u32, ver_max: u32) -> AppRecord {
        let mut r = AppRecord { id: [0; APP_ID_MAX], id_len: id.len(), pk, source, boots, ver_max };
        r.id[..id.len()].copy_from_slice(id.as_bytes());
        r
    }

    fn ledger_with(lineages: &[LineageEntry], apps: &[AppRecord]) -> TrustLedger {
        let mut l = TrustLedger {
            lineages: [lin("x", &[anchor([0; 32], AnchorType::System)], 0); LINEAGES_MAX],
            lineages_len: 0,
            apps: [app_rec("X", [0; 32], AppSource::Builtin, 0, 0); APPS_MAX],
            apps_len: 0,
        };
        for (dst, src) in l.lineages.iter_mut().zip(lineages) {
            *dst = *src;
        }
        for (dst, src) in l.apps.iter_mut().zip(apps) {
            *dst = *src;
        }
        l.lineages_len = lineages.len();
        l.apps_len = apps.len();
        l
    }

    /// 标准装载面（id P3T / lineage official / ver 0.1.0→1000 / 合成摘要）。
    fn face<'a>(sig: Option<&'a [u8]>, lineage: &'a [u8]) -> LoadFace<'a> {
        LoadFace {
            id: b"P3T",
            lineage,
            ver: 1000,
            bap_digest: [0xA1; DIGEST_LEN],
            manifest_digest: [0xB2; DIGEST_LEN],
            sig,
            source: AppSource::Builtin,
        }
    }

    const LIN: &[u8] = b"official";
    const DEV_LIN: &[u8] = b"dev-lin";

    // 1. 策略件解析：三档词全收 + 拒面（未知档/schema/键/缺 tier/CR）。
    #[test]
    fn policy_parse_pins() {
        for (w, p) in [
            ("dev", Policy::Dev),
            ("product-open", Policy::ProductOpen),
            ("product-locked", Policy::ProductLocked),
        ] {
            let src = format!("# load-policy v1\nschema = 1\ntier = \"{w}\"\n");
            assert_eq!(parse_policy(src.as_bytes()), Ok(p));
        }
        assert_eq!(parse_policy(b"schema = 1\ntier = \"dev\""), Ok(Policy::Dev)); // 无尾 LF
        // 开关钉板（§1.3.2 表逐列）。
        assert!(Policy::Dev.sideload() && !Policy::Dev.require_sig() && Policy::Dev.tofu());
        assert!(!Policy::ProductOpen.sideload() && !Policy::ProductOpen.require_sig());
        assert!(!Policy::ProductOpen.tofu());
        assert!(!Policy::ProductLocked.sideload() && Policy::ProductLocked.require_sig());
        assert!(!Policy::ProductLocked.tofu());
        // 拒面。
        assert_eq!(parse_policy(b"schema = 1\ntier = \"vendor\"\n"), Err(PolicyFileErr::UnknownTier));
        assert_eq!(parse_policy(b"schema = 2\ntier = \"dev\"\n"), Err(PolicyFileErr::BadSchema));
        assert_eq!(parse_policy(b"tier = \"dev\"\n"), Err(PolicyFileErr::BadSchema)); // schema 缺
        assert_eq!(parse_policy(b"schema = 1\nschema = 1\ntier = \"dev\"\n"), Err(PolicyFileErr::BadSchema));
        assert_eq!(parse_policy(b"schema = 1\n"), Err(PolicyFileErr::MissingTier));
        assert_eq!(parse_policy(b"schema = 1\ntier = \"dev\"\nextra = 0\n"), Err(PolicyFileErr::UnknownKey));
        assert_eq!(parse_policy(b"schema = 1\r\ntier = \"dev\"\n"), Err(PolicyFileErr::BadLine));
        assert_eq!(parse_policy(b"tier dev\n"), Err(PolicyFileErr::BadLine));
    }

    // 2. 正样例（DoD 1）：system 锚 + 官方件 .sig → Mutate(Verified)，
    //    水位不降（gen 等值幂等面）。
    #[test]
    fn positive_system_anchor() {
        let sk = tkey(0x11);
        let pk = pk_of(&sk);
        let led = ledger_with(&[lin("official", &[anchor(pk, AnchorType::System)], 3)], &[]);
        let sig = envelope(&[sk], LIN, 3, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN]);
        let ctx = AdmitCtx { policy: Policy::ProductLocked, replay_guard: true };
        match admit(&ctx, &led, &face(Some(&sig), LIN)) {
            Verdict::Mutate { note: Note::Verified { gen: 3 }, ledger: nl } => {
                assert_eq!(nl.lineages[0].gen_max, 3, "水位不降（等值幂等）");
                assert_eq!(nl.apps_len, 1);
                assert_eq!(nl.apps[0].id(), b"P3T");
                assert_eq!(nl.apps[0].pk, pk);
                assert_eq!(nl.apps[0].boots, 1);
                assert_eq!(nl.apps[0].ver_max, 1000);
                // 突变账本 canonical round-trip（emit→parse 恒等——DoD 9
                // lib 半面）。
                let mut out = [0u8; crate::ledger::LEDGER_SRC_MAX];
                let n = nl.emit(&mut out).unwrap();
                let back = TrustLedger::parse(&out[..n]).unwrap();
                let mut out2 = [0u8; crate::ledger::LEDGER_SRC_MAX];
                let n2 = back.emit(&mut out2).unwrap();
                assert_eq!(&out2[..n2], &out[..n]);
            }
            v => panic!("正样例应 Mutate(Verified)：{v:?}"),
        }
        // 二次装载（同件）：boots 推进 2、水位不降。
        let led2 = {
            let sig2 = envelope(&[sk], LIN, 3, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN]);
            match admit(&ctx, &led, &face(Some(&sig2), LIN)) {
                Verdict::Mutate { ledger: nl, .. } => nl,
                v => panic!("二次装载应 Mutate：{v:?}"),
            }
        };
        match admit(&ctx, &led2, &face(Some(&sig), LIN)) {
            Verdict::Mutate { ledger: nl, .. } => {
                assert_eq!(nl.apps[0].boots, 2);
                assert_eq!(nl.lineages[0].gen_max, 3);
            }
            v => panic!("三次装载应 Mutate：{v:?}"),
        }
    }

    // 3. 三负样例（DoD 2）+ 换钥/断链：篡改载荷/截断/跨 lineage/换钥/
    //    断链（链节点 rot_sig 篡改）。
    #[test]
    fn negative_faces() {
        let sk1 = tkey(0x11);
        let sk2 = tkey(0x22);
        let pk1 = pk_of(&sk1);
        // 双 lineage 在册（official + dev-lin）：跨 lineage 回声对账用例
        // 需要 face lineage 在册——否则先撞未知 lineage 档位拒。
        let led = ledger_with(
            &[
                lin("official", &[anchor(pk1, AnchorType::System)], 1),
                lin("dev-lin", &[anchor(pk_of(&tkey(0x33)), AnchorType::Tofu)], 0),
            ],
            &[],
        );
        let ctx = AdmitCtx { policy: Policy::ProductLocked, replay_guard: true };
        let sig = envelope(&[sk1], LIN, 1, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN]);
        // 篡改载荷：bap_digest 变 → TopSigBad。
        let f = LoadFace { bap_digest: [0xA1 ^ 1; DIGEST_LEN], ..face(Some(&sig), LIN) };
        assert_eq!(admit(&ctx, &led, &f), Verdict::Reject(Reason::Sig(SigErr::TopSigBad)));
        // 截断 .sig → BadLen（单一解释面）。
        let trunc = &sig[..sig.len() - 1];
        assert_eq!(
            admit(&ctx, &led, &face(Some(trunc), LIN)),
            Verdict::Reject(Reason::Sig(SigErr::BadLen))
        );
        // 跨 lineage：回声不等 → LineageEcho。
        assert_eq!(
            admit(&ctx, &led, &face(Some(&sig), DEV_LIN)),
            Verdict::Reject(Reason::Sig(SigErr::LineageEcho))
        );
        // 换钥（零链、错钥）→ KeySwap（G9 绑定执法：在册 lineage 异 pk
        // 由信封族 KeyMismatch 升策略族拒——admit 决策序单点映射；信封族
        // 原语义〔换锚 = KeyMismatch〕钉板在 test 10 verify_signed 直调）。
        let sig2 = envelope(&[sk2], LIN, 1, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN]);
        assert_eq!(
            admit(&ctx, &led, &face(Some(&sig2), LIN)),
            Verdict::Reject(Reason::Policy(PolicyErr::KeySwap))
        );
        // 断链：sk1→sk2 带链信封、node 0 rot_sig 篡改 → RotSigBad。
        let mut broken = sig_rot(&sk1, &sk2);
        let off = 8 + LIN.len() + 4 + 32 + 1; // node 0 首字节（new_pk）
        broken[off + 40] ^= 0xFF; // rot_sig 中段翻位
        assert_eq!(
            admit(&ctx, &led, &face(Some(&broken), LIN)),
            Verdict::Reject(Reason::Sig(SigErr::RotSigBad))
        );
        // 完好轮换链对照：sk1→sk2 带链 = Mutate（锚学习 + 双锚窗成立）。
        let good = sig_rot(&sk1, &sk2);
        match admit(&ctx, &led, &face(Some(&good), LIN)) {
            Verdict::Mutate { ledger: nl, .. } => {
                assert_eq!(nl.lineages[0].anchors_len, 2, "链到达钥入锚集");
                assert!(nl.lineages[0].anchors().iter().any(|a| a.pk == pk_of(&sk2)));
            }
            v => panic!("完好链应 Mutate：{v:?}"),
        }
    }

    /// sk1→sk2 单节点轮换链信封（gen 1、标准摘要面）。
    fn sig_rot(sk1: &[u8; 32], sk2: &[u8; 32]) -> Vec<u8> {
        envelope(&[*sk1, *sk2], LIN, 1, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN])
    }

    // 4. 水位负（DoD 3）：gen 回降拒（check_gen 单源）+ ver 回降拒。
    #[test]
    fn water_rollback_rejects() {
        let sk = tkey(0x11);
        let pk = pk_of(&sk);
        let led = ledger_with(
            &[lin("official", &[anchor(pk, AnchorType::System)], 5)],
            &[app_rec("P3T", pk, AppSource::Builtin, 2, 2000)], // ver 水位 0.2.0
        );
        let ctx = AdmitCtx { policy: Policy::Dev, replay_guard: true };
        // gen 回降：信封 gen 3 < gen_max 5 → GenRollback。
        let sig_old = envelope(&[sk], LIN, 3, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN]);
        assert_eq!(
            admit(&ctx, &led, &face(Some(&sig_old), LIN)),
            Verdict::Reject(Reason::Ledger(LedgerErr::GenRollback))
        );
        // ver 回降：装载面 ver 1000（0.1.0）< 记录 2000（0.2.0）。
        let sig_new = envelope(&[sk], LIN, 6, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN]);
        assert_eq!(
            admit(&ctx, &led, &face(Some(&sig_new), LIN)),
            Verdict::Reject(Reason::Ledger(LedgerErr::VerRollback))
        );
        // 诚实降级（DoD 8 lib 半面）：增量缺席（replay_guard off）→ 同
        // 上 gen 回降件放行（水位跳检——回放窗口重开如实）。
        let ctx_off = AdmitCtx { policy: Policy::Dev, replay_guard: false };
        assert!(matches!(
            admit(&ctx_off, &led, &face(Some(&sig_old), LIN)),
            Verdict::Mutate { .. }
        ));
    }

    // 5. 防剥离负（DoD 4）：per-app 记录在案的已签 id 无 .sig → 三档同拒。
    #[test]
    fn anti_strip_all_tiers() {
        let pk = pk_of(&tkey(0x11));
        let led = ledger_with(
            &[lin("official", &[anchor(pk, AnchorType::System)], 1)],
            &[app_rec("P3T", pk, AppSource::Builtin, 1, 0)],
        );
        for p in [Policy::Dev, Policy::ProductOpen, Policy::ProductLocked] {
            let ctx = AdmitCtx { policy: p, replay_guard: true };
            assert_eq!(
                admit(&ctx, &led, &face(None, LIN)),
                Verdict::Reject(Reason::Policy(PolicyErr::Stripped)),
                "防剥离恒执法（{p:?}）"
            );
        }
        // 对照：无记录 id 无 .sig → 不触发防剥离（走档位函数）。
        let led0 = ledger_with(&[lin("official", &[anchor(pk, AnchorType::System)], 1)], &[]);
        let ctx = AdmitCtx { policy: Policy::ProductLocked, replay_guard: true };
        assert_eq!(
            admit(&ctx, &led0, &face(None, LIN)),
            Verdict::Reject(Reason::Policy(PolicyErr::NoSig))
        );
    }

    // 6. 策略三档表（DoD 5）：无 .sig（dev/open 降级 loud、locked 拒）+
    //    未知 lineage 带签（dev TOFU、open/locked unknown-src 拒）。
    #[test]
    fn tier_table() {
        let sk = tkey(0x11);
        let pk = pk_of(&sk);
        let led = ledger_with(&[lin("official", &[anchor(pk, AnchorType::System)], 1)], &[]);
        // 无 .sig 三档。
        for (p, want) in [
            (Policy::Dev, Verdict::Allow(Note::DegradeUnsigned)),
            (Policy::ProductOpen, Verdict::Allow(Note::DegradeUnsigned)),
            (Policy::ProductLocked, Verdict::Reject(Reason::Policy(PolicyErr::NoSig))),
        ] {
            let ctx = AdmitCtx { policy: p, replay_guard: true };
            assert_eq!(admit(&ctx, &led, &face(None, LIN)), want, "无 .sig @ {p:?}");
        }
        // 未知 lineage 带签三档（lineage dev-lin 不在册）。
        let sig = envelope(&[sk], DEV_LIN, 1, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN]);
        let ctx = AdmitCtx { policy: Policy::Dev, replay_guard: true };
        assert!(matches!(
            admit(&ctx, &led, &face(Some(&sig), DEV_LIN)),
            Verdict::Mutate { note: Note::TofuFirstSeen { gen: 1 }, .. }
        ));
        for p in [Policy::ProductOpen, Policy::ProductLocked] {
            let ctx = AdmitCtx { policy: p, replay_guard: true };
            assert_eq!(
                admit(&ctx, &led, &face(Some(&sig), DEV_LIN)),
                Verdict::Reject(Reason::Policy(PolicyErr::UnknownSrc)),
                "未知 lineage @ {p:?}"
            );
        }
    }

    // 7. TOFU + 轮换重叠窗（DoD 6）：dev 首见落增量账本（tofu 锚 + per-app
    //    四字段）；二次装载同 lineage 换钥走链；重叠窗双锚任一过。
    #[test]
    fn tofu_and_rotation_overlap() {
        let sk1 = tkey(0x11);
        let sk2 = tkey(0x22);
        let pk1 = pk_of(&sk1);
        let pk2 = pk_of(&sk2);
        let boot = ledger_with(&[lin("official", &[anchor(pk1, AnchorType::System)], 1)], &[]);
        let dev = AdmitCtx { policy: Policy::Dev, replay_guard: true };
        // 首见：lineage dev-lin 未知 → TOFU 记 tofu 锚 pk1 + per-app 记录。
        let sig1 = envelope(&[sk1], DEV_LIN, 2, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN]);
        let nl1 = match admit(&dev, &boot, &face(Some(&sig1), DEV_LIN)) {
            Verdict::Mutate { note: Note::TofuFirstSeen { gen }, ledger } => {
                assert_eq!(gen, 2);
                ledger
            }
            v => panic!("TOFU 首见应 Mutate：{v:?}"),
        };
        {
            let e = nl1.find_lineage(DEV_LIN).expect("tofu lineage 落账");
            assert_eq!(e.gen_max, 2);
            assert_eq!(e.anchors_len, 1);
            assert_eq!(e.anchors()[0].ty, AnchorType::Tofu);
            assert_eq!(e.anchors()[0].pk, pk1);
            let r = nl1.find_app(b"P3T").expect("per-app 四字段落账");
            assert_eq!((r.pk, r.boots, r.ver_max), (pk1, 1, 1000));
            assert_eq!(r.source, AppSource::Builtin);
        }
        // 二次装载同 lineage 换钥走链：pk1→pk2 带链信封过锚 pk1。
        let sig_rot = envelope(&[sk1, sk2], DEV_LIN, 3, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN]);
        let nl2 = match admit(&dev, &nl1, &face(Some(&sig_rot), DEV_LIN)) {
            Verdict::Mutate { note: Note::Verified { gen }, ledger } => {
                assert_eq!(gen, 3);
                ledger
            }
            v => panic!("换钥走链应 Mutate：{v:?}"),
        };
        assert_eq!(nl2.find_lineage(DEV_LIN).unwrap().gen_max, 3);
        // 重叠窗双锚任一过：锚集 {pk1, pk2}——零链 pk1 件与零链 pk2 件均过。
        assert!(nl2.find_lineage(DEV_LIN).unwrap().anchors().iter().any(|a| a.pk == pk2));
        let sig_pk1 = envelope(&[sk1], DEV_LIN, 3, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN]);
        let sig_pk2 = envelope(&[sk2], DEV_LIN, 3, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN]);
        for s in [&sig_pk1, &sig_pk2] {
            assert!(
                matches!(admit(&dev, &nl2, &face(Some(s), DEV_LIN)), Verdict::Mutate { .. }),
                "重叠窗双锚任一过（{}）",
                if s == &sig_pk1 { "旧锚" } else { "新锚" }
            );
        }
    }

    // 8. sideload 开关（DoD 7）：关 = 拒装 + 显式错误；开 = 装 + 账本
    //    记录来源 sideload。
    #[test]
    fn sideload_gate() {
        let sk = tkey(0x11);
        let pk = pk_of(&sk);
        let led = ledger_with(&[lin("official", &[anchor(pk, AnchorType::System)], 1)], &[]);
        let sig = envelope(&[sk], LIN, 1, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN]);
        let side = LoadFace { source: AppSource::Sideload, ..face(Some(&sig), LIN) };
        // 关（产品两档）= 拒装 + 显式错误。
        for p in [Policy::ProductOpen, Policy::ProductLocked] {
            let ctx = AdmitCtx { policy: p, replay_guard: true };
            assert_eq!(
                admit(&ctx, &led, &side),
                Verdict::Reject(Reason::Policy(PolicyErr::SideloadOff))
            );
        }
        // 开（dev）= 装 + 记录来源 sideload。
        let ctx = AdmitCtx { policy: Policy::Dev, replay_guard: true };
        match admit(&ctx, &led, &side) {
            Verdict::Mutate { ledger: nl, .. } => {
                assert_eq!(nl.find_app(b"P3T").unwrap().source, AppSource::Sideload);
            }
            v => panic!("dev sideload 应 Mutate：{v:?}"),
        }
    }

    // 9. eco 词表钉板（CLI 决策行消费面——三族分派单源）。
    #[test]
    fn eco_words_pin() {
        assert_eq!(Reason::Policy(PolicyErr::NoSig).word(), "nosig");
        assert_eq!(Reason::Policy(PolicyErr::UnknownSrc).word(), "unknown-src");
        assert_eq!(Reason::Policy(PolicyErr::SideloadOff).word(), "sideload");
        assert_eq!(Reason::Policy(PolicyErr::Stripped).word(), "stripped");
        assert_eq!(Reason::Policy(PolicyErr::KeySwap).word(), "key-swap");
        assert_eq!(Reason::Sig(SigErr::TopSigBad).word(), "top-sig-bad");
        assert_eq!(Reason::Sig(SigErr::KeyMismatch).word(), "key-mismatch");
        assert_eq!(Reason::Sig(SigErr::LineageEcho).word(), "lineage-echo");
        assert_eq!(Reason::Ledger(LedgerErr::GenRollback).word(), "gen-rollback");
        assert_eq!(Reason::Ledger(LedgerErr::VerRollback).word(), "ver-rollback");
        assert_eq!(Policy::ProductLocked.word(), "product-locked");
    }

    // 10. verify_signed 单源消费钉板：berry-sign verify 语义（单锚零链
    //     锚等/pk 不符拒）在 lib 面直接可验（bin 委托同一函数）。
    #[test]
    fn verify_signed_anchor_semantics() {
        let sk = tkey(0x11);
        let pk = pk_of(&sk);
        let sig = envelope(&[sk], LIN, 1, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN]);
        let one = [anchor(pk, AnchorType::System)];
        assert!(verify_signed(&one, LIN, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN], &sig).is_ok());
        // 换锚拒（KeyMismatch）+ 回声拒（LineageEcho）+ 摘要拒（TopSigBad）。
        assert_eq!(
            verify_signed(&[anchor(pk_of(&tkey(0x99)), AnchorType::System)], LIN,
                          &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN], &sig),
            Err(Reason::Sig(SigErr::KeyMismatch))
        );
        assert_eq!(
            verify_signed(&one, DEV_LIN, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN], &sig),
            Err(Reason::Sig(SigErr::LineageEcho))
        );
        assert_eq!(
            verify_signed(&one, LIN, &[0xC3; DIGEST_LEN], &[0xB2; DIGEST_LEN], &sig),
            Err(Reason::Sig(SigErr::TopSigBad))
        );
        // 带链：锚 = 链首旧钥 → 过；锚 = 末钥（零链语义）→ 过（重叠窗）。
        let rot = envelope(&[sk, tkey(0x22)], LIN, 1, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN]);
        let mut head = [0u8; ENVELOPE_MAX];
        let n = SigEnvelopeV1::parse(&rot).unwrap().encode(&mut head).unwrap();
        assert!(verify_signed(&one, LIN, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN], &rot).is_ok());
        let _ = (head, n);
    }

    // 11. G9 lineage↔pk 绑定执法（2026-11-12，38 号 §4 G9 行）：首见锚落 /
    //     同 pk 复见过 / 同 lineage 零链异 pk 全档拒（换钥走仪式不走
    //     TOFU）/ 换钥带链（仪式）对照过 / 账本缺 lineage 各档（dev TOFU
    //     首见装、open/locked unknown-src 拒——§1.3.2 表原文：open 不
    //     TOFU）。
    #[test]
    fn g9_lineage_pk_binding() {
        let sk1 = tkey(0x11);
        let sk2 = tkey(0x22);
        let pk1 = pk_of(&sk1);
        let pk2 = pk_of(&sk2);
        let boot = ledger_with(&[lin("official", &[anchor(pk_of(&tkey(0x33)), AnchorType::System)], 1)], &[]);
        let dev = AdmitCtx { policy: Policy::Dev, replay_guard: true };
        // (a) 首见锚落：dev TOFU 首见 dev-lin → tofu 锚 pk1 入账本。
        let sig1 = envelope(&[sk1], DEV_LIN, 2, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN]);
        let nl1 = match admit(&dev, &boot, &face(Some(&sig1), DEV_LIN)) {
            Verdict::Mutate { ledger: nl, .. } => nl,
            v => panic!("TOFU 首见应 Mutate：{v:?}"),
        };
        let e = nl1.find_lineage(DEV_LIN).expect("首见锚落");
        assert_eq!((e.anchors_len, e.anchors()[0].pk, e.anchors()[0].ty), (1, pk1, AnchorType::Tofu));
        // (b) 同 pk 复见过：零链 pk1 再见 → Mutate(Verified)（水位等值幂等）。
        assert!(matches!(
            admit(&dev, &nl1, &face(Some(&sig1), DEV_LIN)),
            Verdict::Mutate { note: Note::Verified { gen: 2 }, .. }
        ));
        // (c) 同 lineage 异 pk 拒（全档）：零链 pk2 未走仪式 → KeySwap。
        let sig2 = envelope(&[sk2], DEV_LIN, 2, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN]);
        for p in [Policy::Dev, Policy::ProductOpen, Policy::ProductLocked] {
            let ctx = AdmitCtx { policy: p, replay_guard: true };
            assert_eq!(
                admit(&ctx, &nl1, &face(Some(&sig2), DEV_LIN)),
                Verdict::Reject(Reason::Policy(PolicyErr::KeySwap)),
                "同 lineage 换钥未走仪式 = 全档拒（{p:?}）"
            );
        }
        // (d) 仪式对照：同换钥带轮换链（pk1→pk2 BSIG-ROT）→ 过 + 到达钥
        //     入锚集（重叠双签窗——「换钥走仪式」正路保持畅通）。
        let sig_rot = envelope(&[sk1, sk2], DEV_LIN, 3, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN]);
        match admit(&dev, &nl1, &face(Some(&sig_rot), DEV_LIN)) {
            Verdict::Mutate { ledger: nl, .. } => {
                let e = nl.find_lineage(DEV_LIN).unwrap();
                assert!(e.anchors().iter().any(|a| a.pk == pk2), "链到达钥入锚集");
            }
            v => panic!("换钥走链（仪式）应 Mutate：{v:?}"),
        }
        // (e) 账本缺 lineage 各档：dev TOFU 首见装；open/locked
        //     unknown-src 拒（TOFU 行为 = 档位函数，§1.3.2 表）。
        let fresh: &[u8] = b"fresh-lin";
        let sig_f = envelope(&[sk2], fresh, 1, &[0xA1; DIGEST_LEN], &[0xB2; DIGEST_LEN]);
        assert!(matches!(
            admit(&dev, &boot, &face(Some(&sig_f), fresh)),
            Verdict::Mutate { note: Note::TofuFirstSeen { gen: 1 }, .. }
        ));
        for p in [Policy::ProductOpen, Policy::ProductLocked] {
            let ctx = AdmitCtx { policy: p, replay_guard: true };
            assert_eq!(
                admit(&ctx, &boot, &face(Some(&sig_f), fresh)),
                Verdict::Reject(Reason::Policy(PolicyErr::UnknownSrc)),
                "账本缺 lineage @ {p:?}"
            );
        }
    }
}
