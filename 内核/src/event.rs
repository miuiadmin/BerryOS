//! Event 对象（3-4c；`IPC设计.md` §3 异步通知、能力模型 §3、ABI §8.3）。
//!
//! v1 语义 = **纯位掩码 + poll-only**：`object_signal` 置/清位（门控
//! RIGHT_SIGNAL），`object_wait_one` 非阻塞查一次（命中回写**全满足集**，
//! Zircon `zx_object_wait_one` 同形；未命中 -EAGAIN）。**阻塞等待排队留 P4**
//! ——deadline ≠ 0 一律 -ENOTSUP（syscall 主体不可抢占的 v1 免锁根基下没有
//! 安全的睡眠路径）。
//!
//! Event 是最简单的内核对象：无队列、无对端、无钉住——就一个 u64。跨任务
//! 通知 = 各持同一 Event 的 handle（dup 授权），signal 即写共享对象（syscall
//! 全程 IRQ 屏蔽 → 无并发面）。

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
