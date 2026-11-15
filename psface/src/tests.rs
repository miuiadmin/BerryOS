//! psface host 钉板（W3-Q72）——PSCI 版本门/路由门/FEATURES 侦察全表。

use crate::*;

// ---- 版本编码域：major/minor 拆装往返 ----

#[test]
fn version_encode_decode_roundtrip() {
    // PSCI 规范实锚：1.0 = 0x00010000；0.2 = 0x00000002（SYSTEM_OFF 强制线）。
    assert_eq!(version_encode(1, 0), 0x0001_0000);
    assert_eq!(version_encode(0, 2), 0x0000_0002);
    assert_eq!(version_encode(1, 1), 0x0001_0001);
    // 全域 2^16 × 边角采样拆装往返（全域笛卡尔 65536² 过大，取边角 + 步进扫描）。
    for major in [0u32, 1, 2, 0x7FFF, 0x8000, 0xFFFE, 0xFFFF] {
        for minor in [0u32, 1, 2, 0x7FFF, 0x8000, 0xFFFE, 0xFFFF] {
            let ver = version_encode(major, minor);
            assert_eq!(version_major(ver), major);
            assert_eq!(version_minor(ver), minor);
        }
    }
    // 步进扫描：连续 65536 值拆装恒等 + 拼装幂等。
    for v in 0u32..=0xFFFF {
        let m = version_major(v);
        let n = version_minor(v);
        assert!(m <= 0xFFFF && n <= 0xFFFF);
        assert_eq!(version_encode(m, n), v);
    }
}

#[test]
fn version_encode_masks_overflow_fields() {
    // 敌意超宽入参：域外位被掩掉（拼装不回卷不进位）。
    assert_eq!(version_encode(0x1_0001, 0x2_0003), 0x0001_0003);
    assert_eq!(version_encode(u32::MAX, u32::MAX), 0xFFFF_FFFF);
}

// ---- 版本有效门 ----

#[test]
fn version_valid_gate() {
    // 三锚：1.0 恰过、0xFFFF（0.FFFF）恰拒、-1 截断形一律拒。
    assert!(version_valid(0x0001_0000));
    assert!(!version_valid(0x0000_FFFF));
    assert!(!version_valid(0xFFFF_FFFF));
    // 边角：0 拒（无 conduit 假返回）、0x00010001 过、u32::MAX 拒。
    assert!(!version_valid(0));
    assert!(version_valid(0x0001_0001));
    assert!(!version_valid(u32::MAX));
    // 全域扫描钉死单调分界：[0, 0xFFFF] 拒 / [0x10000, 0xFFFFFFFE] 过 / MAX 拒。
    for v in 0u32..=0xFFFF {
        assert!(!version_valid(v), "v={v:#x} 应拒");
    }
    for v in 0x0001_0000..=0xFFFF_FFFE {
        if v % 0x0100_0000 == 0 || v == 0x0001_0000 || v == 0xFFFF_FFFE {
            assert!(version_valid(v), "v={v:#x} 应过");
        }
    }
}

#[test]
fn version_valid_sign_semantics() {
    // SMCCC -1 的两种截断面：i64 -1 as u32 = 0xFFFFFFFF；零扩展 0x00000000FFFFFFFF
    // 同值——「先截 u32 再判」律下无歧义。i64 正值高位垃圾同理被截断律吸收。
    let ret: i64 = -1;
    assert_eq!(ret as u32, 0xFFFF_FFFF);
    assert!(!version_valid(ret as u32));
    // 高 32 位非零但低位域有效：截断后应过（不赌 conduit 位扩展行为）。
    let garbage_high: i64 = 0x1_0000_0001;
    assert_eq!(garbage_high as u32, 0x0000_0001);
    assert!(!version_valid(garbage_high as u32)); // 1 < 1.0 仍拒
    let garbage_high_valid: i64 = 0x1_0001_0000;
    assert_eq!(garbage_high_valid as u32, 0x0001_0000);
    assert!(version_valid(garbage_high_valid as u32)); // 截断后 = 1.0 恰过
}

// ---- 探测路由门 ----

#[test]
fn probe_route_gate_order() {
    // 门住两形先于探测形：EFI 门优先级最高（efi_boot ∧ boot_el==1 仍 EFI 门）。
    assert_eq!(probe_route(true, 1), ProbeRoute::EfiGated);
    assert_eq!(probe_route(true, 2), ProbeRoute::EfiGated);
    assert_eq!(probe_route(true, 0), ProbeRoute::EfiGated);
    // 非 EL1 门：EL2/EL0/EL3 各形。
    assert_eq!(probe_route(false, 2), ProbeRoute::BootElGated(2));
    assert_eq!(probe_route(false, 0), ProbeRoute::BootElGated(0));
    assert_eq!(probe_route(false, 3), ProbeRoute::BootElGated(3));
    // EL1 唯一探测形。
    assert_eq!(probe_route(false, 1), ProbeRoute::Probe);
}

#[test]
fn probe_route_gate_implies_no_hvc() {
    // 门住形 = 零 HVC 零副作用（类型层面只携带 EL，无探测参数）。
    for (efi, el) in [(true, 1u64), (true, 2), (false, 2), (false, 0), (false, 3)] {
        match probe_route(efi, el) {
            ProbeRoute::EfiGated => assert!(efi),
            ProbeRoute::BootElGated(e) => {
                assert!(!efi);
                assert_ne!(e, 1);
            }
            ProbeRoute::Probe => {
                assert!(!efi);
                assert_eq!(el, 1);
            }
        }
    }
}

// ---- FEATURES 侦察 ----

#[test]
fn features_scout_table() {
    // 三档现实表：0 = 支持；-1 = TCG 实测拒；其它非零 = 不支持。
    assert!(features_suspend_supported(0));
    assert!(!features_suspend_supported(-1));
    assert!(!features_suspend_supported(1));
    assert!(!features_suspend_supported(-3));
    assert!(!features_suspend_supported(i64::MIN));
    assert!(!features_suspend_supported(i64::MAX));
}

// ---- SYSTEM_OFF 能力判 ----

#[test]
fn off_supported_conduit_alive_semantics() {
    // conduit 活 = PSCI_VERSION 有效（0.2 起强制函数不查 FEATURES）——
    // 比 susp 三档表宽：TCG version 有效但 FEATURES -1 → off=1 susp=0。
    assert!(off_supported_from(0x0001_0000));
    assert!(!off_supported_from(0xFFFF_FFFF));
    assert!(!off_supported_from(0x0000_FFFF));
    // 与 susp 判的独立性：version 有效 + FEATURES 拒 → off 过 susp 拒。
    let ver = 0x0001_0000u32;
    assert!(off_supported_from(ver));
    assert!(!features_suspend_supported(-1));
}

// ---- 常量冻结锚 ----

#[test]
fn frozen_constants() {
    // PSCI 1.0 fnid（32 位函数号域 0x84000000+）+ SMCCC 错码锚。
    assert_eq!(PSCI_VERSION_FNID, 0x8400_0000);
    assert_eq!(PSCI_FEATURES_FNID, 0x8400_000A);
    assert_eq!(PSCI_SYSTEM_SUSPEND_FNID, 0x8400_000C);
    assert_eq!(PSCI_SYSTEM_OFF_FNID, 0x8400_0008);
    assert_eq!(PSCI_VERSION_MIN, 0x0001_0000);
    assert_eq!(NOT_SUPPORTED, -1);
    assert_eq!(INVALID_PARAMETERS, -3);
}
