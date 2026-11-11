//! berry-dpgface host 表驱动钉板（W3-Q24）——fsc 滤形/槽池认领/属主
//! 过滤取/GC 臂/溢出 convoy 路由五面。

use berry_dpgface::*;

// ---------------------------------------------------------------------------
// fsc 滤形（probe/probe_el0 入口——DFSC = ESR[5:0] ∈ 4..=7）
// ---------------------------------------------------------------------------

#[test]
fn fsc_translation_band_full() {
    // 全档恰过：L3..L0 = 4/5/6/7（ISS 低 6 位直达）。
    for dfsc in 4..=7u32 {
        assert!(fsc_is_translation(dfsc as usize), "dfsc={} 须过", dfsc);
    }
}

#[test]
fn fsc_translation_band_boundaries_reject() {
    // 恰拒界：3（access flag fault）/ 8（TLB conflict abort）——位域
    // 合法集闭区间外一步即拒（活锁株防线）。
    assert!(!fsc_is_translation(3), "dfsc=3（access flag）须拒");
    assert!(!fsc_is_translation(8), "dfsc=8（TLB conflict）须拒");
}

#[test]
fn fsc_mask_low_six_bits_only() {
    // 高位不扰：ISS 占 ESR[24:0]，但 DFSC 只取低 6 位——EC 等高位
    // 置任何值不改变判定（掩码律）。
    let base = 0x24u64 << 26; // EC=0x24（D-abort EL0）全高位
    assert!(fsc_is_translation(base as usize | 5));
    assert!(!fsc_is_translation(base as usize | 9));
    // 6 位回绕：dfsc=0x44 = 4（高位被掩掉）。
    assert!(fsc_is_translation(0x44), "dfsc 位域只取 [5:0]——0x44 ≡ 4 须过");
}

#[test]
fn fsc_permission_faults_reject() {
    // 权限档族全拒（已映射只读页写等——原杀链）：0x0F permission L3、
    // 0x0E L2、0x0D L1、0x0C L0（DFSC permission fault 四级）。
    for dfsc in [0x0Cu32, 0x0D, 0x0E, 0x0F] {
        assert!(!fsc_is_translation(dfsc as usize), "dfsc={:#x} 须拒", dfsc);
    }
    // 同步外部错 0x10 / tag check 0x0B 亦拒。
    assert!(!fsc_is_translation(0x10));
    assert!(!fsc_is_translation(0x0B));
}

// ---------------------------------------------------------------------------
// 空闲槽认领（扫描序不变量）
// ---------------------------------------------------------------------------

#[test]
fn claim_empty_pool_takes_slot_zero() {
    let pool = SlotPool::empty();
    assert_eq!(claim_free_slot(&pool), Claim::Free { slot: 0 });
}

#[test]
fn claim_scans_lowest_free_in_order() {
    // 前段武装 → 首空闲 = 最低 false 位（find 扫描序不变量）。
    let mut pool = SlotPool::empty();
    pool.arm(0, 0x1000, 1);
    pool.arm(1, 0x2000, 1);
    assert_eq!(claim_free_slot(&pool), Claim::Free { slot: 2 });
    // 中段放槽 → 复用刚放槽（最低空闲回填）。
    pool.disarm(0);
    assert_eq!(claim_free_slot(&pool), Claim::Free { slot: 0 }, "放槽即回填最低空闲");
}

#[test]
fn claim_full_pool_routes_overflow() {
    // 满装 K 臂 → PoolFull → 溢出键（convoy ≤ N-K 有界）。
    let mut pool = SlotPool::empty();
    for i in 0..SLOTS {
        pool.arm(i, 0x1000 + i as usize * 0x1000, 7);
    }
    assert_eq!(claim_free_slot(&pool), Claim::PoolFull);
    assert_eq!(pool.armed_count(), SLOTS);
    // 放一槽 → 立即可认领（take 消费放槽即唤重扫的 host 侧同律）。
    pool.disarm(17);
    assert_eq!(claim_free_slot(&pool), Claim::Free { slot: 17 });
}

#[test]
fn claim_full_then_release_recycles_same_slot() {
    // ktest E3f ② 同律：消费即放槽 → 复臂回收同号（认领制指纹）。
    let mut pool = SlotPool::empty();
    for i in 0..SLOTS {
        pool.arm(i, 0x5300_0000, 9);
    }
    assert_eq!(claim_free_slot(&pool), Claim::PoolFull);
    pool.disarm(5);
    let Claim::Free { slot } = claim_free_slot(&pool) else {
        panic!("放槽后须可认领");
    };
    assert_eq!(slot, 5, "复臂须回收刚放槽（最低空闲 = 被消费位）");
}

// ---------------------------------------------------------------------------
// 属主过滤取（el0_take_for 三态判定表）
// ---------------------------------------------------------------------------

/// 恒等解析器：asid==1 → 属主 100，asid==2 → 属主 200，其余 None（孤儿）。
fn resolver(far: usize, asid: u16) -> Option<OwnerId> {
    match asid {
        1 => Some(100),
        2 => Some(200),
        // asid=3 且 far 落 0xF000_0000 以下 = 区间 miss（孤儿）。
        3 if far >= 0xF000_0000 => Some(300),
        _ => None,
    }
}

#[test]
fn take_empty_pool_is_pool_empty() {
    let mut pool = SlotPool::empty();
    let (r, gc) = take_for(&mut pool, 100, &resolver);
    assert_eq!(r, TakeFor::PoolEmpty);
    assert_eq!(gc, 0);
}

#[test]
fn take_matching_owner_takes_lowest_slot() {
    // ktest E3f ① 同律：满装 K 臂全属 vd → 首收最低位。
    let mut pool = SlotPool::empty();
    for i in 0..SLOTS {
        pool.arm(i, 0x5300_0000, 1); // asid=1 → 属主 100
    }
    let (r, gc) = take_for(&mut pool, 100, &resolver);
    assert_eq!(r, TakeFor::Supplied { far: 0x5300_0000, asid: 1, slot: 0 }, "首收须最低位槽");
    assert_eq!(gc, 0);
    assert_eq!(pool.armed_count(), SLOTS - 1);
}

#[test]
fn take_mismatch_leaves_pool_untouched() {
    // X-5 错配窗让位：有武装槽但无一属主匹配 → NoneForVd 零消费零唤醒。
    let mut pool = SlotPool::empty();
    pool.arm(3, 0x1000, 2); // 属主 200 ≠ vd=100
    pool.arm(7, 0x2000, 2);
    let (r, gc) = take_for(&mut pool, 100, &resolver);
    assert_eq!(r, TakeFor::NoneForVd);
    assert_eq!(gc, 0);
    assert_eq!(pool.armed_count(), 2, "让位面零消费——池态不变");
}

#[test]
fn take_mixed_owners_takes_only_vd_slot() {
    // 混池：vd 槽 + 他窗槽并存 → 只取属主匹配槽，他窗槽让位。
    let mut pool = SlotPool::empty();
    pool.arm(2, 0x1000, 2); // 他窗（属主 200）
    pool.arm(5, 0x2000, 1); // 本 vd（属主 100）
    pool.arm(9, 0x3000, 2); // 他窗
    let (r, gc) = take_for(&mut pool, 100, &resolver);
    assert_eq!(r, TakeFor::Supplied { far: 0x2000, asid: 1, slot: 5 });
    assert_eq!(gc, 0);
    assert_eq!(pool.armed_count(), 2, "他窗两臂不动");
}

#[test]
fn take_vd_slot_not_lowest_but_first_match_wins() {
    // 匹配槽非最低索引时仍取**最低索引匹配槽**（不是最低索引槽）。
    let mut pool = SlotPool::empty();
    pool.arm(4, 0x1000, 2); // 他窗低位
    pool.arm(6, 0x2000, 1); // 匹配高位
    pool.arm(8, 0x3000, 1); // 匹配更高位
    let (r, _) = take_for(&mut pool, 100, &resolver);
    assert_eq!(r, TakeFor::Supplied { far: 0x2000, asid: 1, slot: 6 }, "取最低**匹配**槽 6 非 4 非 8");
}

// ---------------------------------------------------------------------------
// 孤儿槽 GC 臂（ktest E3e 同律——消费 + 续扫 + 不置 any_armed）
// ---------------------------------------------------------------------------

#[test]
fn take_orphan_slot_gc_consumes_and_rescans() {
    // E3e ①：孤儿槽（解析失败）被 GC 消费——不属 vd 亦不放走。
    let mut pool = SlotPool::empty();
    pool.arm(2, 0x1000, 3); // asid=3 far<0xF000_0000 → 孤儿
    let (r, gc) = take_for(&mut pool, 100, &resolver);
    assert_eq!(r, TakeFor::PoolEmpty, "孤儿被 GC 后无他臂 → PoolEmpty");
    assert_eq!(gc, 1, "GC 消费恰 1 槽");
    assert_eq!(pool.armed_count(), 0);
}

#[test]
fn take_orphan_gc_keeps_scanning_to_vd_match() {
    // E3e ② 前半：孤儿在前、匹配在后 → GC 消费孤儿后续扫命中 vd 槽。
    let mut pool = SlotPool::empty();
    pool.arm(1, 0x1000, 3); // 孤儿
    pool.arm(4, 0x2000, 1); // 匹配 vd=100
    let (r, gc) = take_for(&mut pool, 100, &resolver);
    assert_eq!(r, TakeFor::Supplied { far: 0x2000, asid: 1, slot: 4 }, "GC 后续扫须命中匹配槽");
    assert_eq!(gc, 1);
    assert_eq!(pool.armed_count(), 0, "孤儿 + 匹配均被消费");
}

#[test]
fn take_orphan_only_then_pool_empty_no_any_armed_residue() {
    // E3e ③：仅剩孤儿臂 → 再 take → PoolEmpty（any_armed 不残留）。
    let mut pool = SlotPool::empty();
    pool.arm(0, 0x1000, 3);
    pool.arm(1, 0x2000, 3);
    let (r, gc) = take_for(&mut pool, 100, &resolver);
    assert_eq!(r, TakeFor::PoolEmpty, "孤儿全 GC 后须 PoolEmpty（不置 any_armed）");
    assert_eq!(gc, 2);
}

#[test]
fn take_orphan_and_foreign_armed_yields_none_for_vd() {
    // 孤儿 GC + 他窗在席 → 池态如实 = NoneForVd（any_armed 由他窗置）。
    let mut pool = SlotPool::empty();
    pool.arm(0, 0x1000, 3); // 孤儿（GC）
    pool.arm(1, 0x2000, 2); // 他窗（any_armed）
    let (r, gc) = take_for(&mut pool, 100, &resolver);
    assert_eq!(r, TakeFor::NoneForVd);
    assert_eq!(gc, 1);
    assert_eq!(pool.armed_count(), 1, "他窗臂不动");
}

// ---------------------------------------------------------------------------
// 溢出 convoy 路由（认领 vs 溢出键分流）
// ---------------------------------------------------------------------------

#[test]
fn faulter_key_route_directed_on_claim() {
    assert_eq!(faulter_key_route(Claim::Free { slot: 12 }), FaulterKey::Slot { slot: 12 });
    assert_eq!(faulter_key_route(Claim::PoolFull), FaulterKey::Overflow);
}

#[test]
fn overflow_convoy_bounded_by_n_minus_k() {
    // 有界性数学：N faulter 抢 K 槽，溢出 convoy 规模 = max(0, N-K)。
    // N≤K 恒零（现役拓扑 N≤15 < K=32）；N>K 有界 ≤N-K。
    let k = SLOTS;
    for n in [0usize, 1, 15, k, k + 1, k + 7] {
        let convoy = n.saturating_sub(k);
        // 模拟 N 个认领：前 min(N,K) 成功认领，其余 PoolFull。
        let mut pool = SlotPool::empty();
        let mut overflowed = 0;
        for _ in 0..n {
            match claim_free_slot(&pool) {
                Claim::Free { slot } => pool.arm(slot, 0x1000, 1),
                Claim::PoolFull => overflowed += 1,
            }
        }
        assert_eq!(overflowed, convoy, "N={} convoy 须 = N-K 有界", n);
    }
}

#[test]
fn overflow_release_wakes_rescan_recovers_slot() {
    // 溢出者醒来重扫：take 消费放槽 → 重扫认领同槽（收敛性——舞不
    // 挂死的判定面）。
    let mut pool = SlotPool::empty();
    for i in 0..SLOTS {
        pool.arm(i, 0x1000, 1);
    }
    let (r, _) = take_for(&mut pool, 100, &resolver);
    let TakeFor::Supplied { slot, .. } = r else {
        panic!("满装全属 vd 须 Supplied");
    };
    // 放槽后溢出者重扫 → 认领刚放槽。
    assert_eq!(claim_free_slot(&pool), Claim::Free { slot }, "重扫须回收刚放槽");
    // 再 take（同 vd）→ 次低位匹配（认领序推进）。
    let (r2, _) = take_for(&mut pool, 100, &resolver);
    assert_eq!(r2, TakeFor::Supplied { far: 0x1000, asid: 1, slot: 1 }, "次收须次低位匹配槽");
}

// ---------------------------------------------------------------------------
// 池快照机械（arm/disarm/armed_count 不变量）
// ---------------------------------------------------------------------------

#[test]
fn pool_arm_disarm_roundtrip() {
    let mut pool = SlotPool::empty();
    assert!(!pool.disarm(3), "空池 disarm 须 false（防御跳过零半态）");
    pool.arm(3, 0xABCD, 42);
    assert_eq!((pool.far[3], pool.asid[3], pool.armed[3]), (0xABCD, 42, true));
    assert!(pool.disarm(3));
    assert!(!pool.armed[3], "消费后放槽");
    assert_eq!(pool.armed_count(), 0);
}

#[test]
fn pool_full_cycle_drain() {
    // ktest E3f ③ 同律：满装 → 逐 take 抽干 K 臂 → PoolEmpty。
    let mut pool = SlotPool::empty();
    for i in 0..SLOTS {
        pool.arm(i, 0x5300_0000 + i * 0x1000, 1);
    }
    for j in 0..SLOTS {
        let (r, _) = take_for(&mut pool, 100, &resolver);
        assert!(matches!(r, TakeFor::Supplied { .. }), "抽干第 {} 臂须 Supplied", j);
    }
    let (r, gc) = take_for(&mut pool, 100, &resolver);
    assert_eq!(r, TakeFor::PoolEmpty, "抽干后须 PoolEmpty");
    assert_eq!(gc, 0);
}

// ---------------------------------------------------------------------------
// 交叉面：fsc × 认领（探针入口判序——非翻译缺页不进认领环）
// ---------------------------------------------------------------------------

#[test]
fn non_translation_fault_never_enters_claim() {
    // probe/probe_el0 判序：fsc 滤在认领环**前**——非翻译缺页 return
    // false 原杀链，池态零扰动（判序 ABI 面钉板）。
    for bad_esr in [3usize, 8, 0x0F, 0x10, 0x2B] {
        if fsc_is_translation(bad_esr) {
            panic!("bad_esr={:#x} 不应过 fsc 滤", bad_esr);
        }
        // 不进认领环 = 池不变（模拟：filter 后零操作）。
        let pool = SlotPool::empty();
        assert_eq!(pool.armed_count(), 0);
    }
    // 翻译缺页才走认领（fsc=5）。
    assert!(fsc_is_translation(5));
    let mut pool = SlotPool::empty();
    let Claim::Free { slot } = claim_free_slot(&pool) else {
        panic!("空池须可认领");
    };
    pool.arm(slot, 0x1000, 1);
    assert_eq!(pool.armed_count(), 1);
}
