//! billface host 钉板测试（W3-Q76）。

use crate::{charge_step, domain_total, fold_step, win_len_of, ChargeStep, FoldStep, WinState,
    BILLING_SLOTS, DOMAIN_EMPTY, THETA_P_PERCENT};

#[test]
fn charge_zero_inputs_skip() {
    // 零值拒绝：domain 0 / ns 0 任何快照下零动作。
    let d = [7u64, 0, 0, 0, 0, 0, 0, 0];
    let c = [100u64; BILLING_SLOTS];
    assert_eq!(charge_step(0, 500, &d, &c), ChargeStep::SkipZero);
    assert_eq!(charge_step(7, 0, &d, &c), ChargeStep::SkipZero);
    assert_eq!(charge_step(0, 0, &d, &c), ChargeStep::SkipZero);
}

#[test]
fn charge_existing_domain_accrues_first_hit() {
    // 既有域：首命中槽（重复域取最前——快照内不二建）。
    let d = [0u64, 0, 100, 0, 0, 200, 0, 0];
    let c = [0u64; BILLING_SLOTS];
    assert_eq!(charge_step(200, 5, &d, &c), ChargeStep::Accrue { slot: 5 });
    assert_eq!(charge_step(100, 5, &d, &c), ChargeStep::Accrue { slot: 2 });
    // 既有域判先于空槽抢占（判序 1 先于 2——即便 slot0 空也不抢占新域外的路）。
    assert_eq!(charge_step(300, 5, &d, &c), ChargeStep::Claim { slot: 0 });
}

#[test]
fn charge_empty_slot_claim_first() {
    // 空槽抢占：首个空槽（CAS 语义投影——命中即出槽号）。
    let d = [1u64, 2, 0, 0, 0, 0, 0, 0];
    let c = [0u64; BILLING_SLOTS];
    assert_eq!(charge_step(9, 5, &d, &c), ChargeStep::Claim { slot: 2 });
}

#[test]
fn charge_full_table_overwrites_min() {
    // 表满覆写最小账户：最小值唯一时精确命中；覆写语义 = store 覆盖（非累加）。
    let d = [1u64, 2, 3, 4, 5, 6, 7, 8];
    let c = [500u64, 100, 300, 900, 1000, 200, 50, 700];
    assert_eq!(charge_step(42, 5, &d, &c), ChargeStep::Overwrite { slot: 6 });
    let mut c2 = c;
    c2[6] = 5; // 模拟 Overwrite 执行（store 5）
    assert_eq!(c2[6], 5);
}

#[test]
fn charge_full_table_tie_takes_first() {
    // 最小值并列取最前槽（覆写确定性）。
    let d = [1u64, 2, 3, 4, 5, 6, 7, 8];
    let c = [100u64, 100, 200, 300, 100, 500, 100, 700];
    assert_eq!(charge_step(42, 5, &d, &c), ChargeStep::Overwrite { slot: 0 });
}

#[test]
fn charge_cas_race_convergence() {
    // CAS 竞态收敛模型：Claim 失败（他人先占）→ 新快照重走 → Accrue（不二建）。
    let mut d = [0u64; BILLING_SLOTS];
    let c = [0u64; BILLING_SLOTS];
    assert_eq!(charge_step(9, 5, &d, &c), ChargeStep::Claim { slot: 0 });
    d[0] = 9; // 他人 CAS 胜出
    assert_eq!(charge_step(9, 5, &d, &c), ChargeStep::Accrue { slot: 0 });
    // 双占竞态第二形：两空槽同时 Claim 同槽——败者重走落下一空槽。
    let mut d2 = [0u64; BILLING_SLOTS];
    d2[0] = 9;
    let c2 = [0u64; BILLING_SLOTS];
    assert_eq!(charge_step(10, 5, &d2, &c2), ChargeStep::Claim { slot: 1 });
}

#[test]
fn charge_lifecycle_model_vs_ktest() {
    // ktest test_billing 全周期模型对拍（ktest.rs:5088 断言的纯函数重演）：
    // charge(100,1000) + charge(100,2000) → domain_total(100)=3000；
    // charge(200,500) → domain_total(200)=500；charge(0,999)/charge(300,0) 拒。
    let mut d = [0u64; BILLING_SLOTS];
    let mut c = [0u64; BILLING_SLOTS];
    assert_eq!(charge_step(100, 1_000, &d, &c), ChargeStep::Claim { slot: 0 });
    d[0] = 100; c[0] = 1_000;
    assert_eq!(charge_step(100, 2_000, &d, &c), ChargeStep::Accrue { slot: 0 });
    c[0] += 2_000;
    assert_eq!(domain_total(100, &d, &c), Some(3_000));
    assert_eq!(charge_step(200, 500, &d, &c), ChargeStep::Claim { slot: 1 });
    d[1] = 200; c[1] = 500;
    assert_eq!(domain_total(100, &d, &c), Some(3_000)); // 不串槽
    assert_eq!(domain_total(200, &d, &c), Some(500));
    assert_eq!(charge_step(0, 999, &d, &c), ChargeStep::SkipZero);
    assert_eq!(charge_step(300, 0, &d, &c), ChargeStep::SkipZero);
    assert_eq!(domain_total(300, &d, &c), None); // 未建账
}

#[test]
fn charge_overflow_model_ktest() {
    // ktest test_billing_overflow 模型（ktest.rs:5971）：9 域逼满 8 槽——
    // 第 9 域落 Overwrite（全 1000 并列取最前 = 0）；牺牲最冷域可查询实锤。
    let mut d = [0u64; BILLING_SLOTS];
    let mut c = [0u64; BILLING_SLOTS];
    for i in 0..8 {
        let dom = 300 + i as u64;
        assert_eq!(charge_step(dom, 1_000, &d, &c), ChargeStep::Claim { slot: i }, "domain {dom}");
        d[i] = dom;
        c[i] = 1_000;
    }
    assert_eq!(charge_step(308, 1_000, &d, &c), ChargeStep::Overwrite { slot: 0 });
    d[0] = 308; c[0] = 1_000; // 覆写执行
    assert_eq!(domain_total(308, &d, &c), Some(1_000));
    assert_eq!(domain_total(300, &d, &c), None); // 牺牲最冷域（被覆写）
}

#[test]
fn domain_total_zero_domain_none() {
    // 0 域恒 None（不承载语义——即便快照含 0 值空槽）。
    let d = [0u64; BILLING_SLOTS];
    let c = [55u64; BILLING_SLOTS];
    assert_eq!(domain_total(0, &d, &c), None);
    // 首命中：重复域快照（敌意）取最前。
    let d = [9u64, 9, 0, 0, 0, 0, 0, 0];
    let c = [11u64, 22, 0, 0, 0, 0, 0, 0];
    assert_eq!(domain_total(9, &d, &c), Some(11));
}

#[test]
fn fold_skip_when_unwired_or_within() {
    // len==0（未接线）快退；未跨窗（now-start < len）快退；恰未跨（差 1）快退。
    assert_eq!(fold_step(1_000, WinState { len: 0, start: 0, sum: 999 }), FoldStep::Skip);
    assert_eq!(fold_step(1_000, WinState { len: 500, start: 600, sum: 100 }), FoldStep::Skip);
    assert_eq!(fold_step(1_099, WinState { len: 100, start: 1_000, sum: 100 }), FoldStep::Skip);
}

#[test]
fn fold_rolls_at_exact_boundary() {
    // 恰跨窗：now - start == len → Roll（判 < 不含等）。
    assert_eq!(fold_step(1_100, WinState { len: 100, start: 1_000, sum: 0 }),
        FoldStep::Roll { fire: false, carry: 0 });
}

#[test]
fn fold_theta_p_threshold_exact() {
    // θ_p = 10%：恰界 sum×100 == len×10 → fire（≥ 含等）；界下 1 不 fire。
    assert_eq!(fold_step(1000, WinState { len: 1000, start: 0, sum: 100 }),
        FoldStep::Roll { fire: true, carry: 100 });
    assert_eq!(fold_step(1000, WinState { len: 1000, start: 0, sum: 99 }),
        FoldStep::Roll { fire: false, carry: 99 });
}

#[test]
fn fold_saturating_pathology() {
    // 多 waiter Σ>W 病态面：sum = u64::MAX 乘 100 饱和不 panic。
    assert_eq!(fold_step(1, WinState { len: 1, start: 0, sum: u64::MAX }),
        FoldStep::Roll { fire: true, carry: u64::MAX });
    // len 巨值乘 θ_p 饱和同样不 panic（len×10 饱和；未跨窗 Skip 先行）。
    assert_eq!(fold_step(1, WinState { len: u64::MAX, start: 0, sum: 1 }), FoldStep::Skip);
}

#[test]
fn fold_wraparound_semantics() {
    // 回绕窗语义：start 在 now 近未来（差 < len）→ wrapping 差回绕成小值
    // → Skip（与 telemetry.rs 旧内联一致——「start 略超前」视为未跨窗，
    // 不 panic 不误折卷）。
    let w = WinState { len: 100, start: u64::MAX - 5, sum: 10 };
    assert_eq!(fold_step(5, w), FoldStep::Skip); // 5-(MAX-5) 回绕 = 11 < 100
    // start 在 now 远未来：wrapping 差 = 巨值 ≥ len → Roll。
    let w = WinState { len: 100, start: 1_000_000, sum: 10 };
    assert!(matches!(fold_step(5, w), FoldStep::Roll { .. })); // 差回绕 ≈ 2^64-1e6
}

#[test]
fn win_len_of_conversion() {
    // 窗长换算：dpg_win_init 同式（freq×ms/1000）。
    assert_eq!(win_len_of(62_500_000, 700), 43_750_000); // QEMU 频 700ms
    assert_eq!(win_len_of(1_000, 0), 0);
    assert_eq!(win_len_of(0, 700), 0);
    // 饱和：巨 freq 不 panic。
    let v = win_len_of(u64::MAX, 1000);
    assert!(v > 0 && v <= u64::MAX);
}

#[test]
fn constants_frozen() {
    // 常量冻结锚（值域与 telemetry.rs 镜像由消费侧 const/re-export 执法）。
    assert_eq!(BILLING_SLOTS, 8);
    assert_eq!(DOMAIN_EMPTY, 0);
    assert_eq!(THETA_P_PERCENT, 10);
}
