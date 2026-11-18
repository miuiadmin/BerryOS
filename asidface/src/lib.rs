//! `berry-asidface` —— 分代位图与在跑核钉死 ASID 分配器纯逻辑面（F0-2）。
//!
//! 解决的核心缺陷：
//! 1. 旧 `AsidAllocator` 仅维护裸递增计数器，回绕时盲目将仍存活常驻进程（如 `svcmgr` 的
//!    `ASID=1`）重发给新子进程，导致两存活进程共享同一硬件 ASID、TLB 互相串表；
//! 2. 旧 `mmu.rs` 未按 `ID_AA64MMFR0_EL1.ASIDBits` 置 `TCR_EL1.AS`（bit 36），且把
//!    8 位 ASID 错移 `<< 56`（写进 `TTBR0_EL1[63:56]` RES0 域，而架构 8 位与 16 位
//!    ASID 均始于 bit 48）。
//!
//! 本 crate 零 `unsafe`、零堆分配，提供：
//! - [`ASID_SHIFT`]（恒 `48`）与 [`tcr_as_config`]（`ID_AA64MMFR0_EL1` → `(width, tcr_as_bit)`）；
//! - [`AsidState`]：1024 个 `u64` 位图（覆盖 `0..=65535`，`bit 0` 永久保留给内核空表）+
//!   单调游标 + 回卷代号 + 在跑核活跃 ASID 钉死掩码。

#![cfg_attr(not(test), no_std)]

/// ARMv8-A `TTBR0_EL1` 与 `TLBI ASIDE1*` 的 ASID 起始位恒为 bit 48
/// （8 位模式用 `[55:48]`，16 位模式 `TCR_EL1.AS=1` 用 `[63:48]`）。
pub const ASID_SHIFT: u32 = 48;

/// `TCR_EL1.AS`（bit 36）：置 `1` 启用 16 位硬件 ASID（须 `ID_AA64MMFR0_EL1.ASIDBits == 2`）。
pub const TCR_AS_BIT: u64 = 1u64 << 36;

/// 16 位 ASID 位图字数（`1024 * 64 = 65536` 位 = 8KiB）。
pub const BITMAP_WORDS_16: usize = 1024;

/// 8 位 ASID 位图有效字数（`4 * 64 = 256` 位）。
pub const BITMAP_WORDS_8: usize = 4;

/// 解码 `ID_AA64MMFR0_EL1`（bits[7:4] `ASIDBits`）：
/// - `0b0010` → `(16, TCR_AS_BIT)`；
/// - 其余（`0b0000` 8 位或保留值）→ `(8, 0)`。
#[inline(always)]
pub const fn tcr_as_config(mmfr0: u64) -> (u32, u64) {
    if ((mmfr0 >> 4) & 0xF) == 2 {
        (16, TCR_AS_BIT)
    } else {
        (8, 0)
    }
}

/// 分代位图 ASID 状态机。
pub struct AsidState {
    width: u32,
    cursor: u16,
    generation: u64,
    active: usize,
    bitmap: [u64; BITMAP_WORDS_16],
}

impl AsidState {
    /// 构造 `width`（仅接受 `8` 或 `16`）位宽的分配器；`ASID 0` 永久置位保留。
    pub const fn new(width: u32) -> Option<Self> {
        if width != 8 && width != 16 {
            return None;
        }
        let mut bitmap = [0u64; BITMAP_WORDS_16];
        bitmap[0] = 1; // bit 0 = ASID 0 永久保留给内核空表
        Some(Self {
            width,
            cursor: 1,
            generation: 1,
            active: 0,
            bitmap,
        })
    }

    /// 硬件位宽（`8` 或 `16`）。
    #[inline(always)]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// 最大可用 ASID（8 位为 `255`，16 位为 `65535`）。
    #[inline(always)]
    pub const fn max_asid(&self) -> u16 {
        if self.width == 8 { 0x00FF } else { 0xFFFF }
    }

    /// 当前回卷代号（初始 `1`，每次回卷递增 `1`）。
    #[inline(always)]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// 当前已分配且未释放的非零 ASID 数量。
    #[inline(always)]
    pub const fn active_count(&self) -> usize {
        self.active
    }

    /// 查询 `asid` 是否处于已分配状态（`0` 恒视作保留已占）。
    #[inline]
    pub const fn is_allocated(&self, asid: u16) -> bool {
        if asid > self.max_asid() {
            return false;
        }
        let w = (asid as usize) >> 6;
        let b = (asid as usize) & 63;
        ((self.bitmap[w] >> b) & 1) != 0
    }

    #[inline(always)]
    const fn word_count(&self) -> usize {
        if self.width == 8 {
            BITMAP_WORDS_8
        } else {
            BITMAP_WORDS_16
        }
    }

    #[inline]
    fn pin_mask_for_word(word_idx: usize, pinned_on_cpus: &[u16]) -> u64 {
        let mut m = 0u64;
        let mut i = 0;
        while i < pinned_on_cpus.len() {
            let p = pinned_on_cpus[i] as usize;
            if (p >> 6) == word_idx {
                m |= 1u64 << (p & 63);
            }
            i += 1;
        }
        m
    }

    /// 在闭区间 `[start_id, end_id]` 内扫描首个既未分配也未被任一 CPU 钉死的 ASID。
    fn find_free_in(&self, start_id: u16, end_id: u16, pinned_on_cpus: &[u16]) -> Option<u16> {
        if start_id > end_id {
            return None;
        }
        let start_w = (start_id as usize) >> 6;
        let end_w = (end_id as usize) >> 6;
        let words = self.word_count();
        if start_w >= words {
            return None;
        }
        let last_w = if end_w < words { end_w } else { words - 1 };
        let mut w = start_w;
        while w <= last_w {
            let mut occ = self.bitmap[w] | Self::pin_mask_for_word(w, pinned_on_cpus);
            if w == start_w {
                let low_bits = (start_id as usize) & 63;
                if low_bits > 0 {
                    occ |= (1u64 << low_bits) - 1;
                }
            }
            if w == last_w {
                let high_bit = (end_id as usize) & 63;
                if high_bit < 63 {
                    occ |= !((1u64 << (high_bit + 1)) - 1);
                }
            }
            if occ != u64::MAX {
                let bit = (!occ).trailing_zeros() as usize;
                let id = (w << 6) | bit;
                if id <= end_id as usize {
                    return Some(id as u16);
                }
            }
            w += 1;
        }
        None
    }

    /// 分配一个空闲且未在任何核激活钉死的 ASID，返回 `Some((asid, rolled))`：
    /// - 优先在 `[cursor..=max_asid]` 线性推进：若命中末尾 `max_asid`，游标归 `1` 并触发
    ///   `rolled = true`（递增 `generation`），与存量末号回卷契约兼容；
    /// - 若前向区间全满，回卷扫描 `[1..cursor)`：命中则置 `rolled = true`（递增 `generation`）
    ///   并推进游标至 `id + 1`；
    /// - 若全范围 `[1..=max_asid]` 均在用或被钉死，返回 `None`（绝不重复发放活跃号）。
    pub fn alloc(&mut self, pinned_on_cpus: &[u16]) -> Option<(u16, bool)> {
        let max = self.max_asid();
        if let Some(id) = self.find_free_in(self.cursor, max, pinned_on_cpus) {
            let w = (id as usize) >> 6;
            let b = (id as usize) & 63;
            self.bitmap[w] |= 1u64 << b;
            self.active += 1;
            if id == max {
                self.cursor = 1;
                self.generation = self.generation.wrapping_add(1);
                return Some((id, true));
            }
            self.cursor = id + 1;
            return Some((id, false));
        }
        if self.cursor > 1 {
            if let Some(id) = self.find_free_in(1, self.cursor - 1, pinned_on_cpus) {
                let w = (id as usize) >> 6;
                let b = (id as usize) & 63;
                self.bitmap[w] |= 1u64 << b;
                self.active += 1;
                self.cursor = if id == max { 1 } else { id + 1 };
                self.generation = self.generation.wrapping_add(1);
                return Some((id, true));
            }
        }
        None
    }

    /// 归还已销毁地址空间的 ASID（`0`、越界或重复释放均为安全空操作；不回退 `cursor`，
    /// 使刚释放的号推迟到下一代全量 TLB 失效后再被复用）。
    pub fn free(&mut self, asid: u16) {
        if asid == 0 || asid > self.max_asid() {
            return;
        }
        let w = (asid as usize) >> 6;
        let b = (asid as usize) & 63;
        let mask = 1u64 << b;
        if (self.bitmap[w] & mask) != 0 {
            self.bitmap[w] &= !mask;
            self.active = self.active.saturating_sub(1);
        }
    }

    /// 测试快进注入（`smoke-smp` `asidroll` 门专用）：将游标推至 `max_asid`，
    /// 使下一次 [`alloc`](Self::alloc) 必定触发真实回卷信号（无论 `max_asid` 空闲或已占）。
    pub fn force_near_roll(&mut self) {
        self.cursor = self.max_asid();
    }
}
