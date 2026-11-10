//! RFC 8032 §7.1 官方向量首批 + ed25519 向量门框架。
//!
//! **空转框架警示（P0-1 状态）**：自研原语（P0-2 独立刀）合入前，本向量
//! 门为**空转框架**——[`run_gate`] 恒返 [`GateStatus::Idle`]，不构成任何
//! 「已过门」证据。**纪律（26 号 §3-2）：原语向量门（RFC 8032 + Wycheproof
//! 全向量，strict 语义）不过不合入**——P0-2 合入门槛 = [`PRIMITIVES_IN`]
//! 翻真后 [`run_gate_forced`] 全绿（CI 门挂点，24 号 §3.6）。
//!
//! 向量表两源核（24 号 §3.6 表首两行）：源 1 = RFC 8032 §7.1 官方向量
//! （`https://datatracker.ietf.org/doc/html/rfc8032#section-7.1`——本轮
//! 实施时直抓复核，四元组 sk/pk/msg/sig 逐字对齐）；源 2 = dalek 仓
//! TESTVECTORS（`https://raw.githubusercontent.com/dalek-cryptography/
//! curve25519-dalek/main/ed25519-dalek/TESTVECTORS`——24 号 §3.6 直抓
//! 实测首行即 TEST 1 四元组，格式 sk:pk:msg:sig）。首批 = TEST 1–3 +
//! 长消息例 TEST 1023（RFC 名 "TEST 1024"，消息 1023B）。
//!
//! 后续批次（P0-2 随原语刀接入，不属本刀）：Wycheproof ed25519 全向量
//! （small-order/mixed-order 公钥拒收例——strict 语义执法面；Apache-2.0
//! LICENSE 注随 fixture 落仓）+ SHA-512/256 FIPS 180-4 向量 + 信封三负
//! 样例（envelope tests 已先行钉板结构拒面）。

/// 单条 ed25519 测试向量（hex 字段零依赖存表；运行时解码——`msg` 帽按
/// 首批最长 1023B 定长缓冲）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ed25519Vector {
    /// 向量名（RFC 8032 §7.1 原名）。
    pub name: &'static str,
    /// 私钥（64 hex = 32B）。
    pub sk: &'static str,
    /// 公钥（64 hex = 32B）。
    pub pk: &'static str,
    /// 消息（hex，可空串 = 空消息）。
    pub msg: &'static str,
    /// 签名（128 hex = 64B）。
    pub sig: &'static str,
    /// 期望：true = 应验证过（RFC 8032 官方向量全为正样例；拒收样例随
    /// P0-2 Wycheproof 批次入表——表形前瞻定死，add-only）。
    pub expect_ok: bool,
}

/// RFC 8032 §7.1 首批四条（TEST 1 / 2 / 3 / TEST 1024）——官方向量逐字
/// 落表（源 1 本轮直抓 datatracker 复核；源 2 dalek TESTVECTORS 24 号
/// §3.6 实测在册）。
pub const RFC8032_ED25519: &[Ed25519Vector] = &[
    Ed25519Vector {
        name: "TEST 1",
        sk: "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
        pk: "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
        msg: "",
        sig: "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e0652249015\
              55fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
        expect_ok: true,
    },
    Ed25519Vector {
        name: "TEST 2",
        sk: "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb",
        pk: "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c",
        msg: "72",
        sig: "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69d\
              a085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00",
        expect_ok: true,
    },
    Ed25519Vector {
        name: "TEST 3",
        sk: "c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7",
        pk: "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025",
        msg: "af82",
        sig: "6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3a\
              c18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a",
        expect_ok: true,
    },
    Ed25519Vector {
        name: "TEST 1024",
        sk: "f5e5767cf153319517630f226876b86c8160cc583bc013744c6bf255f5cc0ee5",
        pk: "278117fc144c72340f67d0f2316e8386ceffbf2b2428c9c51fef7c597f1d426e",
        msg: "08b8b2b733424243760fe426a4b54908632110a66c2f6591eabd3345e3e4eb9\
              8fa6e264bf09efe12ee50f8f54e9f77b1e355f6c50544e23fb1433ddf73be84d8\
              79de7c0046dc4996d9e773f4bc9efe5738829adb26c81b37c93a1b270b20329d\
              658675fc6ea534e0810a4432826bf58c941efb65d57a338bbd2e26640f89ffbc\
              1a858efcb8550ee3a5e1998bd177e93a7363c344fe6b199ee5d02e82d522c4fe\
              ba15452f80288a821a579116ec6dad2b3b310da903401aa62100ab5d1a36553e\
              06203b33890cc9b832f79ef80560ccb9a39ce767967ed628c6ad573cb116dbef\
              efd75499da96bd68a8a97b928a8bbc103b6621fcde2beca1231d206be6cd9ec7\
              aff6f6c94fcd7204ed3455c68c83f4a41da4af2b74ef5c53f1d8ac70bdcb7ed1\
              85ce81bd84359d44254d95629e9855a94a7c1958d1f8ada5d0532ed8a5aa3fb2\
              d17ba70eb6248e594e1a2297acbbb39d502f1a8c6eb6f1ce22b3de1a1f40cc24\
              554119a831a9aad6079cad88425de6bde1a9187ebb6092cf67bf2b13fd65f270\
              88d78b7e883c8759d2c4f5c65adb7553878ad575f9fad878e80a0c9ba63bcbcc\
              2732e69485bbc9c90bfbd62481d9089beccf80cfe2df16a2cf65bd92dd597b07\
              07e0917af48bbb75fed413d238f5555a7a569d80c3414a8d0859dc65a46128ba\
              b27af87a71314f318c782b23ebfe808b82b0ce26401d2e22f04d83d1255dc51a\
              ddd3b75a2b1ae0784504df543af8969be3ea7082ff7fc9888c144da2af58429e\
              c96031dbcad3dad9af0dcbaaaf268cb8fcffead94f3c7ca495e056a9b47acdb7\
              51fb73e666c6c655ade8297297d07ad1ba5e43f1bca32301651339e22904cc8c\
              42f58c30c04aafdb038dda0847dd988dcda6f3bfd15c4b4c4525004aa06eeff8\
              ca61783aacec57fb3d1f92b0fe2fd1a85f6724517b65e614ad6808d6f6ee34df\
              f7310fdc82aebfd904b01e1dc54b2927094b2db68d6f903b68401adebf5a7e08\
              d78ff4ef5d63653a65040cf9bfd4aca7984a74d37145986780fc0b16ac451649\
              de6188a7dbdf191f64b5fc5e2ab47b57f7f7276cd419c17a3ca8e1b939ae49e4\
              88acba6b965610b5480109c8b17b80e1b7b750dfc7598d5d5011fd2dcc5600a3\
              2ef5b52a1ecc820e308aa342721aac0943bf6686b64b2579376504ccc493d97e\
              6aed3fb0f9cd71a43dd497f01f17c0e2cb3797aa2a2f256656168e6c496afc5f\
              b93246f6b1116398a346f1a641f3b041e989f7914f90cc2c7fff357876e506b5\
              0d334ba77c225bc307ba537152f3f1610e4eafe595f6d9d90d11faa933a15ef1\
              369546868a7f3a45a96768d40fd9d03412c091c6315cf4fde7cb68606937380d\
              b2eaaa707b4c4185c32eddcdd306705e4dc1ffc872eeee475a64dfac86aba41\
              c0618983f8741c5ef68d3a101e8a3b8cac60c905c15fc910840b94c00a0b9d0",
        sig: "0aab4c900501b3e24d7cdf4663326a3a87df5e4843b2cbdb67cbf6e460fec35\
              0aa5371b1508f9f4528ecea23c436d94b5e8fcd4f681e30a6ac00a9704a188a03",
        expect_ok: true,
    },
];

/// 首批消息长帽（TEST 1024 消息 1023B——解码定长缓冲尺寸）。
pub const MSG_BUF_MAX: usize = 1023;

/// 原语在位旗（P0-2 翻真）。P0-1 恒 false——[`run_gate`] 空转的机械面；
/// **翻真 = P0-2 刀的动作位，与本表数据正交**（对选型零敏感面）。
pub const PRIMITIVES_IN: bool = false;

/// 向量门报告（零分配：失败者只记首名——全量清单随 P0-2 CI 门按需扩）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateReport {
    /// 表内向量总数。
    pub total: usize,
    /// 通过数（verify 结果 == expect_ok）。
    pub passed: usize,
    /// 首个失败向量名（None = 全过）。
    pub failed_first: Option<&'static str>,
}

/// 向量门状态：Idle = 空转（原语未合入）；Ran = 实跑（P0-2 起）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateStatus {
    /// 空转——不构成「已过门」证据（P0-1 状态）。
    Idle,
    /// 实跑报告（仅 P0-2 `PRIMITIVES_IN` 翻真后可出现）。
    Ran(GateReport),
}

/// 向量门入口（P0-1 恒 [`GateStatus::Idle`]——空转框架的执法点）。
/// P0-2 合入后 = CI 门主体：全绿才可合入（26 号 §3-2 纪律）。
pub fn run_gate(verify: &mut dyn FnMut(&[u8], &[u8], &[u8]) -> bool) -> GateStatus {
    if !PRIMITIVES_IN {
        return GateStatus::Idle;
    }
    GateStatus::Ran(run_gate_forced(verify))
}

/// 无门禁实跑（机制自证/P0-2 CI 挂点用——不受 [`PRIMITIVES_IN`] 门控；
/// P0-1 只在 tests 内以 stub 闭包自证机制，非合入门）。
pub fn run_gate_forced(verify: &mut dyn FnMut(&[u8], &[u8], &[u8]) -> bool) -> GateReport {
    let mut report = GateReport { total: 0, passed: 0, failed_first: None };
    for v in RFC8032_ED25519 {
        report.total += 1;
        let mut pk = [0u8; 32];
        let mut msg = [0u8; MSG_BUF_MAX];
        let mut sig = [0u8; 64];
        let ok = hex_decode(v.pk, &mut pk)
            .and_then(|_| hex_prefix(v.msg, &mut msg))
            .and_then(|_| hex_decode(v.sig, &mut sig))
            .map(|_| verify(&pk, &msg[..v.msg.len() / 2], &sig))
            .unwrap_or(false);
        if ok == v.expect_ok {
            report.passed += 1;
        } else if report.failed_first.is_none() {
            report.failed_first = Some(v.name);
        }
    }
    report
}

/// 小写 hex 解码（恰满 out——账本 pk 同款；大写拒）。
fn hex_decode(s: &str, out: &mut [u8]) -> Option<()> {
    let b = s.as_bytes();
    if b.len() != out.len() * 2 {
        return None;
    }
    for (i, pair) in b.chunks(2).enumerate() {
        out[i] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Some(())
}

/// 小写 hex 前缀解码（短于 out 时尾零留白，返回在用长 = len/2——msg 缓冲
/// 定长 1023B 而消息可短至 0B）。
fn hex_prefix(s: &str, out: &mut [u8]) -> Option<()> {
    let b = s.as_bytes();
    if b.len() % 2 != 0 || b.len() / 2 > out.len() {
        return None;
    }
    for (i, pair) in b.chunks(2).enumerate() {
        out[i] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Some(())
}

fn nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 1. 表完整性：四条在册、hex 全解、长度恰满（sk/pk 32B、sig 64B、
    //    msg 偶长 ≤1023B）、pk 无重复、expect_ok 全真（拒收样例随 P0-2
    //    Wycheproof 批次入表）。
    #[test]
    fn table_integrity() {
        assert_eq!(RFC8032_ED25519.len(), 4);
        let mut pks: Vec<[u8; 32]> = Vec::new();
        for v in RFC8032_ED25519 {
            assert!(hex_decode(v.sk, &mut [0u8; 32]).is_some(), "{} sk", v.name);
            let mut pk = [0u8; 32];
            assert!(hex_decode(v.pk, &mut pk).is_some(), "{} pk", v.name);
            let mut sig = [0u8; 64];
            assert!(hex_decode(v.sig, &mut sig).is_some(), "{} sig", v.name);
            assert!(hex_prefix(v.msg, &mut [0u8; MSG_BUF_MAX]).is_some(), "{} msg", v.name);
            assert!(v.msg.len() % 2 == 0);
            assert!(v.expect_ok, "{} —— 首批官方向量全为正样例", v.name);
            assert!(!pks.contains(&pk), "{} pk 重复", v.name);
            pks.push(pk);
        }
    }

    // 2. 消息长钉板：0 / 1 / 2 / 1023B（RFC 8032 §7.1 原文 length 标注）。
    #[test]
    fn message_lengths_pin() {
        let lens: [usize; 4] = RFC8032_ED25519.iter().map(|v| v.msg.len() / 2).collect::<Vec<_>>().try_into().unwrap();
        assert_eq!(lens, [0, 1, 2, 1023]);
    }

    // 3. TEST 1 黄金锚（两源核在册：源 1 = 本轮直抓 datatracker §7.1；
    //    源 2 = dalek TESTVECTORS 首行，24 号 §3.6 实测）。
    #[test]
    fn test1_golden_anchor() {
        let v = &RFC8032_ED25519[0];
        assert_eq!(v.name, "TEST 1");
        assert_eq!(v.msg, "");
        assert!(v.pk.starts_with("d75a980182b10ab7d54bfed3c964073a"));
        assert!(v.sig.starts_with("e5564300c360ac729086e2cc806e828a"));
        assert!(v.sig.ends_with("41438e7a100b"));
        // TEST 1024 长消息例锚（首尾 8B）。
        let v4 = &RFC8032_ED25519[3];
        assert!(v4.msg.starts_with("08b8b2b733424243"));
        assert!(v4.msg.ends_with("40b94c00a0b9d0"));
    }

    // 4. 向量门空转（P0-1 状态钉板）：PRIMITIVES_IN 恒 false，run_gate
    //    恒 Idle——不构成「已过门」证据（26 号 §3-2：向量门不过不合入）。
    #[test]
    fn gate_idle_in_p01() {
        assert!(!PRIMITIVES_IN);
        assert_eq!(run_gate(&mut |_, _, _| true), GateStatus::Idle);
        assert_eq!(run_gate(&mut |_, _, _| false), GateStatus::Idle);
    }

    // 5. 机制自证（非合入门——run_gate_forced + stub 闭包）：全过/首败
    //    两态、计数与表长一致——P0-2 翻真后同一机制即为 CI 门主体。
    #[test]
    fn gate_mechanism_proof() {
        let all = run_gate_forced(&mut |_, _, _| true);
        assert_eq!(all, GateReport { total: 4, passed: 4, failed_first: None });
        // 恰拒 TEST 2（按 pk 首字节分派）。
        let one_fail = run_gate_forced(&mut |pk, _, _| pk[0] != 0x3d);
        assert_eq!(one_fail.total, 4);
        assert_eq!(one_fail.passed, 3);
        assert_eq!(one_fail.failed_first, Some("TEST 2"));
        // expect_ok=false 向量的语义臂（表形前瞻——P0-2 Wycheproof 拒收例
        // 复用同一机制：verify 过而期望拒 = 失败）。
        let inverted = run_gate_forced(&mut |_, _, _| false);
        assert_eq!(inverted.passed, 0);
        assert_eq!(inverted.failed_first, Some("TEST 1"));
    }
}
