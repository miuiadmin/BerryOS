//! berry-poll —— 多路等待聚合器纯函数面（A2-1，docs/事件循环A2设计草案.md
//! §3.1）。把内核 `object_wait_many` 的 n≤16 上限翻越成任意 N 路的进程内
//! 等待集：批切分 + poll-first 收割 + 分段阻塞轮转。
//!
//! 三个「免费」语义（A2-0 草案 §2 关键洞察，全部来自内核既有行为，零新内核面）：
//! - **LT（level-trigger）免费**：wait_many 命中回写全满足集且不消费对象
//!   状态——未处理完的就绪项下轮 poll-first 自然再报（epoll LT 对齐）；
//! - **非阻塞探测免费**：deadline==0 纯 poll（-EAGAIN 即返）——调用方
//!   `wait(0)` 即得非阻塞快照；
//! - **边沿聚合免费**：WAIT_ANY 哨兵语义下任何对象的唤醒都会打断阻塞段
//!   ——本库的分段轮转即建立在「醒来重验」之上，虚假唤醒无害。
//!
//! v1 裁定（头注立册，改前先读）：
//! - **收割到干**：一次 `wait` 的 poll 轮把命中批收割到 -EAGAIN（epoll_wait
//!   允许部分返回，但全量快照让调用方免写补报循环——A2-0 草案的「就绪
//!   缓存」层随之砍除：LT 由 poll-first 免费提供，缓存层无增量价值）；
//! - **错误传播不摘槽**：wait_many 返回 -EBADF 等错误码时库无法定位批内
//!   坏对象（syscall 只回码不回 idx）——若本轮已收割事件则事件优先返回
//!   （错误下轮裸露），否则原码向上传播，由调用方排查摘除（P6 A-4「EBADF
//!   分流勿 trap」同源纪律：库不猜、不静默吞）；
//! - **同 handle 单注册**：重复 add = 拒（epoll_ctl ADD 同 fd 重复 = EEXIST
//!   同款）；mask==0 拒（永不命中的等待项浪费批容量——挂哑对照项请用真
//!   Event 对象，C-4/INT 先例）；
//! - **阻塞只等批 0**：其余批的就绪发现延迟 ≤ 轮转片（`slice_ms`）——
//!   每段醒来回全批 poll 重扫。跨批延迟就是 kill criteria 的 3× 之源，
//!   轮转片实施期按 TCG 实测定标（候选 50-200ms）。
//!
//! 消费者纪律（P5 A-2「恒满足项」教训的库版）：`add` 前先问——这项在
//! 稳态会不会恒满足？恒满足项不入等待集（条件化入列是调用方责任）。

#![cfg_attr(not(test), no_std)]
extern crate alloc;

use alloc::vec::Vec;

/// 单次 wait_many 的对象数上限——与 `kernel/src/syscall.rs` 的
/// `WAIT_MANY_MAX` 同值（帽常量双侧同步三纪律：改内核侧必同步此处）。
pub const WAIT_MANY_MAX: usize = 16;

/// PollSet 容量帽（16 批上限——事件循环 v1 消费面远用不满，先钉墙）。
pub const POLL_SET_MAX: usize = 256;

/// 内核错误码（Linux 通用 errno 值，与 berry-libc/内核 syscall 同口径）。
pub const ERR_EAGAIN: i32 = -11;
pub const ERR_TIMEDOUT: i32 = -110;
pub const ERR_BADF: i32 = -9;

/// 一次 `wait` 收割到的就绪项。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollEvent {
    /// PollSet 槽号（`add` 返回值——调用方回关联上下文的稳定键）。
    pub slot: usize,
    /// 命中对象的 handle。
    pub handle: u32,
    /// 该对象的**全满足集**（内核回写口径：可能超出请求 mask 的超集）。
    pub observed: u64,
}

/// wait_many + 时钟注入面。host 测试挂假实现（确定性推进），guest 消费面
/// 包 `berry_sys::berry_object_wait_many` + `berry_clock_read`。
pub trait WaitMany {
    /// 等一批（≤[`WAIT_MANY_MAX`]）对象任一命中。语义与内核
    /// `object_wait_many` 对齐：
    /// - `deadline == 0` 纯 poll：无命中 `Err(ERR_EAGAIN)`；
    /// - 绝对时刻阻塞：到点 `Err(ERR_TIMEDOUT)`；
    /// - `Ok((i, observed))`：`handles[i]` 命中，observed = 该对象全满足集；
    /// - 坏 handle：`Err(ERR_BADF)`（无 idx 定位——见头注「错误传播」）。
    fn wait_many(&mut self, handles: &[u32], signals: &[u64], deadline: u64)
        -> Result<(usize, u64), i32>;

    /// 当前时刻（毫秒口径，guest = clock_read 换算）。仅在阻塞路径消费
    /// （纯 poll 模式零时钟依赖）。
    fn now(&mut self) -> u64;
}

/// 进程内多路等待集：任意 N 路 handle → 批切分的集中等待。
pub struct PollSet<W: WaitMany> {
    /// 槽表：Some((handle, mask)) 活跃 / None 空槽（freelist 复用保 idx 密集稳定）。
    entries: Vec<Option<(u32, u64)>>,
    freelist: Vec<usize>,
    w: W,
    slice_ms: u64,
}

/// `add` 失败因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddError {
    /// 容量满（[`POLL_SET_MAX`]）。
    Full,
    /// 同 handle 已注册（单注册裁定——头注）。
    Duplicate,
    /// mask == 0（永不命中——头注）。
    ZeroMask,
}

impl<W: WaitMany> PollSet<W> {
    /// 新建（默认轮转片 100ms——TCG 定标前的占位值）。
    pub fn new(w: W) -> Self {
        Self { entries: Vec::new(), freelist: Vec::new(), w, slice_ms: 100 }
    }

    /// 新建并指定阻塞轮转片（毫秒）。
    pub fn with_slice(w: W, slice_ms: u64) -> Self {
        Self { entries: Vec::new(), freelist: Vec::new(), w, slice_ms }
    }

    /// 注册一个等待对象，返回槽号（remove 后同槽复用——idx 稳定键）。
    pub fn add(&mut self, handle: u32, mask: u64) -> Result<usize, AddError> {
        if mask == 0 {
            return Err(AddError::ZeroMask);
        }
        if self.entries.iter().any(|e| matches!(e, Some((h, _)) if *h == handle)) {
            return Err(AddError::Duplicate);
        }
        if let Some(slot) = self.freelist.pop() {
            self.entries[slot] = Some((handle, mask));
            Ok(slot)
        } else if self.entries.len() < POLL_SET_MAX {
            self.entries.push(Some((handle, mask)));
            Ok(self.entries.len() - 1)
        } else {
            Err(AddError::Full)
        }
    }

    /// 摘除一个对象（按 handle 唯一定位）。不存在返回 false。
    pub fn remove(&mut self, handle: u32) -> bool {
        let found = self.entries.iter().position(|e| matches!(e, Some((h, _)) if *h == handle));
        match found {
            Some(i) => {
                self.entries[i] = None;
                self.freelist.push(i);
                true
            }
            None => false,
        }
    }

    /// 活跃对象数。
    pub fn len(&self) -> usize {
        self.entries.iter().filter(|e| e.is_some()).count()
    }

    /// 是否空集（空集 `wait` 零 syscall 即返）。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 多路等待（LT 语义）。`timeout_ms == 0` = 非阻塞快照。
    ///
    /// - `Ok(events)`：本轮收割到的就绪集（空集 = 超时无事件，epoll_wait
    ///   返回 0 同款）；
    /// - `Err(code)`：底层 wait_many 错误码原样传播（本轮无收割时——头注
    ///   「错误传播不摘槽」）。
    ///
    /// `now_ms` = 调用方时钟快照（阻塞路径的 deadline 基准；纯 poll 模式
    /// 不消费）。
    pub fn wait(&mut self, timeout_ms: u64, now_ms: u64) -> Result<Vec<PollEvent>, i32> {
        let mut out: Vec<PollEvent> = Vec::new();
        // 活跃槽快照：wait 全程槽集不变（错误传播不自动摘槽——头注）。
        let slots: Vec<usize> = (0..self.entries.len())
            .filter(|&i| self.entries[i].is_some())
            .collect();
        if slots.is_empty() {
            return Ok(out);
        }
        let deadline = if timeout_ms == 0 { 0 } else { now_ms.saturating_add(timeout_ms) };

        loop {
            // ① poll-first 全批扫（deadline=0 纯探测），批内收割到干。
            let mut any_ready = false;
            let mut b = 0usize;
            while b < slots.len() {
                let end = core::cmp::min(b + WAIT_MANY_MAX, slots.len());
                let mut live: Vec<usize> = slots[b..end].to_vec();
                while !live.is_empty() {
                    let (hs, ss) = self.proj(&live);
                    match self.w.wait_many(&hs, &ss, 0) {
                        Ok((i, obs)) => {
                            let slot = live.remove(i);
                            if let Some((h, _)) = self.entries[slot] {
                                out.push(PollEvent { slot, handle: h, observed: obs });
                            }
                            any_ready = true;
                        }
                        Err(ERR_EAGAIN) => break,
                        Err(e) => {
                            if out.is_empty() {
                                return Err(e);
                            }
                            return Ok(out); // 事件优先，错误下轮裸露
                        }
                    }
                }
                b = end;
            }
            if any_ready || deadline == 0 {
                return Ok(out);
            }

            // ② 全空：阻塞等批 0（slots 前 16），段帽 = min(剩余, 轮转片)。
            let cur = self.w.now();
            if cur >= deadline {
                return Ok(out);
            }
            let seg = core::cmp::min(deadline - cur, self.slice_ms);
            let seg_deadline = cur.saturating_add(seg);
            let n = core::cmp::min(WAIT_MANY_MAX, slots.len());
            let (hs, ss) = self.proj(&slots[..n]);
            match self.w.wait_many(&hs, &ss, seg_deadline) {
                // 命中不入 out：回 ① 统一收割（poll-first 会再命中它——
                // LT 同源；observed 由 ① 重取，等价）。
                Ok(_) => continue,
                // 段醒（片到点或真超时）→ 回 ① 全批重扫；真超时由下轮
                // ② 的 now >= deadline 判据收口。
                Err(ERR_TIMEDOUT) => continue,
                Err(e) => {
                    if out.is_empty() {
                        return Err(e);
                    }
                    return Ok(out);
                }
            }
        }
    }

    /// 槽号列表 → (handles, masks) 平行数组。
    fn proj(&self, live: &[usize]) -> (Vec<u32>, Vec<u64>) {
        let mut hs = Vec::with_capacity(live.len());
        let mut ss = Vec::with_capacity(live.len());
        for &s in live {
            if let Some((h, m)) = self.entries[s] {
                hs.push(h);
                ss.push(m);
            }
        }
        (hs, ss)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    /// 假 wait_many/时钟：确定性推进，可编程「阻塞段中外部置位」。
    struct Fake {
        /// handle → 当前满足集（对象信号态；命中不消费——LT 之源）。
        ready: HashMap<u32, u64>,
        /// 坏对象（wait_many 一律 -EBADF）。
        dead: HashSet<u32>,
        /// 当前时刻（阻塞调用推到段 deadline）。
        now: u64,
        wait_many_calls: usize,
        /// 观察到的 deadline 序列（段帽断言用）。
        deadlines: Vec<u64>,
        /// 编程唤醒：下一次**阻塞**调用（deadline>0 且 poll 空）时先注入
        /// 该 (handle, bits) 再重 poll——模拟「睡中被外部置位唤醒」。
        arm_wake: Option<(u32, u64)>,
    }

    impl Fake {
        fn new() -> Self {
            Fake {
                ready: HashMap::new(),
                dead: HashSet::new(),
                now: 1000,
                wait_many_calls: 0,
                deadlines: Vec::new(),
                arm_wake: None,
            }
        }
    }

    impl WaitMany for Fake {
        fn wait_many(
            &mut self,
            handles: &[u32],
            signals: &[u64],
            deadline: u64,
        ) -> Result<(usize, u64), i32> {
            self.wait_many_calls += 1;
            self.deadlines.push(deadline);
            // poll 一圈：坏对象优先（内核查表序），命中回全满足集。
            for (i, &h) in handles.iter().enumerate() {
                if self.dead.contains(&h) {
                    return Err(ERR_BADF);
                }
                let sat = self.ready.get(&h).copied().unwrap_or(0);
                if sat & signals[i] != 0 {
                    return Ok((i, sat));
                }
            }
            if deadline == 0 {
                return Err(ERR_EAGAIN);
            }
            // 阻塞段：可编程唤醒注入，否则睡到段 deadline。
            if let Some((h, bits)) = self.arm_wake.take() {
                *self.ready.entry(h).or_insert(0) |= bits;
                // 注入后重 poll 一圈（唤醒 → poll-first 命中即返）。
                for (i, &h2) in handles.iter().enumerate() {
                    let sat = self.ready.get(&h2).copied().unwrap_or(0);
                    if sat & signals[i] != 0 {
                        return Ok((i, sat));
                    }
                }
            }
            self.now = self.now.max(deadline);
            Err(ERR_TIMEDOUT)
        }

        fn now(&mut self) -> u64 {
            self.now
        }
    }

    const IN: u64 = 1 << 0; // 模拟 EPOLLIN ↔ SIGNAL_READABLE
    const OUT: u64 = 1 << 1;

    #[test]
    fn add_remove_roundtrip_and_slot_reuse() {
        let mut ps = PollSet::new(Fake::new());
        let a = ps.add(0x100, IN).unwrap();
        let b = ps.add(0x101, IN).unwrap();
        let c = ps.add(0x102, OUT).unwrap();
        assert_eq!((a, b, c), (0, 1, 2));
        assert_eq!(ps.len(), 3);
        assert!(ps.remove(0x101));
        assert!(!ps.remove(0x101)); // 双摘 false
        assert_eq!(ps.len(), 2);
        // freelist 复用：摘掉的槽 1 被下一个 add 复用（idx 稳定密集）。
        let d = ps.add(0x103, IN).unwrap();
        assert_eq!(d, 1);
    }

    #[test]
    fn add_rejections() {
        let mut ps = PollSet::new(Fake::new());
        assert_eq!(ps.add(0x100, 0), Err(AddError::ZeroMask));
        ps.add(0x100, IN).unwrap();
        assert_eq!(ps.add(0x100, OUT), Err(AddError::Duplicate));
        // 容量帽：填满 256 再加拒 Full；摘一个后可复入。
        for h in 0x200..0x200 + (POLL_SET_MAX - 1) as u32 {
            ps.add(h, IN).unwrap();
        }
        assert_eq!(ps.len(), POLL_SET_MAX);
        assert_eq!(ps.add(0x999, IN), Err(AddError::Full));
        assert!(ps.remove(0x200));
        assert!(ps.add(0x999, IN).is_ok());
    }

    #[test]
    fn wait_empty_set_zero_syscall() {
        let mut ps = PollSet::new(Fake::new());
        let out = ps.wait(50, 1000).unwrap();
        assert!(out.is_empty());
        // 假实现的 wait_many 从未被调（空集即返——纯函数零 syscall 面）。
        assert_eq!(ps.w.wait_many_calls, 0);
    }

    #[test]
    fn wait_poll_mode_harvest_to_dry() {
        // 非阻塞快照（timeout=0）：批内多对象分次收割到干 + 全满足集透传。
        let mut f = Fake::new();
        f.ready.insert(0x100, IN | OUT); // 满足集超出请求 mask（内核全满足集口径）
        f.ready.insert(0x102, IN);
        let mut ps = PollSet::new(f);
        ps.add(0x100, IN).unwrap();
        ps.add(0x101, IN).unwrap();
        ps.add(0x102, IN).unwrap();
        let out = ps.wait(0, 1000).unwrap();
        assert_eq!(out.len(), 2);
        let obs: HashMap<u32, u64> = out.iter().map(|e| (e.handle, e.observed)).collect();
        assert_eq!(obs[&0x100], IN | OUT); // 超集透传非请求子集
        assert_eq!(obs[&0x102], IN);
        // 0x101 未就绪不在集。
    }

    #[test]
    fn wait_poll_mode_idle_batch_count() {
        // 全空非阻塞：每批恰一次 poll（16 路 = 1 次 syscall），零阻塞调用。
        let mut ps = PollSet::new(Fake::new());
        for i in 0..16u32 {
            ps.add(0x100 + i, IN).unwrap();
        }
        let out = ps.wait(0, 1000).unwrap();
        assert!(out.is_empty());
        assert_eq!(ps.w.wait_many_calls, 1);
        assert_eq!(ps.w.deadlines, vec![0]); // 纯 poll：deadline 恒 0
    }

    #[test]
    fn batching_over_16_cross_batch_discovery() {
        // N=20 → 批切分 16+4：第二批的命中在 poll 轮被发现（跨批收割）。
        let mut f = Fake::new();
        f.ready.insert(0x113, IN); // 第 20 个对象（第二批）
        let mut ps = PollSet::new(f);
        for i in 0..20u32 {
            ps.add(0x100 + i, IN).unwrap();
        }
        let out = ps.wait(0, 1000).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].handle, 0x113);
        assert_eq!(out[0].slot, 19);
        // 批 0 空（1 次）+ 批 1 收割到干（1 次命中 + 1 次 EAGAIN）= 3 次。
        assert_eq!(ps.w.wait_many_calls, 3);
    }

    #[test]
    fn blocking_slice_caps_and_timeout() {
        // 全空阻塞：段帽 = slice，到总 deadline 返回空集（超时 = epoll 0 形态）。
        let mut ps = PollSet::with_slice(Fake::new(), 50);
        for i in 0..4u32 {
            ps.add(0x100 + i, IN).unwrap();
        }
        let out = ps.wait(120, 1000).unwrap();
        assert!(out.is_empty());
        let f = &ps.w;
        // 段序列：1000+50=1050 → 1100 → 1120（min(剩余,50) 收口）→ 超时返。
        let blocks: Vec<u64> = f.deadlines.iter().copied().filter(|d| *d > 0).collect();
        assert_eq!(blocks, vec![1050, 1100, 1120]);
    }

    #[test]
    fn blocking_wake_returns_harvested() {
        // 批 0 睡中被置位唤醒（arm_wake 模拟外部置位）→ ① 重扫收割该对象。
        let mut f = Fake::new();
        f.arm_wake = Some((0x100, IN));
        let mut ps = PollSet::with_slice(f, 50);
        ps.add(0x100, IN).unwrap();
        ps.add(0x101, IN).unwrap();
        let out = ps.wait(1000, 1000).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].handle, 0x100);
    }

    #[test]
    fn blocking_wake_harvests_sibling_batch() {
        // 批 1 就绪在阻塞段激活后出现（唤醒 → ① 全批重扫跨批收割）：
        // Fake.arm_wake 注入批 0 唤醒位后，poll 轮批 1 的对象仍空——
        // 用 ready 双注入模拟「唤醒涟漪」（唤醒处理中兄弟对象也置位）。
        struct Fake2(Fake);
        impl WaitMany for Fake2 {
            fn wait_many(
                &mut self,
                h: &[u32],
                s: &[u64],
                d: u64,
            ) -> Result<(usize, u64), i32> {
                if d > 0 {
                    // 阻塞调用一次性注入两批的就绪（模拟外部事件连带置位）。
                    self.0.ready.insert(0x100, IN);
                    self.0.ready.insert(0x110, IN);
                }
                self.0.wait_many(h, s, d)
            }
            fn now(&mut self) -> u64 {
                self.0.now()
            }
        }
        let mut inner = Fake::new();
        inner.arm_wake = None; // 唤醒走外层注入
        let mut ps = PollSet::with_slice(Fake2(inner), 50);
        for i in 0..20u32 {
            ps.add(0x100 + i, IN).unwrap(); // 0x110 = 第 17 个（批 1）
        }
        let out = ps.wait(1000, 1000).unwrap();
        let handles: Vec<u32> = out.iter().map(|e| e.handle).collect();
        assert!(handles.contains(&0x100) && handles.contains(&0x110));
    }

    #[test]
    fn lt_semantics_repoll_reports_again() {
        // LT 免费：就绪态未消费（不清 ready）时下一轮 wait 再报同一对象。
        let mut f = Fake::new();
        f.ready.insert(0x100, IN);
        let mut ps = PollSet::new(f);
        ps.add(0x100, IN).unwrap();
        let o1 = ps.wait(0, 1000).unwrap();
        assert_eq!(o1.len(), 1);
        let o2 = ps.wait(0, 1000).unwrap(); // 未消费：同对象同位再报
        assert_eq!(o2.len(), 1);
        assert_eq!(o2[0].handle, 0x100);
    }

    #[test]
    fn dead_handle_error_propagates_when_no_harvest() {
        let mut f = Fake::new();
        f.dead.insert(0x100);
        let mut ps = PollSet::new(f);
        ps.add(0x100, IN).unwrap();
        ps.add(0x101, IN).unwrap();
        assert_eq!(ps.wait(0, 1000), Err(ERR_BADF)); // 无收割 → 原码传播
    }

    #[test]
    fn events_precede_error() {
        // 先收割后撞错：事件优先返回（错误下轮裸露——头注裁定）。
        let mut f = Fake::new();
        f.ready.insert(0x100, IN);
        f.dead.insert(0x101);
        let mut ps = PollSet::new(f);
        ps.add(0x100, IN).unwrap();
        ps.add(0x101, IN).unwrap();
        let out = ps.wait(0, 1000).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].handle, 0x100);
        // 下轮（0x100 仍未消费）再撞 0x101 的 EBADF——仍有收割则事件优先。
        let out2 = ps.wait(0, 1000).unwrap();
        assert_eq!(out2.len(), 1);
    }

    #[test]
    fn timeout_zero_is_pure_poll() {
        // timeout=0 全空：零阻塞调用（deadlines 全 0）。
        let mut ps = PollSet::new(Fake::new());
        ps.add(0x100, IN).unwrap();
        assert!(ps.wait(0, 1000).unwrap().is_empty());
        assert!(ps.w.deadlines.iter().all(|d| *d == 0));
    }
}
