//! ASID 分配器（`内存管理设计.md` §5.2，随 2b-2）。
//!
//! v1 轮转计数器：进程数个位数，冲突/回卷成本无关紧要。**ASID 0 保留**（= 无地址
//! 空间 / 内核空表，与 TTBR0 ASID 字段零值语义一致）。
//!
//! 架构无关约束：分配器不碰硬件——宽度（8/16 位）由调用方探测后注入；回卷信号由
//! 调用方消费并做全量失效（aarch64 上是 `tlbi vmalle1is`，见 HAL）。

/// ASID 分配器（轮转计数器）。
///
/// 单核顺序使用（与调度器同一临界区语境）；SMP（G2）后换分代位图。
pub struct AsidAllocator {
    /// ASID 位宽（8 或 16）。
    width: u32,
    /// 下一个待分配的 ASID。
    next: u16,
}

impl AsidAllocator {
    /// `width` 须为 8 或 16（aarch64 `ID_AA64MMFR0_EL1.ASIDBits` 探测后传入；
    /// 0 → 8 位、2 → 16 位，见 §5.2）。
    pub fn new(width: u32) -> Self {
        assert!(width == 8 || width == 16, "ASID 宽度须为 8 或 16：{width}");
        Self { width, next: 1 }
    }

    /// 分配一个 ASID。返回 `(asid, rolled)`：
    /// - `rolled == false`：常规递增；
    /// - `rolled == true`：计数器在本轮末尾回卷——调用方须**全量失效**
    ///   （`tlbi vmalle1is`）再启用新值，防上一轮同号映射经 TLB 复活。
    ///
    /// 回卷语义：信号随本轮**最后一个** ASID 发出，其后的全量失效覆盖整个新轮次。
    /// 全量失效是**兜底**——日常真正拦住同号残留的，是激活路径每次切换 TTBR0 后的
    /// 无条件按号失效（HAL `activate_aspace` → `tlbi aside1is`）。
    pub fn alloc(&mut self) -> (u16, bool) {
        let a = self.next;
        let max: u16 = if self.width == 8 { 0xff } else { 0xffff };
        if self.next >= max {
            self.next = 1;
            (a, true)
        } else {
            self.next += 1;
            (a, false)
        }
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
        // 快进到 next=255（已发 1、2；循环补 3..=254），最后一个 255 应带回卷信号。
        for _ in 2..254 {
            let (_, rolled) = a.alloc();
            assert!(!rolled);
        }
        assert_eq!(a.alloc(), (255, true));
        // 新轮次从 1 重新开始、无信号（旧号已随上一次全量失效清空）。
        assert_eq!(a.alloc(), (1, false));
    }

    #[test]
    fn width16_never_hits_zero_or_reserved() {
        let mut a = AsidAllocator::new(16);
        let (first, _) = a.alloc();
        assert_ne!(first, 0, "ASID 0 保留");
        assert_eq!(a.alloc(), (2, false));
    }

    #[test]
    #[should_panic]
    fn rejects_bad_width() {
        let _ = AsidAllocator::new(13);
    }
}
