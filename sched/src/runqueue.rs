//! Runqueue（侵入式双向环形链表，`NonNull<Task>` 裸指针）+ CPU 拓扑（`Topo`）。

use crate::task::Task;
use core::cell::Cell;
use core::ptr::NonNull;

/// 单 CPU 的运行队列。G0 单核：全局一个。
///
/// 链表游标 `head` 用 `Cell`：使 [`crate::class::SchedClass::pick_next`] 在
/// `&Runqueue`（不可变）下仍能旋转游标（round-robin），**无需改冻结合约为
/// `&mut Runqueue`**（plan Bug 5）。
///
/// 环形：尾节点的 `rq_next == head`、`head.rq_prev == 尾`。`head` 是「下一个将被
/// pick 的任务」；`rotate()` 返回旧 head 并把游标前移到 `head.next`。
pub struct Runqueue {
    head: Cell<Option<NonNull<Task>>>,
    /// 环长记账（link_back 增 / unlink 减——均在实际接线后）。`ring_check`
    /// 以「走查步数 == len」执法：**孤儿段形态下从 head 走圈仍闭合**（活圈
    /// 自闭合），只有对账才能显形「有成员不在头可达圈上」（复盘雷 2 残留
    /// 诊断——五任务静默失联的确诊机制即此形态）。
    len: Cell<usize>,
}

/// 环走查步数帽：远大于合法环长（现矩阵 runnable ≤~20），只挡断链死循环
/// （走不回 head 的形态）——超帽即断言违约。
pub const RING_WALK_CAP: usize = 128;

/// 入环前置判据：`t` 的链接字段须均 `None`（不在任何环上）。对在环者
/// `link_back` 会**覆盖写**其 prev/next——把环劈成「头可达活圈 + 孤儿段」
/// 的唯一结构原语（被劈出的任务 Ready 却永不被 pick = 静默失联；其
/// 「不在环上」契约此前只有文档零执法）。生产执法 = `link_back` 入口
/// assert 同一判据；独立导出供 ktest RING3 直测判据本身（真触发断言 =
/// panic = halt，ktest 只能测判据不能测炸）。
pub fn linkable(t: &Task) -> bool {
    t.rq_prev.get().is_none() && t.rq_next.get().is_none()
}

impl Runqueue {
    pub const fn new() -> Self {
        Runqueue {
            head: Cell::new(None),
            len: Cell::new(0),
        }
    }

    /// 环长（走查对账用）。
    pub fn len(&self) -> usize {
        self.len.get()
    }

    /// 环完整性走查（复盘雷 2 残留诊断）：从 head 沿 `rq_next` 走一圈须回到
    /// head 且步数 ≤ `RING_WALK_CAP`。**只读、零分配**；返回步数——调用方
    ///（ktest / `on_irq_tick` 金丝雀分支）对账「步数 == [`len`](Self::len)」：
    /// 步数 < len = 有成员不在头可达圈上（孤儿段）；步数到帽 = 断链。
    /// 调用契约：环变更屏蔽窗内或其串行等价（IRQ 上下文 / ktest 本地环）。
    pub fn ring_check(&self) -> usize {
        let Some(h) = self.head.get() else {
            return 0;
        };
        // SAFETY: 环上任务均调度器拥有（pick_next 扫描同款不变式）；调用方
        // 契约保证走查期间无并发出/入环。
        unsafe {
            let mut cur = h;
            let mut steps = 0;
            loop {
                steps += 1;
                assert!(
                    steps <= RING_WALK_CAP,
                    "ring_check: ring walk exceeds cap (broken links?)"
                );
                let next = (*cur.as_ptr())
                    .rq_next
                    .get()
                    .expect("ring_check: on-ring node must have next");
                cur = next;
                if cur == h {
                    return steps;
                }
            }
        }
    }

    /// 队列是否为空。
    pub fn is_empty(&self) -> bool {
        self.head.get().is_none()
    }

    /// 把 `task` 插到环尾（head 的前驱）——新任务排到最后被 pick。
    ///
    /// # Safety
    /// `task` 须当前不在任何 runqueue 上（`rq_prev`/`rq_next` 均为 `None`），且
    /// 在被摘下前地址保持稳定（调度器拥有其 `Box`）。
    pub(crate) unsafe fn link_back(&self, task: NonNull<Task>) {
        let t = task.as_ptr();
        // 入环执法断言（复盘雷 2 残留诊断）：对在环者再 link_back 是唯一能把
        // 环劈成「头可达活圈 + 孤儿段」的结构原语（覆盖写 prev/next 造子环，
        // 被劈出者 Ready 却永不被 pick——30s 长跑五任务静默失联的确诊机制）。
        // 契约此前只有文档零执法；断言把静默分裂变成当场、带位置的响亮失败。
        // 成本 = 两次 load + branch（IRQ 屏蔽热路径、零分配纪律不破）。
        assert!(
            (*t).rq_prev.get().is_none() && (*t).rq_next.get().is_none(),
            "link_back: task already in-ring (ring-split hazard)"
        );
        match self.head.get() {
            None => {
                // 单元素环：prev=next=self。
                (*t).rq_prev.set(Some(task));
                (*t).rq_next.set(Some(task));
                self.head.set(Some(task));
            }
            Some(head) => {
                let h = head.as_ptr();
                let prev = (*h).rq_prev.get().expect("环非空 → head 有前驱");
                let p = prev.as_ptr();
                // 在 prev 与 head 之间插入 task（成为新尾）。head 不变。
                (*p).rq_next.set(Some(task));
                (*t).rq_prev.set(Some(prev));
                (*t).rq_next.set(Some(head));
                (*h).rq_prev.set(Some(task));
            }
        }
        self.len.set(self.len.get() + 1);
    }

    /// 把 `task` 从环上摘下（任意位置）。若 `task == head`，游标前移到 `next`。
    /// 不在环上（`rq_prev==None`）则幂等返回。
    ///
    /// # Safety
    /// 同 [`link_back`]。
    pub(crate) unsafe fn unlink(&self, task: NonNull<Task>) {
        let t = task.as_ptr();
        let Some(prev) = (*t).rq_prev.get() else {
            return; // 不在环上
        };
        let next = (*t).rq_next.get().expect("环上必同时有 next");

        if prev == task && next == task {
            // 单元素环 → 空。判据必须锚定 task 自身：两元环 {X,Y} 上
            // X.rq_prev == X.rq_next == Y，旧判据 `prev == next` 同样为真——
            // 会误走空环分支把 head 清 None，幸存者 Y 带 stale 链静默失联
            // （P4 收官审计实踩；现矩阵常驻环 ≥6 打不中，矩阵收缩即踩中）。
            self.head.set(None);
        } else {
            (*prev.as_ptr()).rq_next.set(Some(next));
            (*next.as_ptr()).rq_prev.set(Some(prev));
            if self.head.get() == Some(task) {
                self.head.set(Some(next));
            }
        }
        (*t).rq_prev.set(None);
        (*t).rq_next.set(None);
        self.len.set(self.len.get() - 1);
    }

    /// 环游标（4-3a：`RoundRobin::pick_next` 环形扫描的起点）。crate 内策略
    /// 实现细节——环形链表的遍历原语，不承载调度语义。
    /// （旧 `rotate()` =「取 head + 游标前移」的纯 FIFO 出选原语，4-3a 起
    /// pick 按 `prio_effective` 环形扫描取代之——语义拆为 `head()` +
    /// `advance_past()` 两半，见 `class.rs`。）
    pub(crate) fn head(&self) -> Option<NonNull<Task>> {
        self.head.get()
    }

    /// 把游标前移到 `node` 的后继（4-3a：pick 选中 `node` 后调用——环形扫描
    /// 出选可不命中 head；对 head 调用即旧 `rotate` 的游标前移，语义不变）。
    /// 选中者留在环上（runnable 常驻环），仅游标移动。
    ///
    /// # Safety
    /// `node` 须当前在本环上（环上节点必有 next）。
    pub(crate) unsafe fn advance_past(&self, node: NonNull<Task>) {
        // SAFETY: 调用方契约（在环节点）；环非空必有 next。
        let next = (*node.as_ptr()).rq_next.get().expect("环上节点必有 next");
        self.head.set(Some(next)); // 单元素环时 next==node，游标不变（保持指向它）
    }

    /// 只读探峰：环上是否存在 `prio_effective` 严格高于 `v` 的任务（至多一圈，
    /// 不动游标、不做出选）。供 [`crate::sched::try_preempt`] 的前置判定——
    /// 「不可能切走就根本不 pick」（pick_next 会 `advance_past` 消耗 head 槽位：
    /// 出选后不切走 = 幻影出选，见 try_preempt 注释）。
    pub(crate) fn has_eff_above(&self, v: i32) -> bool {
        let Some(h) = self.head.get() else {
            return false; // 空环
        };
        // SAFETY: 环上任务均调度器拥有且在环（pick_next 扫描同款不变式）；
        // 单核无并发 pick（IRQ 屏蔽切换位互斥）——只读扫描无并发出/入环。
        unsafe {
            let mut cur = h;
            loop {
                if (*cur.as_ptr()).prio_effective > v {
                    return true;
                }
                cur = (*cur.as_ptr())
                    .rq_next
                    .get()
                    .expect("环上节点必有 next");
                if cur == h {
                    return false; // 整圈扫完
                }
            }
        }
    }
}

/// CPU 拓扑（capacity 表占位）。G0 单核：一个条目。多核（G2+）由 HAL 填表，
/// 核心逻辑架构无关（`调度器设计.md` §3.1 的 capacity(0..1023) 抽象 v3 落地）。
#[derive(Clone, Copy)]
pub struct Topo {
    /// CPU 数。G0 = 1。
    pub nr_cpus: u32,
}

impl Topo {
    /// 单核拓扑（G0/G1）。
    pub const fn single() -> Self {
        Topo { nr_cpus: 1 }
    }
}

impl Default for Topo {
    fn default() -> Self {
        Self::single()
    }
}
