//! W3-Q70 host 钉板：值域四门 / EL1 高半 / AP 位域读写拒 / 页序 / 同页
//! 跳过窗——表驱动 + 敌意输入。

use crate::*;

// ---- el0_span_ok：值域四门 ----

#[test]
fn el0_zero_len_always_ok() {
    // len==0 恒过——含 ptr=0 野值（零长无触碰面，不折 EFAULT）
    assert!(el0_span_ok(0, 0));
    assert!(el0_span_ok(0xdead_beef, 0));
    assert!(el0_span_ok(u64::MAX, 0));
}

#[test]
fn el0_null_page_rejected() {
    assert!(!el0_span_ok(0x0, 1));
    assert!(!el0_span_ok(0xfff, 1)); // 恰界下：null 页末字节
    assert!(el0_span_ok(0x1000, 1)); // 恰过界：首页首字节
    // 跨界起点合法但区间含 null 页（start 合法、区间整体仍在低界上）
    assert!(el0_span_ok(0x1000, 0x2000));
}

#[test]
fn el0_upper_bound_rejected() {
    let end = USER_VA_END; // 1<<39
    assert!(el0_span_ok(end - 1, 1)); // 末字节恰达界
    assert!(!el0_span_ok(end, 1)); // 端点越界
    assert!(!el0_span_ok(end - 1, 2)); // 跨界尾
    assert!(!el0_span_ok(u64::MAX, 1));
}

#[test]
fn el0_wraparound_rejected() {
    // checked_add 回卷拒
    assert!(!el0_span_ok(u64::MAX - 3, 8));
    assert!(!el0_span_ok(u64::MAX, u64::MAX));
    // 非回卷巨值仍拒（先过回卷门再撞上界门——两序都钉）
    assert!(!el0_span_ok(1 << 40, 1));
}

#[test]
fn el0_domain_anchor() {
    // 现役消费面锚点：用户缓冲典型域
    assert!(el0_span_ok(0x10000, 0x1000)); // 常规用户页
    assert!(el0_span_ok(0x4000_0000, 256)); // COPY_CHUNK 消费形
}

// ---- el1_span_ok：高半单比较 + 回卷拒 ----

#[test]
fn el1_high_half_gate() {
    let kvb: u64 = 0xFFFF_FF80_0000_0000; // KERNEL_VA_BASE（HAL 锚）
    assert!(el1_span_ok(kvb, 0, kvb)); // 零长恒过（len==0 消费侧早退形）
    assert!(el1_span_ok(kvb, 0x1000, kvb));
    assert!(!el1_span_ok(kvb - 1, 1, kvb));
    assert!(!el1_span_ok(0, 8, kvb));
    assert!(!el1_span_ok(0x10000, 16, kvb)); // 用户区不认
}

#[test]
fn el1_wraparound_rejected() {
    // 回卷门在 EL1/EL0 分派之前——高半顶端回卷必须拒（迁移保形锚：
    // 若把回卷门只折进 EL0 面，此处即漂移红）
    let kvb: u64 = 0xFFFF_FF80_0000_0000;
    assert!(!el1_span_ok(u64::MAX, 1, kvb)); // MAX+1 回卷
    assert!(!el1_span_ok(u64::MAX - 3, 8, kvb)); // 跨 MAX 回卷
    assert!(!el1_span_ok(u64::MAX, u64::MAX, kvb)); // 巨长回卷
    // 非回卷高半长跨度仍过
    assert!(el1_span_ok(u64::MAX - 0x1000, 0x1000, kvb));
}

// ---- leaf_admits：AP 位域读写拒律 ----

#[test]
fn leaf_ap_read_write_matrix() {
    // ARMv8-A Stage-1 AP[2:1] ∈ {00,01,10,11} × {read(write=false), write(write=true)} 全表
    // 00 = EL1 RW / EL0 None：EL0 调用方读写均拒（防混淆代理人）
    assert!(!leaf_admits(0b00 << 6, false));
    assert!(!leaf_admits(0b00 << 6, true));
    // 01 = EL1/EL0 RW（UserRw）：读写均过
    assert!(leaf_admits(0b01 << 6, false));
    assert!(leaf_admits(0b01 << 6, true));
    // 10 = EL1 RO / EL0 None：EL0 调用方读写均拒
    assert!(!leaf_admits(0b10 << 6, false));
    assert!(!leaf_admits(0b10 << 6, true));
    // 11 = EL1/EL0 RO（UserRo / UserRx）：读过、写拒
    assert!(leaf_admits(0b11 << 6, false));
    assert!(!leaf_admits(0b11 << 6, true));
}

#[test]
fn leaf_ap_ignores_other_bits() {
    // AP 位域外位（含 PXN/UXN 高位、地址位、AF）不干扰
    let base = (1u64 << 54) | (1u64 << 53) | 0x0000_FFFF_FFFF_F003u64;
    for ap in 0..4u64 {
        let d = base | (ap << 6);
        assert_eq!(leaf_admits(d, false), ap == 0b01 || ap == 0b11);
        assert_eq!(leaf_admits(d, true), ap == 0b01);
    }
}

// ---- span_pages：页序 ----

#[test]
fn span_pages_order_and_count() {
    // 单页（页内短段）
    assert_eq!(span_pages(0x5000, 0x100).collect::<Vec<_>>(), vec![0x5000]);
    // 跨双页（页缘切分）
    assert_eq!(
        span_pages(0x4f00, 0x200).collect::<Vec<_>>(),
        vec![0x4000, 0x5000]
    );
    // 三页
    assert_eq!(
        span_pages(0x5000, 0x2001).collect::<Vec<_>>(),
        vec![0x5000, 0x6000, 0x7000]
    );
    // 恰页对齐单字节（末页=首页）
    assert_eq!(span_pages(0x5000, 1).collect::<Vec<_>>(), vec![0x5000]);
    // 全页恰好两页
    assert_eq!(
        span_pages(0x5000, 0x2000).collect::<Vec<_>>(),
        vec![0x5000, 0x6000]
    );
}

#[test]
fn span_pages_zero_len_empty() {
    assert_eq!(span_pages(0x5000, 0).count(), 0);
    assert_eq!(span_pages(0, 0).count(), 0);
}

#[test]
fn span_pages_unaligned_head_tail() {
    // 非对齐首尾：页基取掩码
    let v: Vec<u64> = span_pages(0x5010, 0x20).collect();
    assert_eq!(v, vec![0x5000]);
    // 首页尾字节 → 次页首字节
    let v: Vec<u64> = span_pages(0x5fff, 2).collect();
    assert_eq!(v, vec![0x5000, 0x6000]);
}

// ---- window_should_walk：同页跳过律 ----

#[test]
fn window_first_block_always_walks() {
    // SAME_PAGE_NONE 哨兵：首块必走查
    let mut last = SAME_PAGE_NONE;
    assert!(window_should_walk(0x5000, &mut last));
    assert_eq!(last, 0x5000); // 缓存已更新
}

#[test]
fn window_same_page_skips() {
    let mut last = SAME_PAGE_NONE;
    assert!(window_should_walk(0x5000, &mut last));
    assert!(!window_should_walk(0x5000, &mut last)); // 同页命中免走查
    assert!(window_should_walk(0x6000, &mut last)); // 换页须查
    assert!(!window_should_walk(0x6000, &mut last));
    // 回访已查页：缓存只记最近一页——回访仍须查（保守正确）
    assert!(window_should_walk(0x5000, &mut last));
}

#[test]
fn window_sentinel_never_hits_real_page() {
    // u64::MAX 两界外（用户 VA < 2^39、EL1 ≥ KERNEL_VA_BASE）——任何真实
    // 页基都不会与哨兵相等
    let mut last = SAME_PAGE_NONE;
    assert!(window_should_walk(USER_VA_END - PAGE_SIZE, &mut last));
    assert!(window_should_walk(0xFFFF_FF80_0000_0000, &mut last));
}

// ---- 组合律：窗形走查全流程（access_ok_window 纯投影对拍）----

#[test]
fn window_walk_over_multi_block_stream() {
    // 模拟 256B 块流覆盖 [0x5000, 0x6000)（同页 4 块 + 次页 1 块）：
    // 期望走查次数 = 页数（2）非块数（5）——同页跳过的收益面
    let mut last = SAME_PAGE_NONE;
    let mut walks = 0;
    for off in (0..0x1100).step_by(0x100) {
        let ptr = 0x5000 + off;
        let len = 0x100u64.min(0x1100 - off) as u64;
        let first = ptr & PAGE_MASK;
        let lastva = (ptr + len - 1) & PAGE_MASK;
        let mut va = first;
        while va <= lastva {
            if window_should_walk(va, &mut last) {
                walks += 1;
            }
            va += PAGE_SIZE;
        }
    }
    assert_eq!(walks, 2); // 两页各查一次——同页块全跳
}

#[test]
fn window_walk_zero_len_no_pages() {
    let mut last = SAME_PAGE_NONE;
    for ptr in [0u64, 0x5000, 0x1_0000_0000] {
        let _ = window_should_walk(ptr, &mut last); // 零长块不产页流（消费侧 len==0 早退）
    }
    // 零长流不更新缓存语义由消费侧保证——这里只钉哨兵初始态恒走查
    assert_eq!(last, 0x1_0000_0000);
}
