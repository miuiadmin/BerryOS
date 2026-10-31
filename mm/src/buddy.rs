//! 纯 Rust **buddy 页帧分配器**核心。
//!
//! 操作对象是**页号**（`PAGE_SIZE` 单位的下标），不碰物理内存本身。
//!
//! 设计要点（呼应 规则/13「小步提交 / 架构无关优先」+ 架构综合纲领 §3）：
//! - **不持有全局状态**：所有运行期状态由调用方借入的两片后备切片承载。
//! - **无 `unsafe`**：纯安全 Rust。
//! - **不依赖 `alloc`**：后备用 `&mut [bool]` / `&mut [u8]`，no_std 友好。
//!   host 测试用 `Vec` 切片；内核侧（Step 3）用静态 BSS 数组。
//!
//! 算法是教科书 buddy：每阶 `o` 维护「空闲块头页」集合；`alloc` 找最小够用阶、
//! 逐阶拆分；`free` 逐阶尝试与 buddy 合并。
//!
//! 空闲头索引 = **每阶位图 + 摘要位图两级**（K-1 起，v2 优化提前落地）：
//! 每阶一片位图（bit p = 页 p 是否该阶空闲头）+ 一片摘要（bit = 位图字非零），
//! `find_free_head` = 摘要字扫 + `trailing_zeros`——查询 O(摘要字数)，维护
//! O(1)。落地动机 = K-1 回归（2026-10-27 钉死）：旧线性扫在「该阶全窗无
//! 空闲头」地形下每次分配付 32768 页 × 7 指令 = 28,672 icount 单位的**保证
//! 失败全窗扫**，而「有没有该阶头」由保留前缀 k = `__kernel_end>>12` 的
//! 奇偶翻页决定——任意让镜像跨页的无害 commit 都在重掷这台骰子（boot 期
//! slab 分配史 × k 对齐的阈值事件，94c1fd5 的 .rodata 恰 +0x1000 触发，
//! frame_alloc4k ×59 / frame_alloc64k 自 B-0 起潜伏 ×58）。位图化后分配
//! 成本与 k 奇偶/布局脱钩，此类回归结构性绝迹；且顺带收回 64k 慢带。

use core::sync::atomic::{AtomicUsize, Ordering};

/// buddy 最大阶。一个 `2^MAX_ORDER` 页的块。
/// 128MiB / 4KiB = 32768 页；`MAX_ORDER = 13` → 最大块 8192 页 = 32MiB。
/// **必须 ≥ 内核整页大对象的真实需求帽**：heap 整页路径单次连续分配，
/// `ROOTFS_MAX`（32MiB）帽下的 rootfs 装载 Vec 是最大在册单块（C-2a 实证：
/// 14.5MiB rootfs 在 MAX_ORDER=11 下 order 12 被拒 → boot 期 alloc panic——
/// 帽没有分配器能力支撑 = 纯空头支票；C-2c 复刻同律：text 批 8 命令令
/// rootfs ≈21.5MiB → order 13，16MiB 帽/12 阶双双不够 → 帽与阶同步
/// 12→13、16→32MiB。**A1-2 帽分家后**：`VMO_MAX_SIZE` 128MiB 走 VMO 逐页
/// commit 不经堆单块，与本阶脱钩不再联动——`容量工程前置-A1设计草案.md` §3.1）。
pub const MAX_ORDER: usize = 13;

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
    /// 每阶空闲头位图（扁平：阶 `o` 占 `[o*W, (o+1)*W)`，`W` = `bmap_words`）。
    /// bit p = 页 p 是否阶 `o` 空闲头——**不变量**：
    /// `free[p] && order[p] == o` ⟺ 阶 `o` 位图 p 位置位（K-1 起）。
    bmaps: &'a mut [u64],
    /// 位图的摘要位图（扁平：阶 `o` 占 `[o*S, (o+1)*S)`，`S` = `sum_words`）。
    /// bit w = 阶 `o` 位图第 `w` 字非零——`find_free_head` 先扫摘要（字数 =
    /// 位图字数的 1/64）再 `trailing_zeros`，免「空阶全位图扫」（K-1 回归
    /// 的病态路径正是空阶的全窗线性扫）。
    sums: &'a mut [u64],
    /// 每阶位图字数 = ceil(num_pages / 64)。
    bmap_words: usize,
    /// 每阶摘要字数 = ceil(bmap_words / 64)。
    sum_words: usize,
    /// 已分配页数水位（D24，S-2 前置读侧原子化）：`alloc_order` 成功 +
    /// `2^order`、`free` 成功 − `2^order`、`reserve`/`release` 不动（RESERVED
    /// 恒不计——与旧全窗扫口径逐位一致，`#[cfg(test)]` 对拍钉板）。
    /// `allocated_pages()` 从 O(num_pages) 扫落为 O(1) load（B-0 泄漏金丝雀
    /// 每次 boot 执法 + F37 watchdog 锁窗读 + S-3 拆锁后跨核读的撕裂面）。
    allocated: AtomicUsize,
}

impl<'a> BuddyAllocator<'a> {
    /// 用调用方提供的后备切片构造，并把整个区域 `[0, num_pages)` 初始化为空闲。
    ///
    /// `free.len()` 与 `order.len()` 必须相等（= 页数）；`bmaps.len()` /
    /// `sums.len()` 须 ≥ `(MAX_ORDER+1) ×` 每阶字数（见字段注释的几何）。
    pub fn new(
        free: &'a mut [bool],
        order: &'a mut [u8],
        bmaps: &'a mut [u64],
        sums: &'a mut [u64],
    ) -> Self {
        let num_pages = free.len();
        assert_eq!(num_pages, order.len(), "free/order 后备切片长度必须一致");
        assert!(num_pages > 0, "至少要 1 页");
        let bmap_words = (num_pages + 63) / 64;
        let sum_words = (bmap_words + 63) / 64;
        assert!(
            bmaps.len() >= (MAX_ORDER + 1) * bmap_words,
            "bmaps 后备装不下：需 {} × {} 字",
            MAX_ORDER + 1,
            bmap_words
        );
        assert!(
            sums.len() >= (MAX_ORDER + 1) * sum_words,
            "sums 后备装不下：需 {} × {} 字",
            MAX_ORDER + 1,
            sum_words
        );
        // 先全部置「非空闲头 / 尾页」，再贪心切大块。
        for f in free.iter_mut() {
            *f = false;
        }
        for o in order.iter_mut() {
            *o = TAIL;
        }
        // 位图/摘要全清零（host 复用切片时有旧值；内核 BSS 已零，重复清零
        // 正确且只花 boot 一次性 ~56KiB memset 量级）。
        for w in bmaps.iter_mut() {
            *w = 0;
        }
        for w in sums.iter_mut() {
            *w = 0;
        }
        let mut me = Self {
            num_pages,
            free,
            order,
            bmaps,
            sums,
            bmap_words,
            sum_words,
            allocated: AtomicUsize::new(0),
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
            self.bm_set(o, p);
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
                // 逐阶拆分到目标阶（split 内部同步位图）。
                while o > order {
                    self.split(p, o);
                    o -= 1;
                }
                // 标记已分配：不再是空闲头；order 记为分配阶。
                self.bm_clear(order, p);
                self.free[p] = false;
                self.order[p] = order as u8;
                // D24 水位：分配成功 +2^order（split 净零——拆出的低半本就
                // 计在本次分配里，buddy 半块留在空闲集合未计）。
                self.allocated.fetch_add(1usize << order, Ordering::Relaxed);
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
        // 置为空闲块头（order 字段已 == order，保持），入阶位图。
        self.free[p] = true;
        self.bm_set(order, p);
        // 逐阶向上合并。
        self.merge_up(p, order);
        // D24 水位：释放成功 −2^order（merge_up 净零——合并只动空闲集合）。
        self.allocated.fetch_sub(1usize << order, Ordering::Relaxed);
        Ok(())
    }

    /// 把阶 `order` 的空闲块头 `head` 逐阶向上与 buddy 合并（`free` 与
    /// [`release_range`] 共用的合并环——单一实现，不长第二套）。
    ///
    /// 前置：`head` 已是阶 `order` 空闲头（free=true、位图已置）。
    fn merge_up(&mut self, head: usize, order: usize) {
        let mut o = order;
        let mut head = head;
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
                // tail 退为尾页：离开 o 阶空闲集合。
                self.bm_clear(o, tail);
                self.free[tail] = false;
                self.order[tail] = TAIL;
                // new_head 从 o 阶升 (o+1) 阶（head/buddy 中非 tail 的一方，
                // 合并前本是 o 阶空闲头——先出 o 阶位图再入 o+1）。
                self.bm_clear(o, new_head);
                self.free[new_head] = true;
                self.order[new_head] = (o + 1) as u8;
                self.bm_set(o + 1, new_head);
                head = new_head;
                o += 1;
            } else {
                break;
            }
        }
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
            // 此刻 h == p（order-0 块只含自身）且 free——标保留单元
            //（先出 order-0 位图：离开空闲集合）。
            self.bm_clear(0, p);
            self.free[p] = false;
            self.order[p] = RESERVED;
            p += 1;
        }
    }

    /// Boot 期回收（A1-3b）：[`reserve_range`] 的逆——把 `[a, b)` 内的
    /// RESERVED 哨兵页逐页归还空闲池（order-0 空闲头 + [`merge_up`] 与
    /// 邻块自然合并）。**只回收 RESERVED 页**：普通已分配页 / 已空闲页 /
    /// 尾页一律跳过（幂等；不越权释放他源占用）。返回**实际归还页数**
    ///（区间内非 RESERVED 页不计——与区间长度的差 = 跳过面）。
    ///
    /// 契约：boot 单线程；调用方保证区间确属自己此前 reserve 的
    /// （boot 期手误回收他源保留区 = 内存腐化——本面只挡「已分配页
    /// 不被回收」这一半，保留区之间的归属靠调用方纪律，同 reserve）。
    /// 与 `free` 的分工：free 按分配阶整块释放且对 RESERVED 恒
    /// OrderMismatch（结构性拒）；本面是保留语义的专用出口。
    pub fn release_range(&mut self, a: usize, b: usize) -> usize {
        let b = b.min(self.num_pages);
        let mut p = a.min(b);
        let mut released = 0usize;
        while p < b {
            if self.order[p] != RESERVED {
                // 已空闲 / 已分配 / 尾页：不属本面，逐页步进。
                p += 1;
                continue;
            }
            // RESERVED 单页 → order-0 空闲头，入位图，向上合并（相邻
            // RESERVED 页随后逐页到达，合并环自然拼接出大块）。
            self.free[p] = true;
            self.order[p] = 0;
            self.bm_set(0, p);
            self.merge_up(p, 0);
            released += 1;
            // p 若在合并中退为尾页（p != 合并头），后续步进照走（TAIL ≠
            // RESERVED → 跳过），区间内下一 RESERVED 页继续回收。
            p += 1;
        }
        released
    }

    /// 在阶 `o` 的空闲块头里找**最小页号**者，返回其页号。
    ///
    /// 位图 + 摘要两级索引（K-1）：摘要字扫（每阶 ≤ ceil(W/64) 字，内核
    /// 32768 页 = 8 字）→ 非零字 `trailing_zeros` 定页——与旧线性扫同返回
    /// （同为最小页号优先），成本从 O(num_pages) 落到 O(摘要字数)。
    /// **该阶无空闲头时**只付摘要全零扫（旧版付全窗页扫——K-1 回归 ×59
    /// 的病态路径：32768 页 × 7 指令的保证失败扫描）。
    fn find_free_head(&self, o: usize) -> Option<usize> {
        let w = self.bmap_words;
        let sw = self.sum_words;
        for si in 0..sw {
            let s = self.sums[o * sw + si];
            if s == 0 {
                continue;
            }
            let wi = si * 64 + s.trailing_zeros() as usize;
            let word = self.bmaps[o * w + wi];
            debug_assert!(word != 0, "摘要位与非零位图字失同步（阶 {o} 字 {wi}）");
            // release 纵深（K-1 处置刀）：不变量万一破裂时 debug 构建由上面
            // debug_assert loud；release 剔除断言后 tz(0)=64 会把 p 推到
            // wi*64+64——非尾字时落在窗内 = 对非空闲头页 split/标记的静默
            // 错分配（尾字才越界 panic）。一条比较把失效形态收敛为「跳过
            // 该摘要字继续扫」；正常路径永不命中（debug 构建该比较被断言
            // 前置证伪、死码消除——icount 五跑逐值不变）。
            if word == 0 {
                continue;
            }
            let p = wi * 64 + word.trailing_zeros() as usize;
            return Some(p);
        }
        None
    }

    /// 阶 `o` 位图置位页 `p`（页成为该阶空闲头时调用），同步摘要。
    fn bm_set(&mut self, o: usize, p: usize) {
        let w = self.bmap_words;
        let wi = p / 64;
        let bit = 1u64 << (p % 64);
        let old = self.bmaps[o * w + wi];
        self.bmaps[o * w + wi] = old | bit;
        if old == 0 {
            let sw = self.sum_words;
            self.sums[o * sw + wi / 64] |= 1u64 << (wi % 64);
        }
    }

    /// 阶 `o` 位图清位页 `p`（页离开该阶空闲集合时调用），字归零同步清摘要。
    fn bm_clear(&mut self, o: usize, p: usize) {
        let w = self.bmap_words;
        let wi = p / 64;
        let bit = 1u64 << (p % 64);
        self.bmaps[o * w + wi] &= !bit;
        if self.bmaps[o * w + wi] == 0 {
            let sw = self.sum_words;
            self.sums[o * sw + wi / 64] &= !(1u64 << (wi % 64));
        }
    }

    /// 把 `(p, o)` 块拆成两个 `o-1` 块：保留 `p` 为头（降阶），`buddy` 置为空闲头。
    /// 调用方保证 `p` 是阶 `o` 的空闲块头且 `o > 0`。
    fn split(&mut self, p: usize, o: usize) {
        debug_assert!(o > 0, "split 阶必须 > 0");
        let buddy = p + (1usize << (o - 1));
        // p 降阶为 o-1 的空闲头（free[p] 不动，仍为 true）。
        self.bm_clear(o, p);
        self.order[p] = (o - 1) as u8;
        self.bm_set(o - 1, p);
        // buddy 成为 o-1 的空闲头。
        self.free[buddy] = true;
        self.order[buddy] = (o - 1) as u8;
        self.bm_set(o - 1, buddy);
    }

    /// 检查某页号是否当前已分配（非空闲）。仅供测试 / 断言用。
    #[cfg(test)]
    pub fn is_allocated(&self, p: usize) -> bool {
        !self.free[p] && self.order[p] != TAIL
    }

    /// **已分配页总数**（B-0 泄漏金丝雀的对账口径：churn 前后必须相等）。
    ///
    /// D24（S-2）起 = **O(1) 水位读**：`alloc_order`/`free` 成功路径 ±`2^order`
    /// 维护的 AtomicUsize 单字 load——语义口径与旧全窗扫逐位一致（已分配块头
    /// 按 `2^order` 累计；尾页 TAIL 不单独计；保留页 RESERVED 恒不计），由
    /// [`allocated_pages_scan`] 对拍测试钉板。原 O(num_pages) 扫的消费者
    ///（BENCH churn 断言 / F37 watchdog 锁窗 / soakmon）全数改走本 O(1) 面。
    pub fn allocated_pages(&self) -> usize {
        self.allocated.load(Ordering::Relaxed)
    }

    /// 旧全窗扫口径（D24 独立实现——对拍基准，非运行期 API）：扫已分配块头
    ///（`free=false` 且非 TAIL/RESERVED）按 `2^order` 累计。水位与之失同步
    /// = alloc/free 挂点漏改——对拍测试 loud。
    #[cfg(test)]
    fn allocated_pages_scan(&self) -> usize {
        let mut n = 0usize;
        for p in 0..self.num_pages {
            if !self.free[p] {
                let o = self.order[p];
                if o != TAIL && o != RESERVED {
                    n += 1usize << o;
                }
            }
        }
        n
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
        let bw = (n + 63) / 64;
        let sw = (bw + 63) / 64;
        let bmaps: &'static mut [u64] =
            Box::leak(vec![0u64; (MAX_ORDER + 1) * bw].into_boxed_slice());
        let sums: &'static mut [u64] =
            Box::leak(vec![0u64; (MAX_ORDER + 1) * sw].into_boxed_slice());
        BuddyAllocator::new(free, order, bmaps, sums)
    }

    /// 旧版 `find_free_head` 的线性扫参照实现（K-1 位图索引的对拍基准——
    /// P6 A-5 双实现纪律：位图索引必须有对旧语义的机器守卫，防「双绿即
    /// 收敛」退化成对新索引自证）。
    fn linear_find_free_head(ba: &BuddyAllocator, o: usize) -> Option<usize> {
        let mut p = 0;
        while p < ba.num_pages {
            if ba.free[p] && ba.order[p] == o as u8 {
                return Some(p);
            }
            p += 1;
        }
        None
    }

    /// 位图/摘要**不变量直断**（K-1 处置刀）：对拍守卫验的是「find_free_head
    /// 返回值 == 线性扫」——「多余脏位且低于当阶最小真头」的瞬时态不改变
    /// 返回值，须等真头耗尽/当阶清空才显形（理论盲区）。本函数把两条不变量
    /// 逐位直接断言，把盲区钉成每调必炸：
    /// ① `free[p] && order[p]==o` ⟺ 阶 `o` 位图 p 位置位；
    /// ② 摘要位 ⟺ 对应 64 字 span 内存在非零位图字。
    fn assert_bitmap_invariants(ba: &BuddyAllocator) {
        let w = ba.bmap_words;
        let sw = ba.sum_words;
        for o in 0..=MAX_ORDER {
            for p in 0..ba.num_pages {
                let bit = (ba.bmaps[o * w + p / 64] >> (p % 64)) & 1 == 1;
                assert_eq!(
                    bit,
                    ba.free[p] && ba.order[p] == o as u8,
                    "阶 {o} 页 {p}：位图位={bit} free={} order={}（不变量①破）",
                    ba.free[p],
                    ba.order[p]
                );
            }
            for si in 0..sw {
                for wi in si * 64..((si + 1) * 64).min(w) {
                    let sbit = (ba.sums[o * sw + si] >> (wi % 64)) & 1 == 1;
                    assert_eq!(
                        sbit,
                        ba.bmaps[o * w + wi] != 0,
                        "阶 {o} 摘要字 {si} 位 {wi}（不变量②破）"
                    );
                }
            }
        }
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
    fn rootfs_cap_single_alloc_fits() {
        // C-2a 回归钉板：13.8MiB rootfs 装载 Vec 的单块分配必须在 MAX_ORDER
        // 内（128MiB RAM = 32768 页真身口径）。C-2a 前史：MAX_ORDER=11 时
        // order_for(3530)=12 被拒 → boot alloc panic——「16MiB 帽」与分配器
        // 能力脱钩。常量 = C-2a 首版 rootfs.cpio 实寸 14_456_248B（对抗审查
        // 实抓：首版误钉排障期中间构建的 14_534_824B——同一事实两数）；
        // 8–16MiB 域内阶恒 12。
        let mut ba = make(32768); // 128MiB / 4KiB
        let npages: usize = (14_456_248 + 4096 - 1) / 4096; // ceil = 3530
        // heap.rs order_for 的口径（最小 2^order >= npages）。
        let order = (usize::BITS - (npages - 1).leading_zeros()) as usize;
        assert_eq!(order, 12, "C-2a 尺寸的阶漂移 = 帽口径变了");
        assert!(order <= MAX_ORDER, "ROOTFS_MAX 32MiB 帽须有 buddy 能力支撑");
        let p = ba.alloc_order(order).expect("rootfs 单块在满盘 buddy 必可分配");
        ba.free(p, order).expect("free");

        // C-2c 钉板（16–32MiB 域）：text 批 8 命令后 rootfs ≈21.5MiB → 阶 13
        // = 恰达新 MAX_ORDER——16MiB 帽下此分配被拒（C-2a 同型断裂复发），
        // 故帽/阶同步 16→32MiB、12→13。域内（16MiB, 32MiB] 阶恒 13；后续
        // 刀内容增删在域内漂移时本断言恒成立。
        let npages2: usize = (21_500_000 + 4096 - 1) / 4096; // ≈21.5MiB ceil = 5249
        let order2 = (usize::BITS - (npages2 - 1).leading_zeros()) as usize;
        assert_eq!(order2, 13, "C-2c 尺寸的阶漂移 = 帽口径变了");
        assert!(order2 <= MAX_ORDER, "ROOTFS_MAX 32MiB 帽须有 buddy 能力支撑");
        let p2 = ba.alloc_order(order2).expect("rootfs 单块在满盘 buddy 必可分配");
        ba.free(p2, order2).expect("free");
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

    // ---- A1-3b release_range 面（reserve 的逆）----

    #[test]
    fn release_restores_full_pool_shape() {
        // 保留 [16,24) → 回收同区间：整窗应恢复成与「从未保留」等价的
        // 形状（64 页全可分配 + 回收计数恰 8 + 位图不变量保持）。
        let mut ba = make(64);
        ba.reserve_range(16, 24);
        assert_eq!(ba.release_range(16, 24), 8);
        assert_bitmap_invariants(&ba);
        let mut got = Vec::new();
        while let Ok(p) = ba.alloc() {
            got.push(p);
        }
        assert_eq!(got.len(), 64, "回收后整窗全可分配");
        // 再回收（幂等面）：无 RESERVED 页可回收 → 0。
        assert_eq!(ba.release_range(16, 24), 0);
    }

    #[test]
    fn release_merges_back_to_max_order() {
        // 保留正中 [32,33)（o6 对称点）→ 回收后合并环应还原出完整 o6
        // 单块（reserve_merge_blocked_by_reserved_page 的逆命题）。
        let mut ba = make(64);
        ba.reserve_range(32, 33);
        assert_eq!(ba.release_range(32, 33), 1);
        assert_eq!(ba.debug_free_heads(), vec![(0, 6)]);
        assert_bitmap_invariants(&ba);
    }

    #[test]
    fn release_skips_allocated_and_free_pages() {
        // 先占后混：[0,4) 正常分配、[8,12) 保留、[16,20) 保持空闲——
        // release [0,20) 只回收 4 个 RESERVED 页（已分配/已空闲页不越权），
        // 已分配页随后正常 free（不腐化）。
        let mut ba = make(64);
        let a = ba.alloc_order(2).unwrap(); // [0,4) 已分配
        assert_eq!(a, 0);
        ba.reserve_range(8, 12);
        assert_eq!(ba.release_range(0, 20), 4);
        assert_eq!(ba.allocated_pages(), 4, "已分配页不受 release 影响");
        assert_bitmap_invariants(&ba);
        ba.free(a, 2).unwrap(); // 正常回收面原样可用
        let mut got = Vec::new();
        while let Ok(p) = ba.alloc() {
            got.push(p);
        }
        assert_eq!(got.len(), 64);
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

    #[test]
    fn bitmap_index_matches_linear_scan() {
        // K-1 位图索引的**语义对拍**：位图/摘要两级的 `find_free_head` 必须与
        // 旧线性扫逐值同构（同返回最小页号、同返回 None）。落地动机 = K-1
        // 回归（分配成本依赖保留前缀奇偶的刀锋态），索引换装只许变快、不许
        // 变语义——本测试把「最小页号优先」钉死成机器守卫。
        // 确定性 LCG（无外部依赖，同种子同序列）：975 = 2048-1073 的裸数字，
        // 与随机论无关。
        let mut ba = make(975);
        ba.reserve_range(300, 500); // 非对齐保留前缀 + 中段保留：制造空阶地形
        let mut rng: u64 = 0x2026_1027;
        let mut held: Vec<(usize, usize)> = Vec::new();
        for step in 0..4000 {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let pick = (rng >> 33) % 4;
            match pick {
                0 | 1 => {
                    // 单页分配（走 find_free_head(0) 全链）。
                    if let Ok(p) = ba.alloc() {
                        held.push((p, 0));
                    }
                }
                2 => {
                    // 随机阶分配。
                    let o = ((rng >> 40) % 5) as usize;
                    if let Ok(p) = ba.alloc_order(o) {
                        held.push((p, o));
                    }
                }
                _ => {
                    // 释放（约 1/4 步——held 非空时）。
                    if let Some((p, o)) = held.pop() {
                        ba.free(p, o).unwrap();
                    }
                }
            }
            // 每步全阶对拍：位图索引 == 线性扫（值与 None 双面）。
            for o in 0..=MAX_ORDER {
                assert_eq!(
                    ba.find_free_head(o),
                    linear_find_free_head(&ba, o),
                    "step={} 阶 {} 索引/线性扫分歧（位图维护不变量破）",
                    step,
                    o
                );
            }
            // 每 100 步不变量直断（瞬时脏位盲区钉死，见 assert_bitmap_invariants）。
            if step % 100 == 0 {
                assert_bitmap_invariants(&ba);
            }
        }
        // 尾声：全量释放后再对拍一轮（合并路径的位图维护面）。
        for &(p, o) in held.iter() {
            ba.free(p, o).unwrap();
        }
        for o in 0..=MAX_ORDER {
            assert_eq!(ba.find_free_head(o), linear_find_free_head(&ba, o));
        }
        assert_bitmap_invariants(&ba);
    }

    #[test]
    fn bitmap_words_partial_tail_window() {
        // 页数非 64 倍数（= 位图尾字部分页）+ 尾部高阶块跨摘要字边界：
        // 构造 n = 130（两字位图 + 2 页尾字）确保尾窗 bit 位与摘要边界
        // 不错位——K-1 首版潜在坑面（位图几何按 ceil 切，尾字高位恒 0
        // 须由「只 set 合法页」保证，这里钉死）。
        let mut ba = make(130);
        // 占满再全释放：所有页过一遍位图 set/clear。
        let mut got = Vec::new();
        while let Ok(p) = ba.alloc() {
            got.push(p);
        }
        assert_eq!(got.len(), 130);
        for &p in got.iter() {
            ba.free(p, 0).unwrap();
        }
        // 全并回单块（128 + 2）。
        assert_eq!(ba.debug_free_heads(), vec![(0, 7), (128, 1)]);
        for o in 0..=MAX_ORDER {
            assert_eq!(ba.find_free_head(o), linear_find_free_head(&ba, o));
        }
        assert_bitmap_invariants(&ba);
    }

    #[test]
    fn bitmap_multi_summary_word_geometry() {
        // K-1 处置刀：仓内对拍双测（975/130 页）的摘要字数均 = 1，内核真身
        // 32768 页 = 8 摘要字的**跨摘要字几何**（si 递进、si*64 跳字、
        // wi%64 回绕）在 host 面无隔离钉板——本测用 n = 8192（位图 128 字
        // → 摘要 2 字）补上：①全量单页填充 = 双射（每页恰出一次、零重复
        // 零越界）；②乱序全释放 → 幂窗精确并回单块 o13@0；③保留 + 混合阶
        // 随机相逐阶线性对拍 + 周期不变量直断 + 终态与全新构造形状对拍。
        let n = 8192usize; // 2^13 幂窗
        let mut ba = make(n);
        assert_eq!(ba.bmap_words, 128);
        assert_eq!(ba.sum_words, 2, "本测前提：跨摘要字几何（sum_words=2）");
        assert_bitmap_invariants(&ba);

        // ① 全量单页填充 = 双射。
        let mut seen = vec![false; n];
        for _ in 0..n {
            let p = ba.alloc().expect("满窗 8192 页全量分配");
            assert!(p < n, "越界分配 p={p}");
            assert!(!seen[p], "重复分配 p={p}");
            seen[p] = true;
        }
        assert!(seen.iter().all(|&s| s), "双射破：有页从未分配");
        assert_eq!(ba.alloc(), Err(AllocError::OutOfMemory));
        assert_eq!(ba.allocated_pages(), n);
        assert_bitmap_invariants(&ba);

        // ② LCG Fisher-Yates 乱序全释放 → 幂窗精确并回单块 o13@0。
        let mut rng: u64 = 0xdead_beef_cafe_f00d;
        let mut perm: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let j = ((rng >> 33) as usize) % (i + 1);
            perm.swap(i, j);
        }
        for &p in perm.iter() {
            ba.free(p, 0).unwrap();
        }
        assert_eq!(ba.debug_free_heads(), vec![(0, 13)], "幂窗全释放须精确并回单块");
        assert_eq!(ba.allocated_pages(), 0);
        assert_bitmap_invariants(&ba);

        // ③ 保留前缀 + 混合阶随机相（对拍节奏同 bitmap_index_matches_linear_scan，
        //    阶域放宽到 0..=6 触发跨摘要字的高阶空扫面）。
        ba.reserve_range(300, 500);
        let mut held: Vec<(usize, usize)> = Vec::new();
        for step in 0..600 {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let pick = (rng >> 33) % 4;
            match pick {
                0 | 1 => {
                    if let Ok(p) = ba.alloc() {
                        held.push((p, 0));
                    }
                }
                2 => {
                    let o = ((rng >> 40) % 7) as usize;
                    if let Ok(p) = ba.alloc_order(o) {
                        held.push((p, o));
                    }
                }
                _ => {
                    if let Some((p, o)) = held.pop() {
                        ba.free(p, o).unwrap();
                    }
                }
            }
            for o in 0..=MAX_ORDER {
                assert_eq!(
                    ba.find_free_head(o),
                    linear_find_free_head(&ba, o),
                    "step={} 阶 {o} 索引/线性扫分歧（跨摘要字几何破）",
                    step
                );
            }
            if step % 100 == 0 {
                assert_bitmap_invariants(&ba);
            }
        }
        for &(p, o) in held.iter() {
            ba.free(p, o).unwrap();
        }
        // 终态形状 == 全新构造 + 同保留的形状（终态等价钉板）。
        let mut fresh = make(n);
        fresh.reserve_range(300, 500);
        assert_eq!(ba.debug_free_heads(), fresh.debug_free_heads(), "终态形状漂移");
        assert_eq!(ba.allocated_pages(), 0);
        assert_bitmap_invariants(&ba);
        for o in 0..=MAX_ORDER {
            assert_eq!(ba.find_free_head(o), linear_find_free_head(&ba, o));
        }
    }

    /// D24 水位对拍（P6 A-5 双实现纪律同源）：混序 alloc/free/reserve/release
    /// 全生命期每步断言 `allocated_pages()`（O(1) 水位）== `allocated_pages_scan()`
    ///（全窗扫口径）——挂点漏改/口径漂移即 loud。确定性 LCG（K-1 对拍同款，
    /// 勿换随机源——同输入同输出纪律）。
    #[test]
    fn allocated_watermark_matches_scan() {
        let n = 1024usize;
        let mut ba = make(n);
        assert_eq!(ba.allocated_pages(), 0);
        assert_eq!(ba.allocated_pages_scan(), 0);
        // 保留页不计入水位（RESERVED 恒不计——旧扫口径）。
        ba.reserve_range(100, 180);
        assert_eq!(ba.allocated_pages(), 0, "reserve 动水位 = 口径破");
        assert_eq!(ba.allocated_pages_scan(), 0);
        let mut held: Vec<(usize, usize)> = Vec::new();
        let mut rng: u64 = 0x5E2_D24;
        for step in 0..500 {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let pick = (rng >> 33) % 4;
            match pick {
                0 | 1 => {
                    if let Ok(p) = ba.alloc() {
                        held.push((p, 0));
                    }
                }
                2 => {
                    let o = ((rng >> 40) % 5) as usize;
                    if let Ok(p) = ba.alloc_order(o) {
                        held.push((p, o));
                    }
                }
                _ => {
                    if let Some((p, o)) = held.pop() {
                        ba.free(p, o).unwrap();
                    }
                }
            }
            assert_eq!(
                ba.allocated_pages(),
                ba.allocated_pages_scan(),
                "step={} 水位/全窗扫分歧（D24 挂点漏改）",
                step
            );
        }
        // 释放保留区亦不动水位（归还的是 RESERVED，本就未计）。
        let rel = ba.release_range(100, 180);
        assert_eq!(rel, 80);
        assert_eq!(ba.allocated_pages(), ba.allocated_pages_scan());
        for &(p, o) in held.iter() {
            ba.free(p, o).unwrap();
        }
        assert_eq!(ba.allocated_pages(), 0, "全释放后水位非零 = 泄漏口径破");
        assert_eq!(ba.allocated_pages(), ba.allocated_pages_scan());
    }
}
