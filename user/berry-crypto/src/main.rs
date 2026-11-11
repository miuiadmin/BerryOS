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
//! - `berry-sign selftest` —— v1.2 §九验收线 1 四件自检（正样例 +
//!   篡改/断链/换钥三负样例；宿主最短路径归属，见 selftest 头注）。
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
        rot_message, top_message, ChainNode, SigEnvelopeV1, ALG_ED25519_SHA512_256, CHAIN_NODES_MAX,
        ENVELOPE_MAX, LINEAGE_MAX, ROT_MSG_MAX, SIG_LEN, TOP_MSG_MAX,
    };
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
            ["verify", pk, bap] => run(cmd_verify(pk, bap, &format!("{bap}.sig"))),
            ["verify", pk, bap, sig] => run(cmd_verify(pk, bap, sig)),
            ["selftest"] => run(selftest()),
            _ => {
                eprintln!(
                    "berry-sign: 参数不合规\n\
                     用法: berry-sign keygen <sk-out> <pk-out>\n\
                     \x20     berry-sign sign <sk> <bap> [--gen N]\n\
                     \x20     berry-sign verify <pk> <bap> [<sig>]\n\
                     \x20     berry-sign selftest"
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

    /// 验签序（核心；selftest 复用）——覆盖域合同见模块头注。
    fn verify_envelope(pk: &[u8; 32], bap: &[u8], mf: &[u8], sig: &[u8]) -> Result<VerifyOk, String> {
        let env = SigEnvelopeV1::parse(sig).map_err(|e| format!("信封拒 {e:?}"))?;
        if env.pk != *pk {
            return Err(format!("换钥拒: 信封 pk {} != 可信 pk {}", hex(&env.pk), hex(pk)));
        }
        let m = berry_manifest::parse(mf)
            .map_err(|e| format!("manifest 拒：{}", e.reason_str()))?;
        let sec = m.security.ok_or("manifest 拒: 无 [security]（无 key_lineage 可回声对账）")?;
        if env.lineage() != sec.key_lineage.as_bytes() {
            return Err(format!(
                "跨 lineage 拒: 信封 {} != manifest {}",
                String::from_utf8_lossy(env.lineage()),
                sec.key_lineage
            ));
        }
        let key_fields = key_fields_bytes(&m)?;
        let md = digest256(&key_fields);
        let bd = digest256(bap);
        // 头重组：parse∘encode canonical 形封闭（envelope.rs encode 发射前
        // 复检不变量）——重组头与签发头逐字节同。
        let mut buf = [0u8; ENVELOPE_MAX];
        let n = env.encode(&mut buf).map_err(|e| format!("信封重发射拒 {e:?}"))?;
        let mut msg = [0u8; TOP_MSG_MAX];
        let mn = top_message(&buf[..n - SIG_LEN], &bd, &md, &mut msg)
            .map_err(|e| format!("覆盖域构建拒 {e:?}"))?;
        if !ed25519::verify(pk, &env.top_sig, &msg[..mn]) {
            return Err("顶签名拒: BSIG-TOP 验签不过（载荷/信封被篡改或钥不符）".to_string());
        }
        // 轮换链逐节点（P1 签发零链；链非空时 node 0 old_pk = 可信 pk）。
        let mut old_pk = *pk;
        for (i, node) in env.chain().iter().enumerate() {
            let mut rm = [0u8; ROT_MSG_MAX];
            let rn = rot_message(&old_pk, &node.new_pk, env.gen, env.lineage(), &mut rm)
                .map_err(|e| format!("链覆盖域拒 node{i} {e:?}"))?;
            if !ed25519::verify(&old_pk, &node.rot_sig, &rm[..rn]) {
                return Err(format!("断链拒: node{i} BSIG-ROT 验签不过"));
            }
            old_pk = node.new_pk;
        }
        Ok(VerifyOk {
            lineage: String::from_utf8_lossy(env.lineage()).into_owned(),
            gen: env.gen,
            chain_len: env.chain_len,
        })
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
        Ok(fails)
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
