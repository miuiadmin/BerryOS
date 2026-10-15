//! 内核 slab 堆：`#[global_allocator]` 实现，使能 `alloc` crate（Box/Vec/…）。
//!
//! 架构：
//!  - **小对象**（`max(size, align) <= 2048`）：按 2 的幂 size-class（8…2048）取自
//!    slab——每页 4KiB 切成若干等大槽，空槽用侵入式单链表（槽首存 next 指针）。
//!  - **大对象**（`> 2048`）：直接向帧分配器取 `2^order` 连续页，VA=PA（身份）。
//!
//! dealloc 据 `Layout` 重算同样判定（对称），无需元数据旁路。
//!
//! 后备帧来自内核全局 [`crate::mm::frames`]（buddy）。**G0 简化**：slab 页即使全槽空闲
//! 也不归还 buddy（内核堆小、128MiB RAM 充裕）；大对象页如实归还。
//!
//! **G0 单核**：freelist/帧源用 `UnsafeCell` + `unsafe impl Sync`；无真并发，SMP(G2)
//! 后需加锁或改每核 slab。
//!
//! **分配上下文纪律**：IRQ 热路径零分配（timer/GIC 异常路径不碰堆）；SVC
//! **同步异常上下文允许**分配（syscall handler：入口硬件置 DAIF、单核、
//! 不解蔽——无重入/并发窗口；分配失败的 no-panic 纪律见 3b3 审查 H2 try 化）。

use alloc::alloc::{GlobalAlloc, Layout};
use alloc::boxed::Box;
use berry_mm::va::{phys_to_virt, PAGE_SHIFT, PAGE_SIZE};
use core::cell::UnsafeCell;
use core::ptr;

/// size-class（2 的幂，字节）。每类一条 freelist。
const SLAB_CLASSES: [usize; 9] = [8, 16, 32, 64, 128, 256, 512, 1024, 2048];
/// 走 slab 的上限；超过则走整页路径。
const MAX_CLASS: usize = 2048;

pub struct SlabHeap {
    /// 每类一条侵入式 freelist 头（`*mut u8`，槽首存 next 指针）。
    freelists: UnsafeCell<[*mut u8; SLAB_CLASSES.len()]>,
}

impl SlabHeap {
    pub const fn new() -> Self {
        SlabHeap {
            freelists: UnsafeCell::new([ptr::null_mut(); SLAB_CLASSES.len()]),
        }
    }
}

// SAFETY: G0 单核、无抢占、无并发访问。SMP(G2) 后重构（加锁 / 每核 slab）。
unsafe impl Sync for SlabHeap {}

unsafe impl GlobalAlloc for SlabHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let eff = core::cmp::max(layout.size(), layout.align());
        match class_index(eff) {
            // slab 路径
            Some(ci) => {
                let fl = &mut *self.freelists.get();
                if let Some(slot) = pop(fl, ci) {
                    return slot;
                }
                // freelist 空：取一页，切成 class 槽，槽 1.. 推入 freelist，返回槽 0。
                let class = SLAB_CLASSES[ci];
                let frame = match crate::mm::frames().alloc() {
                    Ok(pfn) => pfn,
                    Err(_) => return ptr::null_mut(),
                };
                let base = phys_to_virt(frame << PAGE_SHIFT) as *mut u8;
                let nslots = PAGE_SIZE / class;
                for i in 1..nslots {
                    push(fl, ci, base.add(i * class));
                }
                base
            }
            // 整页路径
            None => {
                let npages = div_round_up(layout.size(), PAGE_SIZE);
                let order = order_for(npages);
                match crate::mm::frames().alloc_order(order) {
                    Ok(pfn) => phys_to_virt(pfn << PAGE_SHIFT) as *mut u8,
                    Err(_) => ptr::null_mut(),
                }
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let eff = core::cmp::max(layout.size(), layout.align());
        match class_index(eff) {
            Some(ci) => push(&mut *self.freelists.get(), ci, ptr),
            None => {
                let order = order_for(div_round_up(layout.size(), PAGE_SIZE));
                // 身份映射：VA==PA，pfn = ptr >> PAGE_SHIFT。
                let _ = crate::mm::frames().free((ptr as usize) >> PAGE_SHIFT, order);
            }
        }
    }
}

/// Fallible 装箱（3b3 审查 H2 try 化基础件）：`Box::new` 在分配失败时 abort
/// （= 内核停机）——EL0 可经 vmo_create/process_start/channel_create 触发，违
/// ABI §8.0「永不 panic」。syscall 可达路径的装箱一律走本助手，失败折
/// -ENOMEM；boot/ktest 可信语境仍可用 `Box::new`/`expect`。
pub fn try_box<T>(val: T) -> Option<Box<T>> {
    let layout = core::alloc::Layout::for_value(&val);
    // SAFETY: layout 非零（T 至少 1 字节）；走本模块的全局分配器。
    let ptr = unsafe { alloc::alloc::alloc(layout) } as *mut T;
    if ptr.is_null() {
        return None; // OOM：不 abort（Box::new 走 handle_alloc_error 即 abort）
    }
    // SAFETY: ptr 有效未初始化——write 转移所有权。
    unsafe { ptr.write(val) };
    // SAFETY: 该块按全局分配器 + for_value 布局分配，from_raw 收回构成合法 Box
    // （std `Box::try_new` 内部实现同构；try_new 本体仍锁 allocator_api）。
    Some(unsafe { Box::from_raw(ptr) })
}

// ---- freelist（侵入式：槽首存 next 指针）----

unsafe fn pop(fl: &mut [*mut u8; SLAB_CLASSES.len()], ci: usize) -> Option<*mut u8> {
    let slot = fl[ci];
    if slot.is_null() {
        return None;
    }
    fl[ci] = *(slot as *const *mut u8); // 头 = next
    Some(slot)
}

unsafe fn push(fl: &mut [*mut u8; SLAB_CLASSES.len()], ci: usize, slot: *mut u8) {
    *(slot as *mut *mut u8) = fl[ci]; // next = 旧头
    fl[ci] = slot;
}

// ---- class / order 算术 ----

/// 返回 `>= eff` 的最小 class 下标；`eff > MAX_CLASS` 时 None（走整页）。
fn class_index(eff: usize) -> Option<usize> {
    if eff > MAX_CLASS {
        return None;
    }
    SLAB_CLASSES.iter().position(|&c| c >= eff)
}

/// 向上取整除法。
fn div_round_up(n: usize, d: usize) -> usize {
    (n + d - 1) / d
}

/// `npages` 页所需 buddy 阶（最小 `2^order >= npages`）。`npages >= 1`。
fn order_for(npages: usize) -> usize {
    if npages <= 1 {
        0
    } else {
        (usize::BITS - (npages - 1).leading_zeros()) as usize
    }
}
