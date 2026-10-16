//! malloc 族 —— 静态 256KiB `.bss` 堆 + first-fit 空闲链（G-eco-d 真实现档）
//!
//! v1 口径（附录 A）：
//! - 子进程无 root vmar handle（bootstrap 单 handle），**不向内核要页**——
//!   堆 = 链接进 `.bss` 的静态数组；v2 随 d-3 bootstrap 消息协议携带的
//!   vmar handle 换 VMO 增长；
//! - first-fit + 地址序插入 + 相邻合并（无合并的碎片化在 Vec 增长模式下
//!   几轮就散架）；
//! - 单线程根基（pthread_create 桩永不产线程）⇒ 无锁 `static mut` 直写。
//!
//! 块布局：`[Block 头 16B | payload]`，`size` 记 payload 字节数。

use core::ptr;

use crate::{c_int, c_void, size_t};

pub const HEAP_SIZE: usize = 256 * 1024;

#[repr(align(16))]
struct Heap {
    _data: [u8; HEAP_SIZE],
}

static mut HEAP: Heap = Heap { _data: [0; HEAP_SIZE] };

#[repr(C)]
struct Block {
    size: usize,          // payload 字节数（不含头）
    next: *mut Block,     // 空闲链 next（分配态块复用为 0）
}

static mut FREE_HEAD: *mut Block = ptr::null_mut();

#[inline]
unsafe fn block_payload(b: *mut Block) -> *mut u8 {
    unsafe { b.add(1) as *mut u8 }
}

#[inline]
unsafe fn block_end(b: *mut Block) -> usize {
    unsafe { b as usize + core::mem::size_of::<Block>() + (*b).size }
}

/// 惰性初始化：整堆一枚大空闲块。
unsafe fn heap_init() {
    unsafe {
        let base = ptr::addr_of_mut!(HEAP) as *mut Block;
        (*base).size = HEAP_SIZE - core::mem::size_of::<Block>();
        (*base).next = ptr::null_mut();
        FREE_HEAD = base;
    }
}

/// 地址序插回 + 与前后相邻块合并（hole 合并是 first-fit 的命门）。
unsafe fn free_insert(b: *mut Block) {
    unsafe {
        let mut prev: *mut Block = ptr::null_mut();
        let mut cur = FREE_HEAD;
        while !cur.is_null() && (cur as usize) < b as usize {
            prev = cur;
            cur = (*cur).next;
        }
        // 合并后邻：b 尾 == cur 头
        if !cur.is_null() && block_end(b) == cur as usize {
            (*b).size += core::mem::size_of::<Block>() + (*cur).size;
            (*b).next = (*cur).next;
        } else {
            (*b).next = cur;
        }
        // 合并前邻：prev 尾 == b 头
        if !prev.is_null() && block_end(prev) == b as usize {
            (*prev).size += core::mem::size_of::<Block>() + (*b).size;
            (*prev).next = (*b).next;
        } else if prev.is_null() {
            FREE_HEAD = b;
        } else {
            (*prev).next = b;
        }
    }
}

unsafe fn inner_malloc(n: usize) -> *mut u8 {
    unsafe {
        if FREE_HEAD.is_null() {
            heap_init();
        }
        let need = (n + 15) & !15;
        let mut prev: *mut Block = ptr::null_mut();
        let mut cur = FREE_HEAD;
        while !cur.is_null() {
            let min_split = need + core::mem::size_of::<Block>() + 32; // 分裂残块 ≥ 头+32B 才值得
            if (*cur).size >= need {
                if (*cur).size >= min_split {
                    // 分裂：残块留在链上，前半分配
                    let rest = (block_payload(cur) as *mut Block).add(need / core::mem::size_of::<Block>());
                    (*rest).size = (*cur).size - need - core::mem::size_of::<Block>();
                    (*rest).next = (*cur).next;
                    if prev.is_null() {
                        FREE_HEAD = rest;
                    } else {
                        (*prev).next = rest;
                    }
                    (*cur).size = need;
                } else {
                    // 整块吃下（内部碎片记账在 size 里，free 原样归还）
                    if prev.is_null() {
                        FREE_HEAD = (*cur).next;
                    } else {
                        (*prev).next = (*cur).next;
                    }
                }
                (*cur).next = ptr::null_mut();
                return block_payload(cur);
            }
            prev = cur;
            cur = (*cur).next;
        }
        ptr::null_mut()
    }
}

#[no_mangle]
pub extern "C" fn malloc(n: size_t) -> *mut c_void {
    let p = unsafe { inner_malloc(if n == 0 { 1 } else { n }) };
    if p.is_null() {
        crate::set_errno(crate::ENOMEM);
    }
    p as *mut c_void
}

#[no_mangle]
pub extern "C" fn free(p: *mut c_void) {
    if p.is_null() {
        return;
    }
    unsafe {
        let b = (p as *mut Block).sub(1);
        free_insert(b);
    }
}

#[no_mangle]
pub extern "C" fn realloc(p: *mut c_void, n: size_t) -> *mut c_void {
    if p.is_null() {
        return malloc(n);
    }
    if n == 0 {
        free(p);
        return ptr::null_mut();
    }
    unsafe {
        let old = (*(p as *mut Block).sub(1)).size;
        let newp = inner_malloc(n);
        if newp.is_null() {
            crate::set_errno(crate::ENOMEM);
            return ptr::null_mut();
        }
        let copy = if old < n { old } else { n };
        crate::strmem::memcpy_volatile(newp, p as *const u8, copy);
        free(p);
        newp as *mut c_void
    }
}

#[no_mangle]
pub extern "C" fn calloc(n: size_t, sz: size_t) -> *mut c_void {
    let bytes = match n.checked_mul(sz) {
        Some(b) if b <= HEAP_SIZE => b,
        _ => {
            crate::set_errno(crate::ENOMEM);
            return ptr::null_mut();
        }
    };
    let p = malloc(bytes);
    if !p.is_null() {
        unsafe { crate::strmem::memset_volatile(p as *mut u8, 0, bytes) };
    }
    p
}

#[no_mangle]
pub extern "C" fn posix_memalign(out: *mut *mut c_void, align: size_t, n: size_t) -> c_int {
    if align == 0 || (align & (align - 1)) != 0 {
        return crate::EINVAL;
    }
    if align <= 16 {
        unsafe { *out = malloc(n) };
        return if unsafe { *out }.is_null() { crate::ENOMEM } else { 0 };
    }
    // 超对齐：过量分配 + 对齐落位；前缀 [原块, 对齐点) 永久泄漏（罕见路径，
    // memalign 语义可接受——free 走对齐点上的假头，size 记到原块尾）。
    unsafe {
        let base = inner_malloc(n + align + core::mem::size_of::<Block>());
        if base.is_null() {
            *out = ptr::null_mut();
            return crate::ENOMEM;
        }
        let orig_hdr = (base as *mut Block).sub(1);
        let end = block_end(orig_hdr);
        let p = (base as usize + align - 1) & !(align - 1);
        let hdr = (p as *mut Block).sub(1);
        (*hdr).size = end - p - core::mem::size_of::<Block>();
        (*hdr).next = ptr::null_mut();
        *out = p as *mut c_void;
    }
    0
}

/// 堆余量（调试/`STD ok` 自检面可读）。
pub fn heap_free_bytes() -> usize {
    unsafe {
        if FREE_HEAD.is_null() {
            return HEAP_SIZE - core::mem::size_of::<Block>();
        }
        let mut cur = FREE_HEAD;
        let mut sum = 0usize;
        while !cur.is_null() {
            sum += (*cur).size;
            cur = (*cur).next;
        }
        sum
    }
}
