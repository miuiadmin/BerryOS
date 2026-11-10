//! RFC 8032 §7.1 官方向量全集 + ed25519 向量门（P0-2 翻真）。
//!
//! **状态（P0-2 + 姊妹刀合流）**：原语本体（[`crate::ed25519`]）已合入，
//! [`PRIMITIVES_IN`] 翻真，[`run_gate`] 由 P0-1 空转框架转实跑门——
//! **RFC 8032 §7.1 官方向量全 5 条（TEST 1/2/3/1024/SHA(abc)）全绿 +
//! Wycheproof 210 条 strict 门全绿（本版起，[`run_wycheproof_gate`]）
//! = 合入门槛**（26 号 §3-2 纪律；§3-3 fallback 时钟挂 Wycheproof
//! 门——acceptable 29 全拒后不再欠账）。
//!
//! 向量表两源核（24 号 §3.6 表首两行 + P0-2 补全注）：
//! - 源 1 = RFC 8032 §7.1 官方向量（datatracker
//!   `rfc8032#section-7.1` 本轮直抓复核：**§7.1 全集恰 5 条**——TEST 1/
//!   2/3/TEST 1024（消息 1023B）/TEST SHA(abc)（消息 64B = SHA-512("abc")
//!   摘要值）；四元组 sk/pk/msg/sig 逐字对齐）；
//! - 源 2 = ①dalek 仓 TESTVECTORS（cr.yp.to 系，覆盖 TEST 1–3——首行
//!   即 TEST 1 四元组）；②OpenSSL 3.6.4 本机实跑（P0-2 新增第二实现源：
//!   5 条全量 pk 派生 == 官方 pk、确定性重签 == 官方 sig、官方 sig 验签
//!   通过——TEST 1 的 0 字节消息在 OpenSSL CLI 有 oneshot 缓冲分配缺陷，
//!   其 sig 以 dalek 首行双源在册）。
//!
//! Wycheproof 批次（P0-2 姊妹刀**已合流**，本版在位）：ed25519 全向量
//! 210 条入 [`wycheproof_vectors`]（生成文件——`tools/genwycheproof.py` +
//! `tools/wycheproof_ed25519_test.json` fixture；Apache-2.0 LICENSE 注随
//! 生成产物头落仓）+ 本文件 [`run_wycheproof_gate`] strict 批跑门
//! （small-order/mixed-order 公钥拒收例——26 号 §3-2/§3-3 执法面；
//! 拒收机制 = [`crate::ed25519::verify`] 步 1.5 torsion 检查，29 号 §F1
//! 修正——acceptable 29 条全拒是本版合入判据）。后续：SHA-512/256 FIPS
//! 180-4 向量 + 信封三负样例（envelope tests 已先行钉板结构拒面）。

use crate::wycheproof_vectors::{WYCHEPROOF_ED25519, WY_MSG_MAX, WY_SIG_MAX};

/// 单条 ed25519 测试向量（hex 字段零依赖存表；运行时解码——`msg` 帽按
/// 表内最长 1023B 定长缓冲）。
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
    /// Wycheproof 批次入表——表形前瞻定死，add-only）。
    pub expect_ok: bool,
}

/// RFC 8032 §7.1 官方向量全集 5 条（TEST 1 / 2 / 3 / TEST 1024 /
/// TEST SHA(abc)）——官方向量逐字落表（源 1 datatracker 直抓；源 2 =
/// dalek TESTVECTORS（1–3）+ OpenSSL 3.6.4 实跑全量对拍（见模块注））。
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
    Ed25519Vector {
        // RFC 8032 §7.1 末条：sk 由 SHA(abc) 派生的演示向量——消息 =
        // SHA-512("abc") 摘要 64B（RFC 6234 同值向量互证）。
        name: "TEST SHA(abc)",
        sk: "833fe62409237b9d62ec77587520911e9a759cec1d19755b7da901b96dca3d42",
        pk: "ec172b93ad5e563bf4932c70e1245034c35467ef2efd4d64ebf819683467e2bf",
        msg: "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
              2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
        sig: "dc2a4459e7369633a52b1bf277839a00201009a3efbf3ecb69bea2186c26b589\
              09351fc9ac90b3ecfdfbc7c66431e0303dca179c138ac17ad9bef1177331a704",
        expect_ok: true,
    },
];

/// 表内消息长帽（TEST 1024 消息 1023B——解码定长缓冲尺寸）。
pub const MSG_BUF_MAX: usize = 1023;

/// 原语在位旗（**P0-2 翻真**）：ed25519/SHA-512 原语本体已合入
/// （[`crate::ed25519`]/[`crate::sha512`] 及 fe/ge/sc 支撑件）。
/// [`run_gate`] 实跑执法点；表数据与本旗正交（对选型零敏感面）。
pub const PRIMITIVES_IN: bool = true;

/// 向量门报告（零分配：失败者只记首名——全量清单随 Wycheproof 批次
/// 按需扩）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateReport {
    /// 表内向量总数。
    pub total: usize,
    /// 通过数（verify 结果 == expect_ok）。
    pub passed: usize,
    /// 首个失败向量名（None = 全过）。
    pub failed_first: Option<&'static str>,
}

/// 向量门状态：Idle = 空转（原语未合入——P0-1 历史态）；Ran = 实跑。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateStatus {
    /// 空转——不构成「已过门」证据（P0-1 历史态，本版不再出现）。
    Idle,
    /// 实跑报告（P0-2 起 `PRIMITIVES_IN` 恒真）。
    Ran(GateReport),
}

/// 向量门入口（P0-2 起实跑：`PRIMITIVES_IN` 真 → [`run_gate_forced`]）。
/// CI 门主体：全绿才可合入（26 号 §3-2 纪律）——闭包形参序 (pk, msg, sig)。
pub fn run_gate(verify: &mut dyn FnMut(&[u8], &[u8], &[u8]) -> bool) -> GateStatus {
    if !PRIMITIVES_IN {
        return GateStatus::Idle;
    }
    GateStatus::Ran(run_gate_forced(verify))
}

/// 无门禁实跑（机制自证/CI 挂点用——不受 [`PRIMITIVES_IN`] 门控）。
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

/// Wycheproof 门报告（零分配纪律同 [`GateReport`]——失败只记首条；多记
/// 一枚 flags：26 号 §3-3 mixed-order 触发器的诊断面，首败旗型直接可读）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WycheproofGateReport {
    /// 表内向量总数（= WY_TOTAL，210）。
    pub total: usize,
    /// 通过数（verify 结果 == expect_ok——strict 语义下恰 WY_VALID 可全过）。
    pub passed: usize,
    /// 首个失败向量 (name, flags)——None = 全过。
    pub failed_first: Option<(&'static str, &'static str)>,
}

/// Wycheproof 门状态：与 [`GateStatus`] 同形（空转/实跑两态）——独立枚举
/// 因报告体不同（失败记 name+flags 双元；RFC 门只记名）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WycheproofGateStatus {
    /// 空转——不构成「已过门」证据（PRIMITIVES_IN=false 全程；本版已翻真
    /// 不再出现，臂留作门控语义完备）。
    Idle,
    /// 实跑报告（PRIMITIVES_IN 真即出现——本版常态）。
    Ran(WycheproofGateReport),
}

/// Wycheproof 向量门入口（strict 批跑：result=invalid/acceptable 的用例
/// 验签**必须 false**——mixed-order/低阶件拒收是合入纪律不是可选项，
/// 26 号 §3-2；门控位与 [`run_gate`] 同一枚 [`PRIMITIVES_IN`]：翻真后
/// 恒实跑。拒收机制 = verify 步 1.5 torsion 检查，29 号 §F1 修正）。
pub fn run_wycheproof_gate(
    verify: &mut dyn FnMut(&[u8], &[u8], &[u8]) -> bool,
) -> WycheproofGateStatus {
    if !PRIMITIVES_IN {
        return WycheproofGateStatus::Idle;
    }
    WycheproofGateStatus::Ran(run_wycheproof_gate_forced(verify))
}

/// 无门禁实跑（机制自证/CI 挂点——`run_gate_forced` 同款纪律）。
/// sig 变长（0B 空签 ..=96B 带垃圾尾）用 [`hex_prefix`] 前缀解码按实长
/// 切片喂 verify——strict 原语须对非 64B 签返回 false。
pub fn run_wycheproof_gate_forced(
    verify: &mut dyn FnMut(&[u8], &[u8], &[u8]) -> bool,
) -> WycheproofGateReport {
    let mut report = WycheproofGateReport { total: 0, passed: 0, failed_first: None };
    for v in WYCHEPROOF_ED25519 {
        report.total += 1;
        let mut pk = [0u8; 32];
        let mut msg = [0u8; WY_MSG_MAX];
        let mut sig = [0u8; WY_SIG_MAX];
        let ok = hex_decode(v.pk, &mut pk)
            .and_then(|_| hex_prefix(v.msg, &mut msg))
            .and_then(|_| hex_prefix(v.sig, &mut sig))
            .map(|_| verify(&pk, &msg[..v.msg.len() / 2], &sig[..v.sig.len() / 2]))
            .unwrap_or(false);
        if ok == v.expect_ok {
            report.passed += 1;
        } else if report.failed_first.is_none() {
            report.failed_first = Some((v.name, v.flags));
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
    use crate::ed25519;
    use crate::wycheproof_vectors::{WY_ACCEPTABLE, WY_INVALID, WY_TOTAL, WY_VALID};

    /// 形参序适配：run_gate 闭包序 (pk, msg, sig) → ed25519::verify(pk, sig, msg)。
    fn real_verify(pk: &[u8], msg: &[u8], sig: &[u8]) -> bool {
        let mut p = [0u8; 32];
        let mut s = [0u8; 64];
        if pk.len() != 32 || sig.len() != 64 {
            return false;
        }
        p.copy_from_slice(pk);
        s.copy_from_slice(sig);
        ed25519::verify(&p, &s, msg)
    }

    // 1. 表完整性：五条在册、hex 全解、长度恰满（sk/pk 32B、sig 64B、
    //    msg 偶长 ≤1023B）、pk 无重复、expect_ok 全真（拒收样例随
    //    Wycheproof 批次入表）。
    #[test]
    fn table_integrity() {
        assert_eq!(RFC8032_ED25519.len(), 5);
        let mut pks: Vec<[u8; 32]> = Vec::new();
        for v in RFC8032_ED25519 {
            assert!(hex_decode(v.sk, &mut [0u8; 32]).is_some(), "{} sk", v.name);
            let mut pk = [0u8; 32];
            assert!(hex_decode(v.pk, &mut pk).is_some(), "{} pk", v.name);
            let mut sig = [0u8; 64];
            assert!(hex_decode(v.sig, &mut sig).is_some(), "{} sig", v.name);
            assert!(hex_prefix(v.msg, &mut [0u8; MSG_BUF_MAX]).is_some(), "{} msg", v.name);
            assert!(v.msg.len() % 2 == 0);
            assert!(v.expect_ok, "{} —— 官方向量全为正样例", v.name);
            assert!(!pks.contains(&pk), "{} pk 重复", v.name);
            pks.push(pk);
        }
    }

    // 2. 消息长钉板：0 / 1 / 2 / 1023 / 64B（RFC 8032 §7.1 原文 length
    //    标注——SHA(abc) 例 64B）。
    #[test]
    fn message_lengths_pin() {
        let lens: Vec<usize> = RFC8032_ED25519.iter().map(|v| v.msg.len() / 2).collect();
        assert_eq!(lens, vec![0, 1, 2, 1023, 64]);
    }

    // 3. TEST 1 黄金锚 + SHA(abc) 消息 = SHA-512("abc")（RFC 6234 向量
    //    互证——两源核在册：源 1 = datatracker §7.1；源 2 = dalek 首
    //    行 + OpenSSL 实跑全量）。
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
        // TEST SHA(abc)：消息逐字节 == 自研 SHA-512 的 abc 摘要
        //（RFC 6234 向量门已独立钉住该摘要——跨模块互证）。
        let v5 = &RFC8032_ED25519[4];
        let sha_abc = crate::sha512::digest(b"abc");
        let mut msg = [0u8; MSG_BUF_MAX];
        assert!(hex_prefix(v5.msg, &mut msg).is_some());
        assert_eq!(&msg[..64], &sha_abc[..], "SHA(abc) 消息互证");
    }

    // 4. 向量门实跑（P0-2 合入门槛主体）：PRIMITIVES_IN 真 + run_gate
    //    全 5 条全绿——空转态钉板（P0-1）已由翻真取代。
    #[test]
    fn gate_armed_in_p02() {
        assert!(PRIMITIVES_IN, "P0-2：原语在位");
        let st = run_gate(&mut real_verify);
        assert_eq!(
            st,
            GateStatus::Ran(GateReport { total: 5, passed: 5, failed_first: None })
        );
    }

    // 5. 机制自证（非合入门——run_gate_forced + stub 闭包）：计数与表长
    //    一致、首败定位、expect_ok=false 语义臂（Wycheproof 拒收例将
    //    复用同一机制）。
    #[test]
    fn gate_mechanism_proof() {
        let all = run_gate_forced(&mut |_, _, _| true);
        assert_eq!(all, GateReport { total: 5, passed: 5, failed_first: None });
        // 恰拒 TEST 2（按 pk 首字节分派）。
        let one_fail = run_gate_forced(&mut |pk, _, _| pk[0] != 0x3d);
        assert_eq!(one_fail.total, 5);
        assert_eq!(one_fail.passed, 4);
        assert_eq!(one_fail.failed_first, Some("TEST 2"));
        // expect_ok=false 向量的语义臂：verify 过而期望拒 = 失败。
        let inverted = run_gate_forced(&mut |_, _, _| false);
        assert_eq!(inverted.passed, 0);
        assert_eq!(inverted.failed_first, Some("TEST 1"));
    }

    // 6. sign 双向门（24 号 §3.7 DoD ①「verify + sign 双向」）：每条
    //    向量 pk == public_key(sk)、sig == sign(sk, msg)（确定性 nonce
    //    逐字节复现）、官方向量验签过。
    #[test]
    fn sign_side_full_gate() {
        for v in RFC8032_ED25519 {
            let mut sk = [0u8; 32];
            let mut pk = [0u8; 32];
            let mut sig = [0u8; 64];
            let mut msg = [0u8; MSG_BUF_MAX];
            assert!(hex_decode(v.sk, &mut sk).is_some());
            assert!(hex_decode(v.pk, &mut pk).is_some());
            assert!(hex_decode(v.sig, &mut sig).is_some());
            assert!(hex_prefix(v.msg, &mut msg).is_some());
            let m = &msg[..v.msg.len() / 2];
            assert_eq!(ed25519::public_key(&sk), pk, "{} pk 派生", v.name);
            assert_eq!(ed25519::sign(&sk, m), sig, "{} 确定性重签", v.name);
            assert!(ed25519::verify(&pk, &sig, m), "{} 官方向量验签", v.name);
        }
    }

    // 7. Wycheproof 表完整性：210 实数在册、计数三方账（表枚举 == 生成器
    //    常量 == 实数硬锚）、hex/帽全合规、strict 映射自洽（expect_ok ⇔
    //    result=="valid"）、acceptable 全为阶旗——向量数实数入日志。
    #[test]
    fn wycheproof_table_integrity() {
        assert_eq!(WYCHEPROOF_ED25519.len(), WY_TOTAL);
        assert_eq!(WY_TOTAL, 210); // 实数（26 号 §3-2 全向量纪律）
        let (mut valid, mut acceptable, mut invalid) = (0usize, 0usize, 0usize);
        let (mut small_order, mut mixed_order) = (0usize, 0usize);
        for v in WYCHEPROOF_ED25519 {
            assert!(hex_decode(v.pk, &mut [0u8; 32]).is_some(), "{} pk", v.name);
            assert!(
                hex_prefix(v.msg, &mut [0u8; WY_MSG_MAX]).is_some(),
                "{} msg 超 WY_MSG_MAX",
                v.name
            );
            assert!(
                hex_prefix(v.sig, &mut [0u8; WY_SIG_MAX]).is_some(),
                "{} sig 超 WY_SIG_MAX",
                v.name
            );
            match v.result {
                "valid" => {
                    valid += 1;
                    assert!(v.expect_ok, "{} valid 须期望过", v.name);
                }
                "acceptable" => {
                    acceptable += 1;
                    assert!(!v.expect_ok, "{} acceptable 须 strict 拒", v.name);
                    assert!(
                        v.flags.contains("OrderPublicKey"),
                        "{} acceptable 须带阶旗（白名单外即生成器已拒）",
                        v.name
                    );
                }
                "invalid" => {
                    invalid += 1;
                    assert!(!v.expect_ok, "{} invalid 须期望拒", v.name);
                }
                other => panic!("{} result 无类：{}", v.name, other),
            }
            if v.flags.contains("SmallOrderPublicKey") {
                small_order += 1;
            }
            if v.flags.contains("MixedOrderPublicKey") {
                mixed_order += 1;
            }
        }
        assert_eq!((valid, acceptable, invalid), (WY_VALID, WY_ACCEPTABLE, WY_INVALID));
        assert_eq!((valid, acceptable, invalid), (117, 29, 64));
        // 阶旗执法面存在性：22 混阶（21 acceptable + 1 invalid tc210）+
        // 8 小阶——mixed-order 门标的实在场（26 号 §3-3）。
        assert_eq!((mixed_order, small_order), (22, 8));
        eprintln!(
            "wycheproof vectors: total={} valid={} acceptable={} invalid={} \
             mixed-order={} small-order={}",
            WY_TOTAL, valid, acceptable, invalid, mixed_order, small_order
        );
    }

    // 8. Wycheproof 黄金锚（两源核：本表 vs 生成期直抓 JSON——tc1 正例 /
    //    tc152 小阶首例 / tc30 空 sig / tc210 混阶 invalid）。
    #[test]
    fn wycheproof_golden_anchors() {
        let tc = |n: &str| WYCHEPROOF_ED25519.iter().find(|v| v.name == n).unwrap();
        let t1 = tc("tc1");
        assert!(t1.pk.starts_with("7d4d0e7f6153a69b6242b522abbee685"));
        assert!(t1.sig.starts_with("d4fbdb52bfa726b44d1786a8c0d171c3"));
        assert!(t1.sig.ends_with("ebaa9d311c4007"));
        assert_eq!(t1.result, "valid");
        // 小阶首例：pk = 01‖62×0（1-torsion 规范编码点）。
        let t152 = tc("tc152");
        assert_eq!(t152.pk, format!("01{}", "0".repeat(62)));
        assert_eq!(t152.flags, "SmallOrderPublicKey");
        assert!(!t152.expect_ok);
        // 空 sig（TruncatedSignature）：verify 收到空切片也必须拒。
        assert_eq!(tc("tc30").sig, "");
        // 唯一 invalid+MixedOrder（tc210）：验签方程不过——宽松 cofactored
        // 验签器也必须拒（与 acceptable 混阶的策略拒分层）。
        let t210 = tc("tc210");
        assert_eq!(t210.flags, "MixedOrderPublicKey");
        assert_eq!(t210.result, "invalid");
        assert!(!t210.expect_ok);
    }

    // 9. Wycheproof strict 门实跑（本版合入判据主体——29 号 §F1 修正的
    //    生效判据）：真实 verify 过 [`run_wycheproof_gate`] 210 条全绿——
    //    valid 117 全过、acceptable 29 **全拒**（小阶/混阶公钥 torsion
    //    检查拒收）、invalid 64 全拒；分类计数入日志。
    #[test]
    fn wycheproof_strict_gate_armed() {
        assert!(PRIMITIVES_IN, "P0-2：原语在位，门实跑不空转");
        let st = run_wycheproof_gate(&mut real_verify);
        assert_eq!(
            st,
            WycheproofGateStatus::Ran(WycheproofGateReport {
                total: WY_TOTAL,
                passed: WY_TOTAL,
                failed_first: None,
            }),
            "Wycheproof strict 门 210 条须全绿（acceptable 拒收 = F1 生效判据）"
        );
        // 分类对账（逐类实跑）：valid 全过 / acceptable 全拒 / invalid 全拒。
        let (mut valid_ok, mut acceptable_rej, mut invalid_rej) = (0usize, 0usize, 0usize);
        let (mut small_rej, mut mixed_rej) = (0usize, 0usize);
        for v in WYCHEPROOF_ED25519 {
            let mut pk = [0u8; 32];
            let mut msg = [0u8; WY_MSG_MAX];
            let mut sig = [0u8; WY_SIG_MAX];
            assert!(hex_decode(v.pk, &mut pk).is_some());
            assert!(hex_prefix(v.msg, &mut msg).is_some());
            assert!(hex_prefix(v.sig, &mut sig).is_some());
            let got = real_verify(&pk, &msg[..v.msg.len() / 2], &sig[..v.sig.len() / 2]);
            match v.result {
                "valid" => {
                    assert!(got, "{} valid 须验签过", v.name);
                    valid_ok += 1;
                }
                "acceptable" => {
                    assert!(!got, "{} acceptable 须拒（F1：torsion 检查）", v.name);
                    acceptable_rej += 1;
                    if v.flags.contains("SmallOrderPublicKey") {
                        small_rej += 1;
                    }
                    if v.flags.contains("MixedOrderPublicKey") {
                        mixed_rej += 1;
                    }
                }
                _ => {
                    assert!(!got, "{} invalid 须拒", v.name);
                    invalid_rej += 1;
                }
            }
        }
        assert_eq!((valid_ok, acceptable_rej, invalid_rej), (117, 29, 64));
        assert_eq!((mixed_rej, small_rej), (21, 8));
        eprintln!(
            "wycheproof strict gate: valid-ok={} acceptable-rejected={} \
             invalid-rejected={} (mixed-order-rejected={} small-order-rejected={})",
            valid_ok, acceptable_rej, invalid_rej, mixed_rej, small_rej
        );
    }

    // 10. Wycheproof 机制自证（stub 闭包，非合入门——真实门见测试 9）：
    //     ①全收闭包（宽松 cofactored 器形态）恰 valid 117 过、首败
    //     tc10——宽松器挂掉的 93 = invalid 64 + acceptable 29（strict
    //     策略执法差量可证账目）；②全拒闭包恰 93 过、首败 tc1；③strict
    //     模拟（拒阶旗 pk 集 + 拒非 64B sig）按表内期望对账——记账
    //     （计数/首败名+旗）与表语义一致。
    #[test]
    fn wycheproof_gate_mechanism_proof() {
        let all = run_wycheproof_gate_forced(&mut |_, _, _| true);
        assert_eq!(all.total, WY_TOTAL);
        assert_eq!(all.passed, WY_VALID);
        assert_eq!(all.failed_first, Some(("tc10", "InvalidSignature")));
        assert_eq!(WY_TOTAL - all.passed, WY_INVALID + WY_ACCEPTABLE);
        let none = run_wycheproof_gate_forced(&mut |_, _, _| false);
        assert_eq!(none.total, WY_TOTAL);
        assert_eq!(none.passed, WY_INVALID + WY_ACCEPTABLE);
        assert_eq!(none.failed_first, Some(("tc1", "Valid")));

        let order_pks: Vec<[u8; 32]> = WYCHEPROOF_ED25519
            .iter()
            .filter(|v| v.result == "acceptable")
            .map(|v| {
                let mut p = [0u8; 32];
                hex_decode(v.pk, &mut p).unwrap();
                p
            })
            .collect();
        let stub = |pk: &[u8], sig: &[u8]| {
            !order_pks.iter().any(|p| p.as_slice() == pk) && sig.len() == 64
        };
        let mut want_passed = 0;
        let mut want_first = None;
        for v in WYCHEPROOF_ED25519 {
            let verdict = {
                let mut pk = [0u8; 32];
                let mut sig = [0u8; WY_SIG_MAX];
                hex_decode(v.pk, &mut pk).unwrap();
                hex_prefix(v.sig, &mut sig).unwrap();
                stub(&pk, &sig[..v.sig.len() / 2])
            };
            if verdict == v.expect_ok {
                want_passed += 1;
            } else if want_first.is_none() {
                want_first = Some((v.name, v.flags));
            }
        }
        let strict = run_wycheproof_gate_forced(&mut |pk, _msg, sig| stub(pk, sig));
        assert_eq!(strict.total, WY_TOTAL);
        assert_eq!(strict.passed, want_passed);
        assert_eq!(strict.failed_first, want_first);
    }
}
