//! LRU 缓存（段②：图标图集的逐出核；`docs/GUI里程碑规划-2026-10.md`
//! GUI-2 行「图标格式 + LRU」）。定容 N 槽 + **stamp-clock 逐出**：每槽
//! 存 `(戳, 键, 值)`，查询命中即碰热（戳 ← 时钟++），满表插入逐出**戳
//! 最小槽**（最久未用）；空槽优先于逐出。
//!
//! **内部可变性**（`core::cell::Cell`，全 `&self` 口）：图标图集经
//! `&dyn IconAtlas` 进命令（`cmd::ImageSrc::Icon` 持共享引用），回放期
//! 查询碰热必须经 `&self`——Cell 方案使 `&mut` 语境与 `&dyn` 语境同一
//! 套口。约束：K/V 皆 `Copy`（槽值整体换、无 Drop 面）、**单线程 soft
//! 光栅语境**（不涉 Sync——多消费者图集随真后端引入锁面再议）。
//!
//! 时钟回绕：u32 `wrapping_add`，2^32 次碰热后戳序退化为近似 LRU
//! （guest 帧循环量级下不可达，注记换简单）。
//!
//! 契约注记（规划草案 → 落地形态三处签名偏差，详报在案）：① 泛型
//! 参序取 `<const N, K, V>`（const 参前置——消费点 `IconLruCache<2>`
//! 一词即容量，K/V 随推断零噪声）；② K 界 `PartialEq`（非 `Eq`——键
//! 的唯一消费点是命中判别，全序不需要；`IconId` 等新类型两界都满足，
//! 不构成对面收缩）；③ 时钟 u32（非 u64——回绕注记同上，2^32 次碰热
//! 不可达，u64 只白费槽宽）。
//!
//! 组合编排留消费侧：图标 id → BYI1 blob → `crate::icon::decode` →
//! `put` 装载的编排链不进本核（GUI-4/7——no_std 下解码缓冲归属是消费
//! 方决策：静态预解码/按需解码/失败回退缺图各有其主，缓存核只管
//! 「装了什么、逐谁、谁热」，不越权管数据从哪来）。

use core::cell::Cell;

/// 定容 LRU 缓存（stamp-clock；见模块头设计注）。
///
/// 泛型约束即纪律：`K: Copy + PartialEq`（键比较定 identity）、
/// `V: Copy`（槽值整体换）。零分配：槽表随结构体内联（定容数组）。
/// Debug/Clone 手写（derive 会把 `Cell<T>: Clone ⟹ T: Copy` 的要求
/// 错译成 K/V 的 Debug/Clone 边界——Copy 边界在 impl 块上，derive
/// 表达不了；Debug 只打容量/在槽数概要）。
pub struct LruCache<const N: usize, K, V> {
    /// 单调时钟（先 ++ 再用 ⇒ 槽内活戳恒 ≥ 1；0 = 本轮回绕纪元的
    /// 「空/未触」哨兵）。
    clock: Cell<u32>,
    slots: [Cell<Option<(u32, K, V)>>; N],
}

impl<const N: usize, K: Copy + PartialEq, V: Copy> Clone for LruCache<N, K, V> {
    fn clone(&self) -> Self {
        LruCache {
            clock: Cell::new(self.clock.get()),
            slots: self.slots.clone(),
        }
    }
}

impl<const N: usize, K, V> core::fmt::Debug for LruCache<N, K, V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LruCache")
            .field("capacity", &N)
            .field("clock", &self.clock.get())
            .finish_non_exhaustive()
    }
}

impl<const N: usize, K: Copy + PartialEq, V: Copy> LruCache<N, K, V> {
    /// 构造空缓存。
    pub fn new() -> LruCache<N, K, V> {
        LruCache {
            clock: Cell::new(0),
            // [(); N].map 惯形：绕开 `Option<...>: Copy` 推导对 [None; N]
            // 的 const 上下文限制（泛型 K/V 无 Copy 证明时数组重复表达式
            // 不可用，map 对每元素逐点求值即可）。
            slots: [(); N].map(|_| Cell::new(None)),
        }
    }

    /// 容量。
    pub fn capacity(&self) -> usize {
        N
    }

    /// 在槽条目数。
    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.get().is_some()).count()
    }

    /// 空谓词。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 清空（时钟保序续走——清空不重置纪元）。
    pub fn clear(&self) {
        for s in &self.slots {
            s.set(None);
        }
    }

    /// 查询 + 碰热：命中返值并刷新戳；未命中返 `None`（不动任何槽）。
    pub fn get(&self, k: &K) -> Option<V> {
        for s in &self.slots {
            if let Some((_stamp, ek, v)) = s.get() {
                if ek == *k {
                    let fresh = self.tick();
                    s.set(Some((fresh, ek, v)));
                    return Some(v);
                }
            }
        }
        None
    }

    /// 插入/覆盖：同键覆写并碰热；新键 → 空槽优先，满表逐出戳最小槽
    /// （最久未用）。返被逐出的旧条目（`None` = 有空槽或同键覆写——
    /// 覆写不逐出）。
    pub fn put(&self, k: K, v: V) -> Option<(K, V)> {
        // 零容量形态：无处可放、无旧可逐（vacuous 成功——不 panic）。
        if N == 0 {
            return None;
        }
        // ① 同键覆写 + 碰热。
        for s in &self.slots {
            if let Some((_, ek, _)) = s.get() {
                if ek == k {
                    s.set(Some((self.tick(), k, v)));
                    return None;
                }
            }
        }
        // ② 空槽优先。
        for s in &self.slots {
            if s.get().is_none() {
                s.set(Some((self.tick(), k, v)));
                return None;
            }
        }
        // ③ 满表：逐出戳最小槽（N ≥ 1 保证存在；空槽已被 ② 收编，
        // 此处槽全活——戳 ≥ 1 恒可比较）。
        let mut victim = 0usize;
        let mut min_stamp = u32::MAX;
        for (i, s) in self.slots.iter().enumerate() {
            if let Some((stamp, _, _)) = s.get() {
                if stamp < min_stamp {
                    min_stamp = stamp;
                    victim = i;
                }
            }
        }
        let evicted = self.slots[victim].get()?;
        self.slots[victim].set(Some((self.tick(), k, v)));
        let (_, ek, ev) = evicted;
        Some((ek, ev))
    }

    /// 时钟前进一步（wrapping——见模块头回绕注记）。
    fn tick(&self) -> u32 {
        let t = self.clock.get().wrapping_add(1);
        self.clock.set(t);
        t
    }
}

impl<const N: usize, K: Copy + PartialEq, V: Copy> Default for LruCache<N, K, V> {
    fn default() -> Self {
        LruCache::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // —— 基本：空/容量/命中未命中 ——
    #[test]
    fn basics_hit_miss() {
        let c: LruCache<2, u8, u8> = LruCache::new();
        assert!(c.is_empty());
        assert_eq!(c.capacity(), 2);
        assert_eq!(c.get(&1), None, "空表未命中");
        c.put(1, 10);
        c.put(2, 20);
        assert_eq!(c.len(), 2);
        assert_eq!(c.get(&1), Some(10));
        assert_eq!(c.get(&2), Some(20));
        assert_eq!(c.get(&3), None, "键 3 未插入");
        assert_eq!(c.len(), 2, "未命中不扩槽");
    }

    // —— 覆写不逐出 + 碰热（老键覆写后仍最新）——
    #[test]
    fn overwrite_refreshes_no_evict() {
        let c: LruCache<2, u8, u8> = LruCache::new();
        c.put(1, 10);
        c.put(2, 20);
        c.put(1, 11); // 覆写键 1（不逐出、不占新槽）
        assert_eq!(c.len(), 2);
        assert_eq!(c.get(&1), Some(11), "覆写后取新值");
        // 键 1 现在最新 → 满表插入逐出的应是键 2。
        assert_eq!(c.put(3, 30), Some((2, 20)), "逐出最久未用键 2");
        assert_eq!(c.get(&2), None);
        assert_eq!(c.get(&1), Some(11));
        assert_eq!(c.get(&3), Some(30));
    }

    // —— LRU 序逐出：A B C（容量 2）→ B 逐 A；touch A 后插 C 逐 B ——
    #[test]
    fn evicts_least_recently_used() {
        let c: LruCache<2, u8, &str> = LruCache::new();
        assert_eq!(c.put(1, "a"), None, "空槽优先");
        assert_eq!(c.put(2, "b"), None);
        assert_eq!(c.put(3, "c"), Some((1, "a")), "未碰热的键 1 被逐");
        // 现存 {3,2}；碰热 2 → 插 4 逐 3。
        assert_eq!(c.get(&2), Some("b"));
        assert_eq!(c.put(4, "d"), Some((3, "c")));
        assert_eq!(c.get(&3), None);
        assert_eq!(c.get(&4), Some("d"));
        assert_eq!(c.get(&2), Some("b"));
        assert_eq!(c.len(), 2, "容量钉死");
    }

    // —— clear 后空槽复用（不逐出）——
    #[test]
    fn clear_frees_all_slots() {
        let c: LruCache<3, u16, u32> = LruCache::new();
        for i in 0..3u16 {
            c.put(i, u32::from(i) * 7);
        }
        assert_eq!(c.len(), 3);
        c.clear();
        assert!(c.is_empty());
        assert_eq!(c.get(&0), None);
        assert_eq!(c.put(9, 99), None, "清空后插入走空槽（零逐出）");
        assert_eq!(c.get(&9), Some(99));
    }

    // —— &self 内部可变性：共享引用双查询同效（图标图集 &dyn 消费面）——
    #[test]
    fn interior_mutability_via_shared_ref() {
        let c: LruCache<2, u8, u8> = LruCache::new();
        c.put(1, 10);
        c.put(2, 20);
        let shared = &c; // 无 &mut——碰热仍生效
        assert_eq!(shared.get(&1), Some(10));
        assert_eq!(c.put(3, 30), Some((2, 20)), "共享引用的 get 也碰热（LRU 序推进）");
        // 零容量形态：恒未命中 + 插入无处可放（vacuous 返 None，不
        // panic——见 put 头注；非「逐出自身」形态）。
        let z: LruCache<0, u8, u8> = LruCache::new();
        assert_eq!(z.get(&1), None);
        assert_eq!(z.put(1, 1), None, "0 槽无逐出面");
        assert!(z.is_empty());
    }
}
