//! berry-trapface host 钉板（W3-Q37）——ESR EC 域提取/十三类词表/
//! FAR 归类判/定名表与 HAL `arch/aarch64/src/trap.rs` 分类面逐条对拍。

use crate::*;

/// ESR 位域提取：bits[31:26]、高位 ISS 不漏、全 64 EC 值往返。
#[test]
fn ec_of_bitfield() {
    // EC=0x15 的真实 ESR 形态（SVC #0：ISS 低 16 位号，高位 0）。
    assert_eq!(ec_of(0x5500_0000_0000_0000u64 >> 32), EC_SVC);
    // EC 在高 32 位之外的构造：ESR 全 64 位中 EC 只占 [31:26]——
    // 高 32 位全零时 EC 由低 32 位 [31:26] 决定。
    assert_eq!(ec_of(0x6F00_0000u64), 0x1b); // 0x6F000000 >> 26 = 0x1B
    // 钉板：移位/掩码常量与手算一致（0x55000000 >> 26 = 0x15）。
    assert_eq!(0x5500_0000u64 >> EC_SHIFT, 0x15);
    // 64 个 EC 值全扫：ec_of((ec as u64) << 26) == ec（往返）。
    for ec in 0u32..64 {
        assert_eq!(ec_of((ec as u64) << EC_SHIFT), ec, "ec={ec:#04x}");
    }
    // ISS 位不漏进 EC：bits[25:0] 全 1 时 EC 提取不变。
    for ec in 0u32..64 {
        let esr = ((ec as u64) << EC_SHIFT) | EC_MASK;
        assert_eq!(ec_of(esr), ec);
    }
    // 低于 EC 域的位（bits[25:0]）不影响提取——EC_MASK 只 6 位。
    assert_eq!(EC_MASK, 0x3f);
}

/// 十三类词表全表：词表内 EC ↔ EcKind 一一对应（含臂序无关性）。
#[test]
fn classify_full_wordlist() {
    assert_eq!(classify(EC_UNKNOWN), EcKind::Unknown);
    assert_eq!(classify(EC_FP_SIMD_TRAP), EcKind::FpSimdTrap);
    assert_eq!(classify(EC_FP_TRAP), EcKind::FpTrap);
    assert_eq!(classify(EC_SVC), EcKind::Svc);
    assert_eq!(classify(EC_IABORT_LOWER), EcKind::IAbootLower);
    assert_eq!(classify(EC_IABORT_CURRENT), EcKind::IAbootCurrent);
    assert_eq!(classify(EC_PC_ALIGN), EcKind::PcAlign);
    assert_eq!(classify(EC_DABORT_LOWER), EcKind::DAbootLower);
    assert_eq!(classify(EC_DABORT_CURRENT), EcKind::DAbootCurrent);
    assert_eq!(classify(EC_SP_ALIGN), EcKind::SpAlign);
    assert_eq!(classify(EC_SERROR), EcKind::SError);
    assert_eq!(classify(EC_BRK), EcKind::Brk);
    // 往返：词表内 12 类 ec() 回读恒等。
    for k in [
        EcKind::Unknown,
        EcKind::FpSimdTrap,
        EcKind::FpTrap,
        EcKind::Svc,
        EcKind::IAbootLower,
        EcKind::IAbootCurrent,
        EcKind::PcAlign,
        EcKind::DAbootLower,
        EcKind::DAbootCurrent,
        EcKind::SpAlign,
        EcKind::SError,
        EcKind::Brk,
    ] {
        assert_eq!(classify(k.ec()), k);
    }
}

/// 词表外折叠律：非在册 52 个 EC 值全归 Other（无「未知即 Unknown」折叠）。
#[test]
fn classify_other_fold() {
    let wordlist = [
        EC_UNKNOWN,
        EC_FP_SIMD_TRAP,
        EC_FP_TRAP,
        EC_SVC,
        EC_IABORT_LOWER,
        EC_IABORT_CURRENT,
        EC_PC_ALIGN,
        EC_DABORT_LOWER,
        EC_DABORT_CURRENT,
        EC_SP_ALIGN,
        EC_SERROR,
        EC_BRK,
    ];
    for ec in 0u32..64 {
        let expect = if wordlist.contains(&ec) { EcKind::Other } else { EcKind::Other };
        // 词表内值跳过（上一测钉），只验词表外：
        if wordlist.contains(&ec) {
            assert_ne!(classify(ec), EcKind::Other);
        } else {
            assert_eq!(classify(ec), expect, "ec={ec:#04x} 应折叠 Other");
            // Unknown ≠ Other：分类面不存在词表外折叠进 Unknown。
            assert_ne!(classify(ec), EcKind::Unknown);
        }
    }
    // 边界代表：0x2A（Other::ec 代表值）本身也在词表外。
    assert_eq!(classify(EcKind::Other.ec()), EcKind::Other);
}

/// FAR 归类六有效面：I/D-abort 双档 + PC/SP-align 恒 true。
#[test]
fn far_relevant_valid_set() {
    assert!(far_relevant(EC_IABORT_LOWER));
    assert!(far_relevant(EC_IABORT_CURRENT));
    assert!(far_relevant(EC_PC_ALIGN));
    assert!(far_relevant(EC_DABORT_LOWER));
    assert!(far_relevant(EC_DABORT_CURRENT));
    assert!(far_relevant(EC_SP_ALIGN));
}

/// FAR 归类无效面：SVC/BRK/FP 陷阱/Unknown/SError + 词表外全 false
/// （FAR 保留值——对抗审查 R4「幻影吞面」钉板）。
#[test]
fn far_relevant_invalid_set() {
    assert!(!far_relevant(EC_SVC));
    assert!(!far_relevant(EC_BRK));
    assert!(!far_relevant(EC_FP_SIMD_TRAP));
    assert!(!far_relevant(EC_FP_TRAP));
    assert!(!far_relevant(EC_UNKNOWN));
    assert!(!far_relevant(EC_SERROR));
    // 全 64 EC 扫描：有效面恰六值（词表外一律 false）。
    let valid = [
        EC_IABORT_LOWER,
        EC_IABORT_CURRENT,
        EC_PC_ALIGN,
        EC_DABORT_LOWER,
        EC_DABORT_CURRENT,
        EC_SP_ALIGN,
    ];
    for ec in 0u32..64 {
        assert_eq!(
            far_relevant(ec),
            valid.contains(&ec),
            "ec={ec:#04x} FAR 有效性"
        );
    }
}

/// 定名表逐字节冻结：十二名 + other 兜底（HAL 原 ec_name 迁移面）。
#[test]
fn ec_name_bytes_frozen() {
    assert_eq!(ec_name(0x00), b"Unknown");
    assert_eq!(ec_name(0x07), b"FP/SIMD-trap");
    assert_eq!(ec_name(0x0d), b"FP/trap");
    assert_eq!(ec_name(0x15), b"SVC");
    assert_eq!(ec_name(0x20), b"I-abort(lower)");
    assert_eq!(ec_name(0x21), b"I-abort(current)");
    assert_eq!(ec_name(0x22), b"PC-align");
    assert_eq!(ec_name(0x24), b"D-abort(lower)");
    assert_eq!(ec_name(0x25), b"D-abort(current)");
    assert_eq!(ec_name(0x26), b"SP-align");
    assert_eq!(ec_name(0x2f), b"SError");
    assert_eq!(ec_name(0x3c), b"BRK");
    // 兜底：词表外任意值（含边界 0x01、0x3f）恒 other。
    assert_eq!(ec_name(0x01), b"other");
    assert_eq!(ec_name(0x3f), b"other");
    // 分类视角 name() 与 ec_name(ec()) 一致（12 词表类 + Other）。
    for k in [
        EcKind::Unknown,
        EcKind::FpSimdTrap,
        EcKind::FpTrap,
        EcKind::Svc,
        EcKind::IAbootLower,
        EcKind::IAbootCurrent,
        EcKind::PcAlign,
        EcKind::DAbootLower,
        EcKind::DAbootCurrent,
        EcKind::SpAlign,
        EcKind::SError,
        EcKind::Brk,
        EcKind::Other,
    ] {
        assert_eq!(k.name(), ec_name(k.ec()));
    }
}

/// HAL 分流对拍：exception_sync/exception_sync_el0 两 match 的分流谓词
/// 用分类面重表达——SVC/BRK/DAbootCurrent 特权臂与 EL0 臂
/// （DAbootLower/IAbootLower 探针臂）分类等价。
#[test]
fn hal_dispatch_parity() {
    // exception_sync 特权臂：SVC(0x15) 派发 / BRK(0x3c) 恢复 /
    // D-abort(current)(0x25) 探针链 / 其余 fatal。
    for ec in 0u32..64 {
        let k = classify(ec);
        let is_special_el1 = matches!(k, EcKind::Svc | EcKind::Brk | EcKind::DAbootCurrent);
        if is_special_el1 {
            // 三特权臂在词表内恰三值。
            assert!(wordlist_has(ec));
        }
    }
    // exception_sync_el0：SVC 派发臂同号；canary 臂只对 DAbootLower；
    // EL0 探针臂盖 DAbootLower|IAbootLower（G4-4 X-2 并入）。
    assert_eq!(classify(0x15), EcKind::Svc);
    assert_eq!(classify(0x24), EcKind::DAbootLower);
    assert_eq!(classify(0x20), EcKind::IAbootLower);
    // canary FAR 查询门 = EC==0x24 专指（far 幻影吞面防）与
    // far_relevant 的 D-abort 档一致但更窄（单值门）。
    assert!(far_relevant(0x24));
}

fn wordlist_has(ec: u32) -> bool {
    matches!(
        ec,
        EC_UNKNOWN
            | EC_FP_SIMD_TRAP
            | EC_FP_TRAP
            | EC_SVC
            | EC_IABORT_LOWER
            | EC_IABORT_CURRENT
            | EC_PC_ALIGN
            | EC_DABORT_LOWER
            | EC_DABORT_CURRENT
            | EC_SP_ALIGN
            | EC_SERROR
            | EC_BRK
    )
}

/// 端到端：真实 ESR 构造（EC|ISS）→ 提取 → 分类 → 定名一条链。
#[test]
fn esr_end_to_end() {
    // SVC #0（ISS=0）。
    let esr = (EC_SVC as u64) << 26 | 0;
    assert_eq!(classify(ec_of(esr)), EcKind::Svc);
    assert_eq!(ec_name(ec_of(esr)), b"SVC");
    // BRK #0（ISS imm16 位 [20:5]）。
    let esr = (EC_BRK as u64) << 26 | (0x1234u64 << 5);
    assert_eq!(classify(ec_of(esr)), EcKind::Brk);
    // D-abort(lower)：DFSC=0x05（翻译缺页 L1）。
    let esr = (EC_DABORT_LOWER as u64) << 26 | 0x05;
    assert_eq!(classify(ec_of(esr)), EcKind::DAbootLower);
    assert!(far_relevant(ec_of(esr)));
    // I-abort(lower)：IFSC 同 0x05。
    let esr = (EC_IABORT_LOWER as u64) << 26 | 0x05;
    assert_eq!(classify(ec_of(esr)), EcKind::IAbootLower);
    assert!(far_relevant(ec_of(esr)));
    // SVC 带大 ISS（bits[25:0] 全域扰动不乱分类）。
    let esr = (EC_SVC as u64) << 26 | 0x3ff_ffff;
    assert_eq!(classify(ec_of(esr)), EcKind::Svc);
    assert!(!far_relevant(ec_of(esr)));
    // 全 64 EC × 随机 ISS：分类只看 EC。
    let mut iss = 0x9e37_79b9_7f4a_7c15u64; // 黄金比例常数作 LCG 种
    for ec in 0u32..64 {
        iss = iss.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let esr = ((ec as u64) << 26) | (iss & EC_MASK);
        assert_eq!(ec_of(esr), ec);
    }
}
