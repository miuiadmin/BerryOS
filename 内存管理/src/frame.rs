//! 内核侧**页帧分配器**：在 [`BuddyAllocator`] 之上包一层绝对页帧号（PFN）语义 +
//! 平台 RAM 几何，提供 `alloc/free` 整页接口。
//!
//! buddy 核心操作「区域内下标」；本层把下标翻译成全系统 PFN（`base_pfn + 下标`），
//! 并把可分配区间限定在 `[kernel_end, RAM_TOP)`（内核镜像之上的空闲物理 RAM）。
//!
//! 仍不含全局状态：后备切片由调用方（内核 `mm` 模块）借入的静态 BSS 数组提供。

use crate::buddy::{AllocError, BuddyAllocator};
use crate::va::PAGE_SHIFT;

/// 页帧分配器。
pub struct FrameAllocator<'a> {
    /// 可分配区间的起始 PFN（= `kernel_end >> PAGE_SHIFT`）。
    base_pfn: usize,
    /// 可分配页数。
    npages: usize,
    buddy: BuddyAllocator<'a>,
}

impl<'a> FrameAllocator<'a> {
    /// 构造并把 `[kernel_end_pa, ram_top_pa)` 初始化为空闲。
    ///
    /// `kernel_end_pa` / `ram_top_pa` 必须 4KiB 对齐；后备切片长度须 `>= npages`。
    pub fn new(
        kernel_end_pa: usize,
        ram_top_pa: usize,
        free: &'a mut [bool],
        order: &'a mut [u8],
    ) -> Self {
        assert!(
            kernel_end_pa & (1usize << PAGE_SHIFT) - 1 == 0,
            "kernel_end 必须 4KiB 对齐"
        );
        assert!(ram_top_pa >= kernel_end_pa, "ram_top < kernel_end");
        let base_pfn = kernel_end_pa >> PAGE_SHIFT;
        let ram_top_pfn = ram_top_pa >> PAGE_SHIFT;
        let npages = ram_top_pfn - base_pfn;
        assert!(
            free.len() >= npages && order.len() >= npages,
            "后备切片装不下 npages={npages}"
        );
        let buddy = BuddyAllocator::new(&mut free[..npages], &mut order[..npages]);
        Self {
            base_pfn,
            npages,
            buddy,
        }
    }

    /// 分配一页，返回其 PFN。
    pub fn alloc(&mut self) -> Result<usize, AllocError> {
        self.buddy.alloc().map(|i| self.base_pfn + i)
    }

    /// 分配 `2^order` 页的连续块，返回其头页 PFN。
    pub fn alloc_order(&mut self, order: usize) -> Result<usize, AllocError> {
        self.buddy
            .alloc_order(order)
            .map(|i| self.base_pfn + i)
    }

    /// 释放头页 `pfn`、阶 `order` 的块。
    pub fn free(&mut self, pfn: usize, order: usize) -> Result<(), AllocError> {
        if pfn < self.base_pfn || pfn >= self.base_pfn + self.npages {
            return Err(AllocError::OutOfRange);
        }
        self.buddy.free(pfn - self.base_pfn, order)
    }

    /// 可分配页数。
    pub fn npages(&self) -> usize {
        self.npages
    }
}
