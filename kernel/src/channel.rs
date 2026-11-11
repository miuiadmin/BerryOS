//! Channel 端点对（3-4b；`IPC设计.md` §2 同步端点、能力模型 §3）。
//!
//! v1 语义 = **非阻塞有界队列**（syscall 主体不可抢占 → 无阻塞路径）：
//! - `send`：门控 RIGHT_SEND，消息压**对端**队列；满 → -EBUSY；
//! - `recv`：门控 RIGHT_RECV，从**本端**队列弹；空 → -EAGAIN；
//!   ——send/recv 本身 v1 恒非阻塞；**阻塞等空/满走 `object_wait_one`**
//!   （4-2 起支持 Channel：READABLE=本端队列非空 / WRITABLE=对端队列未满，
//!   内核派生位——send 成功唤醒对端 READABLE 等待者、recv 成功唤醒对端
//!   WRITABLE 等待者，见 syscall.rs 两唤醒点）；
//! - 消息 = inline 字节（≤[`CHAN_MSG_MAX`]）+ handle 令牌（≤[`CHAN_HND_MAX`]，
//!   发送方 `take` 出表、接收方 `insert_ref` 入表——能力随消息迁移，能力模型 §4）。
//!
//! **对端互钉**：两端各持对端 `NonNull<ObjRef>` 且构造即 `retain`（不钉 = 对端
//! 先关后本端 send 走 peer = use-after-free）。互钉成环 → 两端 handle 全关也不
//! 释放（安全方向的泄漏，v1 接受；v3 DEAD 检测 + Weak peer 收口）。消息中未
//! 收割的 handle 令牌随队列一起泄漏（refs 不减、永不释放——同安全方向）。
//!
//! v1 裁剪：对端关闭检测不做（需 DEAD 检测，v3——诚实缺口：send/recv 对死端
//! 表现如常，不报 -EBADSTATE）。PI 捐赠（4-3b `kernel/src/pi.rs`）与 send 侧
//! fastpath/slowpath 分层（4-4 `sys_channel_send` 尾部 fork）已落地——本文件
//! 只做端点/消息本体，分层决策在调用方。

use crate::handle::{ObjRef, Rights};
use alloc::vec::Vec;
use core::ptr::NonNull;

// W3-Q9：界常量与队列资格判单源迁 berry-chanface（判据纯函数面，host
// 可测钉板）；本文件 re-export 保持 crate 内路径稳定（hdlface 同款纪律）。
pub use berry_chanface::{
    queue_full_of, queue_readable, CHAN_DEPTH, CHAN_HND_MAX, CHAN_MSG_MAX,
};

/// 单条消息 inline 字节上限（IPC设计 §2：内联载荷 ≤256B；更大走共享 VMO）。
/// **单源 = berry-chanface（W3-Q9 迁出）**——本文件 re-export（上方 pub use）
/// 保 crate 内外路径稳定；沿革注记随源在 chanface。
/// 单条消息随行 handle 上限（ABI §8.3 send/recv 数组界）。T-1：4→6——
/// berry-exec env 块 gift 扩至 6 位；A2-3：6→8——bsh arm 三 extras
/// （rootfs/fs/netd 端）+ base 2 + T-1 自描述对 2 = env 块 7 位（首跑实抓：
/// nhandles=7 > 6 → send -EINVAL → spawn 链 trap 自陷 svcmgr）；libc
/// `io.rs` 收窗与 ktest H3 超限样本同步 8/9 口径。
/// 每端收件队列深度（有界性 = send -EBUSY 的来源）。

/// 一条消息：inline 字节 + 迁移中的 handle 令牌（`take` 出表、未入新表——
/// 引用计数随令牌走，`insert_ref` 不动 refs）。**定长内联**（3b3 审查 H2 try
/// 化）：曾为双 `Box`（bytes/handles 各一分配且在 take **之后**构造——EL0 可
/// 触发 OOM abort 于迁移中途）；内联后 send/recv 全链零分配（队列容量由
/// [`ChannelEnd::try_unpaired`] 预储，push/push_front 不再增长）。多余槽位以
/// dangling 占位，仅前 `nbytes`/`nhandles` 项有效。
pub struct Message {
    pub bytes: [u8; CHAN_MSG_MAX],
    pub nbytes: usize,
    pub handles: [(NonNull<ObjRef>, Rights); CHAN_HND_MAX],
    pub nhandles: usize,
    /// 4-3c intent 消息头（`IPC设计.md` §2：内核可见元数据槽，ADR #42）：
    /// send 侧快照发送者 `intent_effective`、recv 侧结算——用户不可见不传参，
    /// 纯内核随行数据（recv ABI 不回写）。
    pub meta: berry_sched::intent::MsgIntent,
    /// 1.1/S4：受益者付费记账头（`docs/调点调度-v1.1.md` §7.5）——
    /// send 侧快照发送者域 ID（= Process ObjRef 地址 = `Task.pinned`），
    /// recv 侧可读（S4 只记不罚）。`charged_ns` 预留（S5 消费）。
    pub billing: MsgBilling,
}

/// 1.1/S4：消息记账头（内核可见，用户不传参不回写——同 MsgIntent 模式）。
#[derive(Clone, Copy, Debug, Default)]
pub struct MsgBilling {
    /// 发送者域 ID（= Process ObjRef 地址；0 = 内核任务/无归属）。
    pub domain_id: u64,
    /// 本消息已记账 CPU 时长（ns；S5 起 server 侧填充——S4 恒 0）。
    #[allow(dead_code)] // S5 起消费（server 侧 CPU 时长记域）
    pub charged_ns: u64,
    /// v2 协议标签：0 = 未标注（默认——v1 全部计费，含 reply 误设已知局限）
    /// / 1 = 请求（recv 侧设计费区间）/ 2 = 回复（recv 侧**跳过**计费——
    /// 修 client recv reply 误设起点）。发送者经 `channel_send` flags 标注。
    pub msg_type: u8,
}

/// `MsgBilling.msg_type` 值。
pub const MSG_BILLING_UNSPECIFIED: u8 = 0;
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
}

/// channel 端点（`channel_create` 一次建一对，两端的 `peer` 互指）。
pub struct ChannelEnd {
    /// 对端对象（`pair` 配对时置 `Some` 并互钉 retain；出生 = `None`——**Drop 只
    /// 在 Some 时 release**，未配对端点可安全销毁：create 第二端入表失败的回滚
    /// close 走 None 分支，不碰任何指针）。
    peer: Option<NonNull<ObjRef>>,
    /// 本端收件队列（对端 send 压这里；本端 recv 从这里弹）。
    queue: Vec<Message>,
    /// 4-3c opt-out（五件套之 4，QNX `_NTO_CHF_FIXED_PRIORITY` 等价）：
    /// `channel_create` options bit0——置位端 recv **不结算 intent**（固定优先级
    /// server 逃生门）。对称建对 API 下「通道级」语义 = 两端同置、双向皆不传播。
    no_intent: bool,
    /// AM-1.5 wake-pending 冻结属主标：持有本端（接收侧）handle 的进程被
    /// `process_suspend` 全冻时指向该 Process ObjRef（**retain 配平**——freeze
    /// 枚举打标时 +1，thaw 清标/端点 Drop 防御面 -1）。设计意图（AM 设计
    /// 「IPC × 冻结」Q1 裁决）：send 热路径免**反查属主**——非冻结态恒 None，
    /// send 只加一次本位测试分支（非冻结零额外成本）；置位后 send 照常入队
    /// （消息不丢弃），另触发 wake-pending 通知（`sys_channel_send` 尾）。
    /// 冻进程任务全冻 ⇒ 表不可变 ⇒ 冻端点不可能先于 thaw 销毁（Drop 防御面
    /// 仅为 retain 配平纪律）。共享端点（dup/迁移跨表）归**首冻结主**——
    /// v1 单主形，多冻结主共享端点的次主通知面是已知保守缺口（消息仍排队，
    /// 只是次主 token 持有者不被这一跳唤醒）。
    frozen_owner: Option<NonNull<ObjRef>>,
}

impl ChannelEnd {
    /// 建未配对端点（peer 占位 None——**必须**随即经 [`Self::pair`] 完成互钉才
    /// 可用；唯一调用方 `sys_channel_create` 建对后立刻配对，无中间观察窗口）。
    /// **队列预储 [`CHAN_DEPTH`] 容量**（3b3 审查 H2 try 化：send 压队列/recv
    /// 回滚 push_front 曾在 take 之后分配——OOM abort 于迁移中途；预储后运行
    /// 期零增长）。预储失败返 None（调用方折 -ENOMEM，先于一切入表——零回滚面）。
    /// `no_intent` = opt-out 标志（见字段注）。
    pub fn try_unpaired(no_intent: bool) -> Option<Self> {
        let mut queue = Vec::new();
        queue.try_reserve_exact(CHAN_DEPTH).ok()?;
        Some(Self { peer: None, queue, no_intent, frozen_owner: None })
    }

    /// AM-1.5 冻结属主标（send 热路径唯一新增分支的测试位；`None` = 未冻结）。
    pub fn frozen_owner(&self) -> Option<NonNull<ObjRef>> {
        self.frozen_owner
    }

    /// AM-1.5：打冻结属主标。**幂等**——已标（他主或同主）返 `false` 不重复
    /// retain（同表 duplicate 别名 / 跨进程共享端点归首冻结主，见字段注）。
    /// retain 配平点：[`Self::clear_frozen`]（thaw 清标）与本类型 Drop 防御面。
    pub fn mark_frozen(&mut self, owner: NonNull<ObjRef>) -> bool {
        if self.frozen_owner.is_some() {
            return false;
        }
        self.frozen_owner = Some(owner);
        // SAFETY: owner = 调用方（freeze 枚举）持有的在表 Process ObjRef，
        // 对象活；单核 IRQ 屏蔽串行。retain 本身是安全方法。
        unsafe { (*owner.as_ptr()).retain() };
        true
    }

    /// AM-1.5：清由 `owner` 打的冻标（属主不符/未标返 `false` 不动——跨主
    /// 安全）。真清时 release 配平 mark 的 retain。**调用方契约**：枚举清标
    /// 场景（thaw 走查全表）须保证 `owner` 在整轮枚举期间存活（token Drop /
    /// suspend 回滚序此刻必持 ≥1 引用——release 只退 mark 份额，归零销毁
    /// 不落在枚举中途）。
    pub fn clear_frozen(&mut self, owner: NonNull<ObjRef>) -> bool {
        if self.frozen_owner != Some(owner) {
            return false;
        }
        self.frozen_owner = None;
        // SAFETY: 配平 mark_frozen 的 retain；此后本端点不再触 owner。
        unsafe { ObjRef::release(owner) };
        true
    }

    /// opt-out 标志（recv 结算侧查询）。
    pub fn no_intent(&self) -> bool {
        self.no_intent
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
        queue_full_of(self.queue.len())
    }

    /// 队列是否有消息（wait 面 READABLE 信号状态——4-2 内核派生位）。
    pub fn has_message(&self) -> bool {
        queue_readable(self.queue.len())
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
        // AM-1.5 冻结属主标配平：正常序 thaw 清标先于端点销毁（冻进程任务
        // 全冻 ⇒ 表不可变 ⇒ 此分支不可达）；万一到达（防御面）也保 retain
        // 配平不泄漏。SAFETY: freeze 枚举打标时 retain 过；单核顺序同上。
        if let Some(owner) = self.frozen_owner {
            unsafe { ObjRef::release(owner) };
        }
    }
}
