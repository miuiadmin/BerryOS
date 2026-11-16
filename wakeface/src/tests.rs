//! host 钉板（W3-Q66）——归因链三律：号→源映射全表 / 保存值域 / 槽位钳位。
//!
//! 镜像锚纪律（lib 头注）：HAL 冻结值镜像与 ktest WSRC-K A 端点锚同源，
//! HAL 改值/改号时本面红 + HAL 接线编译红双执法。

use crate::*;

/// 板上真值锚（HAL wake.rs src_of 消费面同款——gic.rs 冻结号 + wake_gen.rs
/// 冻结源值；改动任一侧本表红即双面执法）。
fn board() -> IrqMap {
    IrqMap {
        cntv: 27,
        pl011_rx: 33,
        pl1_rx: 40,
        pl061: 39,
        sgi_resched: 1,
        sgi_tlb: 2,
        src_timer: MIRROR_TIMER,
        src_uart_rx: MIRROR_UART_RX,
        src_power_key: MIRROR_POWER_KEY,
        src_sgi_resched: MIRROR_SGI_RESCHED,
        src_sgi_tlb: MIRROR_SGI_TLB,
        src_unknown: MIRROR_UNKNOWN,
    }
}

// ---- §1 号→源映射：板上活锚六点逐位 ----

#[test]
fn s1_board_anchors_exact() {
    let m = board();
    assert_eq!(m.src_of(27), MIRROR_TIMER); // CNTV PPI——tick 到期
    assert_eq!(m.src_of(33), MIRROR_UART_RX); // PL011 RX SPI33
    assert_eq!(m.src_of(40), MIRROR_UART_RX); // PL1 RX SPI40（归并不拆）
    assert_eq!(m.src_of(39), MIRROR_POWER_KEY); // PL061 电源键
    assert_eq!(m.src_of(1), MIRROR_SGI_RESCHED); // SGI 1
    assert_eq!(m.src_of(2), MIRROR_SGI_TLB); // SGI 2
}

#[test]
fn s1_dual_uart_merge_not_split() {
    // sim-S1 裁决：双 UART 同 U 类归并——两号同源、且都≠SGI/UNKNOWN。
    let m = board();
    let a = m.src_of(m.pl011_rx);
    let b = m.src_of(m.pl1_rx);
    assert_eq!(a, b);
    assert_ne!(a, MIRROR_UNKNOWN);
    assert_ne!(a, MIRROR_SGI_RESCHED);
    assert_ne!(a, MIRROR_SGI_TLB);
}

#[test]
fn s1_sgi_segment_orthogonal() {
    // SGI 段 0..16 与 SPI 段 32+ 正交：段内非两活锚的号恒 UNKNOWN；
    // 段边界钉板（0/15/16/31 段缘、32 SPI 首号无主）。
    let m = board();
    for id in [0u32, 3, 15, 16, 17, 31, 32, 34, 38, 41] {
        assert_eq!(m.src_of(id), MIRROR_UNKNOWN, "id={id} 应落未知臂");
    }
}

#[test]
fn s1_spurious_not_in_table() {
    // SPURIOUS(1023) 由 exception_irq 在 enter 之前早退——不进号表；
    // 语义一致：本函数收到 1023 也是未知臂（防误加臂钉板）。
    let m = board();
    assert_eq!(m.src_of(1023), MIRROR_UNKNOWN);
    // 敌意全域扫描：除六锚外全 UNKNOWN（含 1023 与超 1023 的幻影号）。
    let anchor_ids: [u32; 6] = [27, 33, 40, 39, 1, 2];
    for id in 0u32..2048 {
        let hit = anchor_ids.contains(&id);
        assert_eq!(
            m.src_of(id) != MIRROR_UNKNOWN,
            hit,
            "id={id} 锚属性错位"
        );
    }
}

#[test]
fn s1_unknown_never_defined_ring_src() {
    // 未知号产物恒 UNKNOWN——wsrcface::valid 恒 false（不进环），到达-only；
    // 本面只钉产物值 = UNKNOWN 端点锚（进环白名单归 wsrcface 钉板）。
    let m = board();
    for id in 0u32..1024 {
        if !matches!(id, 1 | 2 | 27 | 33 | 39 | 40) {
            assert_eq!(m.src_of(id), MIRROR_UNKNOWN);
        }
    }
}

// ---- §2 参数化镜像：号表锚可变时的判序仍成立（HAL 改号自愈面）----

#[test]
fn s2_parameterized_remap_roundtrip() {
    // 敌意重排六锚（模拟 HAL 改号）：映射跟随锚走，不依赖具体值。
    let m = IrqMap {
        cntv: 100,
        pl011_rx: 200,
        pl1_rx: 201,
        pl061: 202,
        sgi_resched: 5,
        sgi_tlb: 7,
        ..board()
    };
    assert_eq!(m.src_of(100), MIRROR_TIMER);
    assert_eq!(m.src_of(200), MIRROR_UART_RX);
    assert_eq!(m.src_of(201), MIRROR_UART_RX);
    assert_eq!(m.src_of(202), MIRROR_POWER_KEY);
    assert_eq!(m.src_of(5), MIRROR_SGI_RESCHED);
    assert_eq!(m.src_of(7), MIRROR_SGI_TLB);
    // 旧号让位：27/33/40/39/1/2 全落未知臂。
    for old in [27u32, 33, 40, 39, 1, 2] {
        assert_eq!(m.src_of(old), MIRROR_UNKNOWN);
    }
}

#[test]
fn s2_source_values_injectable() {
    // 源值参数化注入（HAL 常量面改值自愈——号表判序与源值域解耦）。
    let m = IrqMap {
        src_timer: 0x08,       // 预留段新源（add-only 下一号）
        src_unknown: 0x7E,     // 敌意改端点
        ..board()
    };
    assert_eq!(m.src_of(27), 0x08);
    assert_eq!(m.src_of(33), MIRROR_UART_RX); // 未改锚不动
    assert_eq!(m.src_of(0), 0x7E);
}

#[test]
fn s2_anchors_helper_matches_lookup() {
    let m = board();
    for (id, src) in m.anchors() {
        assert_eq!(m.src_of(id), src);
    }
    assert_eq!(m.anchors().len(), 6);
}

// ---- §3 保存恢复协议值域 ----

#[test]
fn s3_restore_valid_full_defined_set() {
    // 九源全过——保存值域 = 已定义源集（NONE 合法：副核首任务切入前窗口
    // 被 IRQ 打断时保存的就是 NONE）。
    for s in DEFINED {
        assert!(restore_valid(s), "已定义源 {s:#04x} 应可恢复");
    }
}

#[test]
fn s3_restore_rejects_undefined_and_reserved() {
    // 未定义段 0x08–0x7E（[[wakesrc]] 预留段）与 0x80–0xFF 全拒。
    for v in 0x08u8..=0x7E {
        assert!(!restore_valid(v), "预留段 {v:#04x} 不可恢复");
    }
    for v in 0x80u8..=0xFF {
        assert!(!restore_valid(v), "超域 {v:#04x} 不可恢复");
    }
    // 0x00–0x07 恰九集减一（0x08 起）：全域 256 值恰 9 真 247 假。
    let trues = (0u8..=255).filter(|&v| restore_valid(v)).count();
    assert_eq!(trues, 9);
}

#[test]
fn s3_save_restore_roundtrip_protocol() {
    // 协议模型：enter(src) 返回旧值 → exit(saved) 恢复。旧值来自 cell 读，
    // 其值域 = 写点三处可控集 ∪ 初值集——模型化为 DEFINED 全集：任一已
    // 定义值被保存后恢复，值域判恒过（保存-恢复协议对合法初值封闭）。
    let m = board();
    let arm = m.src_of(27); // UART/Timer 臂写入值也在 DEFINED 内
    assert!(wf_valid_for_restore(arm));
    for saved in DEFINED {
        // exit 语义模型：恢复后 cell == saved，且 saved 可再被后续 enter 保存。
        assert!(restore_valid(saved));
    }
}

/// 测试内辅助：臂写值可保存性（enter 后被打断时 cell 里就是臂源）。
fn wf_valid_for_restore(v: u8) -> bool {
    restore_valid(v)
}

#[test]
fn s3_irq_window_nesting_closed() {
    // IRQ 不嵌套（handler 全程 PSTATE.I=1）⇒ save/restore 窗同核串行闭合：
    // 模型 = 栈深 1 的保存恢复（第二次 enter 的保存值 = 第一次 enter 写入的
    // 臂源——仍在 DEFINED 内，可再恢复）。嵌套若发生，恢复序必倒置才正确
    // ——本面钉「任何深度的保存值都在值域内」，深度纪律归 HAL。
    let m = board();
    let saved_outer = MIRROR_IPC; // 任务语境被 IRQ 打断
    let arm = m.src_of(33); // UART 臂
    let saved_inner_if_nested = arm; // （IRQ 不嵌套——此值不会真出现）
    for v in [saved_outer, saved_inner_if_nested] {
        assert!(restore_valid(v));
    }
}

// ---- §4 槽位钳位 ----

#[test]
fn s4_slot_index_clamp() {
    assert_eq!(slot_index(0x00), 0);
    assert_eq!(slot_index(0x7F), 127);
    assert_eq!(slot_index(0x80), 127); // 毒值钳到 UNKNOWN 端点槽
    assert_eq!(slot_index(0xFF), 127);
    // 恒 < SLOTS（不越界不 panic）。
    for v in 0u8..=255 {
        assert!(slot_index(v) < SLOTS);
    }
}

#[test]
fn s4_slot_index_identity_in_domain() {
    // 值域内恒等（值直索引——table[src] 语义）。
    for v in 0u8..=0x7F {
        assert_eq!(slot_index(v), v as usize);
    }
}

#[test]
fn s4_constants_frozen_anchors() {
    // 冻结锚：槽数 128 / 端点 0x7F / 九源镜像逐位（wake_gen.rs 生成物
    // + abi.toml [[wakesrc]] 三方同源；改值红 = 双面执法）。
    assert_eq!(SLOTS, 128);
    assert_eq!(SRC_MAX, 0x7F);
    assert_eq!(MIRROR_NONE, 0x00);
    assert_eq!(MIRROR_KERNEL_BOOT, 0x01);
    assert_eq!(MIRROR_TIMER, 0x02);
    assert_eq!(MIRROR_UART_RX, 0x03);
    assert_eq!(MIRROR_SGI_RESCHED, 0x04);
    assert_eq!(MIRROR_SGI_TLB, 0x05);
    assert_eq!(MIRROR_IPC, 0x06);
    assert_eq!(MIRROR_POWER_KEY, 0x07);
    assert_eq!(MIRROR_UNKNOWN, 0x7F);
    assert_eq!(DEFINED.len(), 9);
    // 集内无重复（九源互异）。
    let mut s = DEFINED;
    s.sort_unstable();
    for w in s.windows(2) {
        assert_ne!(w[0], w[1]);
    }
}

#[test]
fn s4_wakesrc_addonly_next_free() {
    // add-only 纪律钉板：0x08–0x7E 预留段当前全空（未定义即不可恢复、
    // 也不在任何锚产物位）；新源落 0x08 时只追加不改旧值。
    for v in 0x08u8..=0x7E {
        assert!(!restore_valid(v));
        assert_ne!(board().src_of(27), v);
    }
    // 板上六锚产物恰覆盖 5 个源值（UART 双号归并——TIMER/UART/POWER_KEY/
    // SGI_RESCHED/SGI_TLB），NONE/KERNEL_BOOT/IPC/UNKNOWN 不在 IRQ 号表
    // （NONE/BOOT/IPC 是语境写点产物非 IRQ 号产物——值域正交钉板）。
    let m = board();
    let mut prods: Vec<u8> = m.anchors().iter().map(|&(_, s)| s).collect();
    prods.sort_unstable();
    prods.dedup();
    assert_eq!(prods.len(), 5);
    for ctx_only in [MIRROR_NONE, MIRROR_KERNEL_BOOT, MIRROR_IPC, MIRROR_UNKNOWN] {
        assert!(!prods.contains(&ctx_only));
    }
}
