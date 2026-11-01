//! 内核侧**页帧分配器**：在 [`BuddyAllocator`] 之上包一层绝对页帧号（PFN）语义 +
//! 平台 RAM 几何，提供 `alloc/free` 整页接口。
//!
//! buddy 核心操作「区域内下标」；本层把下标翻译成全系统 PFN（`base_pfn + 下标`）。
//! V-1b 起构造口径 = **全窗** `[base_pa, top_pa)`（不再只管 kernel_end 之上），
//! 占用区经 [`FrameAllocator::reserve`] 显式剔除——QEMU 路与 EFI memmap 路统一
//! 到同一形态（等价性对拍见 buddy 测试 `reserve_prefix_equivalence_*`）。
//!
//! 仍不含全局状态：后备切片由调用方（内核 `mm` 模块）借入的静态 BSS 数组提供。

use crate::buddy::{AllocError, BuddyAllocator};
use crate::va::PAGE_SHIFT;

/// 页帧分配器。
pub struct FrameAllocator<'a> {
    /// 可分配区间的起始 PFN（= `base_pa >> PAGE_SHIFT`；V-1b 起通常 = RAM 首页）。
    base_pfn: usize,
    /// 可分配页数。
    npages: usize,
    buddy: BuddyAllocator<'a>,
}

impl<'a> FrameAllocator<'a> {
    /// 构造并把 `[base_pa, top_pa)` **整窗**初始化为空闲（构造后立即用
    /// [`reserve`](Self::reserve) 剔除占用区——先构造后 reserve，不预留）。
    ///
    /// `base_pa` / `top_pa` 必须 4KiB 对齐；后备切片长度须 `>= npages`
    ///（free/order）/ `>= (MAX_ORDER+1) × ceil(npages/64)`（bmaps）/
    /// `>= (MAX_ORDER+1) × ceil(ceil(npages/64)/64)`（sums）——K-1 起位图
    /// 索引后备（几何语义见 `BuddyAllocator` 字段注释）。
    pub fn new(
        base_pa: usize,
        top_pa: usize,
        free: &'a mut [bool],
        order: &'a mut [u8],
        bmaps: &'a mut [u64],
        sums: &'a mut [u64],
    ) -> Self {
        assert!(
            base_pa & (1usize << PAGE_SHIFT) - 1 == 0,
            "base_pa 必须 4KiB 对齐"
        );
        assert!(top_pa >= base_pa, "top_pa < base_pa");
        let base_pfn = base_pa >> PAGE_SHIFT;
        let top_pfn = top_pa >> PAGE_SHIFT;
        let npages = top_pfn - base_pfn;
        assert!(
            free.len() >= npages && order.len() >= npages,
            "后备切片装不下 npages={npages}"
        );
        let buddy = BuddyAllocator::new(&mut free[..npages], &mut order[..npages], bmaps, sums);
        Self {
            base_pfn,
            npages,
            buddy,
        }
    }

    /// Boot 期保留物理区间 `[pa, pa+len)`：与窗口相交部分整页剔除（V-1b）。
    ///
    /// - 区间越窗口顶/底自动截断（`[max(pa,base), min(pa+len, top))`）；
    /// - 非整页尾向上取整（保留宁多勿少——漏保护半页 = 数据腐化）；
    /// - 返回**实际保留页数**（用于 `MMAP ok` 标记的累计）。
    ///
    /// 契约同 [`BuddyAllocator::reserve_range`]：boot 单线程、构造后立即调用。
    pub fn reserve(&mut self, pa: usize, len: usize) -> usize {
        let base = self.base_pfn << PAGE_SHIFT;
        let top = (self.base_pfn + self.npages) << PAGE_SHIFT;
        // checked：敌意/畸形区间不得回卷成巨窗。
        let end = pa.saturating_add(len);
        let lo = pa.max(base);
        let hi = end.min(top);
        if lo >= hi {
            return 0;
        }
        // 尾页向上取整（ceil），首页已有 4KiB 对齐保证则不取整也不亏。
        let first = lo >> PAGE_SHIFT;
        let last = (hi + PAGE_SHIFT_MASK) >> PAGE_SHIFT; // 半开区间末页号（ceil）
        let n = last - first;
        self.buddy.reserve_range(first - self.base_pfn, last - self.base_pfn);
        n
    }

    /// Boot 期回收保留区间 `[pa, pa+len)`（A1-3b，[`reserve`] 的逆）：
    /// 与窗口相交部分中**此前由 reserve 剔除的页**归还空闲池——普通
    /// 已分配/已空闲页跳过（幂等）。返回**实际归还页数**。
    ///
    /// 契约：boot 单线程；区间归属由调用方保证（误回收他源保留区 =
    /// 内存腐化）。典型用例：handoff v1 init 段页在 VMO 拼装完成后
    /// 归还——段内容双份（EfiLoaderData 段 + VMO 页）的地形死结出口。
    pub fn release(&mut self, pa: usize, len: usize) -> usize {
        let base = self.base_pfn << PAGE_SHIFT;
        let top = (self.base_pfn + self.npages) << PAGE_SHIFT;
        // checked：与 reserve 同款防回卷。
        let end = pa.saturating_add(len);
        let lo = pa.max(base);
        let hi = end.min(top);
        if lo >= hi {
            return 0;
        }
        let first = lo >> PAGE_SHIFT;
        let last = (hi + PAGE_SHIFT_MASK) >> PAGE_SHIFT;
        self.buddy
            .release_range(first - self.base_pfn, last - self.base_pfn)
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

    /// 已分配页总数（透传 [`BuddyAllocator::allocated_pages`]——B-0 泄漏
    /// 金丝雀的对账口径：churn 前后必须相等，确定性严格断言；D24 起 O(1)
    /// 水位读，口径由 buddy 对拍测试钉板）。
    pub fn allocated_pages(&self) -> usize {
        self.buddy.allocated_pages()
    }
}

/// 页大小掩码（`PAGE_SIZE - 1`）——ceil 除法用。
const PAGE_SHIFT_MASK: usize = (1usize << PAGE_SHIFT) - 1;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buddy::MAX_ORDER;

    fn backing(n: usize) -> (&'static mut [bool], &'static mut [u8], &'static mut [u64], &'static mut [u64]) {
        let bw = (n + 63) / 64;
        let sw = (bw + 63) / 64;
        (
            Box::leak(vec![false; n].into_boxed_slice()),
            Box::leak(vec![0u8; n].into_boxed_slice()),
            Box::leak(vec![0u64; (MAX_ORDER + 1) * bw].into_boxed_slice()),
            Box::leak(vec![0u64; (MAX_ORDER + 1) * sw].into_boxed_slice()),
        )
    }

    const KIB4: usize = 1 << 12;

    #[test]
    fn reserve_translates_pfn_and_clamps() {
        // 窗 [0x1000_0000, 0x1000_0000 + 64*4K)：保留中段 [base+8K, base+20K+2B)
        // → 页 8..20（尾 2 字节进位占 1 页）= 13 页；越窗腿截断。
        let base = 0x1000_0000;
        let (f, o, bm, sm) = backing(64);
        let mut fa = FrameAllocator::new(base, base + 64 * KIB4, f, o, bm, sm);
        let n = fa.reserve(base + 8 * KIB4, 12 * KIB4 + 2);
        assert_eq!(n, 13);
        // 全窗外保留 = 0 页；半交叠只留窗内部分。
        assert_eq!(fa.reserve(0, base), 0); // [0, base) 全在窗下
        assert_eq!(fa.reserve(base + 60 * KIB4, 1 << 40), 4); // 越顶截到 64
        // 占尽：恰 64-13-4 = 47 页，保留页永不出现。
        let mut got = Vec::new();
        while let Ok(p) = fa.alloc() {
            got.push(p);
        }
        assert_eq!(got.len(), 47);
        for p in got.iter() {
            let idx = p - (base >> 12);
            assert!(idx < 8 || idx >= 21, "保留页 {idx} 被分配");
            assert!(idx < 60, "越顶保留页 {idx} 被分配");
        }
    }

    #[test]
    fn reserve_saturate_no_wraparound() {
        // 敌意巨长：saturating_add 不回卷成巨窗——保留 [base, top) 全窗后 OOM。
        let base = 0x4000_0000;
        let (f, o, bm, sm) = backing(16);
        let mut fa = FrameAllocator::new(base, base + 16 * KIB4, f, o, bm, sm);
        let n = fa.reserve(0, usize::MAX);
        assert_eq!(n, 16);
        assert_eq!(fa.alloc(), Err(AllocError::OutOfMemory));
    }

    #[test]
    fn release_translates_pfn_clamps_and_counts() {
        // reserve 的逆（A1-3b）：保留 [base+8K, +20K) 13 页 → release 同
        // 区间归还恰 13 页、全窗 64 页复可分配；越窗腿截断 + 幂等 0。
        let base = 0x1000_0000;
        let (f, o, bm, sm) = backing(64);
        let mut fa = FrameAllocator::new(base, base + 64 * KIB4, f, o, bm, sm);
        assert_eq!(fa.reserve(base + 8 * KIB4, 12 * KIB4), 12);
        assert_eq!(fa.reserve(base + 20 * KIB4, 2), 1); // 尾 2 字节进位占 1 页
        assert_eq!(fa.release(base + 8 * KIB4, 12 * KIB4 + 2), 13);
        assert_eq!(fa.release(base + 8 * KIB4, 12 * KIB4 + 2), 0, "幂等");
        let mut got = Vec::new();
        while let Ok(p) = fa.alloc() {
            got.push(p);
        }
        assert_eq!(got.len(), 64, "回收后整窗全可分配");
    }

    #[test]
    fn allocated_pages_accounts_and_returns_to_zero() {
        // B-0 泄漏金丝雀口径钉板：按块头 2^order 累计（尾页 TAIL 不重复计）、
        // 保留页恒不计、全释放归零——churn 对账的「相等」有语义。
        let base = 0x8000_0000;
        let (f, o, bm, sm) = backing(64);
        let mut fa = FrameAllocator::new(base, base + 64 * KIB4, f, o, bm, sm);
        assert_eq!(fa.allocated_pages(), 0, "空窗应为 0");
        fa.reserve(base + 4 * KIB4, 2 * KIB4); // 保留 2 页——不计入
        assert_eq!(fa.allocated_pages(), 0, "保留页不计入 allocated");
        let a = fa.alloc().unwrap(); // 1 页
        let b = fa.alloc_order(2).unwrap(); // 4 页连续块
        let c = fa.alloc().unwrap();
        assert_eq!(fa.allocated_pages(), 6, "1 + 4 + 1");
        fa.free(a, 0).unwrap();
        fa.free(b, 2).unwrap();
        fa.free(c, 0).unwrap();
        assert_eq!(fa.allocated_pages(), 0, "全释放归零（泄漏金丝雀判据）");
    }
}
