//! ASID 分配器（F0-2 升级：基于 `berry-asidface` 的分代位图与在跑核活跃号钉死）。
//!
//! - **ASID 0 永久保留**给无地址空间 / 内核空表；
//! - **存活不撞号**：位图精确跟踪全部在用 ASID，回卷时自动跳过仍存活常驻进程与在跑核钉死号；
//! - **耗尽诚实返 `None`**：全部硬件可用号在用时 [`AsidAllocator::try_alloc`] 返回 `None`，
//!   供 `sys_process_create` 折返 `-ENOMEM`，严禁重复发放活跃号击穿 TLB 隔离。

use berry_asidface::AsidState;

/// 分代位图 ASID 分配器。
pub struct AsidAllocator {
    inner: AsidState,
}

impl AsidAllocator {
    /// `width` 须为 `8` 或 `16`（由硬件 `ID_AA64MMFR0_EL1.ASIDBits` 探测传入）。
    pub fn new(width: u32) -> Self {
        Self {
            inner: AsidState::new(width)
                .unwrap_or_else(|| panic!("ASID 宽度须为 8 或 16：{width}")),
        }
    }

    /// 尝试分配一个未占用且不在 `pinned_on_cpus` 内的 ASID，返回 `Some((asid, rolled))`；
    /// 全范围满载时返回 `None`。
    pub fn try_alloc(&mut self, pinned_on_cpus: &[u16]) -> Option<(u16, bool)> {
        self.inner.alloc(pinned_on_cpus)
    }

    /// 无钉死参数便捷分配（boot / 单测可信语境；耗尽即 panic）。
    pub fn alloc(&mut self) -> (u16, bool) {
        self.inner.alloc(&[]).expect("AsidAllocator::alloc: ASID 耗尽")
    }

    /// 释放已拆毁地址空间的 ASID（`0` 或重复释放为空操作）。
    pub fn free(&mut self, asid: u16) {
        self.inner.free(asid);
    }

    /// 当前已分配非零 ASID 计数。
    pub fn active_count(&self) -> usize {
        self.inner.active_count()
    }

    /// 当前回卷代号。
    pub fn generation(&self) -> u64 {
        self.inner.generation()
    }

    /// 测试快进注入（`smoke-smp` `asidroll` 门专用）。
    pub fn force_near_roll(&mut self) {
        self.inner.force_near_roll();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width8_sequential_and_rollover() {
        let mut a = AsidAllocator::new(8);
        assert_eq!(a.alloc(), (1, false));
        assert_eq!(a.alloc(), (2, false));
        a.free(1);
        for _ in 2..254 {
            let (_, rolled) = a.alloc();
            assert!(!rolled);
        }
        assert_eq!(a.alloc(), (255, true));
        // 新轮次仅复用已释放的 1 号，绝不撞仍存活的 2 号！
        assert_eq!(a.alloc(), (1, false));
        assert_eq!(a.try_alloc(&[]), None);
    }

    #[test]
    fn width16_never_hits_zero_or_reserved() {
        let mut a = AsidAllocator::new(16);
        let (first, _) = a.alloc();
        assert_ne!(first, 0, "ASID 0 保留");
        assert_eq!(a.alloc(), (2, false));
    }

    #[test]
    fn force_near_roll_makes_next_alloc_roll() {
        let mut a = AsidAllocator::new(16);
        a.force_near_roll();
        assert_eq!(a.alloc(), (0xffff, true));
        assert_eq!(a.alloc(), (1, false));
    }

    #[test]
    #[should_panic]
    fn rejects_bad_width() {
        let _ = AsidAllocator::new(13);
    }
}
