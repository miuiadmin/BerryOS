//! berry-ringface ilist —— 侵入式双向环接线律（W3-Q53）。
//!
//! **动机**：runqueue 的接线手术（`link_back`/`unlink`）此前只活在
//! `sched/src/runqueue.rs` 的 `NonNull<Task>` 裸指针语境——sched 依赖
//! HAL global_asm，只 aarch64-unknown-none 可编，host 测不可达。「环
//! 怎么接」的判定律（空环接入/尾插入环/中段摘除/头替换/唯一元素
//! 自指/入环前置判）的回归面需要一张 host 可测的**钉板**：本模块把
//! 接线律按形状分流抽成纯函数 + 表驱动测试，sched 侧消费同一函数
//!（零第二实现——抽离即唯一事实源，非镜像；本 crate W3-Q12 游标面
//! 同款纪律）。
//!
//! 两层结构（frzface scan 单执行器同款）：
//! - **形状层（纯）**：[`linkable`]/[`link_case`]/[`unlink_case`]——
//!   读旧链接态，分流手术形状（含全部陷阱判据：两元环 `prev == next`
//!   不构成单元素环——判据必须锚定自身；对在环者 link = 劈环危害）。
//! - **执行层（单源执行器）**：[`RingStore`] trait + [`run_link`]/
//!   [`run_unlink`]——形状唯一决定写回集，执行器逐字消费；host 测以
//!   map id 域适配、sched 侧 `Cell` 裸指针物理臂适配。语义单源在形状
//!   层与执行器：sched 侧 runqueue.rs 的 match 分支与写回序从此消失
//!   （消费 `run_link`/`run_unlink` + 适配器），改这里的写回 = 改
//!   sched 行为。
//!
//! 与冻结面的关系：`Runqueue` 物理账本（len/sum_vruntime/DL_ON_RING）
//! 的增减点机械留 sched（账本与接线同锁窗的论证在原处——单写者
//! load+store 与全局 RMW 语义各不同，不掺执行器）；环头游标
//! （head Cell）由执行器返回值驱动、sched 侧 set。HAL/时基零依赖。

// ---- 节点链接字段投影 ----

/// 节点链接字段对（prev, next）纯值域投影（`Cell<Option<NonNull<Task>>>`
/// 机械留 sched；指针以 usize id 代——W3-Q24 dpgface 同款纪律）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeLinks {
    /// 前驱链接字（None = 空）。
    pub prev: Option<usize>,
    /// 后继链接字（None = 空）。
    pub next: Option<usize>,
}

impl NodeLinks {
    /// 便利构造。
    pub const fn new(prev: Option<usize>, next: Option<usize>) -> Self {
        NodeLinks { prev, next }
    }
}

/// 入环前置判（runqueue.rs `linkable` 同律单源化）：prev 与 next 均
/// `None` 才可入环。对在环者 `link_back` 会覆盖写其链接字——把环劈成
/// 「头可达活圈 + 孤儿段」的唯一结构原语（被劈出者 Ready 却永不被
/// pick = 静默失联，复盘雷 2）；生产执法 = link_back 入口 assert 同一
/// 判据（ktest RING3 直测判据本身——真触发断言 = panic = halt，ktest
/// 只能测判据不能测炸）。
#[inline]
pub fn linkable(links: &NodeLinks) -> bool {
    links.prev.is_none() && links.next.is_none()
}

// ---- link_back 形状层 ----

/// link_back 手术形状（runqueue.rs `match self.head.get()` 同构）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkCase {
    /// 空环接入：node 自指成单元素环（prev=next=self），head ← node。
    EmptyRing { node: usize },
    /// 尾插入环：在 head 的前驱（环尾，执行器现场读）与 head 之间插入
    /// node（成为新尾），head 不变。
    TailInsert { head: usize, node: usize },
}

/// 按（环头投影, 节点链接投影）分流 link_back 形状。
///
/// 返回 `None` = 违约（节点已在环上——`linkable` 前置判不过；纯面不
/// 炸，「测判据不测炸」纪律，消费侧 assert 响亮失败）。
#[inline]
pub fn link_case(
    ring_head: Option<usize>,
    node: usize,
    node_links: &NodeLinks,
) -> Option<LinkCase> {
    if !linkable(node_links) {
        return None; // 在环者拒绝（ring-split hazard）
    }
    match ring_head {
        None => Some(LinkCase::EmptyRing { node }),
        Some(head) => Some(LinkCase::TailInsert { head, node }),
    }
}

// ---- unlink 形状层 ----

/// unlink 手术形状。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnlinkCase {
    /// 不在环上（prev == None）——幂等早退，零写回。
    NotOnRing,
    /// 单元素环（prev == next == self——判据锚定自身）→ 空环：head ←
    /// None，node 双字清 None。两元环 {X,Y} 上 X.prev == X.next == Y，
    /// `prev == next` 判据同样为真但**不是**单元素环——旧判据误走空环
    /// 分支把 head 清 None，幸存者 Y 带 stale 链静默失联（P4 收官审计
    /// 实踩；现矩阵常驻环 ≥6 打不中，矩阵收缩即踩中——判据锚定 task
    /// 自身即免疫）。
    SoloRing { node: usize },
    /// 中段或头摘除：邻接互缝（prev.next ← next；next.prev ← prev）；
    /// 若 node == head 则 head ← next（游标前移门）。
    MidOrHead {
        node: usize,
        prev: usize,
        next: usize,
        is_head: bool,
    },
}

/// 按节点自身链接投影 + 环头投影分流 unlink 形状。
///
/// 返回 `None` = 违约（prev 有值但 next 缺——环上节点必同时有 next；
/// 消费侧 expect/assert 响亮失败同 runqueue.rs）。
#[inline]
pub fn unlink_case(
    node: usize,
    node_links: &NodeLinks,
    ring_head: Option<usize>,
) -> Option<UnlinkCase> {
    let Some(prev) = node_links.prev else {
        return Some(UnlinkCase::NotOnRing);
    };
    let Some(next) = node_links.next else {
        return None; // 结构违约：环上必同时有 next
    };
    if prev == node && next == node {
        Some(UnlinkCase::SoloRing { node })
    } else {
        Some(UnlinkCase::MidOrHead {
            node,
            prev,
            next,
            is_head: ring_head == Some(node),
        })
    }
}

// ---- 执行层：RingStore trait + 单源执行器 ----

/// 物理链接字读写面（host 测与 sched 物理臂各配适配器——frzface
/// ScanQueues 同款）。
///
/// 实现方契约：id ⟷ 节点双射；对在环节点 `load_*` 返回其真实链接，
/// `store_*` 写目标节点的链接字。
pub trait RingStore {
    /// 读节点前驱链接字。
    fn load_prev(&mut self, id: usize) -> Option<usize>;
    /// 读节点后继链接字。
    fn load_next(&mut self, id: usize) -> Option<usize>;
    /// 写节点前驱链接字。
    fn store_prev(&mut self, id: usize, v: Option<usize>);
    /// 写节点后继链接字。
    fn store_next(&mut self, id: usize, v: Option<usize>);
}

/// link_back 单源执行器：形状分流 + 依形状的写回集（与 runqueue.rs
/// 逐字对齐）。
///
/// 返回新环头（EmptyRing → Some(node)；TailInsert → head 不变）。
/// 违约响亮失败点：入环前置判（在环者——劈环危害）；环非空 head 必有
/// 前驱。
pub fn run_link<S: RingStore>(
    st: &mut S,
    ring_head: Option<usize>,
    node: usize,
) -> Option<usize> {
    let node_links = NodeLinks::new(st.load_prev(node), st.load_next(node));
    let Some(case) = link_case(ring_head, node, &node_links) else {
        panic!("link_back: task already in-ring (ring-split hazard)");
    };
    match case {
        LinkCase::EmptyRing { node } => {
            // 单元素环：prev=next=self。
            st.store_prev(node, Some(node));
            st.store_next(node, Some(node));
            Some(node)
        }
        LinkCase::TailInsert { head, node } => {
            let tail = st.load_prev(head).expect("环非空 → head 有前驱");
            // 在 tail 与 head 之间插入 node（成为新尾）。head 不变。
            st.store_next(tail, Some(node));
            st.store_prev(node, Some(tail));
            st.store_next(node, Some(head));
            st.store_prev(head, Some(node));
            Some(head)
        }
    }
}

/// unlink 单源执行器：形状分流 + 依形状的写回集 + node 双字清 None
/// （与 runqueue.rs 逐字对齐；幂等早退分支零写回零出账）。
///
/// 返回新环头（SoloRing → None；MidOrHead 头摘除 → Some(next)；中段
/// 摘除/不在环 → 原头不变）。
pub fn run_unlink<S: RingStore>(
    st: &mut S,
    ring_head: Option<usize>,
    node: usize,
) -> Option<usize> {
    let node_links = NodeLinks::new(st.load_prev(node), st.load_next(node));
    let Some(case) = unlink_case(node, &node_links, ring_head) else {
        panic!("unlink: on-ring node must have next");
    };
    match case {
        UnlinkCase::NotOnRing => ring_head,
        UnlinkCase::SoloRing { node } => {
            st.store_prev(node, None);
            st.store_next(node, None);
            None
        }
        UnlinkCase::MidOrHead {
            node,
            prev,
            next,
            is_head,
        } => {
            st.store_next(prev, Some(next));
            st.store_prev(next, Some(prev));
            let new_head = if is_head { Some(next) } else { ring_head };
            st.store_prev(node, None);
            st.store_next(node, None);
            new_head
        }
    }
}

// ---- resync 走查累加面 ----

/// 全环走查重算 Σ（`resync_vruntime_sum` 走查段同构）：先访当前节点、
/// 后判回 head（§A136 形状）；步数帽执法单源 [`crate::walk_step_cap`]
/// （W3-Q12——ring_check/dl_on_ring_walk/resync 三处同律）。
///
/// 值投影由闭包注入（host 测任意 u64；sched 侧投 `Task::vruntime`）。
/// 返回 (步数, 和)；空环 (0, 0)。帽违约/断链 panic（执行器语义——同
/// runqueue.rs assert）。
pub fn run_resync<S: RingStore, F: Fn(usize) -> u64>(
    st: &mut S,
    ring_head: Option<usize>,
    vrt_of: F,
    cap: usize,
) -> (usize, u128) {
    let Some(h) = ring_head else {
        return (0, 0);
    };
    let mut cur = h;
    let mut steps: usize = 0;
    let mut sum: u128 = 0;
    loop {
        steps += 1;
        assert!(
            crate::walk_step_cap(steps, cap),
            "resync: ring walk exceeds cap (broken links?)"
        );
        sum += vrt_of(cur) as u128;
        cur = st
            .load_next(cur)
            .expect("resync: on-ring node must have next");
        if cur == h {
            return (steps, sum);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// host 适配器：id ⟷ 链接字双 map（未登记 id 读作 None——投影
    /// `Cell::new(None)` 初值语义）。
    struct MapStore {
        prev: BTreeMap<usize, Option<usize>>,
        next: BTreeMap<usize, Option<usize>>,
    }

    impl MapStore {
        fn new() -> Self {
            MapStore { prev: BTreeMap::new(), next: BTreeMap::new() }
        }
    }

    impl RingStore for MapStore {
        fn load_prev(&mut self, id: usize) -> Option<usize> {
            self.prev.get(&id).copied().flatten()
        }
        fn load_next(&mut self, id: usize) -> Option<usize> {
            self.next.get(&id).copied().flatten()
        }
        fn store_prev(&mut self, id: usize, v: Option<usize>) {
            self.prev.insert(id, v);
        }
        fn store_next(&mut self, id: usize, v: Option<usize>) {
            self.next.insert(id, v);
        }
    }

    /// 从 head 走一圈收集访问序（帽 128——RING_WALK_CAP 同值）。
    fn walk(st: &mut MapStore, head: Option<usize>) -> Vec<usize> {
        let Some(h) = head else { return Vec::new() };
        let mut v = Vec::new();
        let mut cur = h;
        loop {
            v.push(cur);
            cur = st.load_next(cur).expect("on-ring node must have next");
            if cur == h {
                return v;
            }
            assert!(v.len() <= 128, "walk cap");
        }
    }

    // ---- linkable：入环前置判 ----

    #[test]
    fn linkable_all_four_states() {
        assert!(linkable(&NodeLinks::new(None, None)), "双 None 才可入环");
        assert!(!linkable(&NodeLinks::new(Some(1), None)));
        assert!(!linkable(&NodeLinks::new(None, Some(1))));
        assert!(!linkable(&NodeLinks::new(Some(1), Some(2))));
        // 半链（只一侧有值）也拒——覆盖写仍会劈环。
    }

    // ---- link_case：形状分流 ----

    #[test]
    fn link_case_empty_vs_tail_vs_reject() {
        let free = NodeLinks::new(None, None);
        assert_eq!(
            link_case(None, 7, &free),
            Some(LinkCase::EmptyRing { node: 7 })
        );
        assert_eq!(
            link_case(Some(3), 7, &free),
            Some(LinkCase::TailInsert { head: 3, node: 7 })
        );
        // 在环者（任一侧有值）恒 None——劈环危害拒绝。
        assert_eq!(link_case(None, 7, &NodeLinks::new(Some(1), None)), None);
        assert_eq!(link_case(Some(3), 7, &NodeLinks::new(None, Some(1))), None);
        assert_eq!(
            link_case(Some(3), 7, &NodeLinks::new(Some(1), Some(2))),
            None
        );
    }

    // ---- run_link：空环接入 → 单元素自指 ----

    #[test]
    fn run_link_empty_ring_solo_self_loop() {
        let mut st = MapStore::new();
        let head = run_link(&mut st, None, 5);
        assert_eq!(head, Some(5));
        assert_eq!(st.load_prev(5), Some(5));
        assert_eq!(st.load_next(5), Some(5));
        assert_eq!(walk(&mut st, head), vec![5]);
    }

    // ---- run_link：尾插入环（1→2 元、多元中段）----

    #[test]
    fn run_link_tail_insert_two_then_more() {
        let mut st = MapStore::new();
        let head = run_link(&mut st, None, 1);
        // 1 → 2：新尾插入，头不变。
        let head2 = run_link(&mut st, head, 2);
        assert_eq!(head2, Some(1), "尾插头不变");
        assert_eq!(walk(&mut st, head2), vec![1, 2]);
        // 环形状：1.next=2、2.next=1、1.prev=2、2.prev=1。
        assert_eq!(st.load_next(1), Some(2));
        assert_eq!(st.load_next(2), Some(1));
        assert_eq!(st.load_prev(1), Some(2));
        assert_eq!(st.load_prev(2), Some(1));
        // 2 → 3 → 4 依次尾插：访问序 FIFO。
        let head3 = run_link(&mut st, head2, 3);
        let head4 = run_link(&mut st, head3, 4);
        assert_eq!(head4, Some(1));
        assert_eq!(walk(&mut st, head4), vec![1, 2, 3, 4]);
        // 新尾 4 的邻接：prev=3、next=head=1；head 前驱 = 4。
        assert_eq!(st.load_prev(4), Some(3));
        assert_eq!(st.load_next(4), Some(1));
        assert_eq!(st.load_prev(1), Some(4));
    }

    // ---- run_link：在环者 panic（劈环危害执法）----

    #[test]
    #[should_panic(expected = "ring-split hazard")]
    fn run_link_in_ring_panics() {
        let mut st = MapStore::new();
        let head = run_link(&mut st, None, 1);
        let _ = run_link(&mut st, head, 1); // 1 已在环上
    }

    // ---- run_unlink：唯一元素自指 → 空 ----

    #[test]
    fn run_unlink_solo_ring_empties() {
        let mut st = MapStore::new();
        let head = run_link(&mut st, None, 9);
        let new_head = run_unlink(&mut st, head, 9);
        assert_eq!(new_head, None);
        assert_eq!(st.load_prev(9), None);
        assert_eq!(st.load_next(9), None);
        assert_eq!(walk(&mut st, new_head), Vec::<usize>::new());
    }

    // ---- run_unlink：两元环陷阱（prev == next 但非单元素环）----

    #[test]
    fn run_unlink_two_ring_not_solo() {
        // 两元环 {1,2}：摘 1 后 2 须自指成单元素环——而非误清 head。
        let mut st = MapStore::new();
        let head = run_link(&mut st, None, 1);
        let head = run_link(&mut st, head, 2);
        assert_eq!(walk(&mut st, head), vec![1, 2]);
        // 1.prev == 1.next == Some(2)：旧判据 `prev == next` 会误走
        // 空环分支；锚定自身的判据走 MidOrHead。
        assert_eq!(st.load_prev(1), Some(2));
        assert_eq!(st.load_next(1), Some(2));
        let new_head = run_unlink(&mut st, head, 1);
        assert_eq!(new_head, Some(2), "头摘除 → 游标前移到 next");
        // 幸存者 2 自指（不是 stale 链）。
        assert_eq!(st.load_prev(2), Some(2));
        assert_eq!(st.load_next(2), Some(2));
        assert_eq!(walk(&mut st, new_head), vec![2]);
        assert_eq!(st.load_prev(1), None);
        assert_eq!(st.load_next(1), None);
    }

    // ---- run_unlink：头替换（游标前移门）与中段摘除 ----

    #[test]
    fn run_unlink_head_and_mid() {
        let mut st = MapStore::new();
        let mut head = None;
        for id in 1..=4 {
            head = run_link(&mut st, head, id);
        }
        assert_eq!(walk(&mut st, head), vec![1, 2, 3, 4]);
        // 摘中段 2：邻接互缝 1.next=3、3.prev=1；头不变。
        let head2 = run_unlink(&mut st, head, 2);
        assert_eq!(head2, Some(1), "中段摘除头不变");
        assert_eq!(walk(&mut st, head2), vec![1, 3, 4]);
        assert_eq!(st.load_next(1), Some(3));
        assert_eq!(st.load_prev(3), Some(1));
        assert_eq!(st.load_prev(2), None);
        assert_eq!(st.load_next(2), None);
        // 摘尾 4：3 自缝到头 1；头不变。
        let head3 = run_unlink(&mut st, head2, 4);
        assert_eq!(head3, Some(1));
        assert_eq!(walk(&mut st, head3), vec![1, 3]);
        assert_eq!(st.load_next(3), Some(1));
        assert_eq!(st.load_prev(1), Some(3));
        // 摘头 1：游标前移到 3。
        let head4 = run_unlink(&mut st, head3, 1);
        assert_eq!(head4, Some(3), "头摘除 → 游标前移");
        assert_eq!(walk(&mut st, head4), vec![3]);
        // 最后一个也摘：空。
        let head5 = run_unlink(&mut st, head4, 3);
        assert_eq!(head5, None);
    }

    // ---- run_unlink：不在环上幂等早退 ----

    #[test]
    fn run_unlink_not_on_ring_idempotent() {
        let mut st = MapStore::new();
        let mut head = None;
        for id in 1..=3 {
            head = run_link(&mut st, head, id);
        }
        // 未入环的 9：幂等返回原头，零写回。
        let h = run_unlink(&mut st, head, 9);
        assert_eq!(h, head);
        assert_eq!(st.load_prev(9), None);
        assert_eq!(st.load_next(9), None);
        // 已摘下的 2（双 None）再摘同幂等。
        let head2 = run_unlink(&mut st, head, 2);
        let h2 = run_unlink(&mut st, head2, 2);
        assert_eq!(h2, head2);
        assert_eq!(walk(&mut st, h2), vec![1, 3]);
    }

    // ---- unlink_case：违约形（prev 有值 next 缺）----

    #[test]
    fn unlink_case_broken_next_is_none() {
        // prev 有值但 next 缺 = 结构违约 → None（消费侧响亮失败）。
        assert_eq!(
            unlink_case(1, &NodeLinks::new(Some(2), None), Some(1)),
            None
        );
    }

    #[test]
    #[should_panic(expected = "on-ring node must have next")]
    fn run_unlink_broken_next_panics() {
        let mut st = MapStore::new();
        st.store_prev(1, Some(2)); // 只缝一半（断链形态）
        st.store_next(1, None);
        let _ = run_unlink(&mut st, Some(1), 1);
    }

    // ---- 全周期：交错 link/unlink 不变量（长度/序/闭合）----

    #[test]
    fn full_cycle_churn_invariants() {
        let mut st = MapStore::new();
        let mut head = None;
        // 装载 1..=6。
        for id in 1..=6 {
            head = run_link(&mut st, head, id);
        }
        assert_eq!(walk(&mut st, head), vec![1, 2, 3, 4, 5, 6]);
        // 摘 2、4（中段）；摘 1（头）；再装回 2、7。
        head = run_unlink(&mut st, head, 2);
        head = run_unlink(&mut st, head, 4);
        head = run_unlink(&mut st, head, 1);
        assert_eq!(walk(&mut st, head), vec![3, 5, 6]);
        head = run_link(&mut st, head, 2);
        head = run_link(&mut st, head, 7);
        assert_eq!(walk(&mut st, head), vec![3, 5, 6, 2, 7]);
        // 双向闭合不变量：每个在环者 prev(next(x)) == x。
        for id in [3, 5, 6, 2, 7] {
            let n = st.load_next(id).unwrap();
            let p = st.load_prev(id).unwrap();
            assert_eq!(st.load_prev(n), Some(id), "prev(next({id})) == {id}");
            assert_eq!(st.load_next(p), Some(id), "next(prev({id})) == {id}");
        }
        // 头前驱 = 尾 7；尾后继 = 头 3。
        assert_eq!(st.load_prev(3), Some(7));
        assert_eq!(st.load_next(7), Some(3));
    }

    // ---- run_resync：走查累加 + 步数 ----

    #[test]
    fn run_resync_sum_and_steps() {
        let mut st = MapStore::new();
        // 空环 (0, 0)。
        assert_eq!(run_resync(&mut st, None, |_| 1, 128), (0, 0));
        let mut head = None;
        for id in 1..=4 {
            head = run_link(&mut st, head, id);
        }
        let vrt = [10u64, 20, 30, 40];
        let (steps, sum) = run_resync(&mut st, head, |id| vrt[id - 1], 128);
        assert_eq!(steps, 4);
        assert_eq!(sum, 100);
        // u64 域上界：四值全 u64::MAX，u128 累加不溢出。
        let (s2, sum2) = run_resync(&mut st, head, |id| {
            let _ = id;
            u64::MAX
        }, 128);
        assert_eq!(s2, 4);
        assert_eq!(sum2, 4 * u64::MAX as u128);
        // 帽执法：cap=3 时 4 元环走查到帽即 panic（断链同形）。
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut st2 = MapStore::new();
            let mut h = None;
            for id in 1..=4 {
                h = run_link(&mut st2, h, id);
            }
            run_resync(&mut st2, h, |_| 0u64, 3)
        }));
        assert!(result.is_err(), "超帽须 panic");
    }

    // ---- 敌意形态：断链（走查走不回 head）在 resync 帽执法响亮 ----

    #[test]
    #[should_panic(expected = "exceeds cap")]
    fn run_resync_broken_ring_hits_cap() {
        let mut st = MapStore::new();
        let mut head = None;
        for id in 1..=3 {
            head = run_link(&mut st, head, id);
        }
        // 手工造断链：3.next 指向不存在的 99（走不回 head 1）。
        st.store_next(3, Some(99));
        st.store_next(99, Some(99)); // 99 自指成死循环
        let _ = run_resync(&mut st, head, |_| 0, 8);
    }
}
