//! Runqueue（侵入式双向环形链表，`NonNull<Task>` 裸指针）+ CPU 拓扑（`Topo`）。

use crate::spin::SpinLock;
use crate::task::Task;
use core::cell::Cell;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

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
    ///
    /// **S-3c c-2b slice 3（Q3 收口）**：`Cell<usize>` → `AtomicUsize`——
    /// `place()` 表锁语境免锁读远环 len，撕裂读由原子性消解（Q3 二选一取
    /// 原子化：入锁窗方案 = 表窗内碰 RQ 面 F-E1 同族逆序 + C20 后双 RQ
    /// 升序双取；原子化免锁序牵连、终态合法）。取放恒在本队列 RQ 锁窗内
    ///（单写者——load+store 即可，全序由锁提供；原子性只为跨核**读**不
    /// 撕裂），两端 Relaxed；K=2 放置启发式与金丝雀对账容忍近似值。
    /// codegen 与 Cell 等形（ldr/str）——icount 零漂。
    len: AtomicUsize,
    /// Σvruntime 增量账本（1.1/S2b+：EEVDF `avg_vruntime` 的 O(1) 化——
    /// 消费面 = `crate::class::avg_vruntime`，pick_next 热路径少一圈 O(n)
    /// 全环扫描）。账点三处闭环：`link_back` 加 / `unlink` 减（**结构原语
    /// 层**——steal 路径直调 `unlink` 不经 `Fair::dequeue`，挂原语层才无
    /// 漏账）/ `Fair::tick` 随 vdelta 加（在环门控：`IDLE_TASK` 永不在环，
    /// idle tick 不入账——与旧全环扫描从不含 idle 口径一致）。生产代码
    /// vruntime 写点仅此三处 ⇒ 账本与全环扫描逐位一致。
    ///
    /// 写侧 = **多写者 RMW**（fetch_add/fetch_sub，两端 Relaxed）：SMP 下
    /// 三账点不全在本队列 RQ 锁窗内——`Fair::tick`（IRQ 上下文免锁）与对
    /// 核锁窗内 link/unlink 并发写同一账本，load+store 非原子序 = 丢更新
    /// （账本偏低恰一个 vdelta 形态——ring_check 尸检 2026-11-11 定谳；
    /// 同 `DL_ON_RING` 的 fetch_add/fetch_sub 先例与论证）。tick 侧**不可**
    /// 收入锁窗：SVC 窗/SGI 回执忙等等路径持 RQ 锁跨核等待，tick 取锁 =
    /// 双核互等 wedge（实测）。读侧（pick/enqueue 的 avg）恒在锁窗内，值
    /// 稳定口径不变；原子性对免锁读者防撕裂。**已知残留**：走查侧（
    /// `ring_check`/`dl_on_ring_walk` 持锁）与对核免锁 tick 的（vruntime,
    /// 账本）二元组写仍可 ns 级撕裂（两写各自原子、二元组非原子）——
    /// 理论假阳性窗 ≪ 丢更新窗三个量级（本相 bug 已闭），实测未见。
    /// **一致性执法** =
    /// [`Self::ring_check`] 走查对账（每 100 切换金丝雀）：真和 ≠ 账本即
    /// panic——测试在环上直改 `Task::vruntime` 后须
    /// [`Self::resync_vruntime_sum`] 洗账。
    ///
    /// 溢出界：vruntime 增速 ≤ CNTVCT（weight ≥ SCALE=1024 ⇒ vdelta ≤
    /// delta），账本 u64 回绕需 ~9e3 年 @62.5MHz——工程不可达（原
    /// saturating 语义在可达域内与 RMW 等价；漂移域回绕仍被 ring_check
    /// 对账响亮捕获——等式比较对回绕值恒假）。
    sum_vruntime: AtomicU64,
    /// per-queue 自旋锁（S-3c c-2a 落形 → **c-2b 取放启用**；持有位 =
    /// `HELD_RQ0`/`HELD_RQ1`，记账在 `sched::rq_lock_of` 统一承担）。crate 内
    /// 自持 [`SpinLock`]（零 arch 依赖——b-1 时「A4 侦察形 lock 字段被依赖
    /// 方向约束否决」的解方即此；彼时注释预告的「S-3c 拆锁时结构再扩」由本
    /// 字段兑现）。过渡态 KL 全量在场 ⇒ 恒零争用（窗形先行，§A108）。
    lock: SpinLock,
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

/// **S5-c 计数门账本**：全域「在环 deadline 任务数」（消费面 = `class.rs::
/// lazy_unthrottle_deadlines` 头部门——0 时跳过整圈环扫描；v1 常态全员
/// fair，门常闭 = 每次 pick 零扫描）。
///
/// **账点挂结构原语层**（`link_back`/`unlink`——同 `len`/`sum_vruntime`
/// 先例）：出环生产路径不止 `Fair::dequeue` 一条——block 直调 `unlink`
/// （`RR.dequeue` 会置 Exited，语义不符）、steal 摘对核环也直调——挂
/// `Fair::enqueue`/`dequeue` 必漏账。原语层全覆盖：出/入环结构手术
/// 只此两函数。
///
/// **全局而非 per-RQ**（设计取舍）：SMP 下本核 pick 读到的可能是对核
/// 环的份额——保守方向（多扫不漏扫），不会漏。多计来源恒合法：ktest
/// 本地环（只增不减）与 on-ring 降带（ktest D3 直改 prio——计数只反映
/// link 时刻带位）。**漏计（危险向：门跳过需要的解冻扫描）生产不存在**：
/// 生产升 DL 唯一点 = `apply_sched_attr_to_task`，恒在 enqueue 前
/// （`try_prepare_user` → 属性 → `enqueue_prepared_user`）；PI 捐赠只写
/// SLEEPQ 在队者（离环），唤醒入环时按已升值计数；`policy::apply` 权重
/// 夹紧 ±64 与 spawn 基线（≤64）够不到 1000 带。执法 = `integrity_check`
/// 单边对账（真扫 ≤ 账本，见 `dl_on_ring_walk`）。
///
/// **真 RMW 而非 len 的 load+store**：`len`/`sum_vruntime` 是 per-queue
/// 单写者（本队列 RQ 锁窗内免 RMW）；本账本全局双 RQ 锁窗多写者，
/// 须 `fetch_add`/`fetch_sub`。Relaxed 够——纯计数门无序角色（门读偏旧
/// = 保守），账点与结构手术同锁窗（锁全序保证读者可见性，同 `len` 论证）。
static DL_ON_RING: AtomicU32 = AtomicU32::new(0);

/// 计数门读（S5-c）：全域在环 deadline 任务数。0 = 环上（全域）无
/// link 时带位 ≥ PRIO_DEADLINE 的任务——lazy 解冻扫描可整圈跳过。
#[inline]
pub(crate) fn dl_on_ring() -> u32 {
    DL_ON_RING.load(Ordering::Relaxed)
}

impl Runqueue {
    pub const fn new() -> Self {
        Runqueue {
            head: Cell::new(None),
            len: AtomicUsize::new(0),
            sum_vruntime: AtomicU64::new(0),
            lock: SpinLock::new(),
        }
    }

    /// 取本队列自旋锁（S-3c c-2b 启用）。纪律同 `crate::spin` 头注：IRQ 屏蔽
    /// 窗内取放、不可重入、持窗禁切换；过渡态一切取放点在 KL 窗内（§A108
    /// 过渡态逆序——C20 收缩时随分号类别消解）。持有位记账由
    /// `sched::rq_lock_of`/`rq_unlock_of` 统一承担（本面不重复记账）。
    ///
    /// # Safety
    /// 调用方契约如上（屏蔽窗 + 不重入 + 窗内禁切换）。
    pub(crate) unsafe fn lock_acquire(&self) {
        self.lock.acquire();
    }

    /// 放本队列自旋锁（先清持有位后放——位与锁恒同窗，见 `sched::rq_unlock_of`）。
    pub(crate) fn lock_release(&self) {
        self.lock.release();
    }

    /// 环长（走查对账用；Relaxed 免锁读——c-2b slice 3 原子化，跨核读者
    /// （`place`/integrity）无撕裂，本核锁窗内读者值稳定）。
    pub fn len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }

    /// Σvruntime 账本读（S2b+；消费面 = `crate::class::avg_vruntime`——
    /// pick_next/enqueue 的 avg 直除）。Relaxed 免锁读同 [`Self::len`]
    /// 口径：读者恒在锁窗内值稳定，原子性防未来免锁读者撕裂。
    pub(crate) fn vruntime_sum(&self) -> u64 {
        self.sum_vruntime.load(Ordering::Relaxed)
    }

    /// Σvruntime 账本增量入账（S2b+；唯一生产调用点 = `Fair::tick`——
    /// 在环任务 vruntime 前进 vdelta 时同额入账）。RMW（多写者——tick
    /// 免锁窗与对核锁窗 link/unlink 并发，见字段注；load+store 丢更新
    /// 实证即本相 bug）。
    pub(crate) fn vruntime_add(&self, v: u64) {
        self.sum_vruntime.fetch_add(v, Ordering::Relaxed);
    }

    /// 全环重算 Σvruntime 并回写账本（S2b+ 洗账面——**仅测试/诊断用**）：
    /// 在环上直改 `Task::vruntime` 的手搓场景（ktest 的 EEVDF 钉板）改后
    /// 调用，否则账本与真和漂移（`ring_check` 对账将 panic）。生产路径的
    /// vruntime 变更全经 link/unlink/tick 入账，无需 resync。返回重算和；
    /// 空环清零账本。调用契约同 [`Self::ring_check`]（环变更屏蔽窗内或
    /// 其串行等价）。
    pub fn resync_vruntime_sum(&self) -> u64 {
        let Some(h) = self.head.get() else {
            self.sum_vruntime.store(0, Ordering::Relaxed);
            return 0;
        };
        // SAFETY: 环走查同款不变式（§A136 形状：先访当前节点、后判回
        // head）；调用方契约保证走查期间无并发出/入环。
        unsafe {
            let mut cur = h;
            let mut sum: u128 = 0;
            loop {
                sum += (*cur.as_ptr()).vruntime as u128;
                cur = (*cur.as_ptr())
                    .rq_next
                    .get()
                    .expect("resync: on-ring node must have next");
                if cur == h {
                    break;
                }
            }
            let s = sum.min(u64::MAX as u128) as u64;
            self.sum_vruntime.store(s, Ordering::Relaxed);
            s
        }
    }

    /// 环完整性走查（复盘雷 2 残留诊断）：从 head 沿 `rq_next` 走一圈须回到
    /// head 且步数 ≤ `RING_WALK_CAP`。**只读、零分配**；返回步数——调用方
    ///（ktest / `on_irq_tick` 金丝雀分支）对账「步数 == [`len`](Self::len)」：
    /// 步数 < len = 有成员不在头可达圈上（孤儿段）；步数到帽 = 断链。
    /// 走查同时累加环上真 Σvruntime，与 [`Self::sum_vruntime`] 账本对账
    /// （S2b+ 执法面：增量账本漂移 = 当场响亮失败——走查本 O(n)，顺带
    /// 累加零复杂度增量）。调用契约：环变更屏蔽窗内或其串行等价（IRQ
    /// 上下文 / ktest 本地环）。
    pub fn ring_check(&self) -> usize {
        let Some(h) = self.head.get() else {
            assert_eq!(
                self.sum_vruntime.load(Ordering::Relaxed),
                0,
                "ring_check: empty ring but vruntime ledger non-zero"
            );
            return 0;
        };
        // SAFETY: 环上任务均调度器拥有（pick_next 扫描同款不变式）；调用方
        // 契约保证走查期间无并发出/入环。
        unsafe {
            let mut cur = h;
            let mut steps = 0;
            let mut vsum: u128 = 0;
            loop {
                steps += 1;
                // W3-Q12：步数帽执法判单源 berry_ringface::walk_step_cap
                // （host 钉板恰帽/超帽/退化帽面）。
                assert!(
                    berry_ringface::walk_step_cap(steps, RING_WALK_CAP),
                    "ring_check: ring walk exceeds cap (broken links?)"
                );
                vsum += (*cur.as_ptr()).vruntime as u128;
                let next = (*cur.as_ptr())
                    .rq_next
                    .get()
                    .expect("ring_check: on-ring node must have next");
                cur = next;
                if cur == h {
                    let ledger = self.sum_vruntime.load(Ordering::Relaxed) as u128;
                    assert!(
                        vsum == ledger,
                        "ring_check: vruntime ledger {} != walk sum {} (stale ledger? \
                         resync after direct Task::vruntime writes)",
                        ledger,
                        vsum
                    );
                    return steps;
                }
            }
        }
    }

    /// 队列是否为空。
    pub fn is_empty(&self) -> bool {
        self.head.get().is_none()
    }

    /// 真扫本环数 deadline 任务（S5-c 计数门对账面——消费点 =
    /// `sched::integrity_check` 金丝雀：本环真扫值不得高于 [`dl_on_ring`]
    /// 全局账本，**单边断言**）。单边的理由：多计恒合法（ktest 本地环 /
    /// on-ring 降带只增不减）；漏计是危险向（门跳过需要的解冻扫描）——
    /// 只有它须响亮失败。走查判据带位读**当下**值（同 `lazy_unthrottle_
    /// deadlines` 扫描面口径——对账的正是「门看到的账」vs「扫描看到的真」）。
    ///
    /// 调用契约同 [`Self::ring_check`]（本环 RQ 锁窗内或其串行等价——
    /// 走查期间无并发出/入环；账本读须同窗，见消费点注）。
    pub(crate) fn dl_on_ring_walk(&self) -> usize {
        let Some(h) = self.head.get() else {
            return 0;
        };
        // SAFETY: 环走查同款不变式（§A136：先访当前节点、后判回 head）；
        // 环上任务均调度器拥有（ring_check 扫描同款）。
        unsafe {
            let mut cur = h;
            let mut steps = 0;
            let mut n = 0;
            loop {
                steps += 1;
                // W3-Q12：同上单源（walk_step_cap）。
                assert!(
                    berry_ringface::walk_step_cap(steps, RING_WALK_CAP),
                    "dl_on_ring_walk: ring walk exceeds cap (broken links?)"
                );
                if crate::class::is_deadline(&*cur.as_ptr()) {
                    n += 1;
                }
                cur = (*cur.as_ptr())
                    .rq_next
                    .get()
                    .expect("dl_on_ring_walk: on-ring node must have next");
                if cur == h {
                    return n;
                }
            }
        }
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
        // 单写者（本队列 RQ 锁窗内）——load+store 免 fetch_add（ldadd 会破
        // 与 Cell 形的指令等价；全序由锁提供，原子性只为读不撕裂）。
        self.len.store(self.len.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
        // Σvruntime 入账（S2b+）：账入环时刻的终值——Fair::enqueue 的 avg
        // clamp 在 link 前完成，此处入账的即 clamp 后值（先 clamp 后入账）。
        // RMW（多写者——与免锁 tick 的 vruntime_add 并发；字段注），回绕
        // 界见字段注（工程不可达）。
        self.sum_vruntime.fetch_add((*t).vruntime, Ordering::Relaxed);
        // S5-c 计数门入账：link 时刻带位是 DL 则全局账本 +1。账点挂结构
        // 原语层的理由见 `DL_ON_RING` 字段注——block/steal 出环不经
        // `Fair::dequeue`，挂类层必漏账。真 RMW（全局账本双 RQ 锁窗
        // 多写者），与结构接线同锁窗（读者可见性同 `len` 的锁全序论证）。
        if crate::class::is_deadline(&*t) {
            DL_ON_RING.fetch_add(1, Ordering::Relaxed);
        }
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
        // 同 link_back：len 单写者 load+store（RQ 锁窗内——len 的写点全在
        // 锁窗，单写者前提成立）。Σvruntime 出账 = 摘线时刻该任务的
        // vruntime（与入账/后续 tick 入账同额反向）；RMW（多写者——与
        // 免锁 tick 的 vruntime_add 并发；字段注）。幂等早退分支（不在环）
        // 不触此处——无重复出账。漂移域下溢回绕仍被 ring_check 对账响亮
        // 捕获（等式比较对回绕值恒假；回绕界见字段注）。
        self.len.store(self.len.load(Ordering::Relaxed) - 1, Ordering::Relaxed);
        let vr = (*t).vruntime;
        self.sum_vruntime.fetch_sub(vr, Ordering::Relaxed);
        // S5-c 计数门出账：判据读**摘线时刻**带位（与入账的 link 时刻带位
        // 在生产恒同——带位在环上不变，见 `DL_ON_RING` 字段注）。漂移两向
        // 均落安全侧：ktest 降带后摘线 → 漏减 = 账本偏高 = 多扫；理论上
        // （生产不存在）升带后摘线 → 无配对减 = u32 回绕到巨值 = 门常开。
        // 幂等早退分支（不在环——函数头 `else { return; }`）不触此处，无
        // 重复出账；steal 的「对核 unlink + 本核 link_back」两腿同额反向，
        // 全局账本净零。
        if crate::class::is_deadline(&*t) {
            DL_ON_RING.fetch_sub(1, Ordering::Relaxed);
        }
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
        // SAFETY: 调用方契约（在环节点）；环非空必有 next。W3-Q12 起游标
        // 前移律单源 berry_ringface::advance_past（host 钉板：常形前移/
        // 单元素环自指/断链 None 三面）——NonNull 直传（泛型 T），零投影。
        let next = (*node.as_ptr()).rq_next.get().expect("环上节点必有 next");
        let next = berry_ringface::advance_past(node, Some(next)).expect("环上节点必有 next");
        self.head.set(Some(next)); // 单元素环时 next==node，游标不变（保持指向它）
    }

    /// 过滤版只读探峰（S-2b；S-3b 改形）：环上是否存在 `prio_effective` 严格
    /// 高于 `v` 的 **eligible** 任务（叠加 [`crate::class::eligible`] Running
    /// 排除门，至多一圈，不动游标、不做出选）。供 [`crate::sched::try_preempt`]
    /// 的前置判定——「不可能切走就根本不 pick」（pick_next 会 `advance_past`
    /// 消耗 head 槽位：出选后不切走 = 幻影出选，见 try_preempt 注释；S-2b 起
    /// 全调用面走过滤版，无过滤版退役）。
    pub(crate) fn has_eff_above_filtered(
        &self,
        v: i32,
        my_cur: *mut Task,
    ) -> bool {
        let Some(h) = self.head.get() else {
            return false; // 空环
        };
        // SAFETY: 同 has_eff_above——环上不变式 + KERN_LOCK 窗内单 pick（E0）。
        unsafe {
            let mut cur = h;
            loop {
                if crate::class::eligible(my_cur, cur.as_ptr())
                    && (*cur.as_ptr()).prio_effective > v
                {
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
/// S-3b（子案 §9.6 步 b-1）真化：`capacity` 定长数组（v1 = 2 核）由
/// `sched::init` 从 HAL `cpu_capacity()` 填——QEMU virt 同核 1024/1024 占位，
/// 真差异容量（大小核）= EAS 面 v2+。
#[derive(Clone, Copy)]
pub struct Topo {
    /// CPU 数。G0 = 1。
    pub nr_cpus: u32,
    /// 每核算力（0..=1023 同构尺度，Linux `scale_capacity` 口径）。索引 = 核号；
    /// 超出在册核数的槽位恒 0（单核形态只 [0] 有效）。
    pub capacity: [u32; 2],
}

impl Topo {
    /// 单核拓扑（G0/G1）。
    pub const fn single() -> Self {
        Topo { nr_cpus: 1, capacity: [0; 2] }
    }
}

impl Default for Topo {
    fn default() -> Self {
        Self::single()
    }
}
