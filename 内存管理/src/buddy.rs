//! 纯 Rust **buddy 页帧分配器**核心。
//!
//! 操作对象是**页号**（`PAGE_SIZE` 单位的下标），不碰物理内存本身。
//!
//! 设计要点（呼应 CLAUDE.md「小步提交 / 架构无关优先」+ 架构综合纲领 §3）：
//! - **不持有全局状态**：所有运行期状态由调用方借入的两片后备切片承载。
//! - **无 `unsafe`**：纯安全 Rust。
//! - **不依赖 `alloc`**：后备用 `&mut [bool]` / `&mut [u8]`，no_std 友好。
//!   host 测试用 `Vec` 切片；内核侧（Step 3）用静态 BSS 数组。
//!
//! 算法是教科书 buddy：每阶 `o` 维护「空闲块头页」集合；`alloc` 找最小够用阶、
//! 逐阶拆分；`free` 逐阶尝试与 buddy 合并。后备用线性扫描（boot / 低频分配可接受），
//! 运行时高频可换位图索引——留作 v2 优化，本阶段**正确性优先**。

/// buddy 最大阶。一个 `2^MAX_ORDER` 页的块。
/// 128MiB / 4KiB = 32768 页；`MAX_ORDER = 11` → 最大块 2048 页 = 8MiB。
pub const MAX_ORDER: usize = 11;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocError {
    /// 请求的阶超过 `MAX_ORDER`。
    OrderTooLarge,
    /// 没有足够大的连续空闲块。
    OutOfMemory,
    /// 释放的页号越界。
    OutOfRange,
    /// 该页已是空闲（双释放）或从未作为块头分配。
    DoubleFree,
    /// `free` 传入的 `order` 与该块分配时的 `order` 不符。
    OrderMismatch,
}

/// `order` 字段的「块内尾页 / 未用」哨兵。
const TAIL: u8 = 0xFF;

/// Buddy 页帧分配器（借后备切片，无所有权）。
pub struct BuddyAllocator<'a> {
    num_pages: usize,
    /// 每页：该页是否是一个**空闲块的头页**。
    free: &'a mut [bool],
    /// 每页：块阶信息。
    ///  - 空闲块头页 → 该空闲块的阶；
    ///  - 已分配块头页 → 分配时的阶；
    ///  - 块内尾页 → `TAIL`。
    order: &'a mut [u8],
}

impl<'a> BuddyAllocator<'a> {
    /// 用调用方提供的后备切片构造，并把整个区域 `[0, num_pages)` 初始化为空闲。
    ///
    /// `free.len()` 与 `order.len()` 必须相等（= 页数）。
    pub fn new(free: &'a mut [bool], order: &'a mut [u8]) -> Self {
        let num_pages = free.len();
        assert_eq!(num_pages, order.len(), "free/order 后备切片长度必须一致");
        assert!(num_pages > 0, "至少要 1 页");
        // 先全部置「非空闲头 / 尾页」，再贪心切大块。
        for f in free.iter_mut() {
            *f = false;
        }
        for o in order.iter_mut() {
            *o = TAIL;
        }
        let mut me = Self {
            num_pages,
            free,
            order,
        };
        me.reset_free();
        me
    }

    /// 把 `[0, num_pages)` 贪心切成尽量大的空闲块（不要求页数是 2 的幂）。
    fn reset_free(&mut self) {
        let mut p = 0usize;
        while p < self.num_pages {
            // 从最大阶往下找首个「能完整放下、不越界」的阶。
            let mut o = MAX_ORDER;
            while o > 0 && p + (1usize << o) > self.num_pages {
                o -= 1;
            }
            // o 至少为 0（单页必能放下，因为 p < num_pages）。
            self.free[p] = true;
            self.order[p] = o as u8;
            p += 1usize << o;
        }
    }

    /// 区域总页数。
    pub fn num_pages(&self) -> usize {
        self.num_pages
    }

    /// 分配一个 `2^order` 页的连续块，返回其**头页号**。
    pub fn alloc_order(&mut self, order: usize) -> Result<usize, AllocError> {
        if order > MAX_ORDER {
            return Err(AllocError::OrderTooLarge);
        }
        // 找最小可用阶 o >= order。
        let mut o = order;
        while o <= MAX_ORDER {
            if let Some(p) = self.find_free_head(o) {
                // 逐阶拆分到目标阶。
                while o > order {
                    self.split(p, o);
                    o -= 1;
                }
                // 标记已分配：不再是空闲头；order 记为分配阶。
                self.free[p] = false;
                self.order[p] = order as u8;
                return Ok(p);
            }
            o += 1;
        }
        Err(AllocError::OutOfMemory)
    }

    /// 便捷：分配单页，返回头页号。
    pub fn alloc(&mut self) -> Result<usize, AllocError> {
        self.alloc_order(0)
    }

    /// 释放头页 `p`、阶 `order` 的块，并逐阶尝试与 buddy 合并。
    pub fn free(&mut self, p: usize, order: usize) -> Result<(), AllocError> {
        if order > MAX_ORDER {
            return Err(AllocError::OrderTooLarge);
        }
        let block = 1usize << order;
        if p >= self.num_pages || block > self.num_pages || p + block > self.num_pages {
            return Err(AllocError::OutOfRange);
        }
        // 必须是已分配块头：非空闲，且阶匹配。
        if self.free[p] {
            return Err(AllocError::DoubleFree);
        }
        if self.order[p] != order as u8 {
            return Err(AllocError::OrderMismatch);
        }
        // 置为空闲块头（order 字段已 == order，保持）。
        self.free[p] = true;
        // 逐阶向上合并。
        let mut o = order;
        let mut head = p;
        while o < MAX_ORDER {
            let buddy = head ^ (1usize << o);
            // buddy 须是同阶空闲块头才能合并。
            if buddy + (1usize << o) <= self.num_pages
                && self.free[buddy]
                && self.order[buddy] == o as u8
            {
                // 新头 = min(head, buddy) = 清掉第 o 位；另一页退为块内尾页。
                let new_head = head & !(1usize << o);
                let tail = if new_head == head { buddy } else { head };
                self.free[tail] = false;
                self.order[tail] = TAIL;
                self.free[new_head] = true;
                self.order[new_head] = (o + 1) as u8;
                head = new_head;
                o += 1;
            } else {
                break;
            }
        }
        Ok(())
    }

    /// 在阶 `o` 的空闲块头里找首个，返回其页号。
    fn find_free_head(&self, o: usize) -> Option<usize> {
        // 线性扫描；boot / 低频分配可接受（见模块文档）。
        let mut p = 0;
        while p < self.num_pages {
            if self.free[p] && self.order[p] == o as u8 {
                return Some(p);
            }
            p += 1;
        }
        None
    }

    /// 把 `(p, o)` 块拆成两个 `o-1` 块：保留 `p` 为头（降阶），`buddy` 置为空闲头。
    /// 调用方保证 `p` 是阶 `o` 的空闲块头且 `o > 0`。
    fn split(&mut self, p: usize, o: usize) {
        debug_assert!(o > 0, "split 阶必须 > 0");
        let buddy = p + (1usize << (o - 1));
        // p 降阶为 o-1 的空闲头（free[p] 不动，仍为 true）。
        self.order[p] = (o - 1) as u8;
        // buddy 成为 o-1 的空闲头。
        self.free[buddy] = true;
        self.order[buddy] = (o - 1) as u8;
    }

    /// 检查某页号是否当前已分配（非空闲）。仅供测试 / 断言用。
    #[cfg(test)]
    pub fn is_allocated(&self, p: usize) -> bool {
        !self.free[p] && self.order[p] != TAIL
    }

    /// 调试用：返回所有「空闲块头页」的 (页号, 阶) 列表（升序）。
    #[cfg(test)]
    pub fn debug_free_heads(&self) -> Vec<(usize, usize)> {
        let mut v = Vec::new();
        for p in 0..self.num_pages {
            if self.free[p] && self.order[p] != TAIL {
                v.push((p, self.order[p] as usize));
            }
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(n: usize) -> BuddyAllocator<'static> {
        // 把后备放进 leaked Box，让借用 'static；单测进程退出即回收，可接受。
        let free: &'static mut [bool] = Box::leak(vec![false; n].into_boxed_slice());
        let order: &'static mut [u8] = Box::leak(vec![0u8; n].into_boxed_slice());
        BuddyAllocator::new(free, order)
    }

    #[test]
    fn round_trip_single_page() {
        let mut ba = make(8);
        let p = ba.alloc().expect("alloc");
        // 区域初始化为一大块，首次单页分配必从头取 → page 0。
        assert_eq!(p, 0);
        ba.free(p, 0).expect("free");
        // 释放后全合并回原状，再分配仍是 page 0。
        let p2 = ba.alloc().expect("alloc again");
        assert_eq!(p2, 0);
    }

    #[test]
    fn split_and_coalesce_back() {
        let n = 16;
        let mut ba = make(n);
        // 占满：16 个单页。
        let mut pages = Vec::new();
        for _ in 0..n {
            pages.push(ba.alloc().expect("alloc single"));
        }
        eprintln!("alloc order = {:?}", pages);
        // 再要一个应 OOM。
        assert_eq!(ba.alloc(), Err(AllocError::OutOfMemory));
        // 全释放。
        for &p in &pages {
            ba.free(p, 0).expect("free");
        }
        // 现在应能再分配一个 max-order 整块（证明完全合并）。
        let big = ba.alloc_order(4).expect("alloc order-4 after full coalesce");
        assert_eq!(big, 0);
    }

    #[test]
    fn buddy_merge_adjacent() {
        let mut ba = make(8);
        // 仅占 page0、page1（page2..7 仍空闲）。释放后二者应**级联合并**穿过已空闲的
        // buddy，一路并回整个 order-3 块——这验证合并不止一层。
        let a = ba.alloc().expect("alloc a");
        let b = ba.alloc().expect("alloc b");
        assert_eq!(a, 0);
        assert_eq!(b, 1);
        ba.free(a, 0).unwrap();
        ba.free(b, 0).unwrap();
        assert_eq!(ba.debug_free_heads(), vec![(0, 3)]);
    }

    #[test]
    fn double_free_detected() {
        let mut ba = make(4);
        let p = ba.alloc().unwrap();
        ba.free(p, 0).unwrap();
        assert_eq!(ba.free(p, 0), Err(AllocError::DoubleFree));
    }

    #[test]
    fn order_mismatch_detected() {
        let mut ba = make(8);
        let p = ba.alloc_order(0).unwrap(); // 分配阶 0
        assert_eq!(ba.free(p, 1), Err(AllocError::OrderMismatch));
        ba.free(p, 0).unwrap();
    }

    #[test]
    fn order_too_large() {
        let mut ba = make(8);
        assert_eq!(ba.alloc_order(MAX_ORDER + 1), Err(AllocError::OrderTooLarge));
    }

    #[test]
    fn higher_order_reserves_block() {
        let mut ba = make(8);
        let p = ba.alloc_order(2).expect("alloc order-2"); // 4 页
        // 紧接着单页分配不应落在 [p, p+4) 内。
        let q = ba.alloc().expect("alloc single after order-2");
        assert!(q < p || q >= p + 4, "q={} 落入了 order-2 块 [{}..{})", q, p, p + 4);
    }

    #[test]
    fn non_power_of_two_pages() {
        // 13 页：尾部应被切成 8 + 4 + 1，无越界。
        let mut ba = make(13);
        assert_eq!(ba.num_pages(), 13);
        // 能凑出 13 个单页。
        let mut cnt = 0;
        while ba.alloc().is_ok() {
            cnt += 1;
        }
        assert_eq!(cnt, 13);
    }
}
