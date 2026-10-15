//! 匿名 VMO（内存对象，`内存管理设计.md` §2；2b-3 物理页后备先行）。
//!
//! v1 范围（ADR #20 无 demand paging）：`create/commit/read/write`——commit
//! **立即**从帧分配器出页并清零（启动时全映射）；`decommit`（3-2）归还已提交
//! 且**未被映射**的物理页；`share/CoW/pager` 与 5 类后备全集（ADR #29）留
//! Phase 3+，pager 接口留空。
//!
//! 放置说明（与 ADR #29「直接落 `内存管理/`」的偏差，已记设计文档落地记录）：
//! VMO 是内核对象（Ziron 务实派，ADR #4），页记账 `Vec` 需堆——落 `内核/`
//! 对象层；物理页仍走 bos-mm `FrameAllocator`，bos-mm 保持 no-alloc/no-HAL
//! 纯度（host 可测核心不因对象壳失守）。5 类后备/PageInfo 重构时再评估下沉。
//!
//! 全 API 返回 `Result`（syscall 边界无 panic 纪律——Phase 3 直接折状态码）。

use bos_mm::frame::FrameAllocator;
use bos_mm::va::{phys_to_virt, PAGE_SHIFT, PAGE_SIZE};

/// VMO 错误。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum VmoError {
    /// offset/len 越界或加法溢出。
    OutOfRange,
    /// offset/len 未页对齐（v1 整页粒度，与 Zircon 一致）。
    Unaligned,
    /// 读/写的页尚未 commit（v1 无 demand paging：映射前须显式提交）。
    NotCommitted,
    /// 帧分配器无空闲页。
    NoFrames,
    /// 区间内仍有页被映射（decommit 前须先 unmap——归还仍被映射的页 = PA 复用
    /// 后经残留 PTE 静默写坏别人，3-2 映射计数防线）。
    Mapped,
}

/// 匿名 VMO：`size` 字节、按页提交的物理页后备（§2 后备类①「物理页（匿名）」）。
pub struct Vmo {
    /// 容量（字节，向上取整到页）。
    size: usize,
    /// 每页后备 PA；`None` = 未提交。索引 = offset / PAGE_SIZE。
    pages: alloc::vec::Vec<Option<usize>>,
    /// 每页映射计数（vmar `map` +1 / `unmap` -1，3-2）。**悬空 PTE 防线**：
    /// 归还仍被映射的物理页 → PA 被 buddy 复用 → 残留 PTE 经旧 VA 静默写坏
    /// 新主人。本计数让 `decommit`/对象销毁拒绝（或泄漏）此类页。
    mapcnt: alloc::vec::Vec<u16>,
}

impl Vmo {
    /// 建空 VMO（不占物理页）。`size` 向上取整到页。对象层 API：非法 size 断言
    /// （Phase 3 syscall 化时折 Err）。
    pub fn create(size: usize) -> Self {
        assert!(size > 0, "VMO size 须 > 0");
        let npages = size.div_ceil(PAGE_SIZE);
        Self {
            size: npages * PAGE_SIZE,
            pages: alloc::vec![None; npages],
            mapcnt: alloc::vec![0; npages],
        }
    }

    /// 提交页区间：立即出页 + 清零。幂等（已提交页跳过）；整页粒度。
    pub fn commit(
        &mut self,
        fa: &mut FrameAllocator,
        offset: usize,
        len: usize,
    ) -> Result<(), VmoError> {
        if offset % PAGE_SIZE != 0 || len % PAGE_SIZE != 0 {
            return Err(VmoError::Unaligned);
        }
        check_bounds(self.size, offset, len)?;
        for page in offset / PAGE_SIZE..(offset + len) / PAGE_SIZE {
            if self.pages[page].is_none() {
                self.pages[page] = Some(zero_frame(fa)?);
            }
        }
        Ok(())
    }

    /// 归还已提交页（3-2）：先整段验「全已提交且**无映射**」再逐页释放（先查后
    /// 动——不留半截副作用）。映射中的页拒绝（[`VmoError::Mapped`]）——先 unmap。
    pub fn decommit(
        &mut self,
        fa: &mut FrameAllocator,
        offset: usize,
        len: usize,
    ) -> Result<(), VmoError> {
        if offset % PAGE_SIZE != 0 || len % PAGE_SIZE != 0 {
            return Err(VmoError::Unaligned);
        }
        check_bounds(self.size, offset, len)?;
        let pages = offset / PAGE_SIZE..(offset + len) / PAGE_SIZE;
        for page in pages.clone() {
            if self.pages[page].is_none() {
                return Err(VmoError::NotCommitted);
            }
            if self.mapcnt[page] > 0 {
                return Err(VmoError::Mapped);
            }
        }
        for page in pages {
            let pa = self.pages[page].take().expect("上已校验全提交");
            // SAFETY: 该页独占（mapcnt==0 → 无映射别名）且 PA 来自 alloc。
            let _ = fa.free(pa >> PAGE_SHIFT, 0); // 元数据异常只泄漏不 panic
        }
        Ok(())
    }

    /// 经内核 direct map 读已提交区（字节粒度；先查后拷——不留半截副作用）。
    pub fn read(&self, offset: usize, out: &mut [u8]) -> Result<(), VmoError> {
        check_bounds(self.size, offset, out.len())?;
        self.ensure_committed(offset, out.len())?;
        let mut pos = offset;
        while pos < offset + out.len() {
            let (pa, off, n) = self.page_span(pos, (offset + out.len()) - pos);
            // SAFETY: 独占物理页经 direct map 只读；页已提交（ensure_committed 过）。
            unsafe {
                core::ptr::copy_nonoverlapping(
                    phys_to_virt(pa + off) as *const u8,
                    out.as_mut_ptr().add(pos - offset),
                    n,
                );
            }
            pos += n;
        }
        Ok(())
    }

    /// 经内核 direct map 写已提交区（字节粒度；同上，先查后拷）。
    pub fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), VmoError> {
        check_bounds(self.size, offset, data.len())?;
        self.ensure_committed(offset, data.len())?;
        let mut pos = offset;
        while pos < offset + data.len() {
            let (pa, off, n) = self.page_span(pos, (offset + data.len()) - pos);
            // SAFETY: 独占物理页经 direct map 写；页已提交。
            unsafe {
                core::ptr::copy_nonoverlapping(
                    data.as_ptr().add(pos - offset),
                    phys_to_virt(pa + off) as *mut u8,
                    n,
                );
            }
            pos += n;
        }
        Ok(())
    }

    /// 容量（字节，已页对齐）。
    pub fn size(&self) -> usize {
        self.size
    }

    /// 第 `page_idx` 页的后备 PA（未提交 → `None`）。Vmar map 逐页取用。
    pub fn page_pa(&self, page_idx: usize) -> Option<usize> {
        self.pages.get(page_idx).copied().flatten()
    }

    // ---- 映射计数（vmar map/unmap 回调；悬空 PTE 防线，见结构体注释）----

    /// 区间映射计数 +1（vmar `map` 成功后回调）。饱和加：计数是防线不是账本，
    /// 溢出方向的保守错误（多泄漏一页）可接受。
    pub(crate) fn track_map(&mut self, offset: usize, len: usize) {
        for c in self.mapcnt[offset / PAGE_SIZE..(offset + len) / PAGE_SIZE].iter_mut() {
            *c = c.saturating_add(1);
        }
    }

    /// 区间映射计数 -1（vmar `unmap` 成功后回调，经 Region 存的对象指针）。
    pub(crate) fn untrack_map(&mut self, offset: usize, len: usize) {
        for c in self.mapcnt[offset / PAGE_SIZE..(offset + len) / PAGE_SIZE].iter_mut() {
            *c = c.saturating_sub(1);
        }
    }

    /// 任一页仍被映射？（handle close 拒绝面：映射未解除的 VMO 不许销毁。）
    pub fn any_mapped(&self) -> bool {
        self.mapcnt.iter().any(|&c| c > 0)
    }

    /// 校验 `[offset, offset+len)` 全部落已提交页（read/write 前置）。
    fn ensure_committed(&self, offset: usize, len: usize) -> Result<(), VmoError> {
        for page in offset / PAGE_SIZE..(offset + len).div_ceil(PAGE_SIZE) {
            if self.page_pa(page).is_none() {
                return Err(VmoError::NotCommitted);
            }
        }
        Ok(())
    }

    /// `pos` 所在页的 `(PA, 页内偏移, 本步最多拷贝字节)`。
    fn page_span(&self, pos: usize, remaining: usize) -> (usize, usize, usize) {
        let page = pos / PAGE_SIZE;
        let off = pos % PAGE_SIZE;
        let n = core::cmp::min(PAGE_SIZE - off, remaining);
        (self.pages[page].unwrap(), off, n)
    }
}

impl Drop for Vmo {
    /// 对象销毁回收物理页（补 3-0 裁剪项）：归还 mapcnt==0 的已提交页；**仍被
    /// 映射的页泄漏不还**（悬空 PTE 防线的最后一环）。handle 表路径到不了这里
    /// （close 对映射中 VMO 拒 -EBUSY）；本 Drop 只兜 boot/ktest 直构对象——
    /// 其映射常驻不解除（如用户进程 code/data/stack 页），泄漏即 v1 政策。
    fn drop(&mut self) {
        for (page, &backing) in self.pages.iter().enumerate() {
            if let Some(pa) = backing {
                if self.mapcnt[page] == 0 {
                    // SAFETY: 独占页（无映射别名）；PA 来自 FrameAllocator::alloc。
                    let _ = crate::mm::frames().free(pa >> PAGE_SHIFT, 0);
                }
            }
        }
    }
}

/// 越界 + 溢出校验（read/write/commit 共用；对齐检查各自做——commit 整页、
/// read/write 字节粒度）。
fn check_bounds(size: usize, offset: usize, len: usize) -> Result<(), VmoError> {
    match offset.checked_add(len) {
        Some(end) if end <= size => Ok(()),
        _ => Err(VmoError::OutOfRange),
    }
}

/// 从帧分配器取一页并清零，返回 **PA**（`alloc` 给 PFN 须左移；buddy 只管元数据，
/// 帧内容不保证干净）。vmo/vmar 共用（`pub(crate)`）。
pub(crate) fn zero_frame(fa: &mut FrameAllocator) -> Result<usize, VmoError> {
    let pfn = fa.alloc().map_err(|_| VmoError::NoFrames)?;
    let pa = pfn << PAGE_SHIFT;
    // SAFETY: 独占新分配帧（尚无别名引用）；Normal WB。
    unsafe { core::ptr::write_bytes(phys_to_virt(pa) as *mut u8, 0, PAGE_SIZE) };
    Ok(pa)
}
