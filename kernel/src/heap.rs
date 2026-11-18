//! 内核 slab 堆：`#[global_allocator]` 实现，使能 `alloc` crate（Box/Vec/…）。
//!
//! 架构：
//!  - **小对象**（`max(size, align) <= 2048`）：按 2 的幂 size-class（8…2048）取自
//!    slab——每页 4KiB 切成若干等大槽，空槽用侵入式单链表（槽首存 next 指针）。
//!  - **大对象**（`> 2048`）：直接向帧分配器取 `2^order` 连续页，经
//!    `phys_to_virt` 映入内核 direct map（2b-1 起高半，非身份映射）。
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
//!
//! **单源纪律（W3-Q39）**：size-class 取整/align 修正/切页几何/整页 ceil 除与
//! buddy 阶算术唯一事实源在 [`berry_heapface`]（host 钉板 8 测）；本模块
//! 留物理臂——侵入式 freelist `push`/`pop`、`frames()` 帧源、`phys_to_virt`
//! 映射与 loud 报错。alloc/dealloc 路由对称律由 `route`/`dealloc_route`
//! 同一函数结构性保证。

use alloc::alloc::{GlobalAlloc, Layout};
use alloc::boxed::Box;
use berry_hal_aarch64::{uart_put_dec, uart_put_hex64, uart_puts};
use berry_heapface::{
    class_index, dealloc_route, realloc_inplace, route, Route, PAGE_SIZE, SLAB_CLASSES,
};
use berry_mm::va::{phys_to_virt, virt_to_phys, PAGE_SHIFT};
use core::cell::UnsafeCell;
use core::ptr;

// 单源对拍：heapface 与 berry-mm 的页几何常量必须同值（两 crate 无编译期
// 联动，const assert 钉漂移——`route` 的整页算术按 heapface 侧页尺寸走）。
const _: () = assert!(PAGE_SIZE == berry_mm::va::PAGE_SIZE as usize);

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

// SAFETY: F0-3 起 `freelists` 与底层 `FrameAllocator` 统一受第 5 层 `mm::ALLOC_LOCK`（irqsave）互斥保护。
unsafe impl Sync for SlabHeap {}

unsafe impl GlobalAlloc for SlabHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match route(layout.size(), layout.align()) {
            // slab 路径：持 ALLOC_LOCK 弹空槽；若空则同窗向 frames_locked 批取 1 页切槽（零重入）。
            Some(Route::Slab { class, slots }) => {
                let ci = class_index(class).expect("Slab 路由 class 必在表内");
                let mut g = crate::mm::alloc_lock();
                let fl = &mut *self.freelists.get();
                if let Some(slot) = pop(fl, ci) {
                    return slot;
                }
                let frame = match crate::mm::frames_locked(&mut g).alloc() {
                    Ok(pfn) => pfn,
                    Err(_) => return ptr::null_mut(),
                };
                let base = phys_to_virt(frame << PAGE_SHIFT) as *mut u8;
                for i in 1..slots {
                    push(fl, ci, base.add(i * class));
                }
                base
            }
            // 整页路径
            Some(Route::Pages { order, .. }) => {
                match crate::mm::with_frames(|fa| fa.alloc_order(order)) {
                    Ok(pfn) => phys_to_virt(pfn << PAGE_SHIFT) as *mut u8,
                    Err(_) => ptr::null_mut(),
                }
            }
            // 零大小（Layout 非零不变量外的防御面）：拒分配，不 panic。
            None => ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        match dealloc_route(layout.size(), layout.align()) {
            Some(Route::Slab { class, .. }) => {
                let ci = class_index(class).expect("Slab 路由 class 必在表内");
                let _g = crate::mm::alloc_lock();
                push(&mut *self.freelists.get(), ci, ptr)
            }
            Some(Route::Pages { order, .. }) => {
                let pfn = virt_to_phys(ptr as usize) >> PAGE_SHIFT;
                let free_res = crate::mm::with_frames(|fa| fa.free(pfn, order));
                if free_res.is_err() {
                    // 锁外告警打印（严守 ALLOC -> PRINT 正序）。
                    uart_puts(b"heap: large dealloc free failed pfn=0x");
                    uart_put_hex64(pfn as u64);
                    uart_puts(b" order=");
                    uart_put_dec(order as u64);
                    uart_puts(b"\n");
                }
            }
            None => uart_puts(b"heap: dealloc zero-size layout\n"),
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // 原位快路径（内核深度进化审计 01 号机会 4）：新旧布局路由到同一
        // slab class / 同一 buddy 阶时块自足，原指针直返——省「新分配＋
        // memcpy＋释放」整链（Rust 默认 realloc 走该链）。Vec push 增长
        // （24B→32B/600B→1024B 同 class）是全系统热面；判定律单源
        // berry_heapface::realloc_inplace（host 钉板），本臂只做物理分派。
        // SAFETY: ptr 必须来自本分配器先前的 alloc/realloc（GlobalAlloc
        // 合同）；判定只读 layout 数值，不触 *ptr。接口无 new_align——
        // 新块沿用旧块对齐（alloc crate 合同），判据同源无第二实现。
        match realloc_inplace(layout.size(), layout.align(), new_size, layout.align()) {
            // 同 class/阶：块自足（容量 ≥ new_size 且对齐覆盖），零拷贝直返。
            Some(true) => ptr,
            // 需搬移或防御面（new_size == 0）：落「新分配＋拷贝＋释放」
            // 默认链——与 std 默认 realloc 语义一致。
            _ => {
                // Layout 构造失败（new_size 溢出 isize）按 OOM 返 null 不
                // panic（3b3 H2 同纪律；调用方合同下不到达）。
                let new_layout = match Layout::from_size_align(new_size, layout.align()) {
                    Ok(nl) => nl,
                    Err(_) => return core::ptr::null_mut(),
                };
                let new_ptr = unsafe { self.alloc(new_layout) };
                if new_ptr.is_null() {
                    return core::ptr::null_mut();
                }
                // SAFETY: 两块均来自本分配器且不同块（非重叠）；收缩取
                // min 截断、增长拷旧块全长——realloc 合同。
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        ptr,
                        new_ptr,
                        core::cmp::min(layout.size(), new_size),
                    );
                    self.dealloc(ptr, layout);
                }
                new_ptr
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

// ---- class / order 算术：单源外迁 berry-heapface（W3-Q39，host 钉板）----
// 本地仅保留消费面（alloc/dealloc 两臂同源路由 route/dealloc_route）；
// class_index/effective_size/div_round_up/order_for/slots_per_page 的语义
// 钉板见 heapface/tests/heap_tests.rs（经 kernel 公开 re-export 面为零——
// 内核无外部消费者，纯函数族直接消费 berry-heapface crate 即可）。
