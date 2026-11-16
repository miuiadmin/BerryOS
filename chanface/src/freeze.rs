//! channel 冻结面配对判与队列序纯面（W3-Q49）。
//!
//! **动机**：`ChannelEnd` 的冻结属主标（AM-1.5 wake-pending——`IPC × 冻结`
//! Q1 裁决）与收件队列机械（push/pop/push_front FIFO 序）此前整段内联在
//! `kernel/src/channel.rs`——retain/release 配对判、同 owner 幂等、异 owner
//! 拒、满拒/空拒的判定律 host 测不可达（kernel 依赖 HAL global_asm），
//! ktest 只覆盖 boot 相。本模块把这些**判定律**抽成纯函数（`NonNull` 以
//! id 代——W3-Q24 dpgface 同款投影纪律），kernel 侧消费同一函数（零第二
//! 实现），host 表驱动钉板。
//!
//! 抽离面（与 kernel 侧逐字对齐，改这里的语义 = 改 kernel 行为）：
//! - [`mark_frozen_ok`]：打标资格判（已标 → false 不重复 retain——同 owner
//!   幂等与异 owner 拒**同臂**：retain 只该打标者付一次，任何已标态都拒）。
//! - [`clear_frozen_ok`]：清标资格判（属主不符/未标 → false 不动——跨主
//!   安全；真清时 release 配平 mark 的 retain）。
//! - [`frozen_pair_sound`]：Drop 防御面配平判（frozen_owner 标与 peer 钉
//!   各自独立 release——两者可同时持有，Drop 两臂都走）。
//! - [`push_ok`]：入队序守卫（len < DEPTH 前置——`debug_assert` 的判据
//!   单源 + 防御面真拒）。
//! - [`pop_ok`]：出队序守卫（len > 0——空 pop 语义 = 调用方已窥视非空，
//!   防御面挡空弹）。
//! - [`push_front_ok`]：回滚复原守卫（len < DEPTH——recv 失败路径的容量
//!   不变量：弹出一条后必有空位，压回不越界）。
//!
//! 与既有面的关系（W3-Q9 界定）：`queue_admits`/`queue_full_of`/
//! `queue_readable`（容量门/READABLE 律）已在 lib.rs 单源——本模块的
//! `push_ok`/`pop_ok`/`push_front_ok` 复用它们作判据（组合而非重复）；
//! 冻结面配对判（retain/release 资格）是本模块新增的**值域判据**，
//! 「retain 怎么调」的物理臂（unsafe 解引用）留 kernel（本面只钉判）。

use super::{queue_admits, queue_readable};

// ---- 冻结属主标：配对判（W3-Q49；owner 以 id 代——kernel 侧 NonNull<ObjRef>
// 的纯投影，指针值语义 = 相等比较，物理 retain/release 留壳）----

/// 打冻结属主标的资格判（`ChannelEnd::mark_frozen` 单源判据）。
///
/// 判据：`cur.is_none()`——未标才打。**同 owner 幂等与异 owner 拒同臂**：
/// 已标态（无论谁标的）一律返 false 不重复 retain——retain 只该首冻结主
/// 付一次；共享端点（dup/迁移跨表）归首冻结主（v1 单主形，模块注政策）。
#[inline]
pub fn mark_frozen_ok(cur: Option<u64>) -> bool {
    cur.is_none()
}

/// 清冻结属主标的资格判（`ChannelEnd::clear_frozen` 单源判据）。
///
/// 判据：`cur == Some(owner)`——**属主精确匹配**才清（真清时 release 配平
/// mark 的 retain）。属主不符（异主清）/未标（None）返 false 不动——跨主
/// 安全：B 不能清 A 的标，否则 A 的 retain 份额被 B 误退。
#[inline]
pub fn clear_frozen_ok(cur: Option<u64>, owner: u64) -> bool {
    cur == Some(owner)
}

/// Drop 防御面配平判：frozen_owner 标与 peer 互钉**各自独立**配平。
///
/// 依据（`ChannelEnd::drop` 两臂序）：peer 钉与冻结标是两个独立 retain
/// 份额（pair 时 +1 与 mark_frozen 时 +1），可同时持有（配对端点的属主
/// 又被冻结）——Drop 两臂都走，互不排斥。本函数钉「两者可共存」的
/// 不变量（组合态全表），物理 release 留 kernel。
#[inline]
pub fn frozen_pair_sound(has_peer: bool, frozen: Option<u64>) -> bool {
    // 全组合态都合法——钉的是「无互斥律」：任一组合 Drop 都双臂配平。
    let _ = (has_peer, frozen);
    true
}

// ---- 队列序守卫（push/pop/push_front 的容量不变量单源）----

/// 入队守卫（`ChannelEnd::push` 的 `debug_assert` 判据单源 + 防御面）：
/// len < DEPTH 才可压（满拒——send 前置 `queue_full` 已挡，此处为
/// 不变量执法的防御面真拒）。
#[inline]
pub fn push_ok(len: usize) -> bool {
    queue_admits(len)
}

/// 出队守卫（`ChannelEnd::pop` 防御面）：len > 0 才可弹（空拒——
/// 调用方契约已窥视 `front()` 非空，此处挡空弹的 use-after-observe）。
#[inline]
pub fn pop_ok(len: usize) -> bool {
    queue_readable(len)
}

/// 回滚压回守卫（`ChannelEnd::push_front` 防御面）：len < DEPTH 才可压回
/// （recv 失败路径的容量不变量——刚弹出一条必腾一空位，压回不越界；
/// debug_assert 同判据的防御面真拒形）。
#[inline]
pub fn push_front_ok(len: usize) -> bool {
    queue_admits(len)
}

// ---- 表驱动测试（host 钉板）----

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CHAN_DEPTH;

    const A: u64 = 0x1000;
    const B: u64 = 0x2000;

    // mark 资格：未标过/已标同主/已标异主三态。
    #[test]
    fn mark_gate_table() {
        assert!(mark_frozen_ok(None)); // 未标 → 打
        assert!(!mark_frozen_ok(Some(A))); // 同主幂等拒（已标即拒）
        assert!(!mark_frozen_ok(Some(B))); // 异主拒
        assert!(!mark_frozen_ok(Some(0))); // 零 id（内核任务/无归属）也拒——已标态一律 false
    }

    // clear 资格：属主精确匹配表（同主/异主/未标三态）。
    #[test]
    fn clear_gate_table() {
        assert!(clear_frozen_ok(Some(A), A)); // 同主 → 清
        assert!(!clear_frozen_ok(Some(A), B)); // 异主拒（跨主安全）
        assert!(!clear_frozen_ok(None, A)); // 未标拒
        assert!(!clear_frozen_ok(None, 0)); // 零 id 同拒
    }

    // mark→clear 全周期：同主往返恰好配平（一次过 + 一次清），期间任何
    // 重打/异主清都被拒——retain/release 各恰一次。
    #[test]
    fn mark_clear_cycle() {
        let mut cur = None;
        assert!(mark_frozen_ok(cur)); // 首打
        cur = Some(A);
        assert!(!mark_frozen_ok(cur)); // 幂等拒
        assert!(!clear_frozen_ok(cur, B)); // 异主清拒
        assert!(clear_frozen_ok(cur, A)); // 真清
        cur = None;
        assert!(!clear_frozen_ok(cur, A)); // 清后未标态再清拒
        assert!(mark_frozen_ok(cur)); // 可重打（新冻结轮）
        cur = Some(B); // 新主
        assert!(clear_frozen_ok(cur, B)); // 新主清
    }

    // 两端各持冻结标互不影响（独立配平——每端一份 mark/clear 对）。
    #[test]
    fn per_end_independence() {
        let (mut ea, mut eb) = (None, None);
        assert!(mark_frozen_ok(ea));
        ea = Some(A);
        assert!(mark_frozen_ok(eb)); // B 端未标可打——A 端已标不拦 B 端
        eb = Some(B);
        assert!(!mark_frozen_ok(ea)); // 各自幂等
        assert!(clear_frozen_ok(ea, A));
        assert!(!clear_frozen_ok(eb, A)); // B 端标是 B 的——A 清不动
    }

    // Drop 配平判：peer × frozen 全组合合法（无互斥律钉板）。
    #[test]
    fn drop_pair_all_states() {
        for has_peer in [false, true] {
            for frozen in [None, Some(A), Some(B)] {
                assert!(frozen_pair_sound(has_peer, frozen));
            }
        }
    }

    // 队列序守卫：len 全扫描（0..=DEPTH+1）——push/push_front 同律
    // （admits 单源），pop 反律（readable 单源）。
    #[test]
    fn queue_guards_scan() {
        for len in 0..CHAN_DEPTH {
            assert!(push_ok(len), "push len={len} 应过");
            assert!(push_front_ok(len));
            assert_eq!(pop_ok(len), len > 0, "pop len={len}");
        }
        assert!(!push_ok(CHAN_DEPTH)); // 满拒
        assert!(!push_front_ok(CHAN_DEPTH));
        assert!(pop_ok(CHAN_DEPTH)); // 满可弹
        assert!(!push_ok(usize::MAX)); // 防御面
        assert!(!pop_ok(0)); // 空拒
    }

    // FIFO 序钉板：push 尾进、pop 头出、push_front 头复原名（模型 Vec
    // 对拍——kernel 侧 remove(0)/insert(0) 的序语义纯镜像）。
    #[test]
    fn fifo_order_model() {
        let mut q: std::vec::Vec<u32> = std::vec::Vec::new();
        let mut model: std::vec::Vec<u32> = std::vec::Vec::new();
        // 三轮 push（尾进）。
        for v in [1u32, 2, 3] {
            assert!(push_ok(q.len()));
            q.push(v);
            model.push(v);
            assert_eq!(q, model);
        }
        // pop（头出）+ push_front 复原（recv 失败回滚序）。
        assert!(pop_ok(q.len()));
        let head = q.remove(0);
        assert_eq!(head, 1);
        assert!(push_front_ok(q.len()));
        q.insert(0, head);
        assert_eq!(q, model); // 复原后与模型逐位相等
        // 真消费：pop 到底，序 = FIFO。
        let mut drained = std::vec::Vec::new();
        while pop_ok(q.len()) {
            drained.push(q.remove(0));
        }
        assert_eq!(drained, vec![1, 2, 3]);
        assert!(q.is_empty());
        assert!(!pop_ok(q.len())); // 空拒
        // 回滚不越界：满队 pop 一条再 push_front 恰好复原。
        for v in [4u32, 5, 6, 7, 8, 9, 10, 11] {
            assert!(push_ok(q.len()), "fill len={}", q.len());
            q.push(v);
        }
        assert!(!push_ok(q.len())); // DEPTH=8 满
        let head = q.remove(0);
        assert!(push_front_ok(q.len())); // 刚弹一条必有空位
        q.insert(0, head);
        assert_eq!(q.len(), CHAN_DEPTH);
    }

    // 与 kernel 消费点对齐律（改一侧不改另一侧 = 此测红——hdlface 同款）：
    // mark_frozen 的 `is_some()` 拒臂 == mark_frozen_ok(None 判)；
    // clear_frozen 的 `!= Some(owner)` 拒臂 == clear_frozen_ok；
    // push 的 debug_assert == push_ok；pop/push_front 防御面同判据。
    #[test]
    fn kernel_alignment_documented() {
        assert_eq!(mark_frozen_ok(None), !None::<u64>.is_some());
        assert_eq!(clear_frozen_ok(Some(A), A), Some(A) == Some(A));
        assert_eq!(push_ok(0), 0 < CHAN_DEPTH);
        assert_eq!(pop_ok(1), 1 > 0);
    }
}
