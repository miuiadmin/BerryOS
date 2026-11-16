//! berry-sign —— SDK-2 分发签名链 host CLI（P1 签名集成刀：24 号 §4.3
//! 半刀②③「签名产出 + 验签门禁」；设计母法 = docs/SDK-2-分发与签名链
//! 设计-v1.2.md §三；消费方 = tools/mkapp.py `--sign-key`/`--verify-key`）。
//!
//! 子命令：
//! - `berry-sign keygen <sk-out> <pk-out>` —— 产 dev 密钥对：sk 文件 32B
//!   原始私钥（**权限 0600**）、pk 文件 32B（= `ed25519::public_key`）。
//!   熵源 = /dev/urandom（host 文件，零 crate 依赖）。**dev/test 面纪律**：
//!   产出的钥匙只用于开发/自检签发，永不用于真签（真签面 = 平台签发，
//!   随 #77/P3 落）；测试密钥落 /tmp，不落仓。
//! - `berry-sign sign <sk> <bap> [--gen N]` —— 产 `<bap>.sig`（信封 v1
//!   TLV，envelope.rs 构造器单源；gen 缺省 1）。lineage = manifest
//!   `[security].key_lineage` **回声单源**（v1.2 §三 :57——CLI 不收
//!   lineage 参数，防第二来源）；顶签名覆盖域 = BSIG-TOP（信封头 ‖
//!   bap 全字节 SHA-512/256 摘要 ‖ manifest 关键域摘要）。
//! - `berry-sign verify <pk> <bap> [<sig>]` —— 验签：exit 0 = 过，
//!   exit 1 = 拒（stderr 一行拒因）。sig 缺省 = `<bap>.sig`。
//! - `berry-sign sign-raw <sk> <in> [--out <sig-out>]` —— **64B ed25519
//!   裸签名**（RFC 8032 §5.1.6 确定性 nonce；覆盖域 = 输入文件全字节，
//!   **无域分隔标签**——标签归调用方消息构造，如 gensourceindex 的
//!   `"BSRC-TOP"` 前缀）。`--out` 缺省 = stdout 原始二进制 64B（管道面
//!   零附注字节）；`--out` = 落盘 64B + stdout 人读行。**独立合同**：
//!   非 BSIG 信封——`sign` 输出信封（cpio 载荷语义合同，.bap 面），本
//!   子命令只出裸签名，两份合同不互换（ADR #77：索引 64B 裸签名槽是
//!   独立合同，Q3 窗内 SDK-2 §五「源钥签索引」的 host 半边；消费方 =
//!   gensourceindex `sign_index()`/`--verify`）。
//! - `berry-sign verify-raw <pk> <msg> <sig>` —— 裸签名验签（sign-raw
//!   镜像面）：exit 0 = 过，exit 1 = 拒。sig 非 64B = loud 拒。
//! - `berry-sign selftest` —— v1.2 §九验收线 1 四件自检（正样例 +
//!   篡改/断链/换钥三负样例；宿主最短路径归属，见 selftest 头注）+
//!   sign-raw 腿（RFC 8032 §7.1 TEST 1/2 官方向量抽查——64B 逐字节
//!   钉板 + 篡改/换钥负）+ admit 腿（P3-0 三档/防剥离/TOFU/sideload/
//!   水位/诚实降级在制件）。
//! - `berry-sign admit <policy.toml> <ledger.toml> <dir> [--emit-ledger
//!   <out>] [--sideload <id>]… [--no-increment]` —— P3-0 件二：目录内
//!   `*.bap`+`*.bap.sig` 逐件装载裁决（决策行 stdout + 可选增量账本
//!   canonical emit；拒载不连坐——失败隔离；设计 = `docs/设备侧验签与
//!   账本落盘设计-v1.0.md` §二；决策/验签语义单源 = lib `admit` 模块，
//!   本 CLI 只编排）。exit：0 = 全放行；1 = 有拒载（决策已出全）；
//!   2 = 用法/IO/账本损坏（保守拒）。
//!   schema 自述面（G10，38 号 §4）= `berry-sign admit --help` banner：
//!   合法 policy 形（`schema = 1` + `tier` 三档两键闭集）与 ledger 形
//!   （`[[lineage]]` 节键闭集 + V4 四占位恒空 + 可选 `[[app]]` 五键）
//!   各数行示例——lib `admit::parse_policy`/`ledger.rs` 头注例样单源的
//!   摘要（改 schema 须同笔改 banner），不再靠试错或读 p3sim.py。
//!   G9（2026-11-12）lineage↔pk 绑定执法拒因行：`key-swap` 行附
//!   `lineage=` + 两 pk 前缀（`env-pk=`/`anchor-pk=`——结构单源 = lib
//!   `PolicyErr::KeySwap`，本 CLI 只渲染拒因可见面）。
//!
//! ## 覆盖域合同（P0-2 验签合同①②③的消费实现）
//!
//! - **bap_digest** = SHA-512/256（.bap 全字节；`sha512::digest256`）。
//! - **manifest 关键域摘要** = SHA-512/256 over 关键域字节（单解释器
//!   纪律 24 号 §3.3-1 方案 A：输入由 berry-manifest 解析产物单源派生
//!   ——`berry_manifest::parse` 的字段值；本 crate 不出现第二个 manifest
//!   语义判断）。关键域覆盖域 = 24 号 §3.4-② 建议（**推断**，Q3 冻结随
//!   #77）：`"BSIG-MF"` ‖ [app] id/version ‖ [compat] 三值（缺席 0x00 /
//!   在场 0x01+三值）‖ [security] 三键——身份锚独立于容器布局；字段以
//!   u8 长度前缀拼接（防拼接歧义）。
//! - **验签序**：信封结构过检（parse 单解释）→ 信封 pk == 可信 pk
//!   （换钥拒面）→ lineage 回声对账（信封 lineage == manifest
//!   key_lineage——跨 lineage 拒面）→ 顶签名验（BSIG-TOP）→ 轮换链逐
//!   节点验（node 0 old_pk = 可信 pk，BSIG-ROT 链序推进；末节点续接
//!   parse 已执法）。P1 签发面 = 零链（keygen 单钥），链路执法随轮换
//!   仪式（v1.2 §三 :62 pending 位预留）。
//!
//! ## cpio 走查器（本文件私有，只读）
//!
//! `cpio_manifest` 对读 mkcpio.entry 编码几何 + tools/mkapp.py
//! `_cpio_entries` 同口径：magic 070701 + 13×hex8、filesize = 域[6]、
//! namesize = 域[11]（计尾 NUL）、头+名/数据垫 4096（X-3c）、TRAILER!!!
//! 终止——只提取 `manifest.toml`，不产不改正文（非第二编码器；guest 侧
//! svcmgr cpio_walk 为第三对读面，仓内既有格局）。
//!
//! ## 定位注（P1 刀界）
//!
//! 验签组合逻辑（top_message 重组 + 逐节点链验）暂居本 bin（host 最短
//! 路径）；P3 设备侧常驻消费出现时上移 lib（「解析器即产品」面）。
//! 信封格式 Q3 未冻结——P0–P2 内野无消费者，改版零成本（24 号 §4.2）。
//! host target = aarch64-apple-darwin（与 justfile test-host 的
//! berry-crypto 测试行同轨）。

// std 门控同 lib.rs cfg_attr 手法:none target 下本 bin 声明 no_std
// (crate 根不注 std prelude——否则 E0463/panic_handler 炸),代码体另由
// mod cli 的 cfg 整体门控,双保险后 none target 编出空壳。
#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
// 空壳无入口符号(_start)——linker_messages 警告在 none 门内豁免。
#![cfg_attr(target_os = "none", allow(linker_messages))]

// none target(user workspace 默认 target = aarch64-unknown-none,根 config
// build.target——G-eco-b 整仓 `cargo build` 路径)下本 bin 为空壳:
// 实现整体门控在 host 侧——std 面不进 none 编译,lib 的 no_std 纪律
// 不受 bin 牵连;宿主实编 = `cargo build --bin berry-sign --target
// aarch64-apple-darwin`(P1 任务书验证命令)。
#[cfg(not(target_os = "none"))]
mod cli {
    use std::fmt::Write as _;
    use std::fs;
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    use std::process::ExitCode;

    use berry_crypto::ed25519;
    use berry_crypto::envelope::{
        top_message, ChainNode, SigEnvelopeV1, ALG_ED25519_SHA512_256, CHAIN_NODES_MAX,
        ENVELOPE_MAX, LINEAGE_MAX, SIG_LEN, TOP_MSG_MAX,
    };
    use berry_crypto::admit::{self, AdmitCtx, LoadFace, Note, PolicyErr, Reason, Verdict};
    use berry_crypto::ledger::{AppSource, TrustLedger, LEDGER_SRC_MAX as EMIT_BUF};
    use berry_crypto::sha512::digest256;

    /// manifest 关键域域分隔标签（防跨上下文重放——BSIG-ROT/BSIG-TOP 同
    /// 精神；**推断**标注随 Q3/#77 冻结）。
    const MF_LABEL: &[u8] = b"BSIG-MF";

    pub fn main() -> ExitCode {
        let argv: Vec<String> = std::env::args().skip(1).collect();
        let args: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
        match args.as_slice() {
            ["keygen", sk, pk] => run(cmd_keygen(sk, pk)),
            ["sign", sk, bap] => run(cmd_sign(sk, bap, 1)),
            ["sign", sk, bap, "--gen", g] => match g.parse::<u32>() {
                Ok(gen) => run(cmd_sign(sk, bap, gen)),
                Err(_) => {
                    eprintln!("berry-sign: --gen 应为 u32（得 {g:?}）");
                    ExitCode::from(2)
                }
            },
            ["sign-raw", sk, inp] => cmd_sign_raw_cli(sk, inp, None),
            ["sign-raw", sk, inp, "--out", out] => cmd_sign_raw_cli(sk, inp, Some(out)),
            ["verify", pk, bap] => run(cmd_verify(pk, bap, &format!("{bap}.sig"))),
            ["verify", pk, bap, sig] => run(cmd_verify(pk, bap, sig)),
            ["verify-raw", pk, msg, sig] => run(cmd_verify_raw(pk, msg, sig)),
            ["selftest"] => run(selftest()),
            ["admit", rest @ ..] => cmd_admit_main(rest),
            _ => {
                eprintln!(
                    "berry-sign: 参数不合规\n\
                     用法: berry-sign keygen <sk-out> <pk-out>\n\
                     \x20     berry-sign sign <sk> <bap> [--gen N]\n\
                     \x20     berry-sign sign-raw <sk> <in> [--out <sig-out>]\n\
                     \x20     berry-sign verify <pk> <bap> [<sig>]\n\
                     \x20     berry-sign verify-raw <pk> <msg> <sig>\n\
                     \x20     berry-sign selftest\n\
                     \x20     berry-sign admit <policy.toml> <ledger.toml> <dir>\n\
                     \x20                 [--emit-ledger <out>] [--sideload <id>]… [--no-increment]\n\
                     \x20（admit 两件 schema 示例: berry-sign admit --help）"
                );
                ExitCode::from(2)
            }
        }
    }

    fn run(r: Result<String, String>) -> ExitCode {
        match r {
            Ok(line) => {
                println!("{line}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("berry-sign: {e}");
                ExitCode::FAILURE
            }
        }
    }

    // ---- keygen：dev 密钥对（熵 = /dev/urandom；sk 0600）----

    fn cmd_keygen(sk_path: &str, pk_path: &str) -> Result<String, String> {
        let mut sk = [0u8; 32];
        let mut f = fs::File::open("/dev/urandom")
            .map_err(|e| format!("keygen: /dev/urandom 打不开：{e}"))?;
        f.read_exact(&mut sk).map_err(|e| format!("keygen: 熵读失败：{e}"))?;
        write_secret(sk_path, &sk)?;
        let pk = ed25519::public_key(&sk);
        fs::write(pk_path, pk).map_err(|e| format!("keygen: pk 写失败 {pk_path}: {e}"))?;
        Ok(format!("keygen: {pk_path} pk={}", hex(&pk)))
    }

    /// sk 落盘 0600（dev 纪律：其余权限面不给——OpenOptions mode 执法）。
    fn write_secret(path: &str, sk: &[u8; 32]) -> Result<(), String> {
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, sk))
            .map_err(|e| format!("keygen: sk 写失败 {path}: {e}"))
    }

    fn read_exact32(path: &str, what: &str) -> Result<[u8; 32], String> {
        let mut buf = [0u8; 32];
        let mut f = fs::File::open(path).map_err(|e| format!("{what}: 打不开 {path}: {e}"))?;
        f.read_exact(&mut buf)
            .map_err(|e| format!("{what}: {path} 读失败（sk/pk 文件应恰 32 字节）：{e}"))?;
        let mut tail = [0u8; 1];
        match f.read(&mut tail) {
            Ok(0) => Ok(buf),
            _ => Err(format!("{what}: {path} 非 32B（长度不符）")),
        }
    }

    // ---- sign：.bap → .sig（信封 v1 + 顶签名）----

    fn cmd_sign(sk_path: &str, bap_path: &str, gen: u32) -> Result<String, String> {
        let sk = read_exact32(sk_path, "sign")?;
        let bap = fs::read(bap_path).map_err(|e| format!("sign: 读 {bap_path}: {e}"))?;
        let mf = cpio_manifest(&bap)?;
        let sig = sign_envelope(&sk, &bap, mf, gen)?;
        let out = format!("{bap_path}.sig");
        fs::write(&out, &sig).map_err(|e| format!("sign: 写 {out}: {e}"))?;
        let env = SigEnvelopeV1::parse(&sig).map_err(|e| format!("sign: 回验失败 {e:?}"))?;
        Ok(format!(
            "sign: {out} {}B lineage={} gen={} chain=0",
            sig.len(),
            String::from_utf8_lossy(env.lineage()),
            env.gen
        ))
    }

    /// 信封构造 + 顶签名（核心；selftest 复用）。manifest 关键域/lineage
    /// 回声均自 `mf` 单源派生（berry-manifest 解析产物）。
    fn sign_envelope(sk: &[u8; 32], bap: &[u8], mf: &[u8], gen: u32) -> Result<Vec<u8>, String> {
        let m = berry_manifest::parse(mf)
            .map_err(|e| format!("sign: manifest 解析拒：{}", e.reason_str()))?;
        let sec = m.security.ok_or(
            "sign: manifest 无 [security] 三键（ADR #82 V3——key_lineage 是信封回声源，签名面必填）",
        )?;
        let lineage = sec.key_lineage.as_bytes();
        let key_fields = key_fields_bytes(&m)?;
        let md = digest256(&key_fields);
        let bd = digest256(bap);
        let pk = ed25519::public_key(sk);
        let mut env = SigEnvelopeV1 {
            alg_id: ALG_ED25519_SHA512_256,
            lineage: [0; LINEAGE_MAX],
            lineage_len: lineage.len(),
            gen,
            pk,
            pending: None,
            chain: [ChainNode { new_pk: [0; 32], rot_sig: [0; SIG_LEN] }; CHAIN_NODES_MAX],
            chain_len: 0,
            top_sig: [0; SIG_LEN],
        };
        env.lineage[..lineage.len()].copy_from_slice(lineage);
        // 头（magic 至 chain 末字节）先以 top_sig 零占位发射——header 与
        // top_sig 无关（尾 64B），零占位即真头。
        let mut buf = [0u8; ENVELOPE_MAX];
        let n = env.encode(&mut buf).map_err(|e| format!("sign: 信封发射拒 {e:?}"))?;
        let mut msg = [0u8; TOP_MSG_MAX];
        let mn = top_message(&buf[..n - SIG_LEN], &bd, &md, &mut msg)
            .map_err(|e| format!("sign: 覆盖域构建拒 {e:?}"))?;
        env.top_sig = ed25519::sign(sk, &msg[..mn]);
        let n2 = env.encode(&mut buf).map_err(|e| format!("sign: 信封重发射拒 {e:?}"))?;
        Ok(buf[..n2].to_vec())
    }

    // ---- sign-raw / verify-raw：64B 裸签名（索引槽独立合同）----

    /// sign-raw CLI 面：`--out` 缺省 = stdout 原始 64B（零附注——管道纯度，
    /// 不走 `run()` 的 println）；`--out` = 落盘 + 人读行。
    fn cmd_sign_raw_cli(sk: &str, input: &str, out: Option<&str>) -> ExitCode {
        match cmd_sign_raw(sk, input, out) {
            Ok(Some(line)) => {
                println!("{line}");
                ExitCode::SUCCESS
            }
            Ok(None) => ExitCode::SUCCESS, // 裸签名已写 stdout
            Err(e) => {
                eprintln!("berry-sign: {e}");
                ExitCode::FAILURE
            }
        }
    }

    /// `sign-raw <sk> <in> [--out <sig>]`：输入文件全字节 → ed25519 裸签名
    /// 64B（RFC 8032 §5.1.6 确定性 nonce；无域分隔标签——标签归调用方，
    /// 模块头注「独立合同」段）。selftest 复用（CLI 同路直调）。
    fn cmd_sign_raw(sk_path: &str, in_path: &str, out_path: Option<&str>) -> Result<Option<String>, String> {
        let sk = read_exact32(sk_path, "sign-raw")?;
        let msg = fs::read(in_path).map_err(|e| format!("sign-raw: 读 {in_path}: {e}"))?;
        let sig = ed25519::sign(&sk, &msg);
        match out_path {
            Some(p) => {
                fs::write(p, sig).map_err(|e| format!("sign-raw: 写 {p}: {e}"))?;
                Ok(Some(format!(
                    "sign-raw: {p} 64B（ed25519 裸签名，覆盖域 = {in_path} 全字节——独立于 .bap 信封合同）"
                )))
            }
            None => {
                use std::io::Write as _;
                std::io::stdout()
                    .write_all(&sig)
                    .map_err(|e| format!("sign-raw: stdout 写失败：{e}"))?;
                Ok(None)
            }
        }
    }

    /// `verify-raw <pk> <msg> <sig>`：裸签名验签（exit 0/1——`run()` 面）。
    /// sig 恰 64B 单一解释（非 64B loud 拒——裸签名无容器头可辨）。
    fn cmd_verify_raw(pk_path: &str, msg_path: &str, sig_path: &str) -> Result<String, String> {
        let pk = read_exact32(pk_path, "verify-raw")?;
        let msg = fs::read(msg_path).map_err(|e| format!("verify-raw: 读 {msg_path}: {e}"))?;
        let sig_v = fs::read(sig_path).map_err(|e| format!("verify-raw: 读 {sig_path}: {e}"))?;
        if sig_v.len() != SIG_LEN {
            return Err(format!(
                "verify-raw: {sig_path} 非 {SIG_LEN}B 裸签名（got {}B）",
                sig_v.len()
            ));
        }
        let mut sig = [0u8; SIG_LEN];
        sig.copy_from_slice(&sig_v);
        if ed25519::verify(&pk, &sig, &msg) {
            Ok(format!(
                "verify-raw: {sig_path} OK（ed25519 裸签名——覆盖域 = {msg_path} 全字节）"
            ))
        } else {
            Err("verify-raw: 验签拒（裸签名与消息/公钥不符——篡改或换钥）".to_string())
        }
    }

    // ---- verify：可信 pk + .bap (+ .sig) → exit 0/1 ----

    /// 验签产物（人读摘要行字段）。
    #[derive(Debug)]
    struct VerifyOk {
        lineage: String,
        gen: u32,
        chain_len: usize,
    }

    fn cmd_verify(pk_path: &str, bap_path: &str, sig_path: &str) -> Result<String, String> {
        let pk = read_exact32(pk_path, "verify")?;
        let bap = fs::read(bap_path).map_err(|e| format!("verify: 读 {bap_path}: {e}"))?;
        let sig = fs::read(sig_path).map_err(|e| {
            format!("verify: 读 {sig_path}: {e}（已声明验签面，缺 .sig = 拒——防剥离护栏，v1.2 §三 :63）")
        })?;
        let mf = cpio_manifest(&bap)?;
        let ok = verify_envelope(&pk, &bap, mf, &sig)?;
        Ok(format!(
            "verify: {sig_path} OK lineage={} gen={} chain={} (BSIG-TOP 过)",
            ok.lineage, ok.gen, ok.chain_len
        ))
    }

    /// 验签序（核心；selftest 复用）——**P3-0 起单源委托 lib
    /// [`admit::verify_signed`]**（验签组合逻辑上移 lib，设计 §1.3.3；
    /// 本函数 = CLI 适配层：单锚 [{pk}] + manifest 回声/摘要派生不变；
    /// 重叠窗/链语义随 lib）。
    fn verify_envelope(pk: &[u8; 32], bap: &[u8], mf: &[u8], sig: &[u8]) -> Result<VerifyOk, String> {
        let m = berry_manifest::parse(mf)
            .map_err(|e| format!("manifest 拒：{}", e.reason_str()))?;
        let sec = m.security.ok_or("manifest 拒: 无 [security]（无 key_lineage 可回声对账）")?;
        let key_fields = key_fields_bytes(&m)?;
        let md = digest256(&key_fields);
        let bd = digest256(bap);
        let anchors = [berry_crypto::ledger::Anchor {
            pk: *pk,
            ty: berry_crypto::ledger::AnchorType::System,
        }];
        let ok = admit::verify_signed(&anchors, sec.key_lineage.as_bytes(), &bd, &md, sig)
            .map_err(|r| verify_err_text(r, pk))?;
        Ok(VerifyOk {
            lineage: String::from_utf8_lossy(ok.lineage()).into_owned(),
            gen: ok.gen,
            chain_len: ok.chain_len,
        })
    }

    /// lib 结构化拒因 → CLI 文案（人读面；行为 = exit 1 不变；词面 =
    /// 各枚举 `word()` 单源）。
    fn verify_err_text(r: berry_crypto::admit::Reason, pk: &[u8; 32]) -> String {
        use berry_crypto::admit::Reason;
        use berry_crypto::envelope::SigErr;
        match r {
            Reason::Sig(SigErr::KeyMismatch) => {
                format!("换钥拒: 信封当前钥无锚见证（可信 pk {}）", hex(pk))
            }
            Reason::Sig(SigErr::LineageEcho) => {
                "跨 lineage 拒: 信封 lineage != manifest key_lineage".to_string()
            }
            Reason::Sig(SigErr::TopSigBad) => {
                "顶签名拒: BSIG-TOP 验签不过（载荷/信封被篡改或钥不符）".to_string()
            }
            Reason::Sig(SigErr::RotSigBad) => {
                "断链拒: 轮换链无锚可全程见证（BSIG-ROT）".to_string()
            }
            other => format!("验签拒[{other:?}]（{}）", other.word()),
        }
    }

    // ---- manifest 关键域字节（24 号 §3.4-②，推断；berry-manifest 单源）----

    /// 关键域覆盖域字节：`"BSIG-MF"` ‖ L‖id ‖ L‖version ‖ compat 标志
    /// （0x00 缺席 / 0x01 在场‖L‖min‖L‖target‖L‖compile）‖ L‖profile ‖
    /// L‖key_lineage ‖ L‖integrator。L = u8 长度前缀（帽：id 24 /
    /// version 16 / SdkVer 文法天然 ≤11 / sec 三键 16——berry-manifest
    /// 值域已执法，u8 前缀恒容）。
    fn key_fields_bytes(m: &berry_manifest::Manifest) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(128);
        out.extend_from_slice(MF_LABEL);
        let sec = m.security.ok_or("关键域拒: 无 [security] 三键")?;
        push_lstr(&mut out, m.id)?;
        push_lstr(&mut out, m.version)?;
        match &m.compat {
            None => out.push(0x00),
            Some(c) => {
                out.push(0x01);
                for v in [&c.min, &c.target, &c.compile] {
                    let s = ver_str(v);
                    push_lstr(&mut out, &s)?;
                }
            }
        }
        push_lstr(&mut out, sec.profile)?;
        push_lstr(&mut out, sec.key_lineage)?;
        push_lstr(&mut out, sec.integrator)?;
        Ok(out)
    }

    fn push_lstr(out: &mut Vec<u8>, s: &str) -> Result<(), String> {
        let b = s.as_bytes();
        if b.len() > 255 {
            return Err("关键域拒: 字段帽溢出（>255B，值域不应达）".to_string());
        }
        out.push(b.len() as u8);
        out.extend_from_slice(b);
        Ok(())
    }

    /// SdkVer → `x.y.z`（mkapp `_ver` 同构发射——无前导零文法天然封顶 11B）。
    fn ver_str(v: &berry_manifest::SdkVer) -> String {
        format!("{}.{}.{}", v.major, v.minor, v.patch)
    }

    // ---- cpio(newc) 只读走查（模块头注对读声明）----

    /// cpio 头 hex8 域解析（域序：0=c_ino … 6=c_filesize, 11=c_namesize）。
    fn cpio_field(bap: &[u8], off: usize, idx: usize) -> Result<usize, String> {
        let s = std::str::from_utf8(&bap[off + 6 + idx * 8..off + 6 + (idx + 1) * 8])
            .map_err(|_| format!("bap: cpio 坏字段 @ {off} 域{idx}"))?;
        usize::from_str_radix(s, 16).map_err(|_| format!("bap: cpio 坏字段 @ {off} 域{idx}"))
    }

    /// .bap 全字节 → manifest.toml 条目切片（借用输入；越界/坏魔数 loud 拒）。
    fn cpio_manifest(bap: &[u8]) -> Result<&[u8], String> {
        let mut off = 0usize;
        loop {
            if off + 110 > bap.len() {
                return Err(format!("bap: cpio 头越界 @ {off}"));
            }
            if &bap[off..off + 6] != b"070701" {
                return Err(format!("bap: cpio 坏魔数 @ {off}"));
            }
            let filesize = cpio_field(bap, off, 6)?;
            let namesize = cpio_field(bap, off, 11)?;
            if namesize < 1 {
                return Err(format!("bap: cpio 名长非法 @ {off}"));
            }
            let name_off = off + 110;
            if name_off + namesize > bap.len() {
                return Err(format!("bap: cpio 名越界 @ {off}"));
            }
            let name = &bap[name_off..name_off + namesize - 1]; // 剥尾 NUL
            let data_off = (name_off + namesize + 4095) & !4095; // 头+名垫页（X-3c）
            if data_off + filesize > bap.len() {
                return Err(format!("bap: cpio 数据越界 @ {off}"));
            }
            if name == b"TRAILER!!!" {
                break;
            }
            if name == b"manifest.toml" {
                return Ok(&bap[data_off..data_off + filesize]);
            }
            off = (data_off + filesize + 4095) & !4095; // 数据垫页（X-3c）
        }
        Err("bap: 缺 manifest.toml 条目".to_string())
    }

    // ---- selftest：v1.2 §九验收线 1 四件（宿主最短路径主归属）----

    /// 验收线四件自检（v1.2 §九 :130「host：mkapp 产 .sig → 验签过；篡改
    /// 载荷/断轮换链/跨 lineage 三负样例全拒」的信封层，P1 任务书口径四件
    /// = 正/篡改/断链[.sig 截断]/换钥）：
    /// ① 正样例：keygen 落盘（真 /dev/urandom + 0600 面）→ 读回 → 签 →
    ///    验过 + 信封字段对账（lineage 回声/gen/pk）；
    /// ② 篡改负样例：bap 改一字节 → 验拒（BSIG-TOP 覆盖 bap 全字节摘要）；
    /// ③ 断链负样例：.sig 截断 → 验拒（信封 parse BadLen——单一解释面；
    ///    轮换链断（ChainBroken/BSIG-ROT）由 envelope.rs reject_faces +
    ///    验签序链路常驻执法）；
    /// ④ 换钥负样例：第二对钥的 pk → 验拒（信封 pk ≠ 可信 pk）。
    /// 「跨 lineage」拒面在 verify_envelope 常驻执法（manifest 对账）。
    /// bap 载荷用合成字节：信封覆盖域 = 全字节摘要 + manifest 关键域，与
    /// cpio 容器结构无关；容器级集成（mkapp 打包 → sign → verify）由
    /// mkapp `--sign-key`/`--verify-key` CLI 实跑覆盖（P1 验证记录在案）。
    /// 测试密钥纪律：全走 /tmp 临时目录，退出即清场，不落仓。
    fn selftest() -> Result<String, String> {
        let dir = std::env::temp_dir().join(format!("berry-sign-selftest-{}", std::process::id()));
        fs::create_dir_all(&dir).map_err(|e| format!("selftest: 建临时目录失败：{e}"))?;
        let result = selftest_body(&dir);
        let _ = fs::remove_dir_all(&dir); // dev 纪律：测试钥随临时目录清场
        match result {
            Ok(0) => Ok("selftest: ok（验收线四件全过）".to_string()),
            Ok(n) => Err(format!("selftest: FAIL x{n}")),
            Err(e) => Err(format!("selftest: {e}")),
        }
    }

    fn selftest_body(dir: &std::path::Path) -> Result<usize, String> {
        // Path → &str（生命周期随输入；自检路径自造恒 UTF-8）。
        fn ps(p: &std::path::Path) -> &str {
            p.to_str().unwrap_or("<非 UTF-8 路径>")
        }
        let mut fails = 0usize;
        let sk_p = dir.join("t.sk");
        let pk_p = dir.join("t.pk");
        cmd_keygen(ps(&sk_p), ps(&pk_p))?;
        // sk 文件权限钉板（0600 面——dev 纪律执法锚）。
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let md = fs::metadata(&sk_p).map_err(|e| format!("selftest: sk 元数据失败：{e}"))?;
            if md.permissions().mode() & 0o777 != 0o600 {
                println!("  selftest FAIL: keygen-sk-mode {:o} != 600", md.permissions().mode() & 0o777);
                fails += 1;
            }
        }
        let sk = read_exact32(ps(&sk_p), "selftest")?;
        let pk = read_exact32(ps(&pk_p), "selftest")?;
        // 测试 manifest（[security] 三键 + [compat] 三值——关键域全覆盖）。
        let mf = b"[app]\n\
                   id = \"P1SELFTEST\"\n\
                   version = \"0.1.0\"\n\
                   entry = \"bin/p1selftest\"\n\
                   type = \"app\"\n\
                   intent = \"utility\"\n\
                   \n[compat]\n\
                   min = \"0.1.0\"\n\
                   target = \"0.1.0\"\n\
                   compile = \"0.1.0\"\n\
                   \n[capabilities]\n\
                   console = true\n\
                   \n[security]\n\
                   profile = \"ed25519-main\"\n\
                   key_lineage = \"p1-selftest\"\n\
                   integrator = \"berryos-dev\"\n";
        // 合成 bap 载荷（头注声明：全字节摘要面，容器无关）。
        let bap: Vec<u8> = std::iter::successors(Some(0x5au8), |x| Some(x.wrapping_mul(3).wrapping_add(7)))
            .take(4096)
            .collect();
        // ① 正样例：签 → 验过 + 字段对账。
        let sig = sign_envelope(&sk, &bap, mf, 1).map_err(|e| format!("签发失败：{e}"))?;
        match verify_envelope(&pk, &bap, mf, &sig) {
            Ok(o) if o.lineage == "p1-selftest" && o.gen == 1 && o.chain_len == 0 => {
                println!("  selftest 过（预期） : sig-pos sign→verify 过 lineage=p1-selftest gen=1");
            }
            other => {
                println!("  selftest FAIL: sig-pos 验签/字段对账失败 {other:?}");
                fails += 1;
            }
        }
        // ② 篡改负样例（bap 改一字节）。
        let mut bap2 = bap.clone();
        bap2[100] ^= 0x01;
        if verify_envelope(&pk, &bap2, mf, &sig).is_err() {
            println!("  selftest 拒（预期） : sig-neg-tamper bap 改一字节拒");
        } else {
            println!("  selftest FAIL: sig-neg-tamper 被放行");
            fails += 1;
        }
        // ③ 断链负样例（.sig 截断——信封 BadLen 单一解释面）。
        let sig_trunc = &sig[..sig.len() - 1];
        if verify_envelope(&pk, &bap, mf, sig_trunc).is_err() {
            println!("  selftest 拒（预期） : sig-neg-trunc .sig 截断拒");
        } else {
            println!("  selftest FAIL: sig-neg-trunc 被放行");
            fails += 1;
        }
        // ④ 换钥负样例（错 pk——第二对钥）。
        let sk2_p = dir.join("t2.sk");
        let pk2_p = dir.join("t2.pk");
        cmd_keygen(ps(&sk2_p), ps(&pk2_p))?;
        let pk2 = read_exact32(ps(&pk2_p), "selftest")?;
        if verify_envelope(&pk2, &bap, mf, &sig).is_err() {
            println!("  selftest 拒（预期） : sig-neg-wrong-pk 错 pk 拒");
        } else {
            println!("  selftest FAIL: sig-neg-wrong-pk 被放行");
            fails += 1;
        }
        // ⑤ sign-raw/verify-raw 腿（#77 索引签名刀：RFC 8032 §7.1 官方向量
        //    抽查 TEST 1〔空消息 0B 边界〕+ TEST 2〔1B 消息〕——CLI 同路
        //    直调，产物对官方 sig 逐字节钉板 + 篡改/换钥负）。
        fails += selftest_sign_raw(dir)?;
        // ⑥ admit 腿（P3-0 件二在制件：三档表/防剥离/TOFU/sideload/
        //    水位回退/诚实降级/损坏账本保守拒——lib admit 单源的 CLI 面）。
        fails += selftest_admit(dir)?;
        Ok(fails)
    }

    /// selftest 断言腿（gensourceindex selftest `leg` 同形）。
    fn leg(ok: bool, label: &str, fails: &mut usize) {
        println!("  selftest {} : {label}", if ok { "过（预期）" } else { "FAIL" });
        if !ok {
            *fails += 1;
        }
    }

    /// hex 串 → 定长字节数组（selftest 向量钉板用；长度不符 = loud）。
    fn unhex<const N: usize>(s: &str) -> Result<[u8; N], String> {
        if s.len() != N * 2 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("selftest: 坏 hex 向量（长 {}，应 {} hex）", s.len(), N * 2));
        }
        let mut out = [0u8; N];
        for (i, b) in out.iter_mut().enumerate() {
            *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
                .map_err(|e| format!("selftest: hex 解码拒：{e}"))?;
        }
        Ok(out)
    }

    /// sign-raw/verify-raw 自检腿（#77 索引签名刀）：RFC 8032 §7.1 官方
    /// 向量抽查——TEST 1（空消息 0B 边界）+ TEST 2（1B 消息 0x72），sk/
    /// msg 落盘走 CLI 同路（cmd_sign_raw/cmd_verify_raw 直调，不 spawn），
    /// `--out` 产物对官方 sig 逐字节钉板（vectors.rs RFC8032_ED25519 表
    /// 同源向量——host bin 面抽查，全量门在 lib vectors 模块）；负样例 =
    /// 篡改消息拒 + 换钥拒。测试钥纪律：向量的 sk 是公开测试向量（RFC
    /// 官方公开值），临时文件随目录清场。
    fn selftest_sign_raw(dir: &std::path::Path) -> Result<usize, String> {
        fn ps(p: &std::path::Path) -> &str {
            p.to_str().unwrap_or("<非 UTF-8 路径>")
        }
        let mut fails = 0usize;
        // (名, sk hex, pk hex, msg, sig hex)——RFC 8032 §7.1 TEST 1/2 官方值。
        let cases: &[(&str, &str, &str, &[u8], &str)] = &[
            (
                "TEST 1",
                "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
                "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
                b"",
                "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155\
                 5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
            ),
            (
                "TEST 2",
                "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb",
                "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c",
                b"\x72",
                "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69d\
                 a085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00",
            ),
        ];
        for (name, sk_hex, pk_hex, msg, sig_hex) in cases {
            let tag = name.replace(' ', "-");
            let sk_p = dir.join(format!("sr-{tag}.sk"));
            let pk_chk: [u8; 32] = unhex(pk_hex)?;
            let want: [u8; 64] = unhex(sig_hex)?;
            let sk: [u8; 32] = unhex(sk_hex)?;
            write_secret(ps(&sk_p), &sk)?;
            let msg_p = dir.join(format!("sr-{tag}.msg"));
            fs::write(&msg_p, msg).map_err(|e| format!("selftest: msg 写失败：{e}"))?;
            let out_p = dir.join(format!("sr-{tag}.sig"));
            cmd_sign_raw(ps(&sk_p), ps(&msg_p), Some(ps(&out_p)))?;
            let got = fs::read(&out_p).map_err(|e| format!("selftest: sig 读失败：{e}"))?;
            leg(got == want, &format!("sign-raw-{tag} RFC 8032 {name} 钉板（64B 逐字节）"),
                &mut fails);
            // pk 派生对拍（sk → public_key == 官方 pk）+ verify-raw 正样例。
            leg(ed25519::public_key(&sk) == pk_chk,
                &format!("sign-raw-{tag} sk→pk 派生 == 官方向量 pk"), &mut fails);
            let pk_p = dir.join(format!("sr-{tag}.vpk"));
            fs::write(&pk_p, pk_chk).map_err(|e| format!("selftest: pk 写失败：{e}"))?;
            leg(cmd_verify_raw(ps(&pk_p), ps(&msg_p), ps(&out_p)).is_ok(),
                &format!("verify-raw-{tag} 正样例过"), &mut fails);
            // 篡改负（msg 改一字节——空消息臂换 1B 新消息替代翻转）。
            let msg2 = if msg.is_empty() { b"x".to_vec() } else {
                let mut m = msg.to_vec();
                m[0] ^= 0x01;
                m
            };
            let msg2_p = dir.join(format!("sr-{tag}.msg2"));
            fs::write(&msg2_p, &msg2).map_err(|e| format!("selftest: msg2 写失败：{e}"))?;
            leg(cmd_verify_raw(ps(&pk_p), ps(&msg2_p), ps(&out_p)).is_err(),
                &format!("verify-raw-{tag}-tamper 消息篡改拒"), &mut fails);
            // 换钥负（错 pk）。
            let bad_pk_p = dir.join(format!("sr-{tag}.badpk"));
            let mut bad_pk = pk_chk;
            bad_pk[0] ^= 0x01;
            fs::write(&bad_pk_p, bad_pk).map_err(|e| format!("selftest: badpk 写失败：{e}"))?;
            leg(cmd_verify_raw(ps(&bad_pk_p), ps(&msg_p), ps(&out_p)).is_err(),
                &format!("verify-raw-{tag}-wrong-pk 错 pk 拒"), &mut fails);
        }
        Ok(fails)
    }

    /// 单条目 manifest.toml cpio(newc) 容器（cpio_manifest 的写侧对拍
    /// ——自写自读回环；几何 = 头 110B + 名垫 4096 + 数据垫 4096）。
    fn cpio_pack(payload: &[u8]) -> Vec<u8> {
        let name = b"manifest.toml\0";
        let mut out = Vec::new();
        out.extend_from_slice(b"070701");
        for i in 0..13 {
            let v = if i == 6 { payload.len() } else if i == 11 { name.len() } else { 0 };
            out.extend_from_slice(format!("{v:08x}").as_bytes());
        }
        out.extend_from_slice(name);
        while out.len() % 4096 != 0 {
            out.push(0);
        }
        out.extend_from_slice(payload);
        while out.len() % 4096 != 0 {
            out.push(0);
        }
        out
    }

    /// admit 自检腿（设计 §二 DoD 场景的 bin 内制件——E2E 全谱在
    /// tools/p3sim.py，此处最短路径钉决策语义）。
    fn selftest_admit(dir: &std::path::Path) -> Result<usize, String> {
        fn ps(p: &std::path::Path) -> &str {
            p.to_str().unwrap_or("<非 UTF-8 路径>")
        }
        let mut fails = 0usize;
        // 密钥 + 信封（dev 纪律：临时目录，退出清场）。
        let sk_p = dir.join("a.sk");
        let pk_p = dir.join("a.pk");
        cmd_keygen(ps(&sk_p), ps(&pk_p))?;
        let sk = read_exact32(ps(&sk_p), "selftest-admit")?;
        let pk = read_exact32(ps(&pk_p), "selftest-admit")?;
        let mut pkhex = String::with_capacity(64);
        for b in pk {
            let _ = write!(pkhex, "{b:02x}");
        }
        // bap 基件（manifest 三键 lineage=p3-selftest，version 0.1.0）。
        let mf = b"[app]\n\
                    id = \"P3ADMIT\"\n\
                    version = \"0.1.0\"\n\
                    entry = \"bin/p3admit\"\n\
                    type = \"app\"\n\
                    intent = \"utility\"\n\
                    \n[compat]\n\
                    min = \"0.1.0\"\n\
                    target = \"0.1.0\"\n\
                    compile = \"0.1.0\"\n\
                    \n[capabilities]\n\
                    console = true\n\
                    \n[security]\n\
                    profile = \"ed25519-main\"\n\
                    key_lineage = \"p3-selftest\"\n\
                    integrator = \"berryos-dev\"\n";
        let bap = cpio_pack(mf);
        let sig = sign_envelope(&sk, &bap, mf, 3)?;
        let sig_old = sign_envelope(&sk, &bap, mf, 1)?; // gen 回退件
        // 布景文件集。
        fn setup(dir: &std::path::Path, name: &str, files: &[(&str, Vec<u8>)]) -> String {
            let d = dir.join(name);
            std::fs::create_dir_all(&d).unwrap();
            for (f, data) in files {
                std::fs::write(d.join(f), data).unwrap();
            }
            d.to_str().unwrap().to_string()
        }
        let pol = |tier: &str| -> String {
            let p = dir.join(format!("policy-{tier}.toml"));
            std::fs::write(&p, format!("# load-policy v1\nschema = 1\ntier = \"{tier}\"\n")).unwrap();
            p.to_str().unwrap().to_string()
        };
        let pol_dev = pol("dev");
        let pol_open = pol("product-open");
        let pol_lock = pol("product-locked");
        let led_base = format!(
            "schema = 1\n\n[[lineage]]\nid = \"p3-selftest\"\nanchors = [\n  {{ pk = \"{pkhex}\", type = \"system\" }},\n]\ngen_max = 3\nver_max = 0\nkind = \"\"\nissued_by = \"\"\ncert_status = \"\"\nlast_revision = 0\n"
        );
        let led_p = dir.join("led.toml");
        std::fs::write(&led_p, &led_base).unwrap();
        let led = led_p.to_str().unwrap().to_string();
        // DoD 1：正样例（locked 也过——验签面与档位无关）。
        let d_pos = setup(dir, "apps-pos", &[("P3ADMIT.bap", bap.clone()), ("P3ADMIT.bap.sig", sig.clone())]);
        let r = run_admit(&pol_lock, &led, &d_pos, &[], false)?;
        leg(r.rejects == 0 && r.lines.iter().any(|l| l.contains("allow verified lineage=p3-selftest gen=3")),
            "admit-pos locked 正样例过 + 水位不降（gen 等值）", &mut fails);
        // DoD 2 三负（bin 面）：篡改/截断。
        let mut bap_t = bap.clone();
        bap_t[5000] ^= 0x01;
        let d_tamper = setup(dir, "apps-tamper", &[("P3ADMIT.bap", bap_t), ("P3ADMIT.bap.sig", sig.clone())]);
        let r = run_admit(&pol_lock, &led, &d_tamper, &[], false)?;
        leg(r.allows == 0 && r.lines.iter().any(|l| l.contains("reject") && l.contains("top-sig-bad")),
            "admit-neg-tamper 篡改载荷拒（top-sig-bad）", &mut fails);
        let d_trunc = setup(dir, "apps-trunc", &[("P3ADMIT.bap", bap.clone()), ("P3ADMIT.bap.sig", sig[..sig.len() - 1].to_vec())]);
        let r = run_admit(&pol_lock, &led, &d_trunc, &[], false)?;
        leg(r.allows == 0 && r.lines.iter().any(|l| l.contains("bad-len")),
            "admit-neg-trunc .sig 截断拒（bad-len——断链负样例信封层）", &mut fails);
        // DoD 5：无 .sig 三档（dev/open 降级 loud、locked 拒）。
        let d_uns = setup(dir, "apps-uns", &[("P3ADMIT.bap", bap.clone())]);
        let r = run_admit(&pol_dev, &led, &d_uns, &[], false)?;
        leg(r.allows == 1 && r.lines.iter().any(|l| l.contains("allow degrade eco: app P3ADMIT unsigned")),
            "admit-tier-dev 无 .sig 降级 + loud（eco unsigned）", &mut fails);
        let r = run_admit(&pol_open, &led, &d_uns, &[], false)?;
        leg(r.allows == 1 && r.lines.iter().any(|l| l.contains("allow degrade")),
            "admit-tier-open 无 .sig 降级 + loud", &mut fails);
        let r = run_admit(&pol_lock, &led, &d_uns, &[], false)?;
        leg(r.rejects == 1 && r.lines.iter().any(|l| l.contains("reject eco: app P3ADMIT nosig")),
            "admit-tier-locked 无 .sig 拒载 + 显式错误（nosig）", &mut fails);
        // DoD 4：防剥离（记录在案已签 id 无 .sig → 三档同拒）。
        let led_strip_p = dir.join("led-strip.toml");
        std::fs::write(&led_strip_p, format!("{led_base}[[app]]\nid = \"P3ADMIT\"\npk = \"{pkhex}\"\nsource = \"builtin\"\nboots = 1\nver_max = 0\n")).unwrap();
        let led_strip = led_strip_p.to_str().unwrap().to_string();
        for (name, p) in [("dev", &pol_dev), ("open", &pol_open), ("locked", &pol_lock)] {
            let r = run_admit(p, &led_strip, &d_uns, &[], false)?;
            leg(r.allows == 0 && r.lines.iter().any(|l| l.contains("stripped")),
                &format!("admit-anti-strip-{name} 防剥离恒拒（stripped）"), &mut fails);
        }
        // DoD 6 前半：TOFU 首见（dev、未知 lineage p3-other）+ emit 落账。
        let mf2 = b"[app]\n\
                    id = \"P3OTHER\"\n\
                    version = \"0.1.0\"\n\
                    entry = \"bin/p3other\"\n\
                    type = \"app\"\n\
                    intent = \"utility\"\n\
                    \n[compat]\n\
                    min = \"0.1.0\"\n\
                    target = \"0.1.0\"\n\
                    compile = \"0.1.0\"\n\
                    \n[capabilities]\n\
                    console = true\n\
                    \n[security]\n\
                    profile = \"ed25519-main\"\n\
                    key_lineage = \"p3-other\"\n\
                    integrator = \"berryos-dev\"\n";
        let bap2 = cpio_pack(mf2);
        let sig2 = sign_envelope(&sk, &bap2, mf2, 1)?;
        let d_tofu = setup(dir, "apps-tofu", &[("P3OTHER.bap", bap2.clone()), ("P3OTHER.bap.sig", sig2.clone())]);
        let r = run_admit(&pol_dev, &led, &d_tofu, &[], false)?;
        let tofu_ok = r.allows == 1 && r.lines.iter().any(|l| l.contains("allow tofu-first-seen lineage=p3-other"));
        leg(tofu_ok, "admit-tofu dev 首见 tofu-first-seen", &mut fails);
        // 增量账本落账（tofu 锚 + per-app 四字段）——折叠账本 emit →
        // 文件（canonical），再按文件断言（emit 面同产线）。
        let mut ebuf = [0u8; EMIT_BUF];
        let en = r.ledger.emit(&mut ebuf).map_err(|e| format!("selftest: emit 拒 {e:?}"))?;
        let out_p = dir.join("led-tofu.toml");
        std::fs::write(&out_p, &ebuf[..en]).map_err(|e| format!("selftest: 写失败：{e}"))?;
        let emitted = String::from_utf8_lossy(&ebuf[..en]).into_owned();
        let tofu_ledger_ok = emitted.contains("id = \"p3-other\"") && emitted.contains("type = \"tofu\"")
            && emitted.contains("[[app]]") && emitted.contains("id = \"P3OTHER\"")
            && emitted.contains("source = \"builtin\"");
        leg(tofu_ledger_ok, "admit-tofu 增量账本落盘（tofu 锚 + per-app 四字段）", &mut fails);
        // 首见后二次装载：同件 now verified（非再 TOFU）。
        let out_s = out_p.to_str().unwrap().to_string();
        let r2 = run_admit(&pol_dev, &out_s, &d_tofu, &[], false)?;
        leg(r2.allows == 1 && r2.lines.iter().any(|l| l.contains("allow verified lineage=p3-other")),
            "admit-tofu 二次装载 verified（首见即锚后）", &mut fails);
        // G9（2026-11-12）lineage↔pk 绑定执法：同 lineage 零链异 pk 拒
        // （换钥走仪式不走 TOFU）+ 拒因行带 lineage/两 pk 前缀——dev/
        // locked 两档钉 CLI 面（open 同路，全档穷举在 lib 测试 11）。
        let sk_b_p = dir.join("b.sk");
        let pk_b_p = dir.join("b.pk");
        cmd_keygen(ps(&sk_b_p), ps(&pk_b_p))?;
        let sk_b = read_exact32(ps(&sk_b_p), "selftest-admit")?;
        let pk_b = read_exact32(ps(&pk_b_p), "selftest-admit")?;
        let sig_ks = sign_envelope(&sk_b, &bap, mf, 3)?;
        let d_ks = setup(dir, "apps-keyswap", &[("P3ADMIT.bap", bap.clone()), ("P3ADMIT.bap.sig", sig_ks.clone())]);
        for (name, p) in [("dev", &pol_dev), ("locked", &pol_lock)] {
            let r = run_admit(p, &led, &d_ks, &[], false)?;
            leg(r.allows == 0
                && r.lines.iter().any(|l| l.contains("reject eco: app P3ADMIT key-swap")
                    && l.contains("lineage=p3-selftest")
                    && l.contains(&format!("env-pk={}", hex(&pk_b[..4])))
                    && l.contains(&format!("anchor-pk={}", hex(&pk[..4])))),
                &format!("admit-key-swap-{name} 同 lineage 异 pk 拒 + lineage/两 pk 前缀可见"),
                &mut fails);
        }
        // DoD 5 后半：未知 lineage 带签——open/locked unknown-src 拒。
        for (name, p) in [("open", &pol_open), ("locked", &pol_lock)] {
            let r = run_admit(p, &led, &d_tofu, &[], false)?;
            leg(r.allows == 0 && r.lines.iter().any(|l| l.contains("unknown-src")),
                &format!("admit-unknown-src-{name} 未知 lineage 拒（unknown-src）"), &mut fails);
        }
        // DoD 7：sideload 开关（locked 拒 + dev 装 + 记录来源）。
        let d_side = setup(dir, "apps-side", &[("P3ADMIT.bap", bap.clone()), ("P3ADMIT.bap.sig", sig.clone())]);
        let r = run_admit(&pol_lock, &led, &d_side, &["P3ADMIT"], false)?;
        leg(r.allows == 0 && r.lines.iter().any(|l| l.contains("reject eco: app P3ADMIT sideload")),
            "admit-sideload-off 关 = 拒装 + 显式错误", &mut fails);
        let r = run_admit(&pol_dev, &led, &d_side, &["P3ADMIT"], false)?;
        leg(r.allows == 1, "admit-sideload-on 开 = 装", &mut fails);
        let side_rec = r.ledger.find_app(b"P3ADMIT");
        leg(side_rec.is_some() && side_rec.unwrap().source == AppSource::Sideload,
            "admit-sideload-on 账本记录来源 sideload（四字段）", &mut fails);
        // DoD 3：gen 回退拒 + DoD 8：--no-increment 诚实降级（同件放行 +
        // 可见行）。
        let d_rb = setup(dir, "apps-rb", &[("P3ADMIT.bap", bap.clone()), ("P3ADMIT.bap.sig", sig_old.clone())]);
        let r = run_admit(&pol_lock, &led, &d_rb, &[], false)?;
        leg(r.allows == 0 && r.lines.iter().any(|l| l.contains("gen-rollback")),
            "admit-gen-rollback gen 回降拒（check_gen 单源）", &mut fails);
        let r = run_admit(&pol_lock, &led, &d_rb, &[], true)?;
        leg(r.allows == 1 && r.lines.iter().any(|l| l.contains("回放防护未生效")),
            "admit-no-increment 水位跳检 + 「回放防护未生效」可见行", &mut fails);
        // DoD 9：损坏增量保守拒（截断/坏枚举）。
        let led_bad_p = dir.join("led-bad.toml");
        std::fs::write(&led_bad_p, led_base[..led_base.len() / 2].as_bytes()).unwrap();
        let led_bad = led_bad_p.to_str().unwrap().to_string();
        leg(run_admit(&pol_lock, &led_bad, &d_pos, &[], false).is_err(),
            "admit-ledger-corrupt 截断账本保守拒（loud）", &mut fails);
        let led_enum_p = dir.join("led-enum.toml");
        std::fs::write(&led_enum_p, led_base.replace("type = \"system\"", "type = \"vendor\"")).unwrap();
        let led_enum = led_enum_p.to_str().unwrap().to_string();
        leg(run_admit(&pol_lock, &led_enum, &d_pos, &[], false).is_err(),
            "admit-ledger-corrupt 坏枚举保守拒（UnknownAnchorType）", &mut fails);
        // 回 bootseed 重导路径：换回 bootseed 跑同目录 = 过。
        let r = run_admit(&pol_lock, &led, &d_pos, &[], false)?;
        leg(r.rejects == 0, "admit-bootseed-reimport 回 bootseed 重导路径过", &mut fails);
        Ok(fails)
    }

    // ---- admit：P3-0 件二——目录装载裁决（lib admit 单源消费）----

    /// admit 一次跑的产物（决策行 + 计数 + 折叠后账本——selftest 腿断言面）。
    struct AdmitRun {
        lines: Vec<String>,
        allows: usize,
        rejects: usize,
        ledger: TrustLedger,
    }

    /// admit 用法/schema 自述 banner（G10，38 号 §4——policy/ledger 两件
    /// schema 此前无 CLI 自述，OEM 侧三连试错 UnknownKey→NoLineage→
    /// V4NonEmpty 耗 ~48s）：schema 真源 = lib `admit::parse_policy`
    /// （两键闭集）+ `ledger.rs` 头注例样，本 banner 只是摘要——**同步
    /// 义务**：lib 侧改 schema 须同笔改此处示例。
    fn admit_help_text() -> &'static str {
        "admit 用法: berry-sign admit <policy.toml> <ledger.toml> <dir>\n\
         \x20           [--emit-ledger <out>] [--sideload <id>]… [--no-increment]\n\
         合法 policy 形（load-policy v1——lib admit::parse_policy 单源，两键闭集）:\n\
         \x20 # load-policy v1\n\
         \x20 schema = 1        # ≠1 硬拒；未知键硬拒\n\
         \x20 tier = \"dev\"      # 三档: dev | product-open | product-locked\n\
         合法 ledger 形（trust-ledger v1——lib ledger.rs 头注例样的摘要，节键全必填）:\n\
         \x20 schema = 1\n\
         \x20 [[lineage]]       # 节键闭集: id/anchors/gen_max/ver_max/kind/issued_by/cert_status/last_revision\n\
         \x20 id = \"upstream\"   # 1..=16B [a-z0-9-]\n\
         \x20 anchors = [ { pk = \"<64 hex>\", type = \"system\" } ]   # type ∈ tofu|system|ca|oem-system\n\
         \x20 gen_max = 3       # 单调水位（回降拒）；ver_max = 0\n\
         \x20 kind = \"\"         # V4 四占位（kind/issued_by/cert_status/last_revision）v1 恒空——非空保守拒\n\
         \x20 [[app]]           # 可选 per-app 记录五键: id/pk/source(builtin|sideload)/boots/ver_max\n\
         拒面: 未知键/坏枚举/损坏件 = 保守拒 loud（exit 2）——schema 真源在 lib，本 banner 仅自述"
    }

    fn cmd_admit_main(rest: &[&str]) -> ExitCode {
        // G10：schema 自述面——--help/-h 首位即打 banner（零副作用零 IO）。
        if matches!(rest.first(), Some(&"--help") | Some(&"-h")) {
            println!("{}", admit_help_text());
            return ExitCode::SUCCESS;
        }
        match cmd_admit(rest) {
            Ok((footer, rejected)) => {
                println!("{footer}");
                if rejected {
                    ExitCode::FAILURE // 1 = 有拒载（决策行已出全——拒载不连坐）
                } else {
                    ExitCode::SUCCESS
                }
            }
            Err(e) => {
                eprintln!("berry-sign: {e}");
                ExitCode::from(2) // 用法/IO/账本损坏（保守拒）
            }
        }
    }

    /// admit 参数面：`<policy.toml> <ledger.toml> <dir>` + 可选旗标
    /// `--emit-ledger <out>`（折叠后账本 canonical emit）/
    /// `--sideload <id>`（可重复——id 集 = sideload 来源，余 builtin）/
    /// `--no-increment`（增量账本缺席申报——水位跳检 + 「回放防护未
    /// 生效」诚实声明行，§1.3.2/§1.4）。
    fn cmd_admit(rest: &[&str]) -> Result<(String, bool), String> {
        let mut pos: Vec<&str> = Vec::new();
        let mut emit: Option<&str> = None;
        let mut sideload: Vec<&str> = Vec::new();
        let mut no_increment = false;
        let mut it = rest.iter();
        while let Some(a) = it.next() {
            match *a {
                "--emit-ledger" => {
                    emit = Some(*it.next().ok_or("admit: --emit-ledger 缺参数")?);
                }
                "--sideload" => {
                    sideload.push(*it.next().ok_or("admit: --sideload 缺参数")?);
                }
                "--no-increment" => no_increment = true,
                other if other.starts_with("--") => {
                    return Err(format!("admit: 未知旗标 {other:?}（schema 示例: berry-sign admit --help）"));
                }
                other => pos.push(other),
            }
        }
        if pos.len() != 3 {
            return Err(
                "admit: 参数不合规（<policy.toml> <ledger.toml> <dir> + 可选 \
                 --emit-ledger/--sideload/--no-increment；两件 schema 示例: \
                 berry-sign admit --help）"
                    .to_string(),
            );
        }
        let run = run_admit(pos[0], pos[1], pos[2], &sideload, no_increment)?;
        for l in &run.lines {
            println!("{l}");
        }
        if let Some(out_path) = emit {
            let mut buf = [0u8; EMIT_BUF];
            let n = run
                .ledger
                .emit(&mut buf)
                .map_err(|e| format!("admit: 账本 emit 拒 {e:?}"))?;
            fs::write(out_path, &buf[..n])
                .map_err(|e| format!("admit: 写 {out_path}: {e}"))?;
            println!("admit: 增量账本 emit → {out_path}（canonical TOML）");
        }
        let footer = format!(
            "admit: done apps={} allow={} reject={} policy={}",
            run.allows + run.rejects,
            run.allows,
            run.rejects,
            run_policy_word(pos[0])?,
        );
        Ok((footer, run.rejects > 0))
    }

    /// 策略档词回读（footer 面；解析已在 run_admit 单源过门，此处仅复检）。
    fn run_policy_word(policy_path: &str) -> Result<&'static str, String> {
        let src = fs::read(policy_path).map_err(|e| format!("admit: 读 {policy_path}: {e}"))?;
        let p = admit::parse_policy(&src).map_err(|e| format!("admit: 策略件拒 {e:?}"))?;
        Ok(p.word())
    }

    /// 装载裁决主循环：策略件/账本解析（损坏 = 保守拒 loud）→ 目录
    /// `*.bap` 字节序逐件 admit → 折叠账本（单写者语义——设备面 appd
    /// 同款）。
    fn run_admit(
        policy_path: &str,
        ledger_path: &str,
        dir_path: &str,
        sideload: &[&str],
        no_increment: bool,
    ) -> Result<AdmitRun, String> {
        let policy_src =
            fs::read(policy_path).map_err(|e| format!("admit: 读 {policy_path}: {e}"))?;
        let policy = admit::parse_policy(&policy_src)
            .map_err(|e| format!("admit: 策略件拒 {e:?}（{policy_path}——保守拒）"))?;
        let ledger_src =
            fs::read(ledger_path).map_err(|e| format!("admit: 读 {ledger_path}: {e}"))?;
        // 损坏增量 = 保守拒（DoD 9：不装不评 loud；回 bootseed 重导路径 =
        // 调用方换 <ledger.toml>（bootseed）重跑）。
        let ledger = TrustLedger::parse(&ledger_src)
            .map_err(|e| format!("admit: 账本拒 {e:?}（{ledger_path}——保守拒）"))?;
        let mut names: Vec<String> = fs::read_dir(dir_path)
            .map_err(|e| format!("admit: 读目录 {dir_path}: {e}"))?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".bap"))
            .collect();
        names.sort(); // 确定性：文件名字节序（固定种子面）
        let ctx = AdmitCtx { policy, replay_guard: !no_increment };
        let mut cur = ledger;
        let mut lines = Vec::new();
        if no_increment {
            lines.push(
                "admit: 注: 增量账本缺席——回放防护未生效（水位跳检；诚实降级，设计 §1.3.2/§1.4）"
                    .to_string(),
            );
        }
        let (mut allows, mut rejects) = (0usize, 0usize);
        for name in &names {
            let path = format!("{dir_path}/{name}");
            let bap = fs::read(&path).map_err(|e| format!("admit: 读 {path}: {e}"))?;
            let sig_path = format!("{path}.sig");
            let sig = match fs::read(&sig_path) {
                Ok(s) => Some(s),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(format!("admit: 读 {sig_path}: {e}")),
            };
            let mf = cpio_manifest(&bap).map_err(|e| format!("admit: {name}: {e}"))?;
            let m = berry_manifest::parse(mf)
                .map_err(|e| format!("admit: {name}: manifest 拒：{}", e.reason_str()))?;
            let lineage = m.security.map(|s| s.key_lineage).unwrap_or("");
            let ver = ver_triple(m.version).map_err(|e| format!("admit: {name}: {e}"))?;
            let bd = digest256(&bap);
            // manifest 关键域摘要：仅签载路径需要（berry-manifest 单源派生）。
            let md = if sig.is_some() {
                digest256(&key_fields_bytes(&m).map_err(|e| format!("admit: {name}: {e}"))?)
            } else {
                [0; 32]
            };
            let source = if sideload.contains(&m.id) {
                AppSource::Sideload
            } else {
                AppSource::Builtin
            };
            let face = LoadFace {
                id: m.id.as_bytes(),
                lineage: lineage.as_bytes(),
                ver,
                bap_digest: bd,
                manifest_digest: md,
                sig: sig.as_deref(),
                source,
            };
            let id = m.id.to_string();
            match admit::admit(&ctx, &cur, &face) {
                Verdict::Allow(note) => {
                    allows += 1;
                    lines.push(render_allow(&id, lineage, &note));
                }
                Verdict::Mutate { note, ledger } => {
                    allows += 1;
                    cur = ledger; // 单写者折叠
                    lines.push(render_allow(&id, lineage, &note));
                }
                Verdict::Reject(r) => {
                    rejects += 1; // 拒载不连坐（失败隔离——继续下一件）
                    let mut line = format!("admit: {id} reject eco: app {id} {}", r.word());
                    // G9 拒因可见性：换钥拒行附 lineage + 两 pk 前缀
                    // （信封当前钥 vs 该 lineage 首锚——结构单源 = lib
                    // KeySwap，本行只渲染；env pk 经信封单解释器 parse，
                    // 锚经折叠中账本 find_lineage，无第二语义源）。
                    if r == Reason::Policy(PolicyErr::KeySwap) {
                        if let (Some(s), Some(ent)) =
                            (sig.as_deref(), cur.find_lineage(lineage.as_bytes()))
                        {
                            if let Ok(env) = SigEnvelopeV1::parse(s) {
                                let _ = write!(
                                    line,
                                    " lineage={lineage} env-pk={} anchor-pk={}（同 lineage 换钥：零链且 pk ∉ 锚集——换钥走轮换链仪式，不走 TOFU）",
                                    hex(&env.pk[..4]),
                                    hex(&ent.anchors()[0].pk[..4])
                                );
                            }
                        }
                    }
                    lines.push(line);
                }
            }
        }
        Ok(AdmitRun { lines, allows, rejects, ledger: cur })
    }

    /// allow 决策行（loud 语义面——降级行 = eco 家族，§1.3.2）。
    fn render_allow(id: &str, lineage: &str, note: &Note) -> String {
        match note {
            Note::Verified { gen } => {
                format!("admit: {id} allow verified lineage={lineage} gen={gen}")
            }
            Note::TofuFirstSeen { gen } => {
                format!("admit: {id} allow tofu-first-seen lineage={lineage} gen={gen}")
            }
            Note::DegradeUnsigned => {
                format!("admit: {id} allow degrade eco: app {id} unsigned")
            }
        }
    }

    /// 三值版本 `x.y.z` → 可比较 u32（`(maj*1000+min)*1000+patch`，段域
    /// 0..=999 无前导零——mkapp `VER_RE` 域同源；manifest 自由串在装载
    /// 面收窄，gensourceindex `ver_triple` 的 Rust 对读）。
    fn ver_triple(s: &str) -> Result<u32, String> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(format!("版本 {s:?} 非三值文法 x.y.z（水位比较前置）"));
        }
        let mut v = 0u32;
        for p in parts {
            if p.is_empty()
                || p.len() > 3
                || !p.bytes().all(|b| b.is_ascii_digit())
                || (p.len() > 1 && p.starts_with('0'))
            {
                return Err(format!("版本段 {p:?} 越域（0..=999 无前导零）"));
            }
            let seg: u32 =
                p.parse().map_err(|_| format!("版本段 {p:?} 解析拒"))?;
            v = v * 1000 + seg;
        }
        Ok(v)
    }

    fn hex(b: &[u8]) -> String {
        let mut s = String::with_capacity(b.len() * 2);
        for x in b {
            let _ = write!(s, "{x:02x}");
        }
        s
    }

}

#[cfg(not(target_os = "none"))]
fn main() -> std::process::ExitCode {
    cli::main()
}

// 空壳面的 no_std 义务:panic_handler(不可达——none 下本 bin 无运行
// 语义,仅为整仓构建编译检查通过性存在;宿主实编不携本函数)。
#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
