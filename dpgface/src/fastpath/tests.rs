//! W3-Q59 fastpath host 钉板：直切判序三门 + 收口序 + 严格更高律
//! （`berry_ringface::eligible` 单源直引——零第二实现）。

use super::*;
use berry_ringface::{BLOCKED, EXITED, READY};

/// 最小投影便利形（sched 侧 `ring_desc` 同序五字段）。
fn desc(prio: i32) -> TaskDesc {
    TaskDesc::new(READY, 0, prio, 0, 0)
}

fn ctx_of<'a>(cur_prio: i32, ring: &'a [TaskDesc]) -> PreemptCtx<'a> {
    let mut cur = desc(cur_prio);
    cur.state = RUNNING;
    PreemptCtx {
        cur: Some(cur),
        ring,
        // 单元钉板默认全非 cur（Running-elsewhere 过滤面单测钉）。
        is_cur: const { &[] },
    }
}

// ---------------------------------------------------------------------------
// 门① CURRENT null 护栏
// ---------------------------------------------------------------------------

#[test]
fn cur_null_early_out_guard() {
    // sched::init 前 CURRENT == null：早退 false——先于探峰（null 时
    // 探峰入参不可构造，违约显形为无直切而非挂死）。
    assert!(cur_null_early_out(None));
    assert!(!cur_null_early_out(Some(desc(10))));
}

#[test]
fn try_preempt_eligible_null_cur_is_false() {
    // 合判入口：cur 缺席恒 false——即便环上有严格更高者（探峰根本不跑）。
    let ring = [desc(100)];
    let ctx = PreemptCtx { cur: None, ring: &ring, is_cur: &[false] };
    assert!(!try_preempt_eligible(&ctx));
}

// ---------------------------------------------------------------------------
// 门② 探峰（幻影出选防线——只读、不动游标）
// ---------------------------------------------------------------------------

#[test]
fn peek_empty_ring_false() {
    let ctx = PreemptCtx { cur: Some(desc(10)), ring: &[], is_cur: &[] };
    assert!(!has_eff_above(&ctx));
    assert!(!try_preempt_eligible(&ctx));
}

#[test]
fn peek_strictly_higher_boundary() {
    // §A43 恰界：等值不切（同层 FIFO 不插队）；恰高 1 切。
    let eq = [desc(10)];
    let ctx_eq = ctx_of(10, &eq);
    assert!(!has_eff_above(&ctx_eq));
    assert!(!try_preempt_eligible(&ctx_eq));

    let hi = [desc(11)];
    let ctx_hi = ctx_of(10, &hi);
    assert!(has_eff_above(&ctx_hi));
    assert!(try_preempt_eligible(&ctx_hi));
}

#[test]
fn peek_filters_ineligible_higher_band() {
    // 更高带但被 eligible 过滤者不参与竞争（三位逐一钉 + 组合）：
    // Running-elsewhere / HELD_FROZEN / HELD_THROTTLE。
    let mut running_elsewhere = desc(99);
    running_elsewhere.state = RUNNING;
    let mut frozen = desc(99);
    frozen.held_locks = HELD_FROZEN_BIT;
    let mut throttled = desc(99);
    throttled.held_locks = HELD_THROTTLE_BIT;
    let mut frozen_throttled = desc(99);
    frozen_throttled.held_locks = HELD_FROZEN_BIT | HELD_THROTTLE_BIT;

    for t in [
        running_elsewhere,
        frozen,
        throttled,
        frozen_throttled,
    ] {
        let ring = [t];
        let ctx = PreemptCtx {
            cur: Some(desc(10)),
            ring: &ring,
            is_cur: &[false],
        };
        assert!(
            !has_eff_above(&ctx),
            "被过滤高带者不得触发直切：{:?}",
            t
        );
    }
}

#[test]
fn peek_cur_running_on_ring_competes() {
    // is_cur 的 Running-elsewhere 例外：环上「是当前任务」的 RUNNING
    // 成员不被过滤（sched 侧指针等值判镜像——本核 cur 在环上常态）。
    let mut cur_on_ring = desc(11);
    cur_on_ring.state = RUNNING;
    let ring = [cur_on_ring];
    let ctx = PreemptCtx {
        cur: Some(desc(10)),
        ring: &ring,
        is_cur: &[true],
    };
    assert!(has_eff_above(&ctx));
}

#[test]
fn peek_mixed_ring_first_eligible_wins_order_insensitive() {
    // 混合环：过滤位与合法候选混排——判序只依赖「存在性」，与环序
    // 无关（探峰一圈 O(n) 扫描序的语义钉板）。
    let mut frozen = desc(99);
    frozen.held_locks = HELD_FROZEN_BIT;
    let mut running_else = desc(98);
    running_else.state = RUNNING;
    let lower = desc(5);
    let higher = desc(50);
    for ring in [
        [frozen, running_else, lower, higher],
        [higher, lower, frozen, running_else],
        [lower, higher, frozen, running_else],
    ] {
        let ctx = PreemptCtx {
            cur: Some(desc(10)),
            ring: &ring,
            is_cur: &[false, false, false, false],
        };
        assert!(has_eff_above(&ctx));
    }
    // 全过滤环 = 无竞争者。
    let all_filtered = [frozen, running_else];
    let ctx = PreemptCtx {
        cur: Some(desc(10)),
        ring: &all_filtered,
        is_cur: &[false, false],
    };
    assert!(!has_eff_above(&ctx));
}

#[test]
fn peek_scan_visits_whole_ring_no_short_circuit_on_filtered() {
    // 探峰不因前缀过滤者短路（§A136 同形走查：先访后判）——被过滤者
    // 占位不阻断后继合法候选的可见性。
    let mut frozen_hi = desc(100);
    frozen_hi.held_locks = HELD_THROTTLE_BIT;
    let target = desc(20);
    let ring = [frozen_hi, desc(1), desc(2), target];
    let ctx = PreemptCtx {
        cur: Some(desc(10)),
        ring: &ring,
        is_cur: &[false, false, false, false],
    };
    assert!(has_eff_above(&ctx));
}

// ---------------------------------------------------------------------------
// 门③ 合判（严格更高才切——幻影出选防线的「不 pick」半边）
// ---------------------------------------------------------------------------

#[test]
fn try_preempt_eligible_is_peek_gated() {
    // 合判 == 护栏 ∧ 探峰（出选集 == 探峰集：S-3b 同 eligible 门）。
    // 等值环：不切——不 pick = 不消耗 head 槽位（幻影出选防线）。
    let eq = [desc(10), desc(10), desc(10)];
    let ctx = ctx_of(10, &eq);
    assert!(!try_preempt_eligible(&ctx));

    let mixed = [desc(9), desc(10), desc(11)];
    let ctx = ctx_of(10, &mixed);
    assert!(try_preempt_eligible(&ctx));
}

#[test]
fn strictly_higher_law() {
    // sched 窗内再验行镜像：严格大于才切；等值/更低不切。
    assert!(strictly_higher(11, 10));
    assert!(!strictly_higher(10, 10));
    assert!(!strictly_higher(9, 10));
    // 负值域（PI 捐赠可落到负带）与 i32 极值界。
    assert!(strictly_higher(i32::MIN + 1, i32::MIN));
    assert!(!strictly_higher(i32::MAX, i32::MAX));
    assert!(strictly_higher(0, -1));
}

#[test]
fn ring_member_competes_is_eligible_mirror() {
    // 消费面锚：直切语境的 eligible 半边 = ringface 单源。
    let mut d = desc(50);
    assert!(ring_member_competes(&d, false));
    d.state = RUNNING;
    assert!(!ring_member_competes(&d, false));
    assert!(ring_member_competes(&d, true));
    d = desc(50);
    d.held_locks = HELD_FROZEN_BIT;
    assert!(!ring_member_competes(&d, false));
    d.held_locks = HELD_THROTTLE_BIT;
    assert!(!ring_member_competes(&d, true)); // 限流位无视 is_cur
}

// ---------------------------------------------------------------------------
// 供给临界段收口序（线1d 消费面纪律）
// ---------------------------------------------------------------------------

#[test]
fn supply_order_total_chain() {
    use SupplyStep::*;
    // 全序五步：SupplyPage → MagicWrite → Wake → TryPreempt → KlockExit。
    let chain = [SupplyPage, MagicWrite, Wake, TryPreempt, KlockExit];
    for (i, a) in chain.iter().enumerate() {
        for (j, b) in chain.iter().enumerate() {
            assert_eq!(
                supply_done_before_wake(*a, *b),
                i < j,
                "{:?} < {:?} 期望 {}",
                a,
                b,
                i < j
            );
        }
    }
}

#[test]
fn supply_magic_before_wake_law() {
    // 核心律：供给 + magic 写先于唤醒（唤醒时页已可读——faulter 醒后
    // 重试读必命中 magic，直切不渡供给中状态）。
    assert!(supply_done_before_wake(SupplyStep::MagicWrite, SupplyStep::Wake));
    assert!(supply_done_before_wake(SupplyStep::SupplyPage, SupplyStep::Wake));
    // 反向违约即 false。
    assert!(!supply_done_before_wake(SupplyStep::Wake, SupplyStep::MagicWrite));
}

#[test]
fn try_preempt_after_wake_before_klock_exit() {
    // 直切判定在唤醒后、KL 窗收口前（KL 窗内切换合法面——try_preempt
    // 舞蹈按位放锁/切回重取，与 pager 睡 PAGER_KEY 同一舞蹈）。
    assert!(supply_done_before_wake(SupplyStep::Wake, SupplyStep::TryPreempt));
    assert!(supply_done_before_wake(
        SupplyStep::TryPreempt,
        SupplyStep::KlockExit
    ));
    assert!(!supply_done_before_wake(
        SupplyStep::TryPreempt,
        SupplyStep::Wake
    ));
}

// ---------------------------------------------------------------------------
// 敌意快照零 panic（ktest「测判据不测炸」同款纪律）
// ---------------------------------------------------------------------------

#[test]
fn hostile_snapshot_never_panics() {
    // 全状态 × 全持有位 × 极值带笛卡尔：判序纯函数零 panic（探峰比较
    // 与过滤都在值域内闭合）。
    let states = [READY, RUNNING, BLOCKED, EXITED, 200];
    let holds = [0u8, 1, HELD_FROZEN_BIT, HELD_THROTTLE_BIT, 0xff];
    let prios = [i32::MIN, -1, 0, 1, i32::MAX];
    for s in states {
        for h in holds {
            for cur_prio in prios {
                let mut ring = Vec::new();
                for p in prios {
                    ring.push(TaskDesc::new(s, h, p, u64::MAX, u64::MAX));
                }
                let is_cur = vec![false; ring.len()];
                let ctx = PreemptCtx {
                    cur: Some(TaskDesc::new(RUNNING, 0, cur_prio, 0, 0)),
                    ring: &ring,
                    is_cur: &is_cur,
                };
                let _ = try_preempt_eligible(&ctx);
                let _ = has_eff_above(&ctx);
            }
        }
    }
}
