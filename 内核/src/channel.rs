//! Channel 端点对（3-4b；`IPC设计.md` §2 同步端点、能力模型 §3）。
//!
//! v1 语义 = **非阻塞有界队列**（syscall 主体不可抢占 → 无阻塞路径）：
//! - `send`：门控 RIGHT_SEND，消息压**对端**队列；满 → -EBUSY；
//! - `recv`：门控 RIGHT_RECV，从**本端**队列弹；空 → -EAGAIN（v1 无等待，
//!   object_wait 阻塞留 P4）；
//! - 消息 = inline 字节（≤[`CHAN_MSG_MAX`]）+ handle 令牌（≤[`CHAN_HND_MAX`]，
//!   发送方 `take` 出表、接收方 `insert_ref` 入表——能力随消息迁移，能力模型 §4）。
//!
//! **对端互钉**：两端各持对端 `NonNull<ObjRef>` 且构造即 `retain`（不钉 = 对端
//! 先关后本端 send 走 peer = use-after-free）。互钉成环 → 两端 handle 全关也不
//! 释放（安全方向的泄漏，v1 接受；v3 DEAD 检测 + Weak peer 收口）。消息中未
//! 收割的 handle 令牌随队列一起泄漏（refs 不减、永不释放——同安全方向）。
//!
//! v1 裁剪：对端关闭检测不做（需 DEAD 检测，v3——诚实缺口：send/recv 对死端
//! 表现如常，不报 -EBADSTATE）；无 PI/fastpath（P4 分层）。

use crate::handle::{ObjRef, Rights};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ptr::NonNull;

/// 单条消息 inline 字节上限（IPC设计 §2：内联载荷 ≤256B；更大走共享 VMO）。
pub const CHAN_MSG_MAX: usize = 256;
/// 单条消息随行 handle 上限（ABI §8.3 send/recv 数组界）。
pub const CHAN_HND_MAX: usize = 4;
/// 每端收件队列深度（有界性 = send -EBUSY 的来源）。
pub const CHAN_DEPTH: usize = 8;

/// 一条消息：inline 字节 + 迁移中的 handle 令牌（`take` 出表、未入新表——
/// 引用计数随令牌走，`insert_ref` 不动 refs）。
pub struct Message {
    pub bytes: Box<[u8]>,
    pub handles: Box<[(NonNull<ObjRef>, Rights)]>,
}

/// channel 端点（`channel_create` 一次建一对，两端的 `peer` 互指）。
pub struct ChannelEnd {
    /// 对端对象（`pair` 配对时置 `Some` 并互钉 retain；出生 = `None`——**Drop 只
    /// 在 Some 时 release**，未配对端点可安全销毁：create 第二端入表失败的回滚
    /// close 走 None 分支，不碰任何指针）。
    peer: Option<NonNull<ObjRef>>,
    /// 本端收件队列（对端 send 压这里；本端 recv 从这里弹）。
    queue: Vec<Message>,
}

impl ChannelEnd {
    /// 建未配对端点（peer 占位 None——**必须**随即经 [`Self::pair`] 完成互钉才
    /// 可用；唯一调用方 `sys_channel_create` 建对后立刻配对，无中间观察窗口）。
    pub fn unpaired() -> Self {
        Self { peer: None, queue: Vec::new() }
    }

    /// 把两个已入表的端点互指互钉（`channel_create` 的收尾步骤）。
    ///
    /// # Safety
    /// `a`/`b` 须为**在表**且类型门控为 Channel 的两个端点对象（get_raw 过）；
    /// 本调用即完成互钉 retain，此后两端方可使用。
    pub unsafe fn pair(a: NonNull<ObjRef>, b: NonNull<ObjRef>) {
        // SAFETY: 调用方契约：两指针各指在表的 ChannelEnd；互钉各 +1。
        // unwrap 只在内核自违反契约时可达（用户不可达——get_raw 类型门控在前）。
        unsafe {
            (*a.as_ptr()).obj.as_channel_mut().unwrap().peer = Some(b);
            (*b.as_ptr()).obj.as_channel_mut().unwrap().peer = Some(a);
            (*a.as_ptr()).retain();
            (*b.as_ptr()).retain();
        }
    }

    /// 对端对象指针（send 的目的端；`None` = 未配对——v1 唯一调用方 create 即
    /// 配对，运行期不可达的防御面）。
    pub fn peer(&self) -> Option<NonNull<ObjRef>> {
        self.peer
    }

    /// 队列是否已满（send 前置检查 → -EBUSY）。
    pub fn queue_full(&self) -> bool {
        self.queue.len() >= CHAN_DEPTH
    }

    /// 压一条消息进**本端**队列（send 侧实际调的是对端的这个方法）。
    pub fn push(&mut self, msg: Message) {
        debug_assert!(self.queue.len() < CHAN_DEPTH);
        self.queue.push(msg);
    }

    /// 窥视队首消息（recv 先看后取：所有失败路径不破坏消息）。
    pub fn front(&self) -> Option<&Message> {
        self.queue.first()
    }

    /// 弹出队首（recv 全部成功路径的收尾；令牌已被调用方收割）。
    pub fn pop(&mut self) -> Message {
        self.queue.remove(0) // 深度 ≤8：O(n) 可忽略
    }

    /// 消息压回**队首**（recv 失败回滚专用——消费路径的任一中途失败须保序
    /// 复原，FIFO 语义下不能从队尾补回）。
    pub fn push_front(&mut self, msg: Message) {
        self.queue.insert(0, msg);
    }
}

impl Drop for ChannelEnd {
    fn drop(&mut self) {
        // 仅配对端 release 互钉（成环 → 两端 handle 全关也泄漏——安全方向，
        // 模块注政策；未配对 None 端点无钉可放，直接走 Vec drop）。
        // SAFETY: pair 时 retain 过；单核顺序（对象仅在 IRQ 屏蔽路径销毁）。
        if let Some(peer) = self.peer {
            unsafe { ObjRef::release(peer) };
        }
    }
}
