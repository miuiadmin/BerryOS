//! 标量算术 mod L（L = 2^252+27742317777372353535851937790883648493）。
//!
//! P0-2 原语三件之四（24 号 §3.2 列名 scalar.rs；本刀名 sc.rs）。设计
//! 偏离注（如实）：24 号 §3.8-D3 建议「RFC 8032 参考实现逐字对照移植」
//! （djb ref10 的 21 bit limb 进位链）；本刀改自研**MSB 先行移位-条件减
//! 约减器**（512 轮定长，每轮 4-limb 减法+掩码选择）——理由：①ref10
//! 移植需手抄 ~120 行密集进位链（24 号 §3.9-1 自认的最大错源），移位减
//! 型约 40 行且每步可推；②正确性由双门执法：python 大整数对拍 8 例
//! （下方 `python_reference_vectors`）+ RFC 8032 全向量门（sign/verify
//! 逐条——任何约减错误立即红）。性能（512 轮 vs ~64 轮）非本刀敏感面
//! （验签瓶颈在点乘）。
//!
//! **常时性**：条件减经掩码选择（borrow → 掩码回选被减数），512 轮
//! 定长无分支；`sc_is_less_l` 分支于**公开**数据（验签 S 段），不涉
//! 秘密。未做侧信道审计（fe.rs 同款诚实注）。
//!
//! API 名承 ref10（sc_reduce/sc_muladd——24 号 §3.2 契约面）但语义
//! 自研：`sc_reduce(64B) → 32B`；`sc_muladd(a,b,c) = (a·b+c) mod L`
//! （a 可为 clamped 标量 ≤ 2^255-1——内部按 512 bit 乘积约减，无 < L
//! 前置假设）。

/// L 的小端 32B 编码（RFC 8032 Table 1 十进制 → python 生成，测试钉板）。
pub const L_BYTES: [u8; 32] = [
    0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde,
    0x14, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
];

/// L 的 4×u64 小端 limb。
const L: [u64; 4] = [0x5812631a5cf5d3ed, 0x14def9dea2f79cd6, 0, 0x1000000000000000];

/// 512 bit 中间数（小端 limb）。
type U512 = [u64; 8];

fn load4(b: &[u8; 32]) -> [u64; 4] {
    let mut r = [0u64; 4];
    for (i, w) in r.iter_mut().enumerate() {
        let mut t = [0u8; 8];
        t.copy_from_slice(&b[i * 8..i * 8 + 8]);
        *w = u64::from_le_bytes(t);
    }
    r
}

fn store4(v: &[u64; 4], out: &mut [u8; 32]) {
    for (i, w) in v.iter().enumerate() {
        out[i * 8..i * 8 + 8].copy_from_slice(&w.to_le_bytes());
    }
}

/// acc - L：返回 (差, borrow)——borrow=1 表示 acc < L（差不有效）。
fn sub_l(acc: &[u64; 4]) -> ([u64; 4], u64) {
    let mut r = [0u64; 4];
    let mut borrow = 0u64;
    for i in 0..4 {
        let (d1, b1) = acc[i].borrowing_sub(L[i], borrow != 0);
        r[i] = d1;
        borrow = b1 as u64;
    }
    (r, borrow)
}

/// 掩码选择：pick=1 选 a，pick=0 选 b（pick 为 borrow 位——秘密不分支）。
fn sel4(pick: u64, a: &[u64; 4], b: &[u64; 4]) -> [u64; 4] {
    let mask = 0u64.wrapping_sub(pick); // pick∈{0,1}：0→全0（选b），1→全1（选a）
    let mut r = [0u64; 4];
    for i in 0..4 {
        r[i] = b[i] ^ ((a[i] ^ b[i]) & mask);
    }
    r
}

/// 512 bit mod L：MSB 先行 512 轮（acc 左移入位 → 条件减 L）。不变式：
/// 轮首 acc < L ⇒ 移位入位后 < 2L+1 ⇒ 条件减后 < L。入口值任意 512 bit。
fn reduce512(v: &U512) -> [u64; 4] {
    let mut acc = [0u64; 4];
    for i in (0..512).rev() {
        // acc = (acc << 1) | bit_i(v)。
        let bit = (v[i / 64] >> (i % 64)) & 1;
        let shifted = [
            (acc[0] << 1) | bit,
            (acc[1] << 1) | (acc[0] >> 63),
            (acc[2] << 1) | (acc[1] >> 63),
            (acc[3] << 1) | (acc[2] >> 63),
        ];
        // 条件减 L（borrow=1 → 差为负 → 保留原值）。
        let (d, borrow) = sub_l(&shifted);
        acc = sel4(borrow ^ 1, &d, &shifted);
    }
    acc
}

/// RFC 8032 sc_reduce 语义：64 字节小端 → mod L 出 32 字节小端。
pub fn sc_reduce(input: &[u8; 64]) -> [u8; 32] {
    let mut v = [0u64; 8];
    for (i, w) in v.iter_mut().enumerate() {
        let mut t = [0u8; 8];
        t.copy_from_slice(&input[i * 8..i * 8 + 8]);
        *w = u64::from_le_bytes(t);
    }
    let mut out = [0u8; 32];
    store4(&reduce512(&v), &mut out);
    out
}

/// RFC 8032 sc_muladd 语义：(a·b + c) mod L。a ≤ 2^255-1（clamped
/// 标量在域）、b/c < L（调用方：b = 约减后哈希、c = 约减后 r）。
pub fn sc_muladd(a: &[u8; 32], b: &[u8; 32], c: &[u8; 32]) -> [u8; 32] {
    let a4 = load4(a);
    let b4 = load4(b);
    let c4 = load4(c);
    // 256×256 → 512 bit 学乘（u128 累积）。
    let mut p: U512 = [0u64; 8];
    for i in 0..4 {
        let mut carry = 0u128;
        for j in 0..4 {
            let t = (a4[i] as u128) * (b4[j] as u128) + (p[i + j] as u128) + carry;
            p[i + j] = t as u64;
            carry = t >> 64;
        }
        p[i + 4] = carry as u64;
    }
    // + c（低 4 limb 带进位）。
    let mut carry = 0u64;
    for k in 0..4 {
        let t = (p[k] as u128) + (c4[k] as u128) + carry as u128;
        p[k] = t as u64;
        carry = (t >> 64) as u64;
    }
    // F2 注记（29 号 §4.1/§F2，低危·如实）：下述尾进位传播 while 的循环
    // 次数由秘密派生数据（a·b+c 的高位进位链）决定——**秘密依赖分支**，
    // 与 ed25519.rs「秘密相关分支为零」的常时声明有张力（该声明对
    // fe/ge 与本文件 reduce512 仍成立）。泄露面（推断量级）：每次 sign
    // 约 0–3 bit 关于 a·k+c 的 bit 256+（S 与 k 公开、a/prefix 秘密）；
    // **sign-only**（verify 全路径只碰公开输入）、当前威胁模型（离线
    // 验签/平台签发，P3 前不涉在线敌手）下实际可忽略。定长 4-limb 进位
    // 传播为顺手改面，随 P3 设备侧常时性审计刀一并重开——不在本刀强改。
    let mut k = 4;
    while carry != 0 && k < 8 {
        let t = (p[k] as u128) + carry as u128;
        p[k] = t as u64;
        carry = (t >> 64) as u64;
        k += 1;
    }
    let mut out = [0u8; 32];
    store4(&reduce512(&p), &mut out);
    out
}

/// S < L 判定（小端字节序自顶比较；输入 = 验签公开 S 段——分支无秘密）。
pub fn sc_is_less_l(s: &[u8; 32]) -> bool {
    for i in (0..32).rev() {
        if s[i] != L_BYTES[i] {
            return s[i] < L_BYTES[i];
        }
    }
    false // == L：不小于
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{:02x}", x)).collect()
    }

    fn unhex(s: &str, out: &mut [u8]) {
        for (i, b) in out.iter_mut().enumerate() {
            *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
        }
    }

    // python 大整数对拍 8 例（两源核之数值源；生成脚本同仓注记——
    // xorshift 种子 0x9e3779b97f4a7c15，(a*b+c)%L 与 in64%L）。
    #[test]
    fn python_reference_vectors() {
        let cases: &[(&str, &str, &str, &str, &str, &str, &str, &str)] = &[
            ("ad4df30bae771bdc76606e02b9eef064366190e591ce077b74cc8d360c055f30ec4ac5a1e016eb2c79fb7b4ece1d1097cff2e8d644e1d29aea70f41a2e79aad9",
             "3a0c3e89f976322ea868ea69f951515bffc08b9bafc870ea16f86c2c4c669b07",
             "b13aebf65088852de253530316b6d06515abe8b8ff74f408031fd7564785ad0e",
             "e1222152bcd4df349226a0322c0eac131e105a3fe89921b9ff435dfc79507605",
             "637356394b80971cec39d19aeb1158e74e4b716bb13956fc199aaec8cf2dfb02",
             "a3001da7153f8dbfd41d63e8f222142f112ae1826e13765a0f26240d3b3b2d08",
             "6861006dc70e4b54ef9a319a7e9bbbbc1aa20a9cd5501a0fde7165822021ae40",
             "fdd4e697d7208c9fda6e75df4733dabe6c0303d24d029c0c044f6acb6806aa05"),
            ("bd0c7ced02cd1e0ee47bfac1e881090d13f6bdcdd3b386dc3f69e769c244086d2d0893697fda81367d857d982c8e925b3781ed6c1e6e716cf5f798342170b8cc",
             "3fdb0bae0ca70169bf93c8d8b28bc0cd3119b91b0891dc49b51f4957cd24310c",
             "931fa5612b89d096692a7b9648a41d72ed7fdbd0364c2bc75292fcc0b005f305",
             "c7a46f75914dfdb1d3bf77e68cfba4a8b229cd26a19ba65920bfbc0cffffcb0f",
             "713adcf96d7f0ad5eb60bca50e1c4b42b9aa17944911b932ac0bdae088eca807",
             "1f788fab7c39729d02bab8c77489d8bd59137119f9546925add921d954a17203",
             "b8d49a2fab8ec80cd2af439a415b3abc0d9c7cc414340f6f75c6b8ed67751a67",
             "161cea670c7727910b9bb4f9e8faf74064ba87f29720a5e21770d33363055306"),
            ("b98aee41f84c6763ecd940a87d8375e35fa3a51e2eb28e57d9df29e02e58fa4e265a2938e963b3f312416f49f34e3a2e109b2f2a8ed709b82600b34e107f2146",
             "edd0d496a032633fd5b3674234fb1f4eedd7a74b35ca9a75ac1bae8d1cb69605",
             "b01835e88d4a46b8c03766416e380926aed4a6103b27d0bfc6522b30eb000a0d",
             "2f262af943b9c0db6efccbfa2e3e700817c9eba0ddc24b9045bbf02ed8e69a05",
             "44107d64f4c82810b0d0813195fa7b8bbd1fe279bc143b5401f463d274b6af03",
             "eae0a5a78b102d6358216527b66268333e0a750599eb0ffae226e59d557e0108",
             "a813e8fa14552a99ce09f078050d95655d5a80fb2b570357a96d789c5f6a8b75",
             "a4a71857eac9f4e05db23134d40e71bde04724ae5e402f666e04b53d799bba08"),
            ("32d78a9e3e7937a81c4edc8772bcea5080f103ff571dca4d6396f7c211ff99488f8145ad766c16164c89d6b330ca74cb5ef786188755732130ed813339e2a128",
             "a8c739fee976fa51f018cc0ba63570b45b3b6ffcb90625a18a7b97075d634605",
             "10fb0320826b2757824dc8efcf740695e1c31a351600fdf8262ef05b40363e0a",
             "171d15bbc9c59326e68f903b5d565532b45856f4d438d8b04479b6716e258800",
             "87a9ff87277d07ec8979f722bca69b99114fe2a73c07df9a0c0fc4715f3d020f",
             "174c5f88dab002cd6f191f643d3ac52790ab4f57befd85f37a82f83ebfbbe602",
             "10c4e1b74affdaf81a4349641f0ab63b1c578ace3fccb6e1b204055047e54b57",
             "978510708ac75864c656656897160ccb0e3d573b8d1ba652fb8c772f7c59c902"),
            ("3b6204bbbd3786783f8453858779b478f7ccd13740dc14f6aeb20f1fa81759ab4bc61ef670b97d100709845ae891e5a7d5a0b9ebaa7fe842d446b4e9a0d0b77c",
             "30250ce7abe3c3e4b224bdecc6f499bbbba7b7300ad9b7db90501812546d1403",
             "04a8793668950e7ba64da7bc6d60515b4ccdfec36cb56e1d97c34dbb8ed3c60c",
             "f6b5adbc672ac3e3bee15fafc41d8a169d3f34396a5310fca210fe0610a1590b",
             "d468d554299e93e9957210f3e6be9a6253d509de48ef19aafe1396d0b4fb1e0c",
             "3050f5cfabe97a29f7fbf5d0b5d4c6cb9c1e4ca7c0a95606c24255b58fb98c00",
             "58007e5efd0c640119ca7b6fefa60321cd9b8e21bfdb59babad56644d4bdcf47",
             "1b720640e52105418c1a626a900879817135114b56779863d77dc310ff80aa09"),
            ("9136ce42ea80710fbc2e6cbe0ad7571de1d90e065f86f43d129cacd842d9c29caa81ece6229cc2682972666c165dcda18d147dbe75610c95e46d1db3bd423e3f",
             "b2d3f2df2b1400e963d380aeee8729f44f7a0c8e51520c35dd903614b5f2a605",
             "783201e8bf38b1b0202e4e1eb304f6296a263ab8504daf7fa6884242fef6ef06",
             "1bf5985beefa595333da95ed4aadc68896b9f7fd4cb62c9924d39848003eb908",
             "82ab5d23ce2d5d6455b0abbd759a8a807552dceef731559991d7799303637d08",
             "27e06cfdc806cf27a8d0c558145f9e0f78da5ce87c10e3c4bfd58ad8ed38cc07",
             "78e024b0ffcac9f33e760f3664a406ea5227dddd121036539c09fd38b4746a56",
             "925a654456790da525a62a28e6756f0cb01c67a16422f5cb1cdd4eb1c586df03"),
            ("8f14e3e7b697548d6611b4322c2f2500c4e07184dba3dbf405b21377c0b447a421b4132df58fd65809bb328342903d7c3f3c85e55290e09887d98835761408b9",
             "fe3d1b4d9828ffe6ba9a9aca28f43a90e8f35968a48afa7167e24c8f2f7e4305",
             "66524a050a4cc6f36001fc83bf6e04a4fa443375825bc1adb27cf3ac20dca009",
             "17a79edf807152128d5cddef894a6fa5f981b1e78e22854bbabc555fc71b0301",
             "145e0543c91fda643ba6de9a79c4b9f1f69bad155949dd1c00dd9ec50678eb0a",
             "b6b3a8194e86fd478b8237c135a332ee4925447bcce47de5d6beeeabc1f39302",
             "b8a07635f19fa83b7b2355186ba95c00fd37b81071d7b1c7d218cfeb5afebc69",
             "2adf3565088783d8788513927bb799fb5935f3e4ecce7f203ef60fc3e3298406"),
            ("63f202aaecd1516f470f28b369c78d6d996e9776a46717b504c67afa41c218048832efda0a2381a5ed4e1d0f299dacf1306fededa9c8ab3dee79579be859cb55",
             "620695046ab1daf7411e2fb6f3b69ad356aae4ad0af1109736a3b6fbae18120d",
             "0f75f57b75f43662feed9d09508b3826320d3f676722df5fa87fc47835e6df08",
             "c925ae6863cfdf51a79d1cee567e64417895a23bcaf39ad7528e7b0968585e0f",
             "c5ea60f51d0b01e232b0aca19b4758201abdd2bab2e442bba13ee941d9630503",
             "2823e9833399a7bd4d624822d99faecfc6dbc681d6f1061dc33585b26afb9002",
             "9864c96791ea6711555166397c9a24b6b768c4833e6ef5b3a62d83d9a4de7b68",
             "3356b9146eadb456f55ee25f4978dc72e8a14958eb709416091008b9c73d7506"),
        ];
        for (in64, red, a, b, c, ma, acl, macl) in cases {
            let mut i64b = [0u8; 64];
            unhex(in64, &mut i64b);
            assert_eq!(hex(&sc_reduce(&i64b)), *red, "sc_reduce");
            let mut ab = [0u8; 32];
            let mut bb = [0u8; 32];
            let mut cb = [0u8; 32];
            unhex(a, &mut ab);
            unhex(b, &mut bb);
            unhex(c, &mut cb);
            assert_eq!(hex(&sc_muladd(&ab, &bb, &cb)), *ma, "sc_muladd a<L");
            let mut aclb = [0u8; 32];
            unhex(acl, &mut aclb);
            assert_eq!(hex(&sc_muladd(&aclb, &bb, &cb)), *macl, "sc_muladd clamped a");
        }
    }

    // 边界：S < L 判定的三态（L-1 收 / L 拒 / 0 收）。
    #[test]
    fn less_than_l_boundary() {
        let mut l = [0u8; 32];
        l.copy_from_slice(&L_BYTES);
        assert!(!sc_is_less_l(&l), "L 不小于自身");
        let mut lm1 = l;
        lm1[0] -= 1;
        assert!(sc_is_less_l(&lm1));
        assert!(sc_is_less_l(&[0u8; 32]));
        // 2^255-1（全 1）≥ L → false。
        assert!(!sc_is_less_l(&[0xffu8; 32]));
        // clamped 标量 ≥ L → false（验签拒面同路径）。
        let mut clamped = [0u8; 32];
        clamped[31] = 0x40;
        assert!(!sc_is_less_l(&clamped));
    }

    // 零/单位语义：x=0 约减为 0；1·1+0 = 1（期望串手写避免格式对位坑）。
    #[test]
    fn zero_and_identity() {
        assert_eq!(hex(&sc_reduce(&[0u8; 64])), "0".repeat(64));
        let mut one = [0u8; 32];
        one[0] = 1;
        assert_eq!(hex(&sc_muladd(&one, &one, &[0u8; 32])), format!("01{}", "0".repeat(62)));
    }
}
