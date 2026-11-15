//! berry-heapface——内核 slab 堆分配路由算术纯函数面
//! （W3-Q39，自 `kernel/src/heap.rs` 外迁）。
//!
//! 单源纪律（同 berry-dpgface/utrace）：**判定律唯一事实源在此**，
//! `kernel/src/heap.rs` 机械壳消费（侵入式 freelist `push`/`pop`、
//! `crate::mm::frames()` 帧源、`phys_to_virt` 映射等物理臂留内核侧）。
//!
//! 外迁面（逐字自 heap.rs，语义零变化）：
//! - **size-class 取整律**：`SLAB_CLASSES = [8,16,…,2048]`（2 的幂、
//!   9 类），`class_index(eff)` 返回首个 `≥ eff` 的 class 下标；
//!   `eff > MAX_CLASS (2048)` → `None`（路由到整页路径）。
//! - **align 修正律**：`eff = max(size, align)`——对齐需求可把请求抬进
//!   更高 class，但 align > 2048 同样走整页。
//! - **slab 切页几何**：`nslots = PAGE_SIZE / class`——4KiB 页对全部
//!   9 类恰整除（无尾浪费）；切页后槽 1..nslots 推入 freelist、槽 0
//!   即时返回。
//! - **整页路径算术**：`npages = ceil(size/PAGE_SIZE)`（`div_round_up`）
//!   + `order_for(npages)` = 使 `2^order ≥ npages` 的最小阶——buddy
//!   分配器形状约束的直接体现。
//!
//! 钉板不变量（host 测试逐条执法）：
//! - `SLAB_CLASSES` 严格递增且每项 2 的幂、首项 8、末项 = `MAX_CLASS`；
//! - `class_index` 是「向上取整到 class」函数：全域 `[1, 2048]` 上
//!   `SLAB_CLASSES[ci] ≥ eff > SLAB_CLASSES[ci-1]`（ci=0 时 eff≤8）；
//! - `order_for` 与 `npages.next_power_of_two().trailing_zeros()` 全域
//!   逐点一致（npages ≥ 1）；
//! - `div_round_up` 与 `(n + d - 1)/d` 等值且对 `d = PAGE_SIZE` 的
//!   恰除（n 是 d 的倍数时商恰 n/d——无 +1 虚胖）。

#![cfg_attr(not(test), no_std)]

/// size-class（2 的幂，字节）。每类一条 freelist。
pub const SLAB_CLASSES: [usize; 9] = [8, 16, 32, 64, 128, 256, 512, 1024, 2048];

/// 走 slab 的上限；超过则走整页路径。
pub const MAX_CLASS: usize = 2048;

/// 页大小（字节）——与 `berry_mm::va::PAGE_SIZE` 同值；kernel 侧消费时
/// 以 const 断言对拍（heapface 不依赖 berry-mm，保持零依赖纯面）。
pub const PAGE_SIZE: usize = 4096;

/// 有效占用：`max(size, align)`——对齐需求可抬高 class。
#[inline]
pub fn effective_size(size: usize, align: usize) -> usize {
    core::cmp::max(size, align)
}

/// 返回 `>= eff` 的最小 class 下标；`eff > MAX_CLASS` 时 None（走整页）。
pub fn class_index(eff: usize) -> Option<usize> {
    if eff > MAX_CLASS {
        return None;
    }
    SLAB_CLASSES.iter().position(|&c| c >= eff)
}

/// 每页槽位数：`PAGE_SIZE / class`（9 类全整除，无尾浪费）。
///
/// class 非 slab 类（> MAX_CLASS 或 0）时返回 None——切页几何只对
/// class 表内成员有定义。
pub fn slots_per_page(class: usize) -> Option<usize> {
    if SLAB_CLASSES.contains(&class) {
        Some(PAGE_SIZE / class)
    } else {
        None
    }
}

/// 向上取整除法。
#[inline]
pub fn div_round_up(n: usize, d: usize) -> usize {
    (n + d - 1) / d
}

/// `npages` 页所需 buddy 阶（最小 `2^order >= npages`）。`npages >= 1`。
pub fn order_for(npages: usize) -> usize {
    if npages <= 1 {
        0
    } else {
        (usize::BITS - (npages - 1).leading_zeros()) as usize
    }
}

/// 分配路由：请求 → slab 类下标或整页阶（单源判序）。
///
/// 返回 `Some(Route::Slab { class, slots })`（freelist 弹槽/切页补货几
/// 何）或 `Some(Route::Pages { npages, order })`（buddy 整页）或 `None`
/// （零大小请求——`Layout` 非零不变量外的防御面，调用方不应到达）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// slab 路径：class 槽大小 + 每页槽数。
    Slab { class: usize, slots: usize },
    /// 整页路径：ceil 页数 + buddy 阶。
    Pages { npages: usize, order: usize },
}

/// 路由判定（纯）：`route(size, align) = (class_index ∘ effective_size)`
/// 与整页算术的复合单源面。
pub fn route(size: usize, align: usize) -> Option<Route> {
    if size == 0 {
        return None;
    }
    let eff = effective_size(size, align);
    match class_index(eff) {
        Some(ci) => Some(Route::Slab {
            class: SLAB_CLASSES[ci],
            slots: PAGE_SIZE / SLAB_CLASSES[ci],
        }),
        None => {
            let npages = div_round_up(size, PAGE_SIZE);
            Some(Route::Pages {
                npages,
                order: order_for(npages),
            })
        }
    }
}

/// dealloc 对称路由：与 [`route`] 同判（dealloc 据 `Layout` 重算同样
/// 判定，对称性由「同一函数」结构性保证——这是外迁的核心理由）。
pub fn dealloc_route(size: usize, align: usize) -> Option<Route> {
    route(size, align)
}
