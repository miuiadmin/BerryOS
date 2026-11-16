//! Wycheproof ed25519 验签向量表——**生成文件，勿手改**。
//! 单源 = `tools/wycheproof_ed25519_test.json`（**Apache-2.0**，
//! LICENSE 注即此段：出处/版本/指纹三注齐——抓取自
//! https://raw.githubusercontent.com/google/wycheproof/master/testvectors_v1/ed25519_test.json；
//! fixture sha256 = `ea80ffebac085e21cb9d3e903e7fa2b448d96702da5b31f061d0b83b430b74b3`
//! （两源核：JSON numberOfTests=210 == 枚举数——生成器 load 执法）。
//! 再生 = `python3 tools/genwycheproof.py`；守卫 = `--check` 逐字节
//! 比对（gen_abi/genfbfont 产物入仓同款纪律）。
//!
//! 期望映射（26 号 §3-2 strict 终裁——`vectors::run_wycheproof_gate`
//! 执法语义）：valid→true；invalid→false；**acceptable→false**——
//! 小阶/混阶公钥 strict 拒收（RFC 8032 §5.1.7 cofactored 语义下可
//! 接受，本仓选 strict 子群策略：mixed-order 门不过不合入 = 26 号
//! §3-3 fallback 外购触发器）。生成器对白名单外 acceptable 旗 FAIL
//! 重审，不静默放宽。
//!
//! 统计：total 210 = valid 117 + acceptable 29 + invalid 64
//! （acceptable = 21 MixedOrderPublicKey + 8 SmallOrderPublicKey）；expect_ok=true 恰 117。
//! msg 峰 1023B（tc83）/ sig 峰 96B（tc34，含空 sig tc30）——
//! 解码缓冲帽见 WY_MSG_MAX/WY_SIG_MAX（生成器从数据活算）。

/// 单条 Wycheproof ed25519 验签向量（hex 存表零解析——运行时只
/// hex 解码；`name` = "tc" + JSON tcId，失败记账/诊断索引）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WycheproofVector {
    /// 向量名（"tc<N>"——N = fixture tcId，全表唯一）。
    pub name: &'static str,
    /// 公钥（64 hex = 32B，全表恰满；小阶/混阶键原样入表——拒收
    /// 语义由 expect_ok 执法，表不预筛）。
    pub pk: &'static str,
    /// 消息（hex，可空串 = 空消息；偶长 ≤ WY_MSG_MAX×2）。
    pub msg: &'static str,
    /// 签名（hex，**可 ≠64B**——截断/带垃圾/压缩编码例原样入表，
    /// strict verify 须拒；偶长 ≤ WY_SIG_MAX×2）。
    pub sig: &'static str,
    /// fixture 原始判定（"valid"/"acceptable"/"invalid"——不折叠，
    /// acceptable 与 invalid 的拒因不同层：前者策略拒、后者方程拒）。
    pub result: &'static str,
    /// 旗（逗号 join：SmallOrderPublicKey/MixedOrderPublicKey/
    /// SignatureMalleability 等——fail 记账入报告，诊断面）。
    pub flags: &'static str,
    /// 门期望（strict 映射产物：valid=true，invalid/acceptable=false）。
    pub expect_ok: bool,
}

/// fixture 指纹（sha256——重抓漂移即变，`--check`+selftest 双咬）。
pub const WY_FIXTURE_SHA256: &str = "ea80ffebac085e21cb9d3e903e7fa2b448d96702da5b31f061d0b83b430b74b3";
/// 向量源（fixture testGroups[].source 逐组去重——三源混编如实记：
/// google-wycheproof 主源 + 边界回归生成器组，见模块头注）。
pub const WY_SOURCES: &[&str] = &[
    "github/cpu/eddsazerosigngen 1.0",
    "github/paragonie/sodium_compat/boundary-regressions 2.5.2",
    "google-wycheproof 0.9rc5",
];
/// 向量总数（== fixture numberOfTests，两源核）。
pub const WY_TOTAL: usize = 210;
/// valid 数（expect_ok=true 数——strict 门期望通过数）。
pub const WY_VALID: usize = 117;
/// acceptable 数（strict 策略拒——小阶/混阶公钥拒收执法面）。
pub const WY_ACCEPTABLE: usize = 29;
/// invalid 数（验签方程不过——必须拒）。
pub const WY_INVALID: usize = 64;
/// 消息解码缓冲帽（全表最长 = tc83 1023B；
/// 与 RFC 批 MSG_BUF_MAX=1023 同值是数据巧合，非单源）。
pub const WY_MSG_MAX: usize = 1023;
/// 签名解码缓冲帽（全表最长 = tc34 96B
/// 带垃圾尾；最短 tc30 = 0B 空 sig）。
pub const WY_SIG_MAX: usize = 96;

/// Wycheproof ed25519 全向量（tc1..=tc210，fixture 原序。
/// 消费 = `vectors::run_wycheproof_gate_forced`（verify 回调批跑）。
pub const WYCHEPROOF_ED25519: &[WycheproofVector] = &[
    // tc1 [Valid] → valid（须过）
    WycheproofVector {
        name: "tc1",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "",
        sig: "d4fbdb52bfa726b44d1786a8c0d171c3e62ca83c9e5bbe63de0bb2483f8fd6cc\
              1429ab72cafc41ab56af02ff8fcc43b99bfe4c7ae940f60f38ebaa9d311c4007",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc2 [Valid] → valid（须过）
    WycheproofVector {
        name: "tc2",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "78",
        sig: "d80737358ede548acb173ef7e0399f83392fe8125b2ce877de7975d8b726ef5b\
              1e76632280ee38afad12125ea44b961bf92f1178c9fa819d020869975bcbe109",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc3 [Valid] → valid（须过）
    WycheproofVector {
        name: "tc3",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374",
        sig: "7c38e026f29e14aabd059a0f2db8b0cd783040609a8be684db12f82a27774ab0\
              7a9155711ecfaf7f99f277bad0c6ae7e39d4eef676573336a5c51eb6f946b30d",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc4 [Valid] → valid（须过）
    WycheproofVector {
        name: "tc4",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "48656c6c6f",
        sig: "1c1ad976cbaae3b31dee07971cf92c928ce2091a85f5899f5e11ecec90fc9f8e\
              93df18c5037ec9b29c07195ad284e63d548cd0a6fe358cc775bd6c1608d2c905",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc5 [Valid] → valid（须过）
    WycheproofVector {
        name: "tc5",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "657c1492402ab5ce03e2c3a7f0384d051b9cf3570f1207fc78c1bcc98c281c2b\
              f0cf5b3a289976458a1be6277a5055545253b45b07dcc1abd96c8b989c00f301",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc6 [Valid] → valid（须过）
    WycheproofVector {
        name: "tc6",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "000000000000000000000000",
        sig: "d46543bfb892f84ec124dcdfc847034c19363bf3fc2fa89b1267833a14856e52\
              e60736918783f950b6f1dd8d40dc343247cd43ce054c2d68ef974f7ed0f3c60f",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc7 [Valid] → valid（须过）
    WycheproofVector {
        name: "tc7",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "6161616161616161616161616161616161616161616161616161616161616161\
              6161616161616161616161616161616161616161616161616161616161616161\
              61",
        sig: "879350045543bc14ed2c08939b68c30d22251d83e018cacbaf0c9d7a48db577e\
              80bdf76ce99e5926762bc13b7b3483260a5ef63d07e34b58eb9c14621ac92f00",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc8 [Valid] → valid（须过）
    WycheproofVector {
        name: "tc8",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f\
              404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f\
              60",
        sig: "7bdc3f9919a05f1d5db4a3ada896094f6871c1f37afc75db82ec3147d84d6f23\
              7b7e5ecc26b59cfea0c7eaf1052dc427b0f724615be9c3d3e01356c65b9b5109",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc9 [Valid] → valid（须过）
    WycheproofVector {
        name: "tc9",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "ffffffffffffffffffffffffffffffff",
        sig: "5dbd7360e55aa38e855d6ad48c34bd35b7871628508906861a7c4776765ed7d1\
              e13d910faabd689ec8618b78295c8ab8f0e19c8b4b43eb8685778499e943ae04",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc10 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc10",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "0000000000000000000000000000000000000000000000000000000000000000\
              0000000000000000000000000000000000000000000000000000000000000000",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc11 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc11",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "0000000000000000000000000000000000000000000000000000000000000000\
              0100000000000000000000000000000000000000000000000000000000000000",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc12 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc12",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "0000000000000000000000000000000000000000000000000000000000000000\
              ecd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc13 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc13",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "0000000000000000000000000000000000000000000000000000000000000000\
              edd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc14 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc14",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "0000000000000000000000000000000000000000000000000000000000000000\
              edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc15 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc15",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "0100000000000000000000000000000000000000000000000000000000000000\
              0000000000000000000000000000000000000000000000000000000000000000",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc16 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc16",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "0100000000000000000000000000000000000000000000000000000000000000\
              0100000000000000000000000000000000000000000000000000000000000000",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc17 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc17",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "0100000000000000000000000000000000000000000000000000000000000000\
              ecd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc18 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc18",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "0100000000000000000000000000000000000000000000000000000000000000\
              edd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc19 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc19",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "0100000000000000000000000000000000000000000000000000000000000000\
              edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc20 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc20",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "edd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010\
              0000000000000000000000000000000000000000000000000000000000000000",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc21 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc21",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "edd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010\
              0100000000000000000000000000000000000000000000000000000000000000",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc22 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc22",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "edd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010\
              ecd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc23 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc23",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "edd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010\
              edd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc24 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc24",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "edd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010\
              edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc25 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc25",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f\
              0000000000000000000000000000000000000000000000000000000000000000",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc26 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc26",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f\
              0100000000000000000000000000000000000000000000000000000000000000",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc27 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc27",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f\
              ecd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc28 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc28",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f\
              edd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc29 [InvalidSignature] "special values for r and s" → invalid（须拒）
    WycheproofVector {
        name: "tc29",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "3f",
        sig: "edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f\
              edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
        result: "invalid",
        flags: "InvalidSignature",
        expect_ok: false,
    },
    // tc30 [TruncatedSignature] "empty signature" → invalid（须拒）
    WycheproofVector {
        name: "tc30",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374",
        sig: "",
        result: "invalid",
        flags: "TruncatedSignature",
        expect_ok: false,
    },
    // tc31 [TruncatedSignature] "s missing" → invalid（须拒）
    WycheproofVector {
        name: "tc31",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374",
        sig: "7c38e026f29e14aabd059a0f2db8b0cd783040609a8be684db12f82a27774ab0",
        result: "invalid",
        flags: "TruncatedSignature",
        expect_ok: false,
    },
    // tc32 [TruncatedSignature] "signature too short" → invalid（须拒）
    WycheproofVector {
        name: "tc32",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374",
        sig: "7c38e026f29e14aabd059a0f2db8b0cd783040609a8be684db12f82a27774ab0\
              7a9155711ecfaf7f99f277bad0c6ae7e39d4eef676573336a5c51eb6f946",
        result: "invalid",
        flags: "TruncatedSignature",
        expect_ok: false,
    },
    // tc33 [SignatureWithGarbage] "signature too long" → invalid（须拒）
    WycheproofVector {
        name: "tc33",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374",
        sig: "7c38e026f29e14aabd059a0f2db8b0cd783040609a8be684db12f82a27774ab0\
              7a9155711ecfaf7f99f277bad0c6ae7e39d4eef676573336a5c51eb6f946b30d\
              2020",
        result: "invalid",
        flags: "SignatureWithGarbage",
        expect_ok: false,
    },
    // tc34 [SignatureWithGarbage] "include pk in signature" → invalid（须拒）
    WycheproofVector {
        name: "tc34",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374",
        sig: "7c38e026f29e14aabd059a0f2db8b0cd783040609a8be684db12f82a27774ab0\
              7a9155711ecfaf7f99f277bad0c6ae7e39d4eef676573336a5c51eb6f946b30d\
              7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        result: "invalid",
        flags: "SignatureWithGarbage",
        expect_ok: false,
    },
    // tc35 [SignatureWithGarbage] "prepending 0 byte to signature" → invalid（须拒）
    WycheproofVector {
        name: "tc35",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374",
        sig: "007c38e026f29e14aabd059a0f2db8b0cd783040609a8be684db12f82a27774a\
              b07a9155711ecfaf7f99f277bad0c6ae7e39d4eef676573336a5c51eb6f946b3\
              0d",
        result: "invalid",
        flags: "SignatureWithGarbage",
        expect_ok: false,
    },
    // tc36 [SignatureWithGarbage] "prepending 0 byte to s" → invalid（须拒）
    WycheproofVector {
        name: "tc36",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374",
        sig: "7c38e026f29e14aabd059a0f2db8b0cd783040609a8be684db12f82a27774ab0\
              007a9155711ecfaf7f99f277bad0c6ae7e39d4eef676573336a5c51eb6f946b3\
              0d",
        result: "invalid",
        flags: "SignatureWithGarbage",
        expect_ok: false,
    },
    // tc37 [SignatureWithGarbage] "appending 0 byte to signature" → invalid（须拒）
    WycheproofVector {
        name: "tc37",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374",
        sig: "7c38e026f29e14aabd059a0f2db8b0cd783040609a8be684db12f82a27774ab0\
              7a9155711ecfaf7f99f277bad0c6ae7e39d4eef676573336a5c51eb6f946b30d\
              00",
        result: "invalid",
        flags: "SignatureWithGarbage",
        expect_ok: false,
    },
    // tc38 [CompressedSignature] "removing 0 byte from signature" → invalid（须拒）
    WycheproofVector {
        name: "tc38",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "546573743137",
        sig: "93de3ca252426c95f735cb9edd92e83321ac62372d5aa5b379786bae111ab6b1\
              7251330e8f9a7c30d6993137c596007d7b001409287535ac4804e662bc58a3",
        result: "invalid",
        flags: "CompressedSignature",
        expect_ok: false,
    },
    // tc39 [CompressedSignature] "removing 0 byte from signature" → invalid（须拒）
    WycheproofVector {
        name: "tc39",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374313236",
        sig: "dffed33a7f420b62bb1731cfd03be805affd18a281ec02b1067ba6e9d2082656\
              9e742347df59c88ae96db1f1969fb189b0ec34381d85633e1889da48d95e0e",
        result: "invalid",
        flags: "CompressedSignature",
        expect_ok: false,
    },
    // tc40 [CompressedSignature] "removing leading 0 byte from signature" → invalid（须拒）
    WycheproofVector {
        name: "tc40",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "546573743530",
        sig: "6e170c719577c25e0e1e8b8aa7a6346f8b109f37385cc2e85dc3b4c0f46a9c6b\
              cafd67f52324c5dbaf40a1b673fb29c4a56052d2d6999d0838a8337bccb502",
        result: "invalid",
        flags: "CompressedSignature",
        expect_ok: false,
    },
    // tc41 [CompressedSignature] "dropping byte from signature" → invalid（须拒）
    WycheproofVector {
        name: "tc41",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374333437",
        sig: "b0928b46e99fbbad3f5cb502d2cd309d94a7e86cfd4d84b1fcf4cea18075a9c3\
              6993c0582dba1e9e519fae5a8654f454201ae0c3cb397c37b8f4f8eef18400",
        result: "invalid",
        flags: "CompressedSignature",
        expect_ok: false,
    },
    // tc42 [InvalidEncoding] "modified bit 0 in R. Accepted only by verifiers
    //     that ignore this bit when comparing R to the recovered R." → invalid（须拒）
    WycheproofVector {
        name: "tc42",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "647c1492402ab5ce03e2c3a7f0384d051b9cf3570f1207fc78c1bcc98c281c2b\
              1d125e5538f38afbcc1c84e489521083041d24bc6240767029da063271a1ff0c",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc43 [InvalidEncoding] "modified bit 1 in R. Accepted only by verifiers
    //     that ignore this bit when comparing R to the recovered R." → invalid（须拒）
    WycheproofVector {
        name: "tc43",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "677c1492402ab5ce03e2c3a7f0384d051b9cf3570f1207fc78c1bcc98c281c2b\
              c108ca4b87a49c9ed2cf383aecad8f54a962b2899da891e12004d7993a627e01",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc44 [InvalidEncoding] "modified bit 2 in R. Accepted only by verifiers
    //     that ignore this bit when comparing R to the recovered R." → invalid（须拒）
    WycheproofVector {
        name: "tc44",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "617c1492402ab5ce03e2c3a7f0384d051b9cf3570f1207fc78c1bcc98c281c2b\
              9ce23fc6213ed5b87912e9bbf92f5e2c780eae26d15c50a112d1e97d2ea33c06",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc45 [InvalidEncoding] "modified bit 7 in R. Accepted only by verifiers
    //     that ignore this bit when comparing R to the recovered R." → invalid（须拒）
    WycheproofVector {
        name: "tc45",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "e57c1492402ab5ce03e2c3a7f0384d051b9cf3570f1207fc78c1bcc98c281c2b\
              bb3eb51cd98dddb235a5f46f2bded6af184a58d09cce928bda43f41d69118a03",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc46 [InvalidEncoding] "modified bit 8 in R. Accepted only by verifiers
    //     that ignore this bit when comparing R to the recovered R." → invalid（须拒）
    WycheproofVector {
        name: "tc46",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "657d1492402ab5ce03e2c3a7f0384d051b9cf3570f1207fc78c1bcc98c281c2b\
              cd237dda9a116501f67a5705a854b9adc304f34720803a91b324f2c13e0f5a09",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc47 [InvalidEncoding] "modified bit 16 in R. Accepted only by verifiers
    //     that ignore this bit when comparing R to the recovered R." → invalid（须拒）
    WycheproofVector {
        name: "tc47",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "657c1592402ab5ce03e2c3a7f0384d051b9cf3570f1207fc78c1bcc98c281c2b\
              6b167bbdc0d881cc04d28905552c1876f3709851abc5007376940cc8a435c300",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc48 [InvalidEncoding] "modified bit 31 in R. Accepted only by verifiers
    //     that ignore this bit when comparing R to the recovered R." → invalid（须拒）
    WycheproofVector {
        name: "tc48",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "657c1412402ab5ce03e2c3a7f0384d051b9cf3570f1207fc78c1bcc98c281c2b\
              7fd2ac7da14afffcceeb13f2a0d6b887941cb1a5eb57a52f3cb131a16cce7b0e",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc49 [InvalidEncoding] "modified bit 32 in R. Accepted only by verifiers
    //     that ignore this bit when comparing R to the recovered R." → invalid（须拒）
    WycheproofVector {
        name: "tc49",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "657c1492412ab5ce03e2c3a7f0384d051b9cf3570f1207fc78c1bcc98c281c2b\
              7373ba13ebbef99cd2a8ead55ce735c987d85a35320925a8e871702dc7c5c40d",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc50 [InvalidEncoding] "modified bit 63 in R. Accepted only by verifiers
    //     that ignore this bit when comparing R to the recovered R." → invalid（须拒）
    WycheproofVector {
        name: "tc50",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "657c1492402ab54e03e2c3a7f0384d051b9cf3570f1207fc78c1bcc98c281c2b\
              d35bd331c03f0855504ca1cab87b83c36a028425a3cf007ede4f4254c261cb00",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc51 [InvalidEncoding] "modified bit 64 in R. Accepted only by verifiers
    //     that ignore this bit when comparing R to the recovered R." → invalid（须拒）
    WycheproofVector {
        name: "tc51",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "657c1492402ab5ce02e2c3a7f0384d051b9cf3570f1207fc78c1bcc98c281c2b\
              cb35101f73cf467deac8c1a03b6c3dc35af544132734b7e57ab20c89b2e4750d",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc52 [InvalidEncoding] "modified bit 97 in R. Accepted only by verifiers
    //     that ignore this bit when comparing R to the recovered R." → invalid（须拒）
    WycheproofVector {
        name: "tc52",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "657c1492402ab5ce03e2c3a7f2384d051b9cf3570f1207fc78c1bcc98c281c2b\
              b58d2e8878290bff8d3355fdd4ea381924ee578752354eb6dee678ab4011c301",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc53 [InvalidEncoding] "modified bit 127 in R. Accepted only by verifiers
    //     that ignore this bit when comparing R to the recovered R." → invalid（须拒）
    WycheproofVector {
        name: "tc53",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "657c1492402ab5ce03e2c3a7f0384d851b9cf3570f1207fc78c1bcc98c281c2b\
              b978c866187ffb1cc7b29a0b4045aefc08768df65717194ff0c6e63f4dea0d02",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc54 [InvalidEncoding] "modified bit 240 in R. Accepted only by verifiers
    //     that ignore this bit when comparing R to the recovered R." → invalid（须拒）
    WycheproofVector {
        name: "tc54",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "657c1492402ab5ce03e2c3a7f0384d051b9cf3570f1207fc78c1bcc98c281d2b\
              0576ecf8eaf675f00f3dfbe19f75b83b7607a6c96414f6821af920a2498d0305",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc55 [InvalidEncoding] "modified bit 247 in R. Accepted only by verifiers
    //     that ignore this bit when comparing R to the recovered R." → invalid（须拒）
    WycheproofVector {
        name: "tc55",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "657c1492402ab5ce03e2c3a7f0384d051b9cf3570f1207fc78c1bcc98c289c2b\
              e5241a345c7b5428054c74b7c382fa10d4a5f1e8f8b79a71d3fdea2254f1ff0e",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc56 [InvalidEncoding] "modified bit 248 in R. Accepted only by verifiers
    //     that ignore this bit when comparing R to the recovered R." → invalid（须拒）
    WycheproofVector {
        name: "tc56",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "657c1492402ab5ce03e2c3a7f0384d051b9cf3570f1207fc78c1bcc98c281c2a\
              63950c85cd6dc96364e768de50ff7732b538f8a0b1615d799190ab600849230e",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc57 [InvalidEncoding] "modified bit 253 in R. Accepted only by verifiers
    //     that ignore this bit when comparing R to the recovered R." → invalid（须拒）
    WycheproofVector {
        name: "tc57",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "657c1492402ab5ce03e2c3a7f0384d051b9cf3570f1207fc78c1bcc98c281c0b\
              543bd3da0a56a8c9c152f59c9fec12f31fa66434d48b817b30d90cb4efa8b501",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc58 [InvalidEncoding] "modified bit 254 in R. Accepted only by verifiers
    //     that ignore this bit when comparing R to the recovered R." → invalid（须拒）
    WycheproofVector {
        name: "tc58",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "657c1492402ab5ce03e2c3a7f0384d051b9cf3570f1207fc78c1bcc98c281c6b\
              8da07efd07a6dafb015ed6a32fe136319a972ffbc341f3a0beae97ccf8136505",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc59 [InvalidEncoding] "modified bit 255 in R. This flips the sign of x
    //     and yields the canonical encoding of the negated R point. Accepted
    //     only by verifiers that ignore the sign of x when comparing R to the
    //     recovered R." → invalid（须拒）
    WycheproofVector {
        name: "tc59",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "657c1492402ab5ce03e2c3a7f0384d051b9cf3570f1207fc78c1bcc98c281cab\
              227aedf259f910f0f3a759a335062665217925d019173b88917eae294f75d40f",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc60 [InvalidEncoding] "R==0" → invalid（须拒）
    WycheproofVector {
        name: "tc60",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "0000000000000000000000000000000000000000000000000000000000000000\
              e0b8e7770d51c7a36375d006c5bffd6af43ff54aaf47e4330dc118c71d61ec02",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc61 [InvalidEncoding] "invalid R" → invalid（须拒）
    WycheproofVector {
        name: "tc61",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\
              463a1908382e7eb7693acef9884f7cf931a215e0791876be22c631a59881fd0e",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc62 [InvalidEncoding] "all bits flipped in R" → invalid（须拒）
    WycheproofVector {
        name: "tc62",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "313233343030",
        sig: "9a83eb6dbfd54a31fc1d3c580fc7b2fae4630ca8f0edf803873e433673d7e3d4\
              0e94254586cb6188c5386c3febed477cb9a6cb29e3979adc4cb27cf5278fb70a",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc63 [SignatureMalleability] "the encoded s was replaced by s + L. The
    //     scalar is unchanged mod L. Accepted only by verifiers that fail to
    //     enforce s < L. See RFC 8032, Section 5.1.7." → invalid（须拒）
    WycheproofVector {
        name: "tc63",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374",
        sig: "7c38e026f29e14aabd059a0f2db8b0cd783040609a8be684db12f82a27774ab0\
              67654bce3832c2d76f8f6f5dafc08d9339d4eef676573336a5c51eb6f946b31d",
        result: "invalid",
        flags: "SignatureMalleability",
        expect_ok: false,
    },
    // tc64 [SignatureMalleability] "the encoded s was replaced by s + 2L. The
    //     scalar is unchanged mod L. Accepted only by verifiers that fail to
    //     enforce s < L. See RFC 8032, Section 5.1.7." → invalid（须拒）
    WycheproofVector {
        name: "tc64",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374",
        sig: "7c38e026f29e14aabd059a0f2db8b0cd783040609a8be684db12f82a27774ab0\
              5439412b5395d42f462c67008eba6ca839d4eef676573336a5c51eb6f946b32d",
        result: "invalid",
        flags: "SignatureMalleability",
        expect_ok: false,
    },
    // tc65 [SignatureMalleability] "the encoded s was replaced by s + 4L. The
    //     scalar is unchanged mod L. Accepted only by verifiers that fail to
    //     enforce s < L. See RFC 8032, Section 5.1.7." → invalid（须拒）
    WycheproofVector {
        name: "tc65",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374",
        sig: "7c38e026f29e14aabd059a0f2db8b0cd783040609a8be684db12f82a27774ab0\
              2ee12ce5875bf9dff26556464bae2ad239d4eef676573336a5c51eb6f946b34d",
        result: "invalid",
        flags: "SignatureMalleability",
        expect_ok: false,
    },
    // tc66 [SignatureMalleability] "the encoded s was replaced by s + 8L. The
    //     scalar is unchanged mod L. Accepted only by verifiers that fail to
    //     enforce s < L. See RFC 8032, Section 5.1.7." → invalid（须拒）
    WycheproofVector {
        name: "tc66",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374",
        sig: "7c38e026f29e14aabd059a0f2db8b0cd783040609a8be684db12f82a27774ab0\
              e2300459f1e742404cd934d2c595a6253ad4eef676573336a5c51eb6f946b38d",
        result: "invalid",
        flags: "SignatureMalleability",
        expect_ok: false,
    },
    // tc67 [SignatureMalleability] "the encoded s was replaced by s + 2^253. The
    //     scalar differs from the valid s mod L, so verification fails even
    //     without a range check on s. Accepted only by parsers that silently
    //     clear bit 253 of s." → invalid（须拒）
    WycheproofVector {
        name: "tc67",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374",
        sig: "7c38e026f29e14aabd059a0f2db8b0cd783040609a8be684db12f82a27774ab0\
              7a9155711ecfaf7f99f277bad0c6ae7e39d4eef676573336a5c51eb6f946b32d",
        result: "invalid",
        flags: "SignatureMalleability",
        expect_ok: false,
    },
    // tc68 [SignatureMalleability] "the encoded s was replaced by s + 2^254. The
    //     scalar differs from the valid s mod L, so verification fails even
    //     without a range check on s. Accepted only by parsers that silently
    //     clear bit 254 of s." → invalid（须拒）
    WycheproofVector {
        name: "tc68",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374",
        sig: "7c38e026f29e14aabd059a0f2db8b0cd783040609a8be684db12f82a27774ab0\
              7a9155711ecfaf7f99f277bad0c6ae7e39d4eef676573336a5c51eb6f946b34d",
        result: "invalid",
        flags: "SignatureMalleability",
        expect_ok: false,
    },
    // tc69 [SignatureMalleability] "the encoded s was replaced by s + 2^255. The
    //     scalar differs from the valid s mod L, so verification fails even
    //     without a range check on s. Accepted only by parsers that silently
    //     clear bit 255 of s." → invalid（须拒）
    WycheproofVector {
        name: "tc69",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374",
        sig: "7c38e026f29e14aabd059a0f2db8b0cd783040609a8be684db12f82a27774ab0\
              7a9155711ecfaf7f99f277bad0c6ae7e39d4eef676573336a5c51eb6f946b38d",
        result: "invalid",
        flags: "SignatureMalleability",
        expect_ok: false,
    },
    // tc70 [SignatureMalleability] "the encoded s was replaced by s + p where p
    //     is the field prime 2^255 - 19. The scalar differs from the valid s mod
    //     L, so verification fails even without a range check on s. Accepted
    //     only by verifiers that reduce s mod p instead of checking s < L." → invalid（须拒）
    WycheproofVector {
        name: "tc70",
        pk: "7d4d0e7f6153a69b6242b522abbee685fda4420f8834b108c3bdae369ef549fa",
        msg: "54657374",
        sig: "7c38e026f29e14aabd059a0f2db8b0cd783040609a8be684db12f82a27774ab0\
              679155711ecfaf7f99f277bad0c6ae7e39d4eef676573336a5c51eb6f946b38d",
        result: "invalid",
        flags: "SignatureMalleability",
        expect_ok: false,
    },
    // tc71 [Valid] → valid（须过）
    WycheproofVector {
        name: "tc71",
        pk: "a12c2beb77265f2aac953b5009349d94155a03ada416aad451319480e983ca4c",
        msg: "",
        sig: "5056325d2ab440bf30bbf0f7173199aa8b4e6fbc091cf3eb6bc6cf87cd73d992\
              ffc216c85e4ab5b8a0bbc7e9a6e9f8d33b7f6e5ac0ffdc22d9fcaf784af84302",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc72 [Valid] → valid（须过）
    WycheproofVector {
        name: "tc72",
        pk: "a12c2beb77265f2aac953b5009349d94155a03ada416aad451319480e983ca4c",
        msg: "78",
        sig: "481fafbf4364d7b682475282f517a3ac0538c9a6b6a562e99a3d8e5afb4f90a5\
              59b056b9f07af023905753b02d95eb329a35c77f154b79abbcd291615ce42f02",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc73 [Valid] → valid（须过）
    WycheproofVector {
        name: "tc73",
        pk: "a12c2beb77265f2aac953b5009349d94155a03ada416aad451319480e983ca4c",
        msg: "54657374",
        sig: "8a9bb4c465a3863abc9fd0dd35d80bb28f7d33d37d74679802d63f82b20da114\
              b8d765a1206b3e9ad7cf2b2d8d778bb8651f1fa992db293c0039eacb6161480f",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc74 [Valid] → valid（须过）
    WycheproofVector {
        name: "tc74",
        pk: "a12c2beb77265f2aac953b5009349d94155a03ada416aad451319480e983ca4c",
        msg: "48656c6c6f",
        sig: "d839c20abfda1fd429531831c64f813f84b913e9928540310cf060b44c3dbf94\
              57d44a7721fdc0d67724ff81cb450dd39b10cfb65db15dda4b8bf09d26bd3801",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc75 [Valid] → valid（须过）
    WycheproofVector {
        name: "tc75",
        pk: "a12c2beb77265f2aac953b5009349d94155a03ada416aad451319480e983ca4c",
        msg: "313233343030",
        sig: "9bbb1052dcfa8ad2715c2eb716ae4f1902dea353d42ee09fd4c0b4fcb8b52b52\
              19e2200016e1199d0061891c263e31b0bc3b55673c19610c4e0fa5408004160b",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc76 [Valid] → valid（须过）
    WycheproofVector {
        name: "tc76",
        pk: "a12c2beb77265f2aac953b5009349d94155a03ada416aad451319480e983ca4c",
        msg: "000000000000000000000000",
        sig: "f63b5c0667c7897fc283296416f7f60e84bbde9cbd832e56be463ed9f5680697\
              02b17a2f7c341ebf590706a6388ac76ac613c1675ec0f2c7118f2573422a500b",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc77 [Valid] → valid（须过）
    WycheproofVector {
        name: "tc77",
        pk: "a12c2beb77265f2aac953b5009349d94155a03ada416aad451319480e983ca4c",
        msg: "6161616161616161616161616161616161616161616161616161616161616161\
              6161616161616161616161616161616161616161616161616161616161616161\
              61",
        sig: "1bc44d7001e6b5b9090fef34b2ca480f9786bbefa7d279353e5881e8dfb91b80\
              3ccd46500e270ef0109bfd741037558832120bc2a4f20fbe7b5fb3c3aaf23e08",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc78 [Valid] → valid（须过）
    WycheproofVector {
        name: "tc78",
        pk: "a12c2beb77265f2aac953b5009349d94155a03ada416aad451319480e983ca4c",
        msg: "202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f\
              404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f\
              60",
        sig: "ea8e22143b02372e76e99aece3ed36aec529768a27e2bb49bdc135d44378061e\
              1f62d1ac518f33ebf37b2ee8cc6dde68a4bd7d4a2f4d6cb77f015f71ca9fc30d",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc79 [Valid] → valid（须过）
    WycheproofVector {
        name: "tc79",
        pk: "a12c2beb77265f2aac953b5009349d94155a03ada416aad451319480e983ca4c",
        msg: "ffffffffffffffffffffffffffffffff",
        sig: "8acd679e1a914fc45d5fa83d3021f0509c805c8d271df54e52f43cfbd00cb622\
              2bf81d58fe1de2de378df67ee9f453786626961fe50a9b05f12b6f0899ebdd0a",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc80 [Ktv] "draft-josefsson-eddsa-ed25519-02: Test 1" → valid（须过）
    WycheproofVector {
        name: "tc80",
        pk: "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
        msg: "",
        sig: "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155\
              5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc81 [Ktv] "draft-josefsson-eddsa-ed25519-02: Test 2" → valid（须过）
    WycheproofVector {
        name: "tc81",
        pk: "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c",
        msg: "72",
        sig: "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da\
              085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc82 [Ktv] "draft-josefsson-eddsa-ed25519-02: Test 3" → valid（须过）
    WycheproofVector {
        name: "tc82",
        pk: "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025",
        msg: "af82",
        sig: "6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac\
              18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc83 [Ktv] "draft-josefsson-eddsa-ed25519-02: Test 1024" → valid（须过）
    WycheproofVector {
        name: "tc83",
        pk: "278117fc144c72340f67d0f2316e8386ceffbf2b2428c9c51fef7c597f1d426e",
        msg: "08b8b2b733424243760fe426a4b54908632110a66c2f6591eabd3345e3e4eb98\
              fa6e264bf09efe12ee50f8f54e9f77b1e355f6c50544e23fb1433ddf73be84d8\
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
              b2eaaa707b4c4185c32eddcdd306705e4dc1ffc872eeee475a64dfac86aba41c\
              0618983f8741c5ef68d3a101e8a3b8cac60c905c15fc910840b94c00a0b9d0",
        sig: "0aab4c900501b3e24d7cdf4663326a3a87df5e4843b2cbdb67cbf6e460fec350\
              aa5371b1508f9f4528ecea23c436d94b5e8fcd4f681e30a6ac00a9704a188a03",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc84 [Ktv] "Signature with S just under the bound. [David Benjamin]" → valid（须过）
    WycheproofVector {
        name: "tc84",
        pk: "100fdf47fb94f1536a4f7c3fda27383fa03375a8f527c537e6f1703c47f94f86",
        msg: "124e583f8b8eca58bb29c271b41d36986bbc45541f8e51f9cb0133eca447601e",
        sig: "dac119d6ca87fc59ae611c157048f4d4fc932a149dbe20ec6effd1436abf83ea\
              05c7df0fef06147241259113909bc71bd3c53ba4464ffcad3c0968f2ffffff0f",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc85 [InvalidKtv] "Signature with S just above the bound. [David Benjamin]" → invalid（须拒）
    WycheproofVector {
        name: "tc85",
        pk: "100fdf47fb94f1536a4f7c3fda27383fa03375a8f527c537e6f1703c47f94f86",
        msg: "6a0bc2b0057cedfc0fa2e3f7f7d39279b30f454a69dfd1117c758d86b19d85e0",
        sig: "0971f86d2c9c78582524a103cb9cf949522ae528f8054dc20107d999be673ff4\
              e25ebf2f2928766b1248bec6e91697775f8446639ede46ad4df4053000000010",
        result: "invalid",
        flags: "InvalidKtv",
        expect_ok: false,
    },
    // tc86 [Ktv] "Random test failure 1" → valid（须过）
    WycheproofVector {
        name: "tc86",
        pk: "8fd659b77b558ed93882c1157438450ac86ec62d421d568e98ee236f3810295a",
        msg: "b0729a713593a92e46b56eaa66b9e435f7a09a8e7de03b078f6f282285276635\
              f301e7aaafe42187c45d6f5b13f9f16b11195cc125c05b90d24dfe4c",
        sig: "7db17557ac470c0eda4eedaabce99197ab62565653cf911f632ee8be0e5ffcfc\
              88fb94276b42e0798fd3aa2f0318be7fc6a29fae75f70c3dcdc414a0ad866601",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc87 [Ktv] "Random test failure 2" → valid（须过）
    WycheproofVector {
        name: "tc87",
        pk: "2a606bf67ac770c607038b004101b325edb569efd3413d2d1f2c3e6b4e6e3082",
        msg: "a8546e50ba31cae3234310d32672447be213fad91a227a19669c53d309b95978\
              2b0e6b71f8791fdb470043b58122003157d2d96a43a6cbd7d3a8d86bf4c97391\
              883e268d50af80e1e6e12939c2bd50ca746cdadfad4edf1bda87529974072414\
              8efb1ebe73fb60088cda890317658627a5f7ab5a0c075d9d8f3f97b6492b3551\
              9e50ff6b38377432a7081f9176bb1c29a862deac1336ca20b097a47829cec10a\
              6a7cec178eda2d12f6dc6c87f910454af0123555ba184e68804d9cced60fd5c8\
              c90943e56599c8f0ba59a38491ba5e5a53460682474c07e40ca142983314fd76\
              2856bb1093f359da6eb0a756bd93a3160c10dd8feea6b97e7c6a17cb54bd5d76\
              49c05c66d7bdee056671dfdaf689fa3945bb8e29a429f4bd5d355dce9687b06f\
              01d5e33e3999f0e8",
        sig: "67d84d4c3945aaf06e06d524be63acbfb5dbb1988c4aea96a5ee9f7a9b9eecc2\
              9df4f66b8aa1d9e8607a58fb1ef0c2ad69aac005b4f58e34103344a9c8871a09",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc88 [Ktv] "Random test failure 3" → valid（须过）
    WycheproofVector {
        name: "tc88",
        pk: "c9c946cbc5544ac74eef491f07c5881c16faf7ec31ce4aa91bb60ae7b4539051",
        msg: "cd2212eddb0706f62c995cef958634f0cb7793444cbf4d30e81c27c41ebea6cb\
              02607510131f9c015692dfd521b148841e9a2d3564d20ac401f6cb8e40f520fe\
              0cafbeaa88840b83013369d879f013463fe52a13267aa0c8c59c45cde9399cd1\
              e6be8cc64cf48315ac2eb31a1c567a4fb7d601746d1f63b5ac020712adbbe075\
              19bded6f",
        sig: "24087d47f3e20af51b9668ae0a88ce76586802d0ec75d8c0f28fc30962b5e1d1\
              a1d509571a1624ed125a8df92a6e963728d6b5de99200b8e285f70feb6f05207",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc89 [Ktv] "Random test failure 4" → valid（须过）
    WycheproofVector {
        name: "tc89",
        pk: "32ad026f693d0d2afe7f4388d91c4c964426fcb9e3665c3ebd8650009b815c8e",
        msg: "ec5c7cb078",
        sig: "d920d421a5956b69bfe1ba834c025e2babb6c7a6d78c97de1d9bb1116dfdd118\
              5147b2887e34e15578172e150774275ea2aad9e02106f7e8ca1caa669a066f0c",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc90 [Ktv] "Random test failure 5" → valid（须过）
    WycheproofVector {
        name: "tc90",
        pk: "32ad026f693d0d2afe7f4388d91c4c964426fcb9e3665c3ebd8650009b815c8e",
        msg: "4668c6a76f0e482190a7175b9f3806a5fe4314a004fa69f988373f7a",
        sig: "4f62daf7f7c162038552ad7d306e195baa37ecf6ca7604142679d7d1128e1f8a\
              f52e4cb3545748c44ef1ff1c64e877e4f4d248259b7f6eb56e3ef72097dc8e0c",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc91 [Ktv] "Random test failure 6" → valid（须过）
    WycheproofVector {
        name: "tc91",
        pk: "c29ec1894e06d27b4e40486b4fa5063d66a746c7f9c323b12203c03b72b8b78a",
        msg: "0f325ffd87e58131ffa23c05ea4579513b287fdba87b44",
        sig: "6669acf94667c5b541afe5307bde9476b13ae7e0e6058a772101ac8eb0a94331\
              428eb4db0a2c68a9b6c1763b8624dab259b0876cdcfaeacc17b21a18e3fc010a",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc92 [Ktv] "Random test failure 7" → valid（须过）
    WycheproofVector {
        name: "tc92",
        pk: "cfda5b899e35764c5229e59295fe1222b7ddce176643697c29e46ecbba10cf10",
        msg: "ec5c7cb078",
        sig: "30490c28f806298225df62103521dcee047153912c33ab8ab8bbdd1ffabd70fd\
              4fdb360f05be535b067d1cf4e78c2cb432206bf280aab3bd21aaa1cb894c5b06",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc93 [Ktv] "Random test failure 8" → valid（须过）
    WycheproofVector {
        name: "tc93",
        pk: "32ad026f693d0d2afe7f4388d91c4c964426fcb9e3665c3ebd8650009b815c8e",
        msg: "5dc9bb87eb11621a93f92abe53515697d2611b2eef73",
        sig: "deecafb6f2ede73fec91a6f10e45b9c1c61c4b9bfbe6b6147e2de0b1df693897\
              1f7896c3ab83851fb5d9e537037bff0fca0ccb4a3cc38f056f91f7d7a0557e08",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc94 [Ktv] "Random test failure 9" → valid（须过）
    WycheproofVector {
        name: "tc94",
        pk: "cfda5b899e35764c5229e59295fe1222b7ddce176643697c29e46ecbba10cf10",
        msg: "67484059b2490b1a0a4f8dee77979e26",
        sig: "4cd4f77ed473a6647387f3163541c67a1708a3c3bd1673247cb87f0cb68b3c56\
              f04bfa72970c8a483efe659c87009ab4020b590b6641316b3deddb5450544e02",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc95 [Ktv] "Random test failure 10" → valid（须过）
    WycheproofVector {
        name: "tc95",
        pk: "32ad026f693d0d2afe7f4388d91c4c964426fcb9e3665c3ebd8650009b815c8e",
        msg: "7dcfe60f881e1285676f35b68a1b2dbcdd7be6f719a288ababc28d36e3a42ac3\
              010a1ca54b32760e74",
        sig: "7f8663cf98cbd39d5ff553f00bcf3d0d520605794f8866ce75714d77cc51e66c\
              91818b657d7b0dae430a68353506edc4a714c345f5ddb5c8b958ba3d035f7a01",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc96 [Ktv] "Random test failure 11" → valid（须过）
    WycheproofVector {
        name: "tc96",
        pk: "cfda5b899e35764c5229e59295fe1222b7ddce176643697c29e46ecbba10cf10",
        msg: "a020a4381dc9141f47ee508871ab7a8b5a3648727c4281ae9932376f23a8e1bc\
              da0626b7129197d864178631ec89c4332dbb18",
        sig: "1e41a24fe732bd7cab14c2a2f5134ee8c87fcbd2e987e60957ed9239e5c32404\
              d56977e1b4282871896cb10625a1937468e4dc266e16a9c1b8e9891177eca802",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc97 [Ktv] "Random test failure 12" → valid（须过）
    WycheproofVector {
        name: "tc97",
        pk: "32ad026f693d0d2afe7f4388d91c4c964426fcb9e3665c3ebd8650009b815c8e",
        msg: "58e456064dff471109def4ca27fa8310a1df32739655b624f27e6418d34b7f00\
              7173f3faa5",
        sig: "6aab49e5c0bc309b783378ee03ffda282f0185cdf94c847701ff307a6ee8d086\
              5411c44e0a8206f6a5f606107451940c2593af790ce1860f4c14ab25b2deae08",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc98 [Ktv] "Random test failure 13" → valid（须过）
    WycheproofVector {
        name: "tc98",
        pk: "529919c9c780985a841c42ba6c180ff2d67a276ccfbe281080e47ab71a758f56",
        msg: "e1cbf2d86827825613fb7a85811d",
        sig: "01abfa4d6bbc726b196928ec84fd03f0c953a4fa2b228249562ff1442a4f63a7\
              150b064f3712b51c2af768d2c2711a71aabf8d186833e941a0301b82f0502905",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc99 [Ktv] "Random test failure 14" → valid（须过）
    WycheproofVector {
        name: "tc99",
        pk: "cfda5b899e35764c5229e59295fe1222b7ddce176643697c29e46ecbba10cf10",
        msg: "a25176b3afea318b2ec11ddacb10caf7179c0b3f8eabbfa2895581138d3c1e0e",
        sig: "2a833aadecd9f28235cb5896bf3781521dc71f28af2e91dbe1735a61dce3e31a\
              c15ca24b3fc47817a59d386bbbb2ce60a6adc0a2703bb2bdea8f70f91051f706",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc100 [Ktv] "Random test failure 15" → valid（须过）
    WycheproofVector {
        name: "tc100",
        pk: "32ad026f693d0d2afe7f4388d91c4c964426fcb9e3665c3ebd8650009b815c8e",
        msg: "a1",
        sig: "1a74ed2cbdc7d8f3827014e8e6ecf8fd2698ac8f86833acccdd400df710fe0d6\
              b0543c9cfa00d52bf024ab7ce0d91981944097233ec134d5c7abbd44bfd32d0d",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc101 [Ktv] "Random test failure 16" → valid（须过）
    WycheproofVector {
        name: "tc101",
        pk: "2252b3d57c74cbf8bc460dc2e082847926bc022f09ab6ae95756362bfd1167c1",
        msg: "975ef941710071a9e1e6325a0c860becd7c695b5117c3107b686e330e5",
        sig: "af0fd9dda7e03e12313410d8d8844ebb6fe6b7f65141f22d7bcba5695a25414a\
              9e54326fb44d59fb14707899a8aae70857b23d4080d7ab2c396ef3a36d45ce02",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc102 [Ktv] "Random test failure 17" → valid（须过）
    WycheproofVector {
        name: "tc102",
        pk: "c0a773110f975de3732355bb7ec7f0c41c091c0252966070205516693b992a4a",
        msg: "",
        sig: "0280427e713378f49d478df6373c6cac847b622b567daa2376c839e7ac10e22c\
              380ab0fa8617c9dcfe76c4d9db5459b21dc1413726e46cc8f387d359e344f407",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc103 [Ktv] "Random test failure 18" → valid（须过）
    WycheproofVector {
        name: "tc103",
        pk: "cfda5b899e35764c5229e59295fe1222b7ddce176643697c29e46ecbba10cf10",
        msg: "a9e6d94870a67a9fe1cf13b1e6f9150cdd407bf6480ec841ea586ae3935e9787\
              163cf419c1",
        sig: "c97e3190f83bae7729ba473ad46b420b8aad735f0808ea42c0f898ccfe6addd4\
              fd9d9fa3355d5e67ee21ab7e1f805cd07f1fce980e307f4d7ad36cc924eef00c",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc104 [Ktv] "Random test failure 19" → valid（须过）
    WycheproofVector {
        name: "tc104",
        pk: "32ad026f693d0d2afe7f4388d91c4c964426fcb9e3665c3ebd8650009b815c8e",
        msg: "11cb1eafa4c42a8402c4193c4696f7b2e6d4585e4b42dcf1a8b67a80b2da80bc\
              9d4b649fb2f35eaf1f56c426fd0b",
        sig: "14ceb2eaf4688d995d482f44852d71ad878cd7c77b41e60b0065fd01a59b054e\
              e74759224187dbde9e59a763a70277c960892ef89fba997aba2576b2c54ba608",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc105 [Ktv] "Random test failure 20" → valid（须过）
    WycheproofVector {
        name: "tc105",
        pk: "c9c946cbc5544ac74eef491f07c5881c16faf7ec31ce4aa91bb60ae7b4539051",
        msg: "27d465bc632743522aefa23c",
        sig: "c2656951e2a0285585a51ff0eda7e9a23c2dfd2ffa273aee7808f4604e8f9a8c\
              8ea49e9fce4eb2d8d75d36b7238fe6fc13b6c5d9427dd58f8c6615d033c0bd0f",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc106 [Ktv] "Random test failure 21" → valid（须过）
    WycheproofVector {
        name: "tc106",
        pk: "c29ec1894e06d27b4e40486b4fa5063d66a746c7f9c323b12203c03b72b8b78a",
        msg: "5ffa",
        sig: "931e5152fcef078c22cc5d6a3a65f06e396289f6f5f2d1efa6340254a53526ef\
              5dc6874eeddf35c3f50991c53cd02bf06313e37d93ee1f7022128ffa3b8f300b",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc107 [Ktv] "Random test failure 22" → valid（须过）
    WycheproofVector {
        name: "tc107",
        pk: "529919c9c780985a841c42ba6c180ff2d67a276ccfbe281080e47ab71a758f56",
        msg: "25",
        sig: "e4ae21f7a8f4b3b325c161a8c6e53e2edd7005b9c2f8a2e3b0ac4ba94aa80be6\
              f2ee22ac8d4a96b9a3eb73a825e7bb5aff4a3393bf5b4a38119e9c9b1b041106",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc108 [Ktv] "Random test failure 23" → valid（须过）
    WycheproofVector {
        name: "tc108",
        pk: "2252b3d57c74cbf8bc460dc2e082847926bc022f09ab6ae95756362bfd1167c1",
        msg: "80fdd6218f29c8c8f6bd820945f9b0854e3a8824",
        sig: "e097e0bd0370bff5bde359175a11b728ee9639095d5df8eda496395565616edf\
              e079977f7d4dc8c75d6113a83d6a55e6e1676408c0967a2906339b43337dcb01",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc109 [Ktv] "Random test failure 24" → valid（须过）
    WycheproofVector {
        name: "tc109",
        pk: "2a606bf67ac770c607038b004101b325edb569efd3413d2d1f2c3e6b4e6e3082",
        msg: "b477b0480bb84642608b908d29a51cf2fce63f24ee95",
        sig: "28fafbb62b4d688fa79e1ac92851f46e319b161f801d4dc09acc21fdd6780a2c\
              4292b8c1003c61c2bcebe7f3f88ccc4bb26d407387c5f27cb8c94cf6ce810405",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc110 [Ktv] "Random test failure 25" → valid（须过）
    WycheproofVector {
        name: "tc110",
        pk: "32ad026f693d0d2afe7f4388d91c4c964426fcb9e3665c3ebd8650009b815c8e",
        msg: "aa365b442d12b7f3c925",
        sig: "83c40ce13d483cc58ff65844875862d93df4bd367af77efa469ec06a8ed9e6d7\
              905a04879535708ddf225567a815c9b941d405c98e918fd0c151165cea7fb101",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc111 [Ktv] "Random test failure 26" → valid（须过）
    WycheproofVector {
        name: "tc111",
        pk: "54cda623245759ad6d43e620a606908befc633d60792bc7798447a0ef38e7311",
        msg: "27e792b28b2f1702",
        sig: "14d9b497c19b91d43481c55bb6f5056de252d9ecb637575c807e58e9b4c5eac8\
              b284089d97e2192dc242014363208e2c9a3435edf8928fb1d893553e9be4c703",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc112 [Ktv] "Random test failure 27" → valid（须过）
    WycheproofVector {
        name: "tc112",
        pk: "2362bac514d5fad33802642e979a1e82de6eb6f1bcbf6a5b304f2bb02b9e57fe",
        msg: "eef3bb0f617c17d0420c115c21c28e3762edc7b7fb048529b84a9c2bc6",
        sig: "242ddb3a5d938d07af690b1b0ef0fa75842c5f9549bf39c8750f75614c712e7c\
              baf2e37cc0799db38b858d41aec5b9dd2fca6a3c8e082c10408e2cf3932b9d08",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc113 [Ktv] "Random test failure 28" → valid（须过）
    WycheproofVector {
        name: "tc113",
        pk: "32ad026f693d0d2afe7f4388d91c4c964426fcb9e3665c3ebd8650009b815c8e",
        msg: "475f",
        sig: "71a4a06a34075f2fd47bc3abf4714d46db7e97b08cb6180d3f1539ac50b18ce5\
              1f8af8ae95ed21d4fa0daab7235925631ecea1fd9d0d8a2ba7a7583fd04b900c",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc114 [Ktv] "Test case for overflow in signature generation" → valid（须过）
    WycheproofVector {
        name: "tc114",
        pk: "037b55b427dc8daa0f80fcebaf0846902309f8a6cf18b465c0ce9b6539629ac8",
        msg: "01234567",
        sig: "c964e100033ce8888b23466677da4f4aea29923f642ae508f9d0888d78815063\
              6ab9b2c3765e91bbb05153801114d9e52dc700df377212222bb766be4b8c020d",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc115 [Ktv] "Test case for overflow in signature generation" → valid（须过）
    WycheproofVector {
        name: "tc115",
        pk: "9c0007698f177998a7666c7cf7973e2b88e9c4946e33804a7bbe8968d2394b2e",
        msg: "9399a6db9433d2a28d2b0c11c8794ab7d108c95b",
        sig: "176065c6d64a136a2227687d77f61f3fca3b16122c966276fd9a8b14a1a2cea4\
              c33b3533d11101717016684e3810efbea63bb23773f7cc480174199abd734f08",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc116 [Ktv] "Test case for overflow in signature generation" → valid（须过）
    WycheproofVector {
        name: "tc116",
        pk: "ed3a6f9721dc9729c1f76635bcf080d7036e1c2f0228654ccbbe1e738c17b963",
        msg: "7af783afbbd44c1833ab7237ecaf63b94ffdd003",
        sig: "7ca69331eec8610d38f00e2cdbd46966cb359dcde98a257ac6f362cc00c8f4fe\
              85c02285fe4d66e31a44cadb2bf474e1a7957609eb4fe95a71473fe6699aa70d",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc117 [Ktv] "Test case for overflow in signature generation" → valid（须过）
    WycheproofVector {
        name: "tc117",
        pk: "4abfb535313705a6570018440cdec1a3ae33e51f352112fa6acbd0c6bc3ea859",
        msg: "321b5f663c19e30ee7bbb85e48ecf44db9d3f512",
        sig: "f296715e855d8aecccba782b670163dedc4458fe4eb509a856bcac450920fd2e\
              95a3a3eb212d2d9ccaf948c39ae46a2548af125f8e2ad9b77bd18f92d59f9200",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc118 [Ktv] "Test case for overflow in signature generation" → valid（须过）
    WycheproofVector {
        name: "tc118",
        pk: "4f2162e6bf03a712db0efa418b7e7006e23871d9d7ec555a313885c4afd96385",
        msg: "c48890e92aeeb3af04858a8dc1d34f16a4347b91",
        sig: "367d07253a9d5a77d054b9c1a82d3c0a448a51905343320b3559325ef4183960\
              8aa45564978da1b2968c556cfb23b0c98a9be83e594d5e769d69d1156e1b1506",
        result: "valid",
        flags: "Ktv",
        expect_ok: true,
    },
    // tc119 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc119",
        pk: "4abfb535313705a6570018440cdec1a3ae33e51f352112fa6acbd0c6bc3ea859",
        msg: "321b5f663c19e30ee7bbb85e48ecf44db9d3f512",
        sig: "f296715e855d8aecccba782b670163dedc4458fe4eb509a856bcac450920fd2e\
              95a3a3eb212d2d9ccaf948c39ae46a2548af125f8e2ad9b77bd18f92d59f9200",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc120 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc120",
        pk: "4f2162e6bf03a712db0efa418b7e7006e23871d9d7ec555a313885c4afd96385",
        msg: "c48890e92aeeb3af04858a8dc1d34f16a4347b91",
        sig: "367d07253a9d5a77d054b9c1a82d3c0a448a51905343320b3559325ef4183960\
              8aa45564978da1b2968c556cfb23b0c98a9be83e594d5e769d69d1156e1b1506",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc121 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc121",
        pk: "0717d75ce27ea181ed5a30e6456c649b5cf453a6b4c12cd3f9fd16b31e0c25cd",
        msg: "26d5f0631f49106db58c4cfc903691134811b33c",
        sig: "9588e02bc815649d359ce710cdc69814556dd8c8bab1c468f40a49ebefb7f0de\
              7ed49725edfd1b708fa1bad277c35d6c1b9c5ec25990997645780f9203d7dd08",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc122 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc122",
        pk: "db5b9eab7e84e5a13505865fa711c9c896c898609fc11fc9bc1e55028f9496df",
        msg: "2a71f064af982a3a1103a75cef898732d7881981",
        sig: "2217a0be57dd0d6c0090641496bcb65e37213f02a0df50aff0368ee2808e1376\
              504f37b37494132dfc4d4887f58b9e86eff924040db3925ee4f8e1428c4c500e",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc123 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc123",
        pk: "7bac18f6d2625d3915f233434cda38a577247a7332a5170b37142a34644145e0",
        msg: "bf26796cef4ddafcf5033c8d105057db0210b6ad",
        sig: "1fda6dd4519fdbefb515bfa39e8e5911f4a0a8aa65f40ef0c542b8b34b87f9c2\
              49dc57f320718ff457ed5915c4d0fc352affc1287724d3f3a9de1ff777a02e01",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc124 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc124",
        pk: "7bac18f6d2625d3915f233434cda38a577247a7332a5170b37142a34644145e0",
        msg: "bf26796cef4ddafcf5033c8d105057db0210b6ad",
        sig: "1fda6dd4519fdbefb515bfa39e8e5911f4a0a8aa65f40ef0c542b8b34b87f9c2\
              49dc57f320718ff457ed5915c4d0fc352affc1287724d3f3a9de1ff777a02e01",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc125 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc125",
        pk: "38ead304624abebf3e2b31e20e5629531e3fc659008887c9106f5e55adbbc62a",
        msg: "ae03da6997e40cea67935020152d3a9a365cc055",
        sig: "068eafdc2f36b97f9bae7fbda88b530d16b0e35054d3a351e3a4c914b22854c7\
              11505e49682e1a447e10a69e3b04d0759c859897b64f71137acf355b63faf100",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc126 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc126",
        pk: "e9bc95049af7e4817b17c402269ba5e767b7348757ac8002fec9e08390c0a9cf",
        msg: "489d473f7fb83c7f6823baf65482517bccd8f4ea",
        sig: "43670abc9f09a8a415e76f4a21c6a46156f066b5a37b3c1e867cf67248c7b927\
              e8d13a763e37abf936f5f27f7a8aa290539d21f740efd26b65fd5ad27085f400",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc127 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc127",
        pk: "ee8155ca4e8fe7bc5bca5992044eab7f8c3c6a13db1176f42f46c29da5b064f4",
        msg: "1b704d6692d60a07ad1e1d047b65e105a80d3459",
        sig: "56388f2228893b14ce4f2a5e0cc626591061de3a57c50a5ecab7b9d5bb2caeea\
              191560a1cf2344c75fdb4a085444aa68d727b39f498169eaa82cf64a31f59803",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc128 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc128",
        pk: "db507bfcc9576393f7157bb360532b05c5fcf2e764b690cc6698a4a30d349095",
        msg: "dc87030862c4c32f56261e93a367caf458c6be27",
        sig: "553e5845fc480a577da6544e602caadaa00ae3e5aa3dce9ef332b1541b6d5f21\
              bdf1d01e98baf80b8435f9932f89b3eb70f02da24787aac8e77279e797d0bd0b",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc129 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc129",
        pk: "994eaf03309d6ad9d95a656bc1744e2886f029023a3750b34f35086b3c7227f8",
        msg: "7f41ef68508343ef18813cb2fb332445ec6480cd",
        sig: "bc10f88081b7be1f2505b6e76c5c82e358cf21ec11b7df1f334fb587bada465b\
              53d9f7b4d4fec964432ee91ead1bc32ed3c82f2167da1c834a37515df7fe130e",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc130 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc130",
        pk: "127d37e406e0d83e4b55a09e21e8f50fb88af47e4a43f018cdebffc1948757f0",
        msg: "e1ce107971534bc46a42ac609a1a37b4ca65791d",
        sig: "00c11e76b5866b7c37528b0670188c1a0473fb93c33b72ae604a8865a7d6e094\
              ff722e8ede3cb18389685ff3c4086c29006047466f81e71a329711e0b9294709",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc131 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc131",
        pk: "d83ba84edfb4bec49f29be31d80a64b7c0b5a502438cdb1d0dd1e0e3e55786de",
        msg: "869a827397c585cf35acf88a8728833ab1c8c81e",
        sig: "0a6f0ac47ea136cb3ff00f7a96638e4984048999ee2da0af6e5c86bffb0e70bb\
              97406b6ad5a4b764f7c99ebb6ec0fd434b8efe253b0423ef876c037998e8ab07",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc132 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc132",
        pk: "d3c9aa2f3d6ef217a166e8ae403ed436c37facbbe3beceb78df6eb439f8fa04a",
        msg: "619d8c4f2c93104be01cd574a385ceca08c33a9e",
        sig: "b7cbb942a6661e2312f79548224f3e44f5841c6e880c68340756a00ce94a914e\
              8404858265985e6bb97ef01d2d7e5e41340309606bfc43c8c6a8f925126b3d09",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc133 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc133",
        pk: "d53280367c1c0b95ac4112218b92c6a71c51fb6312ce668de196c7d52a136155",
        msg: "5257a0bae8326d259a6ce97420c65e6c2794afe2",
        sig: "27a4f24009e579173ff3064a6eff2a4d20224f8f85fdec982a9cf2e6a3b51537\
              348a1d7851a3a932128a923a393ea84e6b35eb3473c32dceb9d7e9cab03a0f0d",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc134 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc134",
        pk: "94ac2336ba97a476fb4c9f2b5563e4167ca292c6e99e422350a911ae3172c315",
        msg: "5acb6afc9b368f7acac0e71f6a4831c72d628405",
        sig: "985b605fe3f449f68081197a68c714da0bfbf6ac2ab9abb0508b6384ea4999cb\
              8d79af98e86f589409e8d2609a8f8bd7e80aaa8d92a84e7737fbe8dcef41920a",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc135 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc135",
        pk: "e1e7316d231f7f275bdf403360304da1509fdf1af1fd25ca214eaac0a289398f",
        msg: "3c87b3453277b353941591fc7eaa7dd37604b42a",
        sig: "1c8fbda3d39e2b441f06da6071c13115cb4115c7c3341704cf6513324d4cf1ef\
              4a1dd7678a048b0dde84e48994d080befcd70854079d44b6a0b0f9fa002d130c",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc136 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc136",
        pk: "fffbeea71215efaf9888fec2cc68edb3703ff11a66fd629b53cbda5eabc18750",
        msg: "0a68e27ef6847bfd9e398b328a0ded3679d4649d",
        sig: "59097233eb141ed948b4f3c28a9496b9a7eca77454ecfe7e46737d1449a0b76b\
              15aacf77cf48af27a668aa4434cfa26c504d75a2bcc4feac46465446234c0508",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc137 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc137",
        pk: "19ccc0527599cb032e0b4c4d74e60f13901768a99df041c3bc1bf6c0ef271169",
        msg: "4e9bef60737c7d4dd10bd52567e1473a36d3573d",
        sig: "519105608508fe2f1b6da4cc8b23e39798b1d18d25972beed0404cec722e01ba\
              1b6a0f85e99e092cca8076b101b60d4ac5035684357f4d0daacdc642da742a06",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc138 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc138",
        pk: "0e726e27047563aa0a1a9c2e085d8d26af2acba129d0869c65031e3e6cac329a",
        msg: "cc82b3163efda3ba7e9240e765112caa69113694",
        sig: "d8b03ee579e73f16477527fc9dc37a72eaac0748a733772c483ba013944f01ef\
              64fb4ec5e3a95021dc22f4ae282baff6e9b9cc8433c6b6710d82e7397d72ef04",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc139 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc139",
        pk: "e77717b54a2b5e5bce5bccb8f0c5fdb5fd7df77ac254020fc9120dc0d4df4178",
        msg: "923a5c9e7b5635bb6c32c5a408a4a15b652450eb",
        sig: "26da61fdfd38e6d01792813f27840c8b4766b0faaed39d0ee898cb450d94a5d5\
              f57e58b6a003d7f9b56b20561954c6edcf66492d116b8b5e91f205a3a6449d0b",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc140 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc140",
        pk: "6220972d3f7d150b36790d7d522384876d64d640cd9913186815e1629582ed36",
        msg: "6f2f0245de4587062979d0422d349f93ccdc3af2",
        sig: "4adeaff7a58c5010a5a067feea0ae504d37b0c6a76c6c153e222f13409dff2df\
              0fab69bc5059b97d925dc1b89e9851d7c627cb82d65585f9fd976124553f8902",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc141 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc141",
        pk: "7b64a28c50ec7678a90e3e1a21522e30ac9db7b5215aea2bfb33bea037eab987",
        msg: "6e911edb27a170b983d4dee1110554f804330f41",
        sig: "4204d620cde0c3008c0b2901f5d6b44f88f0e3cb4f4d62252bf6f3cb37c1fb15\
              0a9ccb296afe5e7c75f65b5c8edd13dc4910ffe1e1265b3707c59042cf9a5902",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc142 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc142",
        pk: "724452210a9e4c994819229bf12bf84e95768a3a97c08d8d8f5f939a4cad34c5",
        msg: "b8cf807eea809aaf739aa091f3b7a3f2fd39fb51",
        sig: "f8a69d3fd8c2ff0a9dec41e4c6b43675ce08366a35e220b1185ffc246c339e22\
              c20ac661e866f52054015efd04f42eca2adcee6834c4df923b4a62576e4dff0e",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc143 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc143",
        pk: "bad265b294ed2f422cb6a141694086238fbfe987571aa765d8b4f3a24105aa01",
        msg: "01a2b5f7fee813b4e9bd7fc25137648004795010",
        sig: "61792c9442bc6338ac41fd42a40bee9b02ec1836503d60ff725128c63d728088\
              80c36e6190b7da525cbee5d12900aa043547dd14a2709ef9e49d628f37f6b70c",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc144 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc144",
        pk: "0aaee4b723db9b51ba7d22eb23eb8a76a5ac02f4fc9dd06f77bea42e1d37ec5a",
        msg: "0fbf5d47cb5d498feace8f98f1896208da38a885",
        sig: "fa3cd41e3a8c00b19eecd404a63c3cb787cd30de0dfc936966cff2117f5aff18\
              db6bef80fcfd8856f3fb2e9c3dc47593e9471103032af918feee638a33d40505",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc145 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc145",
        pk: "812344af15a91ba83c2c91e96f1727ac0f3c4c41385b9fa84efa399ada5168be",
        msg: "36e67c1939750bffb3e4ba6cb85562612275e862",
        sig: "97fbbcd7a1d0eb42d2f8c42448ef35a2c2472740556b645547865330d6c57068\
              af377fced08aaf810c08cd3c43d296f1975710312e9334c98b485f831efa4103",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc146 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc146",
        pk: "0ee5cb5597fbdf8dccc48b01485e39b33aa133b52d30d23740277267cfec3e3e",
        msg: "13945c894c1d3fe8562e8b20e5f0efaa26ade8e3",
        sig: "d7dbaa337ffd2a5fd8d5fd8ad5aeccc0c0f83795c2c59fe62a40b87903b1ae62\
              ed748a8df5af4d32f9f822a65d0e498b6f40eaf369a9342a1164ee7d08b58103",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc147 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc147",
        pk: "9fba1de92b60b5b4703089763d0d6f9125e4dd7efae41f08a22882aef96892c4",
        msg: "4de142af4b8402f80a47fa812df84f42e283cee7",
        sig: "09a2ed303a2fa7027a1dd7c3b0d25121eeed2b644a2fbc17aa0c8aea4524071e\
              de7e7dd7a536d5497f8165d29e4e1b63200f74bbae39fbbbccb29889c62c1f09",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc148 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc148",
        pk: "7582ab1b52e1316e5c13671f43b39ca36b28133cd0832831bcddd0b0f23398cb",
        msg: "563357f41b8b23b1d83f19f5667177a67da20b18",
        sig: "e6884a6e6b2e60a0b5862251c001e7c79d581d777d6fc11d218d0aecd79f26a3\
              0e2ca22cc7c4674f8b72655bc4ee5cb5494ca07c05177656142ac55cc9d33e02",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc149 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc149",
        pk: "dd2d678bae222f3fb6e8278f08cc9e1a66339c926c29ac0a16f9717f5ee18cd8",
        msg: "931bbf9c877a6571cf7d4609fc3eb867edd43f51",
        sig: "6124c206d864507ea5d984b363b4cf583314db6856a45ded5e61eebff4d5e337\
              e0b4c82b445ae2e52d549d2d961eace2ea01f81158e09a9686baa040db65ad08",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc150 [TinkOverflow] "regression test for arithmetic error" → valid（须过）
    WycheproofVector {
        name: "tc150",
        pk: "ccbe7cb2e4bc215cee2f885e1d22f7e0d582b2bbbd782c104e548b152d26fc69",
        msg: "44530b0b34f598767a7b875b0caee3c7b9c502d1",
        sig: "cfbd450a2c83cb8436c348822fe3ee347d4ee937b7f2ea11ed755cc52852407c\
              9eec2c1fa30d2f9aef90e89b2cc3bcef2b1b9ca59f712110d19894a9cf6a2802",
        result: "valid",
        flags: "TinkOverflow",
        expect_ok: true,
    },
    // tc151 [InvalidEncoding] "R encodes y = 1 with the sign bit of x set. The
    //     only point with y = 1 has x = 0, so decoding must fail (RFC 8032,
    //     Section 5.1.3). Accepted only by verifiers that ignore the sign of x." → invalid（须拒）
    WycheproofVector {
        name: "tc151",
        pk: "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
        msg: "313233343030",
        sig: "0100000000000000000000000000000000000000000000000000000000000080\
              c803ee1f2342aa96ff698a393d1ab5e66f3eda101d6d120b394c3fd32c117d0a",
        result: "invalid",
        flags: "InvalidEncoding",
        expect_ok: false,
    },
    // tc152 [SmallOrderPublicKey] "identity public key universal forgery" → acceptable（须拒）
    WycheproofVector {
        name: "tc152",
        pk: "0100000000000000000000000000000000000000000000000000000000000000",
        msg: "6d657373616765",
        sig: "5866666666666666666666666666666666666666666666666666666666666666\
              0100000000000000000000000000000000000000000000000000000000000000",
        result: "acceptable",
        flags: "SmallOrderPublicKey",
        expect_ok: false,
    },
    // tc153 [SmallOrderPublicKey] "canonical Ed25519 torsion point 1 of 7
    //     (order 8)" → acceptable（须拒）
    WycheproofVector {
        name: "tc153",
        pk: "c7176a703d4dd84fba3c0b760d10670f2a2053fa2c39ccc64ec7fd7792ac037a",
        msg: "746f7273696f6e2d312d3134",
        sig: "b63dd4baa9c75ea9f3f1021ada3bf7dd6872e4cfbdac9653965ef78c4be9845d\
              27df2ce4101220ec96bc5ccbcefa1336871b76e6ef52a0fb35745bb187b1a10d",
        result: "acceptable",
        flags: "SmallOrderPublicKey",
        expect_ok: false,
    },
    // tc154 [SmallOrderPublicKey] "canonical Ed25519 torsion point 2 of 7
    //     (order 4)" → acceptable（须拒）
    WycheproofVector {
        name: "tc154",
        pk: "0000000000000000000000000000000000000000000000000000000000000080",
        msg: "",
        sig: "c459280d27f66a3285aa648ca1cc444cdfcde27a6062d117c6cb399e0969d6ed\
              f8b80551824c3e7275922ac8751e59a42793406560453c4e366999f900480506",
        result: "acceptable",
        flags: "SmallOrderPublicKey",
        expect_ok: false,
    },
    // tc155 [SmallOrderPublicKey] "canonical Ed25519 torsion point 3 of 7
    //     (order 8)" → acceptable（须拒）
    WycheproofVector {
        name: "tc155",
        pk: "26e8958fc2b227b045c3f489f2ef98f0d5dfac05d3c63339b13802886d53fc05",
        msg: "746f7273696f6e2d332d34",
        sig: "afd0270f467226b517fa8f911d1d320906df0065fe0045593c8fd0394770b83e\
              6bb6f264bca826f9a4631816571f8f42681a94db6d3f032e64721265a06c7009",
        result: "acceptable",
        flags: "SmallOrderPublicKey",
        expect_ok: false,
    },
    // tc156 [SmallOrderPublicKey] "canonical Ed25519 torsion point 4 of 7
    //     (order 2)" → acceptable（须拒）
    WycheproofVector {
        name: "tc156",
        pk: "ecffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
        msg: "",
        sig: "f8cea2efb481e92ca61b8972098e07aa1c16f2166cbd8da7ae682b55e4336939\
              20b491d7e08ab6c8e521b2f0300befbfcbfd7078340bc153a251896ef8b7c805",
        result: "acceptable",
        flags: "SmallOrderPublicKey",
        expect_ok: false,
    },
    // tc157 [SmallOrderPublicKey] "canonical Ed25519 torsion point 5 of 7
    //     (order 8)" → acceptable（须拒）
    WycheproofVector {
        name: "tc157",
        pk: "26e8958fc2b227b045c3f489f2ef98f0d5dfac05d3c63339b13802886d53fc85",
        msg: "746f7273696f6e2d352d3231",
        sig: "54df79ed63c1bfdc39fd6408ad5f04143bcfc22b7e1dd0e5ea3bc00eccbbbf82\
              60faa75789f6428d7a8a5a3b850b6779171de7edd504ffcfb5db320dd9864b06",
        result: "acceptable",
        flags: "SmallOrderPublicKey",
        expect_ok: false,
    },
    // tc158 [SmallOrderPublicKey] "canonical Ed25519 torsion point 6 of 7
    //     (order 4)" → acceptable（须拒）
    WycheproofVector {
        name: "tc158",
        pk: "0000000000000000000000000000000000000000000000000000000000000000",
        msg: "",
        sig: "520679470254237dbe8f93dc12e8b05363f5236b5671ebff01251649930f6c28\
              7531efeb34b73a6fe0f3170a211ce386ae269b0d145d5a7e63bb4ba15ce0c408",
        result: "acceptable",
        flags: "SmallOrderPublicKey",
        expect_ok: false,
    },
    // tc159 [SmallOrderPublicKey] "canonical Ed25519 torsion point 7 of 7
    //     (order 8)" → acceptable（须拒）
    WycheproofVector {
        name: "tc159",
        pk: "c7176a703d4dd84fba3c0b760d10670f2a2053fa2c39ccc64ec7fd7792ac03fa",
        msg: "746f7273696f6e2d372d39",
        sig: "b57aca4330dc58dcfd4ca75a1f7f3161aaf394a57769c7a9e9837d27cf7a80f7\
              e88557a67e0b4792628285f53ed0f4fac97bc187e436fac5ea0513d320d5560f",
        result: "acceptable",
        flags: "SmallOrderPublicKey",
        expect_ok: false,
    },
    // tc160 [MixedOrderPublicKey] "prime-order point 1 plus nonidentity
    //     torsion point 1" → acceptable（须拒）
    WycheproofVector {
        name: "tc160",
        pk: "15deb30729eb3e465c88f11b57d0c8d36eb0f670c3b22d2f80ea6049eeb2c23b",
        msg: "6d697865642d6f726465722d302d312d38",
        sig: "49b0d4daf5493b98c7220f7b785fdf10f4befad3262e00f2cd6813c90d1319f6\
              a7c6c0d69149f691d0c6b428642bb08d7046e0abff7e68e6b83547129f51080b",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc161 [MixedOrderPublicKey] "prime-order point 1 plus nonidentity
    //     torsion point 2" → acceptable（须拒）
    WycheproofVector {
        name: "tc161",
        pk: "31f20adf0f25a04f6d0940d2eb6a0b7d4aa2f18c9571a6f7eb74bf059b0d7215",
        msg: "",
        sig: "d9f1148f0b13b3e801455c65d8314e7103706166ac6844ad43ead64ee310cc9f\
              04e345b79bca79e83fbe7511fb5d17b347e2a2a98c247a92ba81b4a182116b0f",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc162 [MixedOrderPublicKey] "prime-order point 1 plus nonidentity
    //     torsion point 3" → acceptable（须拒）
    WycheproofVector {
        name: "tc162",
        pk: "db3274482b52f4937b405c00496b0511a364c58d4798ceae171262803d465729",
        msg: "6d697865642d6f726465722d302d332d31",
        sig: "989e808156e71fb17c146080890b2726d0e9ba1ab29a0832b680bac5849c41e0\
              4e97cb8d756d0890b1c8d7eff77de4b1a0301404338874b8819c4bb5bf204a08",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc163 [MixedOrderPublicKey] "prime-order point 1 plus nonidentity
    //     torsion point 4" → acceptable（须拒）
    WycheproofVector {
        name: "tc163",
        pk: "fb8ae86afa72f86ab074c3a51440df0d72c13c44cd1c4ae510681b419504974a",
        msg: "",
        sig: "df0ea594369382b35dd6c26bb71a7984f0fda2890a19bbb69bbbaf62242ee74d\
              d7f1bba3c0da7caa20ebf1337bcc2d39a2547e4e95e132a2daaf272439af8a05",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc164 [MixedOrderPublicKey] "prime-order point 1 plus nonidentity
    //     torsion point 5" → acceptable（须拒）
    WycheproofVector {
        name: "tc164",
        pk: "d8214cf8d614c1b9a3770ee4a82f372c914f098f3c4dd2d07f159fb6114d3dc4",
        msg: "6d697865642d6f726465722d302d352d32",
        sig: "f31beed5c32641a1baca414ba1f6a39cd4e0da3fe95027d937a41df3d6a6e5a7\
              f70d47674a0fb90b618e6465e7bd133f1615ae72eb790b05082342e12021180d",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc165 [MixedOrderPublicKey] "prime-order point 1 plus nonidentity
    //     torsion point 6" → acceptable（须拒）
    WycheproofVector {
        name: "tc165",
        pk: "bc0df520f0da5fb092f6bf2d1495f482b55d0e736a8e5908148b40fa64f28dea",
        msg: "",
        sig: "bef3aa8b7c0a70ef4b689bc839aff99c7cb5d62b3ec733079338400ec5deb0cc\
              abaafcae9ecd7d8cff17c1094353e85ec9d201e41f308589a3141c23895c030d",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc166 [MixedOrderPublicKey] "prime-order point 1 plus nonidentity
    //     torsion point 7" → acceptable（须拒）
    WycheproofVector {
        name: "tc166",
        pk: "12cd8bb7d4ad0b6c84bfa3ffb694faee5c9b3a72b8673151e8ed9d7fc2b9a8d6",
        msg: "6d697865642d6f726465722d302d372d33",
        sig: "71e7486ab6df0686580902797c785ffcfb2fe3b61d46c8e5da432e168e05d9c5\
              9ec91d092c7c8c635de89a97b37e383a4cec89d3c6015fb3fa7fbf43900dae04",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc167 [MixedOrderPublicKey] "prime-order point 2 plus nonidentity
    //     torsion point 1" → acceptable（须拒）
    WycheproofVector {
        name: "tc167",
        pk: "c1b047f0106ff174487ba336a56d48127ed0563dcca139fd555cf113f609ac71",
        msg: "",
        sig: "3c48db3cce8f2f154ddfe665a697de2f8dce6dd965eb0a18dbb95e90df01f2d5\
              95225ab03a2452216368b9f6f7590b72b804bb8d3258c4c9caa15922e3bff50a",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc168 [MixedOrderPublicKey] "prime-order point 2 plus nonidentity
    //     torsion point 2" → acceptable（须拒）
    WycheproofVector {
        name: "tc168",
        pk: "280fa1145bb421c1586eeadfb0f96ba1a25673111eca768773dafedf9b4e5a38",
        msg: "6d697865642d6f726465722d312d322d38",
        sig: "5c56e4644b99902901a9666a72c52ab6b029d4e81b07bb84b875c39f8f911938\
              db1068dfa06b293fdeadafa97613fe10b2684b85184f5fccc6503096c316ba00",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc169 [MixedOrderPublicKey] "prime-order point 2 plus nonidentity
    //     torsion point 3" → acceptable（须拒）
    WycheproofVector {
        name: "tc169",
        pk: "66f35e3ae56c736132dde77ae1faaeb01ebfc1bd79c98d43737e945b0744aaed",
        msg: "",
        sig: "5caa2cb4b66f9db52ca1dfa1a3068e36ba0fa66f3a5912d7ab9a55808edb6fe2\
              9be93e6714f72d9a0d83e736b88a4a74bd7237d739c85edbbfa99d5c518c7505",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc170 [MixedOrderPublicKey] "prime-order point 2 plus nonidentity
    //     torsion point 4" → acceptable（须拒）
    WycheproofVector {
        name: "tc170",
        pk: "c243dfb46f771baa41247bdb387f07475c78b8b9bb99b1089be6fd371420ccc3",
        msg: "6d697865642d6f726465722d312d342d31",
        sig: "66250edaea83744886f3dfa139424148ced9e0646be033ab72b75d8413afbea4\
              53eefb6735dabe1491aff1f308bbd73060c669c3c3772ae2f94ab4f387642a02",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc171 [MixedOrderPublicKey] "prime-order point 2 plus nonidentity
    //     torsion point 5" → acceptable（须拒）
    WycheproofVector {
        name: "tc171",
        pk: "2c4fb80fef900e8bb7845cc95a92b7ed812fa9c2335ec602aaa30eec09f6538e",
        msg: "",
        sig: "fc91f328ca137afe81ff69d75118b89455b96d6fd6cb88a46b99fc68074175b1\
              ac6be310033c348fe42f41cbef8c004774563c77ab781217c165d13fc273b202",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc172 [MixedOrderPublicKey] "prime-order point 2 plus nonidentity
    //     torsion point 6" → acceptable（须拒）
    WycheproofVector {
        name: "tc172",
        pk: "c5f05eeba44bde3ea79115204f06945e5da98ceee13589788c25012064b1a5c7",
        msg: "6d697865642d6f726465722d312d362d34",
        sig: "cc93d04669be4a2b68dd7d82944563b4468fa1392ae97fa36fe388d89e3d81a5\
              9588aa307e357bd12635702d077014daf525afd7400acb78d0c9a2f9f506c60a",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc173 [MixedOrderPublicKey] "prime-order point 2 plus nonidentity
    //     torsion point 7" → acceptable（须拒）
    WycheproofVector {
        name: "tc173",
        pk: "870ca1c51a938c9ecd2218851e05514fe1403e42863672bc8c816ba4f8bb5512",
        msg: "",
        sig: "c28aa73d32d98a6f0f956392279cd13fc35d9d67ea54c99941eadea5d08ba760\
              398f5845bbbb509102326b53ab93e33398b6c673331a194dd85173495e13ff0a",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc174 [MixedOrderPublicKey] "prime-order point 3 plus nonidentity
    //     torsion point 1" → acceptable（须拒）
    WycheproofVector {
        name: "tc174",
        pk: "85e7bfc77e932799ae948a1abd8ea287c56dd48fb8b4221931e4f8652e1eea20",
        msg: "6d697865642d6f726465722d322d312d30",
        sig: "e85ba67b317ad0f48848892ce0a585d2d47ccd49e7895bf93629bf3e4723f92c\
              3de51cdced1b12654fc834d3e675488e98f2628fa0a01b058f17dfb81603c300",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc175 [MixedOrderPublicKey] "prime-order point 3 plus nonidentity
    //     torsion point 2" → acceptable（须拒）
    WycheproofVector {
        name: "tc175",
        pk: "349a3840d09eb68a97b06eae781f4fbece569c6ffd70c238ff8406ec82fb587d",
        msg: "",
        sig: "1505ddb0ea2479d7751bdb60693727d448d1c22193cfe548ce6905f8c5da1a0c\
              13a889c2a7d2b30c3b78015086972386d377060c9a97ea15d6dd8da04f8c7702",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc176 [MixedOrderPublicKey] "prime-order point 3 plus nonidentity
    //     torsion point 3" → acceptable（须拒）
    WycheproofVector {
        name: "tc176",
        pk: "0cf1b0a8da530a0f0b30f5b130391652f249680e0f3f7c80284bdc5ab955cfa3",
        msg: "6d697865642d6f726465722d322d332d33",
        sig: "793423634e1945c284aa538008fa41767a46964acf14fddabc2ac7eeb3733b4a\
              81b3075118fcaaf8499749a992df4949bc866de91ab0b1a98cae9459db2f710f",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc177 [MixedOrderPublicKey] "prime-order point 3 plus nonidentity
    //     torsion point 4" → acceptable（须拒）
    WycheproofVector {
        name: "tc177",
        pk: "628c21a85bec28f1d0c0ea44b2c6e731f47a1b01be58d1028892f947351bfa61",
        msg: "",
        sig: "f8410c87623213d696e74b9a23b30926fbf8036a931678db8155956440ac192e\
              4ed451d782a41df339f56cd4e8482de55601eefd92301d55bb067e90bb5db009",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc178 [MixedOrderPublicKey] "prime-order point 3 plus nonidentity
    //     torsion point 5" → acceptable（须拒）
    WycheproofVector {
        name: "tc178",
        pk: "68184038816cd866516b75e542715d783a922b70474bdde6ce1b079ad1e115df",
        msg: "6d697865642d6f726465722d322d352d38",
        sig: "a02a0c28d4735b1fa7803ce1e1f410828ffd07018b0b1f5255fe69381a04d788\
              d575682aceb9efe1c12f158b920ef213bfbcc53cdbb36dbd81383ecd91b7dc03",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc179 [MixedOrderPublicKey] "prime-order point 3 plus nonidentity
    //     torsion point 6" → acceptable（须拒）
    WycheproofVector {
        name: "tc179",
        pk: "b965c7bf2f614975684f915187e0b04131a96390028f3dc7007bf9137d04a782",
        msg: "",
        sig: "358d21d481ef812343dffa2b55b1398ade7d630c64ff2a4f48c9a634f7ae9b9f\
              99766d982121c3ea719b338e942d95557cf41e3385c269ba6f51aa31a4956e0a",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc180 [MixedOrderPublicKey] "prime-order point 3 plus nonidentity
    //     torsion point 7" → acceptable（须拒）
    WycheproofVector {
        name: "tc180",
        pk: "e10e4f5725acf5f0f4cf0a4ecfc6e9ad0db697f1f0c0837fd7b423a546aa305c",
        msg: "6d697865642d6f726465722d322d372d32",
        sig: "abce1d42c9c8da97d4ecebbbf58934b5d0f86a775a43c57fc0261b39be7a1ea1\
              b0f208edcf4093ef5c61c9552bbcdd2946be36d1a1e7878d669a42bc523c5d0d",
        result: "acceptable",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
    // tc181 [Valid] "valid control paired with torsion point 0" → valid（须过）
    WycheproofVector {
        name: "tc181",
        pk: "f2741795058d07954f8b3c5aebbf20f28d3ec3bb32e3b51aef97e4be6afb68b5",
        msg: "6d657373616765",
        sig: "854619fb2351b7a4f6ff982512e3be8c35bbb551991f42d241d44c7406dab439\
              8188e758a19c325d24d683556dec79f7d72291a8e5c65f0ea440a95a28d4470d",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc182 [Valid] "valid control paired with torsion point 1" → valid（须过）
    WycheproofVector {
        name: "tc182",
        pk: "f2741795058d07954f8b3c5aebbf20f28d3ec3bb32e3b51aef97e4be6afb68b5",
        msg: "746f7273696f6e2d312d3134",
        sig: "e1427249d2d8f07b7ed4ef639de59e17c823e821ad0d0b1e693b826bdc4cf29e\
              94a83d6d3f382dfe0a0c1f18b73d6a1b501bb9bcd1f7232cd60db05c8a9dc009",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc183 [Valid] "valid control paired with torsion point 2" → valid（须过）
    WycheproofVector {
        name: "tc183",
        pk: "f2741795058d07954f8b3c5aebbf20f28d3ec3bb32e3b51aef97e4be6afb68b5",
        msg: "",
        sig: "8dfc13f28ef98fc6a766f3bbd0c80cde4f9cafe4ba7687e7a993851f82338844\
              a371f634c6f2e16292429426226090347e4d19c6ce60eb0e169fe9a00003d30b",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc184 [Valid] "valid control paired with torsion point 3" → valid（须过）
    WycheproofVector {
        name: "tc184",
        pk: "f2741795058d07954f8b3c5aebbf20f28d3ec3bb32e3b51aef97e4be6afb68b5",
        msg: "746f7273696f6e2d332d34",
        sig: "b149c4197787df8113491f36cb24dc5f11c1e80f9f51e1ae0031e674ea279f49\
              9df81717719f9c27b44611042cf20abb9bf0cbf0449a3b8040ea0fdaf1e93f01",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc185 [Valid] "valid control paired with torsion point 4" → valid（须过）
    WycheproofVector {
        name: "tc185",
        pk: "f2741795058d07954f8b3c5aebbf20f28d3ec3bb32e3b51aef97e4be6afb68b5",
        msg: "",
        sig: "7274931dbc8c3a18a739ff8f187bdb5e6b524320c6d3976b70ce7fc016449344\
              cdfd9e0cc96ba9efe1bf71d8ef5339d9b70f8bceacfc66547e3b94d4e2deef06",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc186 [Valid] "valid control paired with torsion point 5" → valid（须过）
    WycheproofVector {
        name: "tc186",
        pk: "f2741795058d07954f8b3c5aebbf20f28d3ec3bb32e3b51aef97e4be6afb68b5",
        msg: "746f7273696f6e2d352d3231",
        sig: "7e17e7cdfda11ab37b2bf09f724b0bc2c61bbbbe6516d261b291f44ac3523850\
              2071de037a3296bba04ee65258ae39b63407aba333342775a1d7b830b11c9a0a",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc187 [Valid] "valid control paired with torsion point 6" → valid（须过）
    WycheproofVector {
        name: "tc187",
        pk: "f2741795058d07954f8b3c5aebbf20f28d3ec3bb32e3b51aef97e4be6afb68b5",
        msg: "",
        sig: "42558ff4b1a47724c98fb3c15e1d64f68b49ab538b37c33f1c11d0fb44cc201d\
              1b7557b725655fec353a5ae7a47d37e92ef62f9870ccd845298acb2e12246202",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc188 [Valid] "valid control paired with torsion point 7" → valid（须过）
    WycheproofVector {
        name: "tc188",
        pk: "f2741795058d07954f8b3c5aebbf20f28d3ec3bb32e3b51aef97e4be6afb68b5",
        msg: "746f7273696f6e2d372d39",
        sig: "37dcc58c48d0bae5b932bcda52d52361abc2a5616bf77508a3978af9ca42964b\
              9a481ff328c8b2263fdfd6e4a15cfa66908edb16891c9c05183cdfb87ea0fa0a",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc189 [Valid] "valid control paired with mixed-order point 1/1" → valid（须过）
    WycheproofVector {
        name: "tc189",
        pk: "f2741795058d07954f8b3c5aebbf20f28d3ec3bb32e3b51aef97e4be6afb68b5",
        msg: "6d697865642d6f726465722d302d312d38",
        sig: "0ae8d293de2640c14c01f5a8c7c56d5a756128e90e436d3a2ffece7fd5830ba1\
              7181539db012fdf463ed03af9f531c663639fb86a9e2c3fee98690db160ae90a",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc190 [Valid] "valid control paired with mixed-order point 1/2" → valid（须过）
    WycheproofVector {
        name: "tc190",
        pk: "f2741795058d07954f8b3c5aebbf20f28d3ec3bb32e3b51aef97e4be6afb68b5",
        msg: "",
        sig: "61c6d52a5a68f19f24ba14e5cf78fd8a7152ca7d3d40094d58852b2adef86e69\
              59159edc65f0e997d058f1921fe7a042a70e0b4663008187a531ecd3e83a8604",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc191 [Valid] "valid control paired with mixed-order point 1/3" → valid（须过）
    WycheproofVector {
        name: "tc191",
        pk: "f2741795058d07954f8b3c5aebbf20f28d3ec3bb32e3b51aef97e4be6afb68b5",
        msg: "6d697865642d6f726465722d302d332d31",
        sig: "ec00e7504ffa36aec4fb8e721b7f808ec6ca0cfc04a5017af6cdd48efc73d730\
              c40e1874e3e43ff521d62142c8d5dfcbcc214ba2c47f7b5825f2956bd9f19505",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc192 [Valid] "valid control paired with mixed-order point 1/4" → valid（须过）
    WycheproofVector {
        name: "tc192",
        pk: "f2741795058d07954f8b3c5aebbf20f28d3ec3bb32e3b51aef97e4be6afb68b5",
        msg: "",
        sig: "4be9edab3535cc97587f84f2ed5881338755702cdc975a572e328352a5450dd5\
              6ff0fb42afb2cb9a1028eebb3ee0fba061789d4f7abaab06ca38ea332b1ed90c",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc193 [Valid] "valid control paired with mixed-order point 1/5" → valid（须过）
    WycheproofVector {
        name: "tc193",
        pk: "f2741795058d07954f8b3c5aebbf20f28d3ec3bb32e3b51aef97e4be6afb68b5",
        msg: "6d697865642d6f726465722d302d352d32",
        sig: "fe7c8cfaefecc01cae674fb2fc92626d175752824d9756548dd601be73e43530\
              72fd18da1e55e552d1becb2d3be3fe7bb60a2748fd648faa5f08a46d1b963004",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc194 [Valid] "valid control paired with mixed-order point 1/6" → valid（须过）
    WycheproofVector {
        name: "tc194",
        pk: "f2741795058d07954f8b3c5aebbf20f28d3ec3bb32e3b51aef97e4be6afb68b5",
        msg: "",
        sig: "a0a03118b7d9746f89b3d5a0deb5147c0f440e7d914ceb181232b972a099caef\
              ad8258c0fb0fef3e1da74d413d1da6c38a06f2a93abf3cec796ea701c1b1d601",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc195 [Valid] "valid control paired with mixed-order point 1/7" → valid（须过）
    WycheproofVector {
        name: "tc195",
        pk: "f2741795058d07954f8b3c5aebbf20f28d3ec3bb32e3b51aef97e4be6afb68b5",
        msg: "6d697865642d6f726465722d302d372d33",
        sig: "944b48f67a3d51c002e3781990778b57bf71017639e0441de7ae467c55fabc26\
              bef8cf69bfda88633828d56751cfd6c8c8180cec5efe057f97fc6c20b2a5290a",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc196 [Valid] "valid control paired with mixed-order point 2/1" → valid（须过）
    WycheproofVector {
        name: "tc196",
        pk: "2bbc204b9088e455bedb8424c780f8b8a387474644664ef7641902c8ebdf333c",
        msg: "",
        sig: "98b7602a3e45efb7d899687ee0ec2f81f7afe8056837a720e703b3e803cf1f8f\
              3bbb3adeeaf502dd7222db467d04c0a8859a6a80f67c33a84674e2d0476a860e",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc197 [Valid] "valid control paired with mixed-order point 2/2" → valid（须过）
    WycheproofVector {
        name: "tc197",
        pk: "2bbc204b9088e455bedb8424c780f8b8a387474644664ef7641902c8ebdf333c",
        msg: "6d697865642d6f726465722d312d322d38",
        sig: "c0f317d6b580cfd42ac199e94ce489706b6f33bb31c5300b5e13e97ca93a3e5b\
              1268b95e1c103624704528f1c56d456bf957f7b627b4a4c33ff53a4ffe3ea007",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc198 [Valid] "valid control paired with mixed-order point 2/3" → valid（须过）
    WycheproofVector {
        name: "tc198",
        pk: "2bbc204b9088e455bedb8424c780f8b8a387474644664ef7641902c8ebdf333c",
        msg: "",
        sig: "920dc46731eabe1a17f4ecc0c494fdf61f75e25f11e04de75f632d574a5d51fc\
              61d298f11ca8e5c3f9ccae708b5b2dedb525bf543cfa7b15a0fee46450544e0c",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc199 [Valid] "valid control paired with mixed-order point 2/4" → valid（须过）
    WycheproofVector {
        name: "tc199",
        pk: "2bbc204b9088e455bedb8424c780f8b8a387474644664ef7641902c8ebdf333c",
        msg: "6d697865642d6f726465722d312d342d31",
        sig: "26eb6f36777f5d04275cc0e5743c8ab428799288c2df0d5b8700aaebbfc0c664\
              ffa55f90637bc6817390c114526dadfd3bd8a49e1512ddaf0b1b975778f1ea0b",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc200 [Valid] "valid control paired with mixed-order point 2/5" → valid（须过）
    WycheproofVector {
        name: "tc200",
        pk: "2bbc204b9088e455bedb8424c780f8b8a387474644664ef7641902c8ebdf333c",
        msg: "",
        sig: "f107a692bf3ce4f10338f7b546bc16a194f625bf034e57ea9259b6102bfd9082\
              c74224294c828a41ff069bac623f26b815cafe1b4af1bc0cf8297d0e39ea0505",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc201 [Valid] "valid control paired with mixed-order point 2/6" → valid（须过）
    WycheproofVector {
        name: "tc201",
        pk: "2bbc204b9088e455bedb8424c780f8b8a387474644664ef7641902c8ebdf333c",
        msg: "6d697865642d6f726465722d312d362d34",
        sig: "4683fec91d18d841760debf390e7d41cd92ea386b9bb095a5e4629ea7a0f1c41\
              c1daeb539b4a9373858d5f4b843022e91051dbd461b1359c20e2a0775a671300",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc202 [Valid] "valid control paired with mixed-order point 2/7" → valid（须过）
    WycheproofVector {
        name: "tc202",
        pk: "2bbc204b9088e455bedb8424c780f8b8a387474644664ef7641902c8ebdf333c",
        msg: "",
        sig: "390a6f169020d371c8a63ff2580fac5b24cccda3929ccf88d9481aa97d32be2f\
              e410f91c0a22ce4a008e2191a3fc1d6ea2f26afb6d764328c478b40f1322a804",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc203 [Valid] "valid control paired with mixed-order point 3/1" → valid（须过）
    WycheproofVector {
        name: "tc203",
        pk: "8b73de57a413d70e2f3f15bb4d3918ce0b85e4fe41a72efd776d06b8cae4059e",
        msg: "6d697865642d6f726465722d322d312d30",
        sig: "d637793ab7d172e2156000955e1fa77c3ba0641d9f6dd20362d208c8eb6d5de6\
              2d67eeebb72fe8fc50a77b5e7862dca0c5e40f1fde27a29e9a9c9fe81c545007",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc204 [Valid] "valid control paired with mixed-order point 3/2" → valid（须过）
    WycheproofVector {
        name: "tc204",
        pk: "8b73de57a413d70e2f3f15bb4d3918ce0b85e4fe41a72efd776d06b8cae4059e",
        msg: "",
        sig: "46d9381b0b3838aa7ee64fe8a1c7cfed680dc4c40e087417d5afe9bb38352ac0\
              6dfbfe9fac07dc83e62c61c1a75cfffee394644c5e3ff8431d3f41a119448b05",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc205 [Valid] "valid control paired with mixed-order point 3/3" → valid（须过）
    WycheproofVector {
        name: "tc205",
        pk: "8b73de57a413d70e2f3f15bb4d3918ce0b85e4fe41a72efd776d06b8cae4059e",
        msg: "6d697865642d6f726465722d322d332d33",
        sig: "c12e8b07833d88c5c016117273fc60a9531c39ebf89de916839ae8789b535bbc\
              6ebac6f1bc2f3523afa93142573d93643cb20a57f56a4c22dac587da32ddfd04",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc206 [Valid] "valid control paired with mixed-order point 3/4" → valid（须过）
    WycheproofVector {
        name: "tc206",
        pk: "8b73de57a413d70e2f3f15bb4d3918ce0b85e4fe41a72efd776d06b8cae4059e",
        msg: "",
        sig: "ea080c9adc37a7c93155883251286c8fd6dced33073582f6042c5a8cf02790f3\
              b923a0a877b56c5dec0f79b7152f0192f74f3902281f165a4ccff2953d36590b",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc207 [Valid] "valid control paired with mixed-order point 3/5" → valid（须过）
    WycheproofVector {
        name: "tc207",
        pk: "8b73de57a413d70e2f3f15bb4d3918ce0b85e4fe41a72efd776d06b8cae4059e",
        msg: "6d697865642d6f726465722d322d352d38",
        sig: "f6e379c50289cf1ac8c5c8caab924740c6bc07475104c9b714bda0288f7a91e9\
              86c33153568615a8d0f519d6a65b8821d2fcc7b1d8dc8d93fdef3f2142df100d",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc208 [Valid] "valid control paired with mixed-order point 3/6" → valid（须过）
    WycheproofVector {
        name: "tc208",
        pk: "8b73de57a413d70e2f3f15bb4d3918ce0b85e4fe41a72efd776d06b8cae4059e",
        msg: "",
        sig: "c275256bd5699eea59126cd9f5b22c540ebb873c80c046eb4dc7523bd2367a98\
              2283d5edbc2042101e67b803a9725e9f7b23f809172845e9ce670d511fab580f",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc209 [Valid] "valid control paired with mixed-order point 3/7" → valid（须过）
    WycheproofVector {
        name: "tc209",
        pk: "8b73de57a413d70e2f3f15bb4d3918ce0b85e4fe41a72efd776d06b8cae4059e",
        msg: "6d697865642d6f726465722d322d372d32",
        sig: "845813c2ac916dc3977f05862e28fccb403d32f71fbcc817ae3ebe10d841c189\
              dc8b663018aaa36438193f081ea7f57c904d32af78eb2fb20394d8eb640e6f06",
        result: "valid",
        flags: "Valid",
        expect_ok: true,
    },
    // tc210 [MixedOrderPublicKey] "sodium_compat v2.5.1 mixed-order subgroup
    //     regression" → invalid（须拒）
    WycheproofVector {
        name: "tc210",
        pk: "d1e06adf4e05e248e1e52beca14b6be4da0af01b41645d2849b407ec22f41d47",
        msg: "af82",
        sig: "6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac\
              18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a",
        result: "invalid",
        flags: "MixedOrderPublicKey",
        expect_ok: false,
    },
];
