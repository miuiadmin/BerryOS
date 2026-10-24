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

/// `order` 字段的「boot 保留页」哨兵（V-1b `reserve_range` 产物）。
/// 语义 = 单页占用单元：`free` 恒 false、不可拆、不可并；`free()` 的
/// 阶匹配检查对它恒 `OrderMismatch`（0xFE > MAX_ORDER，任何合法 order
/// 参数都到不了等值）——**误释放保留页在结构上被拒**，不靠调用方纪律。
const RESERVED: u8 = 0xFE;

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

    /// Boot 期保留：把页号区间 `[a, b)` 从空闲结构中永久剔除（V-1b）。
    ///
    /// 实现 = **幻影分配**：定向拆分包含各页的空闲块到 order-0，再标
    /// `free=false / order=RESERVED`——buddy 全部不变量原样成立：
    /// `alloc` 只挑空闲头（保留页永不出选）、`free` 合并查 `free[buddy]`
    /// （保留页挡住跨界合并）、对保留页误 `free` 报 `OrderMismatch`
    /// （RESERVED 哨兵 > MAX_ORDER，阶匹配结构性拒——2026-10 host 测试实抓
    /// 初版「与分配页同形」会让 free(保留页, 0) 走 Ok 甚至合并回收）。
    ///
    /// 契约：boot 单线程、且首次 `alloc`/`free` 之前调用（构造后立即）；
    /// `b` 超出 `num_pages` 截断；区间交叠/相邻幂等（落在已占用块内的页
    /// 整块跳过——已保留页不会被二次标记）。
    ///
    /// 复杂度 O(区间页数 × log 块高)——boot 一次性成本，128MiB 全窗量级毫秒。
    pub fn reserve_range(&mut self, a: usize, b: usize) {
        let b = b.min(self.num_pages);
        let mut p = a.min(b);
        while p < b {
            // 找包含 p 的块头：TAIL 尾页的属主头必在下方（页 0 恒为块头）。
            let mut h = p;
            while self.order[h] == TAIL {
                h -= 1;
            }
            if !self.free[h] {
                // 已占用块：整块跳过——交叠保留的幂等面。RESERVED 页是
                // 单页占用单元（步进 1）；普通已分配块按其阶整块跳过。
                p = h + if self.order[h] == RESERVED {
                    1
                } else {
                    1usize << self.order[h]
                };
                continue;
            }
            // 空闲块 [h, h+2^o) 含 p：定向拆分，直到 p 自成 order-0 头。
            // 不变量：块 h 恒含 p（split 的低半保持 h、高半推进 h）。
            while self.order[h] > 0 {
                let o = self.order[h] as usize;
                let half = 1usize << (o - 1);
                self.split(h, o);
                if p >= h + half {
                    h += half; // p 在高半：buddy 成为含 p 的新头
                }
            }
            // 此刻 h == p（order-0 块只含自身）且 free——标保留单元。
            self.free[p] = false;
            self.order[p] = RESERVED;
            p += 1;
        }
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

    // ---- V-1b reserve_range 面 ----

    #[test]
    fn reserve_excludes_range_and_rejects_free() {
        // 保留 [16,24)：占满单页后恰 56 页可分配、保留区永不出现；
        // 对保留页误 free → DoubleFree（幻影分配面执法）。
        let mut ba = make(64);
        ba.reserve_range(16, 24);
        let mut got = Vec::new();
        while let Ok(p) = ba.alloc() {
            got.push(p);
        }
        assert_eq!(got.len(), 64 - 8);
        for p in 16..24 {
            assert!(!got.contains(&p), "保留页 {} 被分配", p);
            // RESERVED 哨兵执法：合法 order 恒 ≠ 0xFE → OrderMismatch。
            assert_eq!(ba.free(p, 0), Err(AllocError::OrderMismatch));
        }
        // 保留区外的页正常可回收复用。
        ba.free(got[0], 0).unwrap();
        assert_eq!(ba.alloc().unwrap(), got[0]);
    }

    #[test]
    fn reserve_merge_blocked_by_reserved_page() {
        // 保留正中一页（[0,64) o6 块的对称点 32）：幻影级联在 32 上方留碎片、
        // 下方 [0,32) 完整 o5。占尽 63 页（32 永不出现）→ 全释放 → 合并被 32
        // 挡住：左并到 o5 封顶、右侧碎片链原形——精确形状对拍钉死。
        let mut ba = make(64);
        ba.reserve_range(32, 33);
        let mut got = Vec::new();
        while let Ok(p) = ba.alloc() {
            got.push(p);
        }
        assert_eq!(got.len(), 63);
        assert!(!got.contains(&32), "保留页 32 被分配");
        for &p in &got {
            ba.free(p, 0).unwrap();
        }
        assert_eq!(
            ba.debug_free_heads(),
            vec![(0, 5), (33, 0), (34, 1), (36, 2), (40, 3), (48, 4)]
        );
    }

    #[test]
    fn reserve_overlap_and_clamp_idempotent() {
        // 交叠保留 [16,24)+[20,30) ≡ 单次 [16,30)；b 越界截断。
        let mut ba = make(64);
        ba.reserve_range(16, 24);
        ba.reserve_range(20, 30);
        ba.reserve_range(60, 999); // 越界腿：截到 64
        let mut got = Vec::new();
        while let Ok(p) = ba.alloc() {
            got.push(p);
        }
        assert_eq!(got.len(), 64 - 14 - 4, "两腿保留后可分配总量");
        for p in 16..30 {
            assert!(!got.contains(&p), "交叠保留页 {} 被分配", p);
        }
        for p in 60..64 {
            assert!(!got.contains(&p), "越界腿保留页 {} 被分配", p);
        }
    }

    #[test]
    fn reserve_prefix_equivalence_with_offset_construct() {
        // k 恰与 buddy 块界对齐（768 = 3×256，窗 1024 = 2×512）时的**强等价**：
        // reserve [0,k) 后剩余空闲结构 ≡ make(n-k) 构造（碎片形状全同）——
        // 同一操作序列下分配页号逐个相同（旧构造页号 +k 平移对拍）。
        // 注意这是对齐情形的特例性质，非普适承诺：非对齐 k 时两侧碎片形状
        // 不同（见 reserve_prefix_conservation_unaligned），本测试守住「对齐
        // 时不得漂移」的回归面。
        let (n, k) = (1024usize, 768usize);
        let mut full = make(n);
        full.reserve_range(0, k);
        let mut old = make(n - k);
        let mut sf = Vec::new();
        let mut so = Vec::new();
        for _ in 0..32 {
            let pf = full.alloc().unwrap();
            let po = old.alloc().unwrap();
            assert_eq!(pf, k + po, "单页分配序漂移");
            sf.push(pf);
            so.push(po);
        }
        let b4f = full.alloc_order(4).unwrap();
        let b4o = old.alloc_order(4).unwrap();
        assert_eq!(b4f, k + b4o, "order-4 落点漂移");
        // 混合释放（偶数序单页）后两结构仍镜像推进。
        for i in (0..32).step_by(2) {
            full.free(sf[i], 0).unwrap();
            old.free(so[i], 0).unwrap();
        }
        let b6f = full.alloc_order(6).unwrap();
        let b6o = old.alloc_order(6).unwrap();
        assert_eq!(b6f, k + b6o, "order-6 落点漂移");
        // 释放大块 → 合并行为一致 → 复取同位。
        full.free(b6f, 6).unwrap();
        old.free(b6o, 6).unwrap();
        assert_eq!(full.alloc_order(6).unwrap(), k + old.alloc_order(6).unwrap());
        // 收尾逐页耗尽：两侧总量与全集对拍。
        let mut cf = 0usize;
        let mut co = 0usize;
        while full.alloc().is_ok() {
            cf += 1;
        }
        while old.alloc().is_ok() {
            co += 1;
        }
        assert_eq!(cf, co, "可分配总量漂移");
    }

    #[test]
    fn reserve_prefix_conservation_unaligned() {
        // 非对齐 k（168 = 128+32+8，不撞任何 2^j 块界）：强等价**不成立**
        //（全窗构造碎片在 k 处向上级联、旧 make(n-k) 碎片在窗尾），守恒面
        // 仍须钉死——可分配总量一致、保留页永不出现、且全窗侧首个分配恰为
        // k 页（保留上界——mm::init「先保内核足迹」依赖的形状）。2026-10
        // EFI 崩溃复盘时立：首版误把对齐特例当普适等价写进 mm.rs 注释。
        let (n, k) = (2048usize, 168usize);
        let mut full = make(n);
        full.reserve_range(0, k);
        let mut old = make(n - k);
        // 首分配形状：全窗侧 = k（保留上界）；旧构造侧 = 窗尾（1880 = 2048-168
        // 的贪心切分最小片是 [1872,1880) o3，级联拆分取 1872）。
        assert_eq!(full.alloc().unwrap(), k);
        assert_eq!(old.alloc().unwrap(), 1872);
        // 守恒：两侧耗尽总量相同（本对刚分配的页之后）。
        let mut cf = 1usize;
        let mut co = 1usize;
        while full.alloc().is_ok() {
            cf += 1;
        }
        while old.alloc().is_ok() {
            co += 1;
        }
        assert_eq!(cf, co, "可分配总量漂移");
        assert_eq!(cf, n - k, "总量 = n-k");
    }
}
