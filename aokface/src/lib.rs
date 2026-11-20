//! berry-aokface —— `access_ok` / `access_ok_window` 用户缓冲准入判定律
//! 纯函数面（W3-Q70）。
//!
//! **动机**：`kernel/src/syscall.rs` 的 [`access_ok`] 族是全部带用户指针
//! syscall 的第一道门——EL0 调用方的值域四门（len==0 恒过 / `checked_add`
//! 回卷拒 / `ptr < 0x1000` null 页拒 / `end > USER_VA_END` 拒）+ EL1 调用
//! 方高半直映单比较 + 逐页叶 AP 位域读写拒律（写需 AP∈{00,01}、读需
//! AP≠11）+ 窗形 [`access_ok_window`] 的同页跳过律（`last_page` 窗内缓存
//! 哨兵——同页块免重复走查）。这些判定律此前只活在 kernel crate 里（host
//! 测不可达），而它们是 -EFAULT 折射面的**语义单源**——错一门 = 要么放
//! 过野指针（安全洞）、要么误拒合法缓冲（功能红）。本 crate 把值域门与
//! AP 位域判按纯投影抽成纯函数 + 表驱动测试钉板；kernel 侧
//! `walk_leaf`（TTBR0 三级走表物理臂）与 PT 锁窗留壳（纪律同
//! safeck/chanface 先例族，先例族第 22 件 face crate）。
//!
//! 抽离面（与消费侧逐字对齐——改这里的语义 = 改消费侧行为）：
//! - [`NULL_PAGE_BASE`] / [`USER_VA_END`] / [`PAGE_SIZE`]:值域常量单源
//!   （自 syscall.rs 迁此，kernel re-export）。
//! - [`el0_span_ok`]：EL0 缓冲 `[ptr, ptr+len)` 的值域四门——纯整数判定。
//! - [`el1_span_ok`]：EL1 高半直映单比较 + 回卷拒（ktest 静态缓冲面）。
//! - [`leaf_admits`]:单叶描述符 AP 位域判——`desc>>6 & 0b11` 写需
//!   `< 0b10`、读需 `!= 0b11`（11 = EL0 专属，内核读不到）。
//! - [`span_pages`]:`[ptr, ptr+len)` 覆盖页基流（对齐掩码迭代——窗形
//!   走查与 host 测试共用同一页序单源）。
//! - [`window_should_walk`]:同页跳过律（`last_page` 缓存命中即免走查；
//!   `SAME_PAGE_NONE` 哨兵首块必走查）。
//!
//! **留壳**（物理臂，不外迁）：`walk_leaf` 三级走表 + PT 锁窗
//! （`copy_from_user_locked` 单窗纪律）；AP 位域的**值域判定**在
//! [`leaf_admits`]（参数化注入 `desc`——W3-Q24 dpgface 纯投影同款）。
//!
//! # 纪律
//! - 无 unsafe、无内核状态读、无用户内存触碰——输入纯值，输出纯判定。
//! - 与消费侧语义逐字等价：迁移期间 kernel 侧判定段换成对这里的调用，
//!   行为零漂移（行为锚点测试钉 syscall.rs 侧原判序）。

#![cfg_attr(not(test), no_std)]

/// null 页下界：用户指针不得低于此（0 页捕获 NULL/小整数误传）。
pub const NULL_PAGE_BASE: u64 = 0x1000;

/// 用户 VA 上界（39-bit VA 布局顶）。
pub const USER_VA_END: u64 = 1 << 39;

/// 页大小（4KiB granule——与 HAL paging.rs PTES 同源口径）。
pub const PAGE_SIZE: u64 = 4096;

/// 页基掩码（`!(PAGE_SIZE-1)`——span_pages/窗形共用）。
pub const PAGE_MASK: u64 = !(PAGE_SIZE - 1);

/// 窗内同页缓存哨兵：用户 VA < 2^39、EL1 高半直映 ≥ KERNEL_VA_BASE，
/// u64::MAX 两界之外——首块必走查（自 syscall.rs 迁此单源）。
pub const SAME_PAGE_NONE: u64 = u64::MAX;

/// EL0 缓冲 `[ptr, ptr+len)` 的值域四门（纯整数判定，不查页表）：
/// - len == 0 → 恒过（零长无触碰面——与消费侧一致，不折 EFAULT）；
/// - `ptr+len` 回卷（checked_add None）→ 拒；
/// - `ptr < 0x1000`（null 页）→ 拒；
/// - `end > USER_VA_END`（2^39）→ 拒。
///
/// 过四门 ≠ 可访问——还须逐页走查叶 AP（[`leaf_admits`]）。
#[inline(always)]
pub const fn el0_span_ok(ptr: u64, len: u64) -> bool {
    if len == 0 {
        return true;
    }
    let Some(end) = ptr.checked_add(len) else {
        return false;
    };
    ptr >= NULL_PAGE_BASE && end <= USER_VA_END
}

/// EL1 调用方跨度门：只认内核高半直映区（ktest 静态缓冲面），且
/// `ptr+len` 回卷拒——与消费侧原判序逐字一致（checked_add 门在
/// EL1/EL0 分派**之前**，两路都拒回卷——迁移时不得把回卷门只折进
/// EL0 面，否则 EL1 高半顶端回卷从拒变放 = 语义漂移）。`base` =
/// KERNEL_VA_BASE（HAL 常量，调用方注入——本 crate 不依赖 HAL）。
#[inline(always)]
pub const fn el1_span_ok(ptr: u64, len: u64, kernel_va_base: u64) -> bool {
    ptr.checked_add(len).is_some() && ptr >= kernel_va_base
}

/// 单叶描述符 AP 位域判（参数化注入 `desc`——物理臂走表后调用）：
/// - AP ∈ {00, 01}：EL1 读写（00 = EL1 专属、01 = EL0/EL1 共享）——读写均过；
/// - AP = 10（EL1 只读）：写拒、读过；
/// - AP = 11（EL0 专属）：读写均拒（内核读不到）。
///
/// 与消费侧判序逐字一致：`write && ap >= 0b10` 拒；`!write && ap == 0b11` 拒。
#[inline(always)]
pub const fn leaf_admits(desc: u64, write: bool) -> bool {
    let ap = (desc >> 6) & 0b11;
    if write && ap >= 0b10 {
        return false;
    }
    if !write && ap == 0b11 {
        return false;
    }
    true
}

/// `[ptr, ptr+len)` 覆盖的页基流（升序迭代器——窗形走查与 host 测试共用
/// 同一页序单源；len==0 空流）。yield 的页基 = `va & PAGE_MASK`。
#[inline]
pub fn span_pages(ptr: u64, len: u64) -> impl Iterator<Item = u64> {
    let first = ptr & PAGE_MASK;
    let count = if len == 0 {
        0
    } else {
        // 首尾页差 +1；end 回卷情形 el0_span_ok 已拒，这里 saturating 防御
        let last = (ptr + len - 1) & PAGE_MASK;
        if last < first {
            1 // 回卷防御：saturating 到单页（值域门已拒，不可达防御面）
        } else {
            (last - first) / PAGE_SIZE + 1
        }
    };
    (0..count).map(move |i| first + i * PAGE_SIZE)
}

/// 窗形同页跳过判（`access_ok_window` 单窗纪律的纯投影）：
/// `va == *last_page` → 免走查（本窗已查过——写者被 PT 锁挡在表外）；
/// 否则走查并更新缓存。返回 `true` = 须走查（调用方查完应把 `va` 写回
/// `*last_page`）；`false` = 同页命中免走查。
///
/// 哨兵语义：`*last_page == SAME_PAGE_NONE` 恒不等于任何真实页基
/// （用户 VA < 2^39、EL1 高半 ≥ KERNEL_VA_BASE、u64::MAX 两界外）——
/// 首块必走查。
#[inline]
pub fn window_should_walk(va: u64, last_page: &mut u64) -> bool {
    if va == *last_page {
        return false;
    }
    *last_page = va;
    true
}

#[cfg(test)]
mod tests;
