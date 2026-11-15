//! droot 判序钉板（W3-Q60 host 测）。

use super::*;

fn s(asid: u16, ptr: usize) -> RootSlot {
    RootSlot { asid, ptr }
}

#[test]
fn register_empty_table_first_slot_wins() {
    let mut t = [EMPTY; DEMAND_ROOT_SLOTS];
    // 空表首登：槽 0 first-wins，实写返回 true。
    assert_eq!(register_step(&mut t, 1, 0x1000), Ok(true));
    assert_eq!(t[0], s(1, 0x1000));
    // 其余槽未动。
    for i in 1..DEMAND_ROOT_SLOTS {
        assert_eq!(t[i], EMPTY);
    }
}

#[test]
fn register_idempotent_same_pair() {
    let mut t = [EMPTY; DEMAND_ROOT_SLOTS];
    register_step(&mut t, 7, 0xAA00).unwrap();
    // 同 (asid, root) 再登记 Ok(false)——幂等，零写（槽位/值均不变）。
    assert_eq!(register_step(&mut t, 7, 0xAA00), Ok(false));
    assert_eq!(t[0], s(7, 0xAA00));
    assert_eq!(register_step(&mut t, 7, 0xAA00), Ok(false));
}

#[test]
fn register_owner_conflict_different_root() {
    let mut t = [EMPTY; DEMAND_ROOT_SLOTS];
    register_step(&mut t, 7, 0xAA00).unwrap();
    // 异 root 同 asid = 单 root 不变量破，防御拒。
    assert_eq!(register_step(&mut t, 7, 0xBB00), Err(DemandRootError::OwnerConflict));
    // 原登记不受扰。
    assert_eq!(t[0], s(7, 0xAA00));
}

#[test]
fn register_fills_first_free_not_append_only() {
    // 中间挖空（注销后复用）：下一个登记落**首个**空槽，非追加尾。
    let mut t = [EMPTY; DEMAND_ROOT_SLOTS];
    register_step(&mut t, 1, 0x1000).unwrap();
    register_step(&mut t, 2, 0x2000).unwrap();
    register_step(&mut t, 3, 0x3000).unwrap();
    deregister_step(&mut t, 2, 0x2000);
    assert_eq!(register_step(&mut t, 9, 0x9000), Ok(true));
    assert_eq!(t[1], s(9, 0x9000), "首空槽 first-wins（槽 1 复用，非槽 3）");
}

#[test]
fn register_idempotent_precedes_full_gate() {
    // 判序钉板：同 asid 幂等/冲突查**先于**空槽扫描与满表门——满表下同对
    // 幂等仍 Ok(false)、异 root 仍 OwnerConflict，新 asid 才 Full。
    let mut full = [EMPTY; DEMAND_ROOT_SLOTS];
    for (i, slot) in full.iter_mut().enumerate() {
        *slot = s((i + 1) as u16, 0x1000 * (i + 1));
    }
    assert_eq!(register_step(&mut full, 3, 0x3000), Ok(false));
    assert_eq!(register_step(&mut full, 3, 0xFFFF), Err(DemandRootError::OwnerConflict));
    assert_eq!(register_step(&mut full, 99, 0x99000), Err(DemandRootError::Full));
    // 满表三探零扰动。
    assert_eq!(full[2], s(3, 0x3000));
}

#[test]
fn register_full_table_rejects() {
    let mut t = [EMPTY; DEMAND_ROOT_SLOTS];
    for i in 0..DEMAND_ROOT_SLOTS {
        register_step(&mut t, (i + 1) as u16, 0x1000 * (i + 1)).unwrap();
    }
    // 16 槽满：新 asid 拒 Full（fail loud，不静默丢）。
    assert_eq!(register_step(&mut t, 99, 0x99000), Err(DemandRootError::Full));
    // 满表不扰动既有登记。
    assert_eq!(t[15], s(16, 0x10000));
}

#[test]
fn lookup_sentinel_and_miss() {
    let mut t = [EMPTY; DEMAND_ROOT_SLOTS];
    register_step(&mut t, 4, 0x4000).unwrap();
    // asid 0 = 空槽哨兵直达 None（分配器从 1 起，0 永不合法键）。
    assert_eq!(lookup_step(&t, 0), None);
    // 未登记键 miss。
    assert_eq!(lookup_step(&t, 5), None);
    // 命中。
    assert_eq!(lookup_step(&t, 4), Some(0x4000));
}

#[test]
fn lookup_half_state_x_zero_is_miss() {
    // 注销半态：asid 在 + ptr=0（deregister asid 先清实现的中间态镜像——
    // 若实现序反转则会出现此态）：按 miss 处理，hint 面零解引用。
    let mut t = [EMPTY; DEMAND_ROOT_SLOTS];
    t[2] = RootSlot { asid: 6, ptr: 0 };
    assert_eq!(lookup_step(&t, 6), None);
    // 后续槽有同 asid 合法对也命中（半态不短路整表扫描——kernel 形态：
    // 命中 asid 后 ptr==0 只跳过该槽继续扫）。
    t[5] = s(6, 0x6000);
    assert_eq!(lookup_step(&t, 6), Some(0x6000));
}

#[test]
fn lookup_first_hit_wins_no_slot_preference() {
    let mut t = [EMPTY; DEMAND_ROOT_SLOTS];
    t[0] = s(8, 0x8000);
    t[9] = s(8, 0x8800); // 敌意/异常态（单 root 不变量破镜像）；线性扫描取首中
    assert_eq!(lookup_step(&t, 8), Some(0x8000));
}

#[test]
fn deregister_double_match_only() {
    let mut t = [EMPTY; DEMAND_ROOT_SLOTS];
    register_step(&mut t, 2, 0x2000).unwrap();
    register_step(&mut t, 3, 0x3000).unwrap();
    // asid 匹配 ptr 不匹配：不清（asid 复用后旧 root 注销不得误清新登记）。
    assert!(!deregister_step(&mut t, 3, 0x9999));
    assert_eq!(t[1], s(3, 0x3000));
    // ptr 匹配 asid 不匹配：不清。
    assert!(!deregister_step(&mut t, 9, 0x3000));
    assert_eq!(t[1], s(3, 0x3000));
    // 双匹配：清成 EMPTY。
    assert!(deregister_step(&mut t, 3, 0x3000));
    assert_eq!(t[1], EMPTY);
    // 双匹配注销幂等：二注销 miss（槽已空）。
    assert!(!deregister_step(&mut t, 3, 0x3000));
    // 邻槽不受扰。
    assert_eq!(t[0], s(2, 0x2000));
}

#[test]
fn deregister_asid_reuse_does_not_clobber_new_registration() {
    // C-4 闭案镜像：asid 复用场景——旧 root 的迟到注销不得误清同 asid 的
    // 新登记（双匹配判据的立法动机）。
    const OLD: usize = 0xA0D0;
    const NEW: usize = 0xE4A0;
    let mut t = [EMPTY; DEMAND_ROOT_SLOTS];
    register_step(&mut t, 5, OLD).unwrap();
    deregister_step(&mut t, 5, OLD);
    register_step(&mut t, 5, NEW).unwrap();
    // 旧 root 迟到注销：asid 同但 ptr 异 → 不清。
    assert!(!deregister_step(&mut t, 5, OLD));
    assert_eq!(lookup_step(&t, 5), Some(NEW));
}

#[test]
fn full_cycle_register_lookup_deregister_reuse() {
    // 全生命周期：登记 → 反查 → 注销 → miss → 复用同槽再登记。
    let mut t = [EMPTY; DEMAND_ROOT_SLOTS];
    for round in 1..=3u16 {
        let ptr = 0x1000 * round as usize;
        assert_eq!(register_step(&mut t, round, ptr), Ok(true));
        assert_eq!(lookup_step(&t, round), Some(ptr));
        assert!(deregister_step(&mut t, round, ptr));
        assert_eq!(lookup_step(&t, round), None);
        // 注销后同 asid 可重登记（槽已清，非幂等路径——实写 true）。
        assert_eq!(register_step(&mut t, round, ptr), Ok(true));
        assert!(deregister_step(&mut t, round, ptr));
    }
    // 三轮后槽 0 空、无残留。
    assert_eq!(t[0], EMPTY);
    for i in 1..DEMAND_ROOT_SLOTS {
        assert_eq!(t[i], EMPTY);
    }
}

#[test]
fn hostile_all_slots_cartesian_zero_panic() {
    // 敌意全表：任意 (asid, ptr) 组合填满 16 槽，三原语零 panic 且语义
    // 稳定（lookup 只返回表中真实 ptr 或 None；register 判序可预测）。
    let mut t = [EMPTY; DEMAND_ROOT_SLOTS];
    for i in 0..DEMAND_ROOT_SLOTS {
        t[i] = RootSlot { asid: (i as u16) % 5, ptr: if i % 3 == 0 { 0 } else { 0xD000 + i } };
    }
    for probe in 0..6u16 {
        let _ = lookup_step(&t, probe); // 零 panic
    }
    for probe in 0..5u16 {
        let _ = register_step(&mut t, probe, 0x7000);
        let _ = deregister_step(&mut t, probe, 0x7000);
    }
    // 语义钉：asid 0 哨兵直达（表内容无关——敌意域内 asid%5==0 槽不参与）。
    assert_eq!(lookup_step(&t, 0), None);
}

#[test]
fn slots_trait_impl_array_drive() {
    // trait 泛型消费面钉板：`[RootSlot; N]` 直驱可用（host 测试形态），
    // N > DEMAND_ROOT_SLOTS 的敌意大表只扫前 16 槽（判序域 = 槽常数）。
    let mut big = [EMPTY; 32];
    big[20] = s(42, 0x42000); // 第 20 槽在扫描域外
    assert_eq!(lookup_step(&big, 42), None);
    // 域内登记可用。
    assert_eq!(register_step(&mut big, 42, 0x42000), Ok(true));
    assert_eq!(lookup_step(&big, 42), Some(0x42000));
}
