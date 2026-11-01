//! Event 对象（3-4c；`IPC设计.md` §3 异步通知、能力模型 §3、ABI §8.3）。
//!
//! 语义 = **纯用户位掩码**：`object_signal` 置/清位（门控 RIGHT_SIGNAL；
//! 4-2 位空间起**只收高 32 位用户位**——低 32 位保留给内核派生信号
//! READABLE/WRITABLE，越界掩码 → -EINVAL），`object_wait_one` 阻塞等
//! `current & signals != 0`（4-1 阻塞化：poll-first + 醒来重验；命中回写
//! **全满足集**，Zircon `zx_object_wait_one` 同形）。
//!
//! Event 是最简单的内核对象：无队列、无对端、无钉住——就一个 u64。跨任务
//! 通知 = 各持同一 Event 的 handle（dup 授权），signal 即写共享对象（syscall
//! 全程 IRQ 屏蔽 → 无并发面）。
//!
//! **写者纪律（S-2 C21）**：`signals` 的写者 = SVC 上下文（KERN_LOCK 锁窗内
//! ——跨核互斥 + 本核 IRQ 屏蔽）；**新增非 SVC 写者一律禁止**（IRQ 上下文绕
//! 锁直写 = 与他核锁窗写者竞速丢更新）。唯一存量例外 = uart RX 通知槽（P5
//! A-1 经函数指针槽在 IRQ 上下文置 RX Event 位）——其安全性依托「生产者
//! SPI33 路由钉 CPU0 + 唯一清位者 console 任务同核 CPU0、SVC 入口先
//! disable_irq 后取锁」的同核互斥（见 uart.rs E26）；S-3 per-CPU 迁移前
//! 结构性成立，别在此之外加第二条 IRQ 写者路径。

/// 事件对象本体：信号位掩码（bit i = 信号 i；语义归用户态约定，内核只管位）。
pub struct Event {
    pub signals: u64,
}

impl Event {
    /// 出生全零（无信号置位）。
    pub fn new() -> Self {
        Self { signals: 0 }
    }

    /// 原子位置/清位（`signals = (signals & !clear) | set`——单条指令级语义，
    /// IRQ 屏蔽下单核天然原子）。返回新值。
    pub fn signal(&mut self, clear_mask: u64, set_mask: u64) -> u64 {
        self.signals = (self.signals & !clear_mask) | set_mask;
        self.signals
    }
}
