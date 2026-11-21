//! `berry-asidface` 宿主机完备性测试钉板（F0-2）。

use berry_asidface::{tcr_as_config, AsidState, ASID_SHIFT, TCR_AS_BIT};

#[test]
fn tcr_as_and_shift_law() {
    // 8 位与 16 位 ASID 在 TTBR0_EL1 / TLBI ASIDE1 中起始位均为 48（严禁 8 位移 56）。
    assert_eq!(ASID_SHIFT, 48);
    // ID_AA64MMFR0_EL1.ASIDBits = 0 → 8 位，TCR.AS = 0。
    assert_eq!(tcr_as_config(0x00), (8, 0));
    // ID_AA64MMFR0_EL1.ASIDBits = 2 → 16 位，TCR.AS = 1<<36。
    assert_eq!(tcr_as_config(0x20), (16, TCR_AS_BIT));
    // 保留值保守退化 8 位。
    assert_eq!(tcr_as_config(0x10), (8, 0));
    assert_eq!(tcr_as_config(0xF0), (8, 0));
}

#[test]
fn rejects_invalid_width() {
    assert!(AsidState::new(0).is_none());
    assert!(AsidState::new(13).is_none());
    assert!(AsidState::new(32).is_none());
}

#[test]
fn width8_never_collides_with_live_or_pinned_across_1000_rolls() {
    let mut s = AsidState::new(8).unwrap();
    // 常驻进程（如 svcmgr=1、fsmgr=2、vnet=3）永久存活不释放。
    assert_eq!(s.alloc(&[]), Some((1, false)));
    assert_eq!(s.alloc(&[]), Some((2, false)));
    assert_eq!(s.alloc(&[]), Some((3, false)));

    // 额外在双核上钉死 ASID 42 与 200（模拟核上延迟切换残影）。
    let pinned = [42u16, 200u16];

    let mut rolls = 0usize;
    let mut gen_prev = s.generation();
    while rolls < 1000 {
        let (id, rolled) = s.alloc(&pinned).expect("每次单短命进程必可分配");
        assert_ne!(id, 0, "永不发保留号 0");
        assert!(id > 3, "永不撞常驻存活号 1..=3，got {id}");
        assert_ne!(id, 42, "永不发核0钉死号 42");
        assert_ne!(id, 200, "永不发核1钉死号 200");
        if rolled {
            rolls += 1;
            assert!(s.generation() > gen_prev);
            gen_prev = s.generation();
        }
        // 短命子进程退出释放。
        s.free(id);
    }
    assert_eq!(s.active_count(), 3);
}

#[test]
fn width8_exhaustion_returns_none_and_recovers_on_free() {
    let mut s = AsidState::new(8).unwrap();
    for expected in 1u16..=254 {
        assert_eq!(s.alloc(&[]), Some((expected, false)));
    }
    assert_eq!(s.alloc(&[]), Some((255, true)));
    assert_eq!(s.active_count(), 255);
    // 全部 255 个可用号在用：第 256 次必须诚实返 None，绝不撞号！
    assert_eq!(s.alloc(&[]), None);

    // 释放 100 号，但它当前仍在 CPU1 钉死 → 仍不可分配（返 None）。
    s.free(100);
    assert_eq!(s.active_count(), 254);
    assert_eq!(s.alloc(&[100]), None);

    // CPU1 切走解除钉死后，100 号即可安全分配。
    let (reused, _) = s.alloc(&[]).expect("解除钉死后应可分配 100");
    assert_eq!(reused, 100);
    assert_eq!(s.active_count(), 255);
}

#[test]
fn width16_full_sweep_and_force_near_roll() {
    let mut s = AsidState::new(16).unwrap();
    assert_eq!(s.alloc(&[]), Some((1, false)));
    s.force_near_roll();
    // 65535 空闲时，force_near_roll 后首调发 65535 且 rolled=true。
    assert_eq!(s.alloc(&[]), Some((0xFFFF, true)));
    // 紧接着下一次回绕跳过存活的 1 号，精准发 2 号！
    assert_eq!(s.alloc(&[]), Some((2, false)));

    // 若 65535 已被占用，force_near_roll 后首调仍跨回卷发 3 号且 rolled=true！
    s.force_near_roll();
    assert_eq!(s.alloc(&[]), Some((3, true)));

    // 释放 0 号或重复释放均为安全无害空操作。
    s.free(0);
    assert!(s.is_allocated(0));
    s.free(2);
    s.free(2);
    assert_eq!(s.active_count(), 3); // 1, 65535, 3
}
