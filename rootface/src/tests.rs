//! host 钉板（W3-Q85）——四门全表 + 敌意值域 + 端到端模型对拍。

use crate::*;

// ---- 1. vol_admit：发现门全表 ----

#[test]
fn vol_admit_table() {
    // 空卷拒（产链契约）。
    assert_eq!(vol_admit(0, 0), VolOutcome::Reject);
    // 单字节合法（域开区间下界邻点）。
    assert!(matches!(vol_admit(1, 0), VolOutcome::Admit(1)));
    // 恰帽过（MAX_VOL 是闭上界——kernel `sz > ROOTFS_MAX` 拒）。
    assert!(matches!(vol_admit(MAX_VOL, 0), VolOutcome::Admit(MAX_VOL)));
    // 帽+1 拒。
    assert_eq!(vol_admit(MAX_VOL + 1, 0), VolOutcome::Reject);
    // Σ 恰帽过（4 卷 × MAX_VOL = TOTAL_MAX）。
    let t3 = match vol_admit(MAX_VOL, MAX_VOL * 3) {
        VolOutcome::Admit(t) => t,
        VolOutcome::Reject => panic!("Σ=cap must admit"),
    };
    assert_eq!(t3, TOTAL_MAX);
    // Σ 帽+1 拒（第 5 卷首字节即越总量帽）。
    assert_eq!(vol_admit(1, TOTAL_MAX), VolOutcome::Reject);
}

#[test]
fn vol_admit_overflow_guard() {
    // 敌意巨值：Σ 溢出 usize 域 = 整源降级（checked_add 守卫）。
    assert_eq!(vol_admit(1, usize::MAX), VolOutcome::Reject);
    assert_eq!(vol_admit(usize::MAX, 0), VolOutcome::Reject);
    // 敌意 sz = usize::MAX-1：单卷帽先拒（判序 2 先于 3）。
    assert_eq!(vol_admit(usize::MAX - 1, 0), VolOutcome::Reject);
}

#[test]
fn vol_admit_prefix_discovery_model() {
    // 连续前缀模型对拍：3 卷 [8MiB, 4MiB, 2MiB] 后 rootfs3 缺席 → 止。
    let vols = [8 << 20, 4 << 20, 2 << 20];
    let mut total = 0usize;
    let mut admitted = 0usize;
    for (i, &sz) in vols.iter().enumerate() {
        match vol_admit(sz, total) {
            VolOutcome::Admit(t) => {
                total = t;
                admitted += 1;
            }
            VolOutcome::Reject => panic!("vol {i} must admit"),
        }
    }
    assert_eq!(admitted, 3);
    assert_eq!(total, 14 << 20);
    // 第 4 卷缺席（kernel 侧 `let Some(e) = … else { break }`）不在本面
    // ——vol_admit 只判在位卷；缺席止是调用侧控制流。
}

// ---- 2. blob_span_ok：v0 域门全表 ----

#[test]
fn blob_span_ok_table() {
    const RB: u64 = 0x4000_0000;
    const RT: u64 = 0x4000_0000 + (1 << 30); // 1GiB DRAM 窗
    // 零拒两形。
    assert!(!blob_span_ok(0, 0, RB, RT));
    assert!(!blob_span_ok(RB, 0, RB, RT));
    assert!(!blob_span_ok(0, 4096, RB, RT));
    // 基址首字节合法。
    assert!(blob_span_ok(RB, 1, RB, RT));
    // 恰贴窗顶（end == ram_top 合法——半开区间上界内）。
    assert!(blob_span_ok(RT - 1, 1, RB, RT));
    // 越窗顶 1 拒。
    assert!(!blob_span_ok(RT, 1, RB, RT));
    // 窗下界之下拒。
    assert!(!blob_span_ok(RB - 1, 1, RB, RT));
    // 越单卷帽拒。
    assert!(!blob_span_ok(RB, MAX_VOL as u64 + 1, RB, RT));
    // 恰帽过。
    assert!(blob_span_ok(RB, MAX_VOL as u64, RB, RT));
}

#[test]
fn blob_span_ok_wraparound() {
    const RB: u64 = 0x4000_0000;
    const RT: u64 = 0x4000_0000 + (1 << 30);
    // 回卷拒：pa 巨值 + len 1 → checked_add 不回卷但越顶拒。
    assert!(!blob_span_ok(u64::MAX - 1, 1, RB, RT));
    // 敌意 pa = u64::MAX：窗检先拒（判序 3 先于 4）。
    assert!(!blob_span_ok(u64::MAX, 1, RB, RT));
    // len 敌意巨值：帽拒先于窗检（判序 2 先于 3/4）。
    assert!(!blob_span_ok(RB, u64::MAX, RB, RT));
}

// ---- 3. segs_total_step：isegs Σ 帽 ----

#[test]
fn segs_total_step_table() {
    // 常规累加。
    assert_eq!(segs_total_step(0, 100), Some(100));
    assert_eq!(segs_total_step(100, 200), Some(300));
    // 恰帽过。
    assert_eq!(segs_total_step(TOTAL_MAX - 1, 1), Some(TOTAL_MAX));
    // 帽+1 拒。
    assert_eq!(segs_total_step(TOTAL_MAX, 1), None);
    assert_eq!(segs_total_step(TOTAL_MAX - 1, 2), None);
    // 溢出守卫。
    assert_eq!(segs_total_step(usize::MAX, 1), None);
    assert_eq!(segs_total_step(usize::MAX - 1, 2), None);
    // 零长段合法（kernel 侧 segs 非空已由 init_segs 域判保证——本面只
    // 管累加帽；0 段 Σ 不变）。
    assert_eq!(segs_total_step(1000, 0), Some(1000));
}

#[test]
fn segs_end_to_end_model() {
    // 端到端模型：31 段 EFI 图残余 ~200 块的典型 5 段累加 + 对账全过。
    let segs = [
        8 << 20,
        8 << 20,
        4 << 20,
        2 << 20,
        (1 << 20) + 4096, // 末段非页倍（kernel 侧 plen 取整面——非本面）
    ];
    let mut total = 0usize;
    for &s in &segs {
        total = segs_total_step(total, s).expect("typical segs must admit");
    }
    assert_eq!(total, 23 << 20 | 4096);
    // 拼装对账：实拼 == Σ。
    assert!(assembly_matches(total, total));
    // 差一即拒（拼装错位）。
    assert!(!assembly_matches(total - 1, total));
    assert!(!assembly_matches(total + 1, total));
}

// ---- 4. assembly_matches ----

#[test]
fn assembly_matches_table() {
    assert!(assembly_matches(0, 0));
    assert!(assembly_matches(1, 1));
    assert!(!assembly_matches(0, 1));
    assert!(!assembly_matches(1, 0));
    // 敌意巨值对拍。
    assert!(assembly_matches(usize::MAX, usize::MAX));
    assert!(!assembly_matches(usize::MAX - 1, usize::MAX));
}

// ---- 常量冻结锚 ----

#[test]
fn constants_frozen() {
    assert_eq!(MAX_VOL, 32 << 20);
    assert_eq!(TOTAL_MAX, 128 << 20);
}
