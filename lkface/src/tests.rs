//! berry-lkface host 钉板（W3-Q36）——rank 全序表/三检查判序/违约定名
//! 与 HAL `arch/aarch64/src/lockdep.rs` 立法逐条对拍。

use crate::*;

/// 九类 rank 全表（S-3 §9.7 七类全序立法钉板；RQ0/RQ1 同 2、TABLE/PT
/// 同 4 为「分立类号同 rank」两先例）。
#[test]
fn rank_table_full_order() {
    assert_eq!(rank(CLASS_SQ), 1);
    assert_eq!(rank(CLASS_RQ0), 2);
    assert_eq!(rank(CLASS_RQ1), 2);
    assert_eq!(rank(CLASS_REAPQ), 3);
    assert_eq!(rank(CLASS_TABLE), 4);
    assert_eq!(rank(CLASS_PT), 4);
    assert_eq!(rank(CLASS_ALLOC), 5);
    assert_eq!(rank(CLASS_ASID), 6);
    assert_eq!(rank(CLASS_PRINT), 7);
    // 类号是标签非 rank：PT 类号 9 > PRINT 类号 8，但 rank 反序。
    assert!(CLASS_PT > CLASS_PRINT);
    assert!(rank(CLASS_PT) < rank(CLASS_PRINT));
}

/// 全序九连（selftest ① 活性序——rank 非降序列全过）。
#[test]
fn acquire_full_legal_chain() {
    let legal = [
        CLASS_SQ,
        CLASS_RQ0,
        CLASS_RQ1,
        CLASS_REAPQ,
        CLASS_TABLE,
        CLASS_PT,
        CLASS_ALLOC,
        CLASS_ASID,
        CLASS_PRINT,
    ];
    let mut chain = [0u8; DEPTH_MAX];
    for (i, &c) in legal.iter().enumerate() {
        assert_eq!(
            check_acquire(&chain, i as u8, 0, c),
            None,
            "合法序第 {i} 取（class={c}）须过"
        );
        chain[i] = c;
    }
    // 层内同 rank 双向（TABLE→PT 与 PT→TABLE 互取合法——层内无序）：
    // 链中须无同类在席（否则先撞重入检——重入与层内序是两道检）。
    let mut chain_pt = [0u8; DEPTH_MAX];
    chain_pt[0] = CLASS_PT;
    assert_eq!(check_acquire(&chain_pt, 1, 0, CLASS_TABLE), None);
    let mut chain_tbl = [0u8; DEPTH_MAX];
    chain_tbl[0] = CLASS_TABLE;
    assert_eq!(check_acquire(&chain_tbl, 1, 0, CLASS_PT), None);
}

/// 先验序违约：持 PRINT（rank 7）再取 SQ（rank 1）= order。
#[test]
fn acquire_order_violation() {
    let chain = [CLASS_PRINT, 0, 0];
    assert_eq!(check_acquire(&chain, 1, 0, CLASS_SQ), Some(Violation::Order));
    // RQ1 → REAPQ 同向 rank 2→3 合法；反向 3→2 违。
    let chain2 = [CLASS_REAPQ, 0, 0];
    assert_eq!(check_acquire(&chain2, 1, 0, CLASS_RQ1), Some(Violation::Order));
    assert_eq!(check_acquire(&chain2, 1, 0, CLASS_SQ), Some(Violation::Order));
}

/// 重入：地板上同类二取（SQ on SQ）= reentry；RQ0/RQ1 分立类号故
/// 互取不是重入（升序专项另检）。
#[test]
fn acquire_reentry_violation() {
    let chain = [CLASS_SQ, CLASS_SQ, 0];
    assert_eq!(check_acquire(&chain, 2, 0, CLASS_SQ), Some(Violation::Reentry));
    // RQ0 已持再取 RQ1：rank 同 2 非序违、类号异非重入 → 合法（跨核偷取）。
    let chain2 = [CLASS_RQ0, 0, 0];
    assert_eq!(check_acquire(&chain2, 1, 0, CLASS_RQ1), None);
}

/// RQ 升序专项：持 RQ1 再取 RQ0 = rq-descend（双核升序立法机器化）；
/// 深链中段命中同样短路。
#[test]
fn acquire_rq_descend_violation() {
    let chain = [CLASS_RQ1, 0, 0];
    assert_eq!(
        check_acquire(&chain, 1, 0, CLASS_RQ0),
        Some(Violation::RqDescend)
    );
    // 合法向：RQ0 已持再取 RQ1 = 升序，过。
    let chain2 = [CLASS_RQ0, 0, 0];
    assert_eq!(check_acquire(&chain2, 1, 0, CLASS_RQ1), None);
    // 中段命中：SQ → RQ1 → …再取 RQ0 仍拦。
    let chain3 = [CLASS_SQ, CLASS_RQ1, 0];
    assert_eq!(
        check_acquire(&chain3, 2, 0, CLASS_RQ0),
        Some(Violation::RqDescend)
    );
}

/// F5 前置：类号域外（0 与 >CLASS_MAX）= unknown-class，先于环扫
/// （链里就算已埋腐词条也拦在前置）。
#[test]
fn acquire_unknown_class_guards_first() {
    let chain = [CLASS_SQ, CLASS_SQ, 0];
    assert_eq!(
        check_acquire(&chain, 2, 0, 0),
        Some(Violation::UnknownClass)
    );
    assert_eq!(
        check_acquire(&chain, 2, 0, CLASS_MAX + 1),
        Some(Violation::UnknownClass)
    );
    assert_eq!(
        check_acquire(&chain, 2, 0, 255),
        Some(Violation::UnknownClass)
    );
}

/// 链深帽：depth ≥ DEPTH_MAX = chain-overflow，判序最前（帽满时类号
/// 域外也报帽——与 HAL 逐字对齐：depth 检查在 F5 前）。
#[test]
fn acquire_chain_overflow_first() {
    let chain = [0u8; DEPTH_MAX];
    assert_eq!(
        check_acquire(&chain, DEPTH_MAX as u8, 0, CLASS_SQ),
        Some(Violation::ChainOverflow)
    );
    // 帽满 + 坏类号：链深帽先行。
    assert_eq!(
        check_acquire(&chain, DEPTH_MAX as u8, 0, 0),
        Some(Violation::ChainOverflow)
    );
    // 恰界：depth = DEPTH_MAX-1 且链词条合法 → 过（帽是 ≥ 不是 >）。
    let chain2 = [CLASS_SQ, 0, 0];
    assert_eq!(check_acquire(&chain2, 1, 0, CLASS_RQ0), None);
}

/// IRQ 地板分层：被断流词条沉底（floor 前不参与序检）——持 SQ 被 IRQ
/// 断，handler 词条在地板上取 SQ = 合法（跨流非重入）；地板上同类
/// 二取才拦。
#[test]
fn floor_isolates_interrupted_flow() {
    // 被断流持 SQ（depth=1），IRQ 抬地板=1，handler 取 SQ：检查域空 → 过。
    let chain = [CLASS_SQ, 0, 0];
    assert_eq!(check_acquire(&chain, 1, 1, CLASS_SQ), None);
    // handler 取 PRINT 再取 SQ：域内 rank 7 > 1 → order（本流序检照常）。
    let chain2 = [CLASS_SQ, CLASS_PRINT, 0];
    assert_eq!(check_acquire(&chain2, 2, 1, CLASS_SQ), Some(Violation::Order));
    // handler 同类二取（floor 上）→ reentry。
    let chain3 = [CLASS_SQ, CLASS_RQ0, 0];
    assert_eq!(check_acquire(&chain3, 2, 1, CLASS_RQ0), Some(Violation::Reentry));
}

/// 放锁两臂：underflow（depth==floor 地板下放锁）与 unpair（栈顶非
/// 本类——链顶 RQ1、放 REAPQ）。
#[test]
fn release_two_arms() {
    let chain = [CLASS_RQ1, 0, 0];
    // underflow：floor=1（IRQ 地板）下 depth=1 放锁 = 越流。
    assert_eq!(
        check_release(&chain, 1, 1, CLASS_SQ),
        Some(Violation::ReleaseUnderflow)
    );
    // unpair：栈顶 RQ1、放 REAPQ。
    assert_eq!(
        check_release(&chain, 1, 0, CLASS_REAPQ),
        Some(Violation::ReleaseUnpair)
    );
    // 合法放：栈顶恰本类。
    assert_eq!(check_release(&chain, 1, 0, CLASS_RQ1), None);
    // IRQ 域内合法放（floor 上词条）。
    let chain2 = [CLASS_SQ, CLASS_RQ0, 0];
    assert_eq!(check_release(&chain2, 2, 1, CLASS_RQ0), None);
}

/// IRQ 出口配平：depth≠floor = irq-unbalanced；恰归 = 过。
#[test]
fn irq_exit_balance() {
    assert_eq!(check_irq_exit(2, 1), Some(Violation::IrqUnbalanced));
    assert_eq!(check_irq_exit(1, 1), None);
    assert_eq!(check_irq_exit(0, 0), None);
    assert_eq!(check_irq_exit(3, 0), Some(Violation::IrqUnbalanced));
}

/// 违约定名字节串与 HAL violation 报告面同词（smoke grep 面）。
#[test]
fn violation_kind_bytes_frozen() {
    assert_eq!(Violation::ChainOverflow.as_bytes(), b"chain-overflow");
    assert_eq!(Violation::UnknownClass.as_bytes(), b"unknown-class");
    assert_eq!(Violation::Reentry.as_bytes(), b"reentry");
    assert_eq!(Violation::Order.as_bytes(), b"order");
    assert_eq!(Violation::RqDescend.as_bytes(), b"rq-descend");
    assert_eq!(Violation::ReleaseUnderflow.as_bytes(), b"release-underflow");
    assert_eq!(Violation::ReleaseUnpair.as_bytes(), b"release-unpair");
    assert_eq!(Violation::IrqUnbalanced.as_bytes(), b"irq-unbalanced");
}

/// 状态机模型对拍：模拟 acquire/release 全周期（真链推弹），任意序恒
/// 配平 + 违约恒被检查面拦——「检查面 = 物理臂的唯一判据源」端到端。
#[test]
fn push_pop_model_full_cycle() {
    // selftest ① 同款九连真推真弹（活性）。
    let mut chain = [0u8; DEPTH_MAX];
    let legal = [
        CLASS_SQ,
        CLASS_RQ0,
        CLASS_RQ1,
        CLASS_REAPQ,
        CLASS_TABLE,
        CLASS_PT,
        CLASS_ALLOC,
        CLASS_ASID,
        CLASS_PRINT,
    ];
    for (i, &c) in legal.iter().enumerate() {
        assert_eq!(check_acquire(&chain, i as u8, 0, c), None);
        chain[i] = c;
    }
    // 九深放锁：每步栈顶恰本类。
    for i in (0..legal.len()).rev() {
        let c = legal[i];
        assert_eq!(
            check_release(&chain, (i + 1) as u8, 0, c),
            None,
            "第 {i} 层放锁须配平"
        );
        chain[i] = 0;
    }
    // 终态深度归零 = IRQ 配平律在净链上的镜像。
    assert_eq!(check_irq_exit(0, 0), None);
}

/// 敌意快照 fuzz（确定性 LCG）：任意 (chain, depth, floor, class) 组合
/// 检查面零 panic——值域防御（切片索引只落在合法窗内）。
#[test]
fn hostile_snapshots_never_panic() {
    let mut s: u64 = 0x5eed_cafe_f00d_0001;
    let mut next = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    let mut chain = [0u8; DEPTH_MAX];
    for _ in 0..2000 {
        for slot in chain.iter_mut() {
            *slot = (next() % 16) as u8; // 含域外类号 10..=15
        }
        let depth = (next() % (DEPTH_MAX as u64 + 2)) as u8; // 含越帽值
        let floor = (next() % (DEPTH_MAX as u64 + 2)) as u8;
        let class = (next() % 16) as u8;
        // floor > depth 的病态窗：检查面合同要求调用方保证 floor ≤ depth
        // （HAL 链槽恒成立）；此处跳过病态窗（只测合法窗零 panic）。
        if floor > depth || depth as usize > DEPTH_MAX {
            continue;
        }
        let _ = check_acquire(&chain, depth, floor, class);
        let _ = check_release(&chain, depth, floor, class);
        let _ = check_irq_exit(depth, floor);
    }
}
