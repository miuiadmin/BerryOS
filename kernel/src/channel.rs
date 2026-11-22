//! Channel 无环端点对（3-4b 初版；F1-3 彻底重构为共享 [`ChannelShared`] + 在途句柄析构 + `SIGNAL_PEER_CLOSED`）。
//!
//! 核心契约：
//! - 双端通过 [`alloc::sync::Arc`] 共享单一 [`ChannelShared`]，彻底废除旧版 `ChannelEnd::pair` 双向
//!   `ObjRef::retain` 自引用死环；两端句柄全部关闭后，`ChannelShared` 与双向队列堆缓冲 100% 释放。
//! - [`Message`] 实现 [`Drop`]：任何滞留在未读队列或在回滚异常中丢弃的消息，自动对前 `nhandles` 枚
//!   在途句柄递减通道开闭计数并调用 [`ObjRef::release`]，彻底根除「塞满 VMO 句柄后关通道」的物理内存泄漏；
//!   `sys_channel_recv` 成功迁入接收方句柄表后调用 [`Message::disarm_handles`] 清零 `nhandles`。
//! - 每端维护句柄级存活计数 `open_ends[side]` 与幂等关闭门闩 `closed_once[side]`：任意一端最后一枚句柄
//!   关闭（含 `handle_close`、`handle_replace`、进程退出/崩溃 `drain_ipc_on_exit` 及未读在途句柄析构）时，
//!   立即清空本端未读队列、释放本端 `frozen_owner` 钉住引用，并在对端派生置位 [`SIGNAL_PEER_CLOSED`]
//!   （`1 << 4`）唤醒对端全部等待者。本端既有未读消息在对端关闭后仍可继续 `channel_recv` 排空；排空后再
//!   `recv` 或向已关对端 `send` 立即返回 [`ERR_BAD_STATE`]（`-37`）。

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use crate::handle::{
    ERR_AGAIN, ERR_BAD_STATE, ERR_BUSY, KernelObj, ObjRef, Rights, SIGNAL_PEER_CLOSED,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};

pub use berry_chanface::{
    clear_frozen_ok, mark_frozen_ok, pop_ok, push_front_ok, push_ok, queue_full_of,
    queue_readable, wait_signal_peer_state, CHAN_DEPTH, CHAN_HND_MAX, CHAN_MSG_MAX,
};

/// 一条在途消息：定长内联字节（≤[`CHAN_MSG_MAX`]）+ 迁移中的句柄令牌（≤[`CHAN_HND_MAX`]）。
pub struct Message {
    pub bytes: [u8; CHAN_MSG_MAX],
    pub nbytes: usize,
    pub handles: [(NonNull<ObjRef>, Rights); CHAN_HND_MAX],
    pub nhandles: usize,
    pub meta: berry_sched::intent::MsgIntent,
    pub billing: MsgBilling,
}

/// 消息记账头（内核可见，用户不传参不回写）。
#[derive(Clone, Copy, Debug, Default)]
pub struct MsgBilling {
    pub domain_id: u64,
    #[allow(dead_code)]
    pub charged_ns: u64,
    pub msg_type: u8,
}

#[allow(dead_code)]
pub const MSG_BILLING_UNSPECIFIED: u8 = 0;
#[allow(dead_code)]
pub const MSG_BILLING_REQUEST: u8 = 1;
pub const MSG_BILLING_REPLY: u8 = 2;

impl Message {
    /// 空消息（零分配）。
    pub fn new() -> Self {
        Self {
            bytes: [0; CHAN_MSG_MAX],
            nbytes: 0,
            handles: [(NonNull::dangling(), 0); CHAN_HND_MAX],
            nhandles: 0,
            meta: berry_sched::intent::MsgIntent::default(),
            billing: MsgBilling::default(),
        }
    }

    /// `sys_channel_recv` 将全部随行句柄成功迁入接收方句柄表且出参写回无误后调用：
    /// 解除本消息对随行句柄的所有权，防止 `Message::Drop` 双重释放。
    #[inline]
    pub fn disarm_handles(&mut self) {
        self.nhandles = 0;
    }
}

impl Drop for Message {
    fn drop(&mut self) {
        let n = self.nhandles;
        self.nhandles = 0;
        for i in 0..n {
            let obj = self.handles[i].0;
            // SAFETY: 在途句柄在 `take` 出发送方表后由本 `Message` 独占持有 1 份引用计数；
            // 若内含另一 `ChannelEnd`，先递减其句柄级开闭计数，再归还 `ObjRef` 本体引用。
            unsafe {
                if let KernelObj::Channel(c) = &(*obj.as_ptr()).obj {
                    c.release_handle();
                }
                ObjRef::release(obj);
            }
        }
    }
}

/// 叶级中断安全短自旋锁：仅保护 `ChannelInner` 队列指针与冻标读写，临界区内绝不分配、
/// 绝不调用 `ObjRef::release` 或调度器唤醒，从根源消除嵌套通道自锁与锁序逆转。
struct LeafIrqLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

struct LeafIrqGuard<'a, T> {
    lock: &'a LeafIrqLock<T>,
    saved_daif: u64,
}

// SAFETY: `LeafIrqLock` 在屏蔽本地 IRQ 条件下以 Acquire/Release 原子自旋互斥跨核访问。
unsafe impl<T: Send> Sync for LeafIrqLock<T> {}
// SAFETY: 同上。
unsafe impl<T: Send> Send for LeafIrqLock<T> {}

impl<T> LeafIrqLock<T> {
    const fn new(val: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(val),
        }
    }

    #[inline]
    fn lock(&self) -> LeafIrqGuard<'_, T> {
        let saved_daif = berry_hal_aarch64::irq_save();
        berry_hal_aarch64::disable_irq();
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            while self.locked.load(Ordering::Relaxed) {
                core::hint::spin_loop();
            }
        }
        LeafIrqGuard { lock: self, saved_daif }
    }
}

impl<T> core::ops::Deref for LeafIrqGuard<'_, T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        // SAFETY: 持锁独占访问。
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> core::ops::DerefMut for LeafIrqGuard<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: 持锁独占可变访问。
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for LeafIrqGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
        berry_hal_aarch64::irq_restore(self.saved_daif);
    }
}

struct ChannelInner {
    /// `queues[side]` 为第 `side` 端的收件队列（对端 `1 - side` 发送时压入 `queues[side]`）。
    queues: [Vec<Message>; 2],
    /// 每端 AM-1.5 冻结属主标（`retain` 配平在锁外执行）。
    frozen_owner: [Option<NonNull<ObjRef>>; 2],
}

/// 通道对共享内核体（双端各持一份 `Arc<ChannelShared>`，零 `ObjRef` 环）。
pub struct ChannelShared {
    /// 每端当前在册句柄数（含句柄表槽与在途消息槽；归零即触发该端关闭）。
    open_ends: [AtomicU32; 2],
    /// 每端关闭动作单次门闩（保证 `release_handle` 与 `ChannelEnd::Drop` 合流幂等）。
    closed_once: [AtomicBool; 2],
    /// 每端弱等待键（= 对应 `ObjRef` 地址，不增加 `ObjRef.refs`；该端关闭即清 `0`）。
    end_objs: [AtomicUsize; 2],
    no_intent: bool,
    inner: LeafIrqLock<ChannelInner>,
}

/// 通道单侧端点（`side` ∈ {0, 1}）。
pub struct ChannelEnd {
    side: usize,
    no_intent: bool,
    shared: Option<Arc<ChannelShared>>,
    /// 仅用于兼容旧式 `try_unpaired` + `pair` 两步构造的预储队列槽（配对后即腾空移入 `ChannelShared`）。
    pre_queue: Vec<Message>,
}

#[inline]
fn notify_channel_wake(wait_key: usize) {
    if wait_key == 0 {
        return;
    }
    let sigs = SIGNAL_PEER_CLOSED | SIGNAL_READABLE | SIGNAL_WRITABLE;
    if !berry_hal_aarch64::kern_lock::kl_held() || berry_sched::sq_is_held() {
        berry_sched::wake_matching(wait_key, sigs);
    } else {
        crate::process::defer_peer_wake(wait_key);
    }
}

impl ChannelEnd {
    /// 一次性创建已配对双端点（`sys_channel_create` 首选路径）：先于入表预储双端队列容量。
    pub fn try_pair(no_intent: bool) -> Option<(Self, Self)> {
        let mut q0 = Vec::new();
        let mut q1 = Vec::new();
        q0.try_reserve_exact(CHAN_DEPTH).ok()?;
        q1.try_reserve_exact(CHAN_DEPTH).ok()?;
        let shared = Arc::new(ChannelShared {
            open_ends: [AtomicU32::new(1), AtomicU32::new(1)],
            closed_once: [AtomicBool::new(false), AtomicBool::new(false)],
            end_objs: [AtomicUsize::new(0), AtomicUsize::new(0)],
            no_intent,
            inner: LeafIrqLock::new(ChannelInner {
                queues: [q0, q1],
                frozen_owner: [None, None],
            }),
        });
        let e0 = Self {
            side: 0,
            no_intent,
            shared: Some(shared.clone()),
            pre_queue: Vec::new(),
        };
        let e1 = Self {
            side: 1,
            no_intent,
            shared: Some(shared),
            pre_queue: Vec::new(),
        };
        Some((e0, e1))
    }

    /// 为 `try_pair` 产出的双端点登记各自的 `ObjRef` 弱等待键（**不调用 `ObjRef::retain`**，零引用环）。
    ///
    /// # Safety
    /// `a` 与 `b` 须为刚插入表的两枚 `Channel` 对象指针。
    pub unsafe fn bind_objs(a: NonNull<ObjRef>, b: NonNull<ObjRef>) {
        // SAFETY: 调用方保证 `a`/`b` 为有效 `Channel` `ObjRef`。
        let end_a = unsafe { (*a.as_ptr()).obj.as_channel().expect("bind_objs: a 非 Channel") };
        if let Some(sh) = &end_a.shared {
            sh.end_objs[0].store(a.as_ptr() as usize, Ordering::Release);
            sh.end_objs[1].store(b.as_ptr() as usize, Ordering::Release);
        }
    }

    /// 兼容内核自检两步构造：创建未配对单端并预储 `CHAN_DEPTH` 容量。
    pub fn try_unpaired(no_intent: bool) -> Option<Self> {
        let mut pre_queue = Vec::new();
        pre_queue.try_reserve_exact(CHAN_DEPTH).ok()?;
        Some(Self {
            side: 0,
            no_intent,
            shared: None,
            pre_queue,
        })
    }

    /// 兼容内核自检两步构造：将两枚未配对端点组装进同一 `Arc<ChannelShared>`（零 `ObjRef::retain`）。
    ///
    /// # Safety
    /// `a` 与 `b` 须为在表且尚未配对的两枚不同 `Channel` `ObjRef`。
    pub unsafe fn pair(a: NonNull<ObjRef>, b: NonNull<ObjRef>) {
        debug_assert_ne!(a.as_ptr(), b.as_ptr());
        // SAFETY: `a != b` 且均为在表 `Channel` 对象。
        unsafe {
            let ea = (*a.as_ptr()).obj.as_channel_mut().expect("pair: a 非 Channel");
            let eb = (*b.as_ptr()).obj.as_channel_mut().expect("pair: b 非 Channel");
            let q0 = core::mem::take(&mut ea.pre_queue);
            let q1 = core::mem::take(&mut eb.pre_queue);
            let no_intent = ea.no_intent || eb.no_intent;
            let shared = Arc::new(ChannelShared {
                open_ends: [AtomicU32::new(1), AtomicU32::new(1)],
                closed_once: [AtomicBool::new(false), AtomicBool::new(false)],
                end_objs: [
                    AtomicUsize::new(a.as_ptr() as usize),
                    AtomicUsize::new(b.as_ptr() as usize),
                ],
                no_intent,
                inner: LeafIrqLock::new(ChannelInner {
                    queues: [q0, q1],
                    frozen_owner: [None, None],
                }),
            });
            ea.side = 0;
            ea.no_intent = no_intent;
            ea.shared = Some(shared.clone());
            eb.side = 1;
            eb.no_intent = no_intent;
            eb.shared = Some(shared);
        }
    }

    /// `HandleTable::duplicate` 复制本端句柄时递增句柄级开闭计数。
    #[inline]
    pub fn retain_handle(&self) {
        if let Some(sh) = &self.shared {
            sh.open_ends[self.side].fetch_add(1, Ordering::AcqRel);
        }
    }

    /// 关闭或销毁一枚指向本端的句柄时递减开闭计数；归零瞬间立即清空本端未读队列并唤醒对端 `PEER_CLOSED`。
    #[inline]
    pub fn release_handle(&self) {
        let Some(sh) = &self.shared else {
            return;
        };
        let prev = sh.open_ends[self.side].fetch_update(
            Ordering::AcqRel,
            Ordering::Acquire,
            |v| Some(v.saturating_sub(1)),
        );
        if prev == Ok(1) {
            self.close_side_idempotent();
        }
    }

    /// 幂等执行本端关闭收口：清空本端未读在途消息、归还冻标引用，并向对端与本端等待者广播关闭信号。
    pub fn close_side_idempotent(&self) {
        let Some(sh) = &self.shared else {
            return;
        };
        if sh.closed_once[self.side].swap(true, Ordering::AcqRel) {
            return;
        }
        sh.open_ends[self.side].store(0, Ordering::Release);
        let self_key = sh.end_objs[self.side].swap(0, Ordering::AcqRel);
        let peer_key = sh.end_objs[1 - self.side].load(Ordering::Acquire);
        let (unread, frozen) = {
            let mut g = sh.inner.lock();
            (
                core::mem::take(&mut g.queues[self.side]),
                g.frozen_owner[self.side].take(),
            )
        };
        // 锁外析构全部未读消息（触发 Message::Drop 释放所有在途 VMO/Channel 等句柄）。
        drop(unread);
        if let Some(owner) = frozen {
            // SAFETY: 配平 `mark_frozen` 的 `retain`。
            unsafe { ObjRef::release(owner) };
        }
        notify_channel_wake(peer_key);
        notify_channel_wake(self_key);
    }

    #[inline]
    pub fn no_intent(&self) -> bool {
        self.shared.as_ref().map_or(self.no_intent, |s| s.no_intent)
    }

    #[inline]
    pub fn is_open(&self) -> bool {
        self.shared.as_ref().map_or(false, |s| {
            !s.closed_once[self.side].load(Ordering::Acquire)
                && s.open_ends[self.side].load(Ordering::Acquire) > 0
        })
    }

    #[inline]
    pub fn peer_is_open(&self) -> bool {
        let peer = 1 - self.side;
        self.shared.as_ref().map_or(false, |s| {
            !s.closed_once[peer].load(Ordering::Acquire)
                && s.open_ends[peer].load(Ordering::Acquire) > 0
        })
    }

    /// 对端在表 `ObjRef` 弱等待键（对端已关或未配对返 `None`；供 `wake_matching` 与 `pi::donor_edge` 使用）。
    #[inline]
    pub fn peer_wait_key(&self) -> Option<usize> {
        if !self.peer_is_open() {
            return None;
        }
        let k = self.shared.as_ref()?.end_objs[1 - self.side].load(Ordering::Acquire);
        if k == 0 { None } else { Some(k) }
    }

    /// AM-1.5：查询本端冻结属主标。
    pub fn frozen_owner(&self) -> Option<NonNull<ObjRef>> {
        let sh = self.shared.as_ref()?;
        sh.inner.lock().frozen_owner[self.side]
    }

    /// AM-1.5：打本端冻结属主标（幂等；锁外 `retain`）。
    pub fn mark_frozen(&mut self, owner: NonNull<ObjRef>) -> bool {
        let Some(sh) = &self.shared else {
            return false;
        };
        {
            let mut g = sh.inner.lock();
            if !mark_frozen_ok(g.frozen_owner[self.side].map(|o| o.as_ptr() as u64)) {
                return false;
            }
            g.frozen_owner[self.side] = Some(owner);
        }
        // SAFETY: 调用方持有在表存活的 `Process` `ObjRef`。
        unsafe { (*owner.as_ptr()).retain() };
        true
    }

    /// AM-1.5：清本端冻结属主标（锁外 `release` 配平）。
    pub fn clear_frozen(&mut self, owner: NonNull<ObjRef>) -> bool {
        let Some(sh) = &self.shared else {
            return false;
        };
        {
            let mut g = sh.inner.lock();
            if !clear_frozen_ok(
                g.frozen_owner[self.side].map(|o| o.as_ptr() as u64),
                owner.as_ptr() as u64,
            ) {
                return false;
            }
            g.frozen_owner[self.side] = None;
        }
        // SAFETY: 配平 `mark_frozen` 的 `retain`。
        unsafe { ObjRef::release(owner) };
        true
    }

    /// 本端是否有未读消息。
    pub fn has_message(&self) -> bool {
        let Some(sh) = &self.shared else {
            return false;
        };
        queue_readable(sh.inner.lock().queues[self.side].len())
    }

    /// 计算本端当前派生信号三元组 `(readable, writable, peer_closed)`。
    pub fn wait_signals(&self) -> (bool, bool, bool) {
        let Some(sh) = &self.shared else {
            return (false, false, true);
        };
        let peer = 1 - self.side;
        let peer_open = self.peer_is_open();
        let g = sh.inner.lock();
        wait_signal_peer_state(g.queues[self.side].len(), g.queues[peer].len(), peer_open)
    }

    /// `sys_channel_send` 前置快照：校验对端存活与队列未满，返回 `(peer_wait_key, peer_empty, peer_frozen)`。
    pub fn send_precheck(&self) -> Result<(usize, bool, Option<NonNull<ObjRef>>), i32> {
        let Some(sh) = &self.shared else {
            return Err(ERR_BAD_STATE);
        };
        if !self.peer_is_open() {
            return Err(ERR_BAD_STATE);
        }
        let peer = 1 - self.side;
        let peer_key = sh.end_objs[peer].load(Ordering::Acquire);
        if peer_key == 0 {
            return Err(ERR_BAD_STATE);
        }
        let g = sh.inner.lock();
        if queue_full_of(g.queues[peer].len()) {
            return Err(ERR_BUSY);
        }
        Ok((peer_key, g.queues[peer].is_empty(), g.frozen_owner[peer]))
    }

    /// 将消息压入对端队列；若并发窗口内对端已关或队列已满，原样退回 `Err((msg, errno))` 供调用方无损回滚。
    pub fn push_to_peer(&self, msg: Message) -> Result<(), (Message, i32)> {
        let Some(sh) = &self.shared else {
            return Err((msg, ERR_BAD_STATE));
        };
        if !self.peer_is_open() {
            return Err((msg, ERR_BAD_STATE));
        }
        let peer = 1 - self.side;
        let mut g = sh.inner.lock();
        if !self.peer_is_open() {
            return Err((msg, ERR_BAD_STATE));
        }
        if queue_full_of(g.queues[peer].len()) {
            return Err((msg, ERR_BUSY));
        }
        debug_assert!(push_ok(g.queues[peer].len()));
        g.queues[peer].push(msg);
        Ok(())
    }

    /// `sys_channel_recv` 弹出本端队首消息：若本端队列非空则优先返回消息（允许对端关闭后排空残余消息）；
    /// 若队列已空且对端已关闭则返回 `Err(ERR_BAD_STATE)`，否则返回 `Err(ERR_AGAIN)`。
    pub fn pop_or_peer_state(&self) -> Result<Message, i32> {
        let Some(sh) = &self.shared else {
            return Err(ERR_BAD_STATE);
        };
        let mut g = sh.inner.lock();
        if !g.queues[self.side].is_empty() {
            debug_assert!(pop_ok(g.queues[self.side].len()));
            return Ok(g.queues[self.side].remove(0));
        }
        if !self.peer_is_open() {
            return Err(ERR_BAD_STATE);
        }
        Err(ERR_AGAIN)
    }

    /// 内核自检专用：向本端收件队列直接压入一条消息。
    pub fn push(&self, msg: Message) {
        let Some(sh) = &self.shared else {
            return;
        };
        if !self.is_open() {
            return;
        }
        let mut g = sh.inner.lock();
        if !self.is_open() || queue_full_of(g.queues[self.side].len()) {
            drop(g);
            drop(msg);
            return;
        }
        debug_assert!(push_ok(g.queues[self.side].len()));
        g.queues[self.side].push(msg);
    }

    /// `recv_rollback` 专用：将未完成拷出的消息保序压回本端队首；若本端已关闭则在锁外安全析构消息。
    pub fn push_front(&self, msg: Message) {
        let Some(sh) = &self.shared else {
            return;
        };
        if !self.is_open() {
            return;
        }
        let mut g = sh.inner.lock();
        if !self.is_open() {
            drop(g);
            drop(msg);
            return;
        }
        debug_assert!(push_front_ok(g.queues[self.side].len()));
        g.queues[self.side].insert(0, msg);
    }
}

impl Drop for ChannelEnd {
    fn drop(&mut self) {
        self.close_side_idempotent();
    }
}
