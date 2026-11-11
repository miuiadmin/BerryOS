//! Event 对象（3-4c；`IPC设计.md` §3 异步通知、能力模型 §3、ABI §8.3）。
//!
//! 语义 = **纯用户位掩码**：`object_signal` 置/清位（门控 RIGHT_SIGNAL；
//! 4-2 位空间起**只收高 32 位用户位**——低 32 位保留给内核派生信号
//! READABLE/WRITABLE，越界掩码 → -EINVAL），`object_wait_one` 阻塞等
//! `current & signals != 0`（4-1 阻塞化：poll-first + 醒来重验；命中回写
//! **全满足集**，Zircon `zx_object_wait_one` 同形）。
//!
//! Event 是最简单的内核对象：无队列、无对端、无钉住——就一个原子 u64。
//! 跨任务通知 = 各持同一 Event 的 handle（dup 授权），signal 即写共享对象。
//!
//! **原子化纪律（S-3c C21 落地）**：`signals` = AtomicU64，**每次 RMW 单条
//! 原子指令语义**（fetch_update CAS 环 / fetch_or / swap——位叠加不丢）。
//! 内存序全 Relaxed：**排序由锁窗承担**（现行 = KERN_LOCK 锁窗；S-3c c-2
//! 拆锁后 = SQ 锁窗，义务③「查位+入队同窗 / 写位+扫描同窗」锚）——原子性
//! 只杀撕裂与丢更新，**不得**用 Acquire/Release 语义替代锁窗。写者纪律
//! 沿袭 S-2 C21：SVC 上下文 + uart RX 通知槽（IRQ 上下文，E27 起 KL 包络）；
//! 新增非 SVC 写者仍一律禁止。

use core::sync::atomic::{AtomicU64, Ordering};

/// 事件对象本体：信号位掩码（bit i = 信号 i；语义归用户态约定，内核只管位）。
pub struct Event {
    pub signals: AtomicU64,
}

impl Event {
    /// 出生全零（无信号置位）。
    pub fn new() -> Self {
        Self { signals: AtomicU64::new(0) }
    }

    /// 原子位置/清位（`signals = (signals & !clear) | set`——单条原子 RMW
    /// 语义，CAS 环实现；无锁窗下并发置/清不丢位）。返回新值。
    pub fn signal(&self, clear_mask: u64, set_mask: u64) -> u64 {
        // Relaxed：序由锁窗承担（模块注「原子化纪律」）；RMW 原子性本身
        // 由 fetch_update 的 compare_exchange 环保证。位律单源
        // berry-evface::apply_signal（W3-Q7——(cur & !clear) | set 纯投影，
        // host 钉板）。
        match self.signals.fetch_update(
            Ordering::Relaxed,
            Ordering::Relaxed,
            |cur| Some(berry_evface::apply_signal(cur, clear_mask, set_mask)),
        ) {
            Ok(new) => new,
            // 闭包恒 Some → CAS 只因值竞争重试后成功，Err 分支不可达；
            // 防御面取当前值（返回值现役调用点均忽略）。
            Err(cur) => cur,
        }
    }

    /// 当前掩码快照（wait 面 poll-first / 重验循环读点）。
    pub fn load(&self) -> u64 {
        self.signals.load(Ordering::Relaxed)
    }
}
