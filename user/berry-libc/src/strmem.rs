//! mem/str 族 —— C-ABI 薄壳层（语义单源已外迁 berry-strmem）
//!
//! **volatile 纪律**（语义源 = berry-strmem `*_sem`，berry-tls/mmap/
//! signal/winplan/argv/spawngeo「纯面外迁」先例族第 7 例）：memcpy/
//! memset/strlen 等的朴素 Rust 循环会被 LLVM「惯用形识别」升格回对同名
//! libc 的调用（自递归）；逐字节/逐 u64 的 `read_volatile`/
//! `write_volatile` 不可识别，代价是慢——TCG 下无所谓，换来零 musl
//! 字节 + 零递归。u64 快路只在 src/dst 都 8 对齐时启用。
//!
//! 本文件只保留 `#[no_mangle]` C 符号面（名/签名/返回值零漂移），函数
//! 体全部转发 berry-strmem 单源；host 测试在 berry-strmem 侧（本 crate
//! 结构性不可 cargo test——crt0 global_asm darwin 拒编，见 lib.rs
//! errno_probe 头注实测在册）。

use berry_strmem::{
    atoi_sem, memcmp_sem, memcpy_sem, memmove_sem, memset_sem, strchr_sem, strcmp_sem, strlen_sem,
    strnlen_sem, strrchr_sem,
};

use crate::{c_char, c_int, c_void, size_t};

pub(crate) unsafe fn memcpy_volatile(dst: *mut u8, src: *const u8, n: usize) {
    memcpy_sem(dst, src, n)
}

pub(crate) unsafe fn memset_volatile(dst: *mut u8, val: u8, n: usize) {
    memset_sem(dst, val, n)
}

#[no_mangle]
pub extern "C" fn memcpy(dst: *mut c_void, src: *const c_void, n: size_t) -> *mut c_void {
    unsafe { memcpy_volatile(dst as *mut u8, src as *const u8, n) };
    dst
}

#[no_mangle]
pub extern "C" fn memmove(dst: *mut c_void, src: *const c_void, n: size_t) -> *mut c_void {
    unsafe { memmove_sem(dst as *mut u8, src as *const u8, n) };
    dst
}

#[no_mangle]
pub extern "C" fn memset(dst: *mut c_void, val: c_int, n: size_t) -> *mut c_void {
    unsafe { memset_volatile(dst as *mut u8, val as u8, n) };
    dst
}

#[no_mangle]
pub extern "C" fn memcmp(a: *const c_void, b: *const c_void, n: size_t) -> c_int {
    unsafe { memcmp_sem(a as *const u8, b as *const u8, n) }
}

/// musl 的 bcmp 返回值语义 = memcmp 同形（非零 = 不等，符号无承诺）。
#[no_mangle]
pub extern "C" fn bcmp(a: *const c_void, b: *const c_void, n: size_t) -> c_int {
    memcmp(a, b, n)
}

#[no_mangle]
pub extern "C" fn strlen(s: *const c_char) -> size_t {
    strlen_sem(s as *const u8)
}

#[no_mangle]
pub extern "C" fn strnlen(s: *const c_char, max: size_t) -> size_t {
    strnlen_sem(s as *const u8, max)
}

#[no_mangle]
pub extern "C" fn strcmp(a: *const c_char, b: *const c_char) -> c_int {
    strcmp_sem(a as *const u8, b as *const u8)
}

#[no_mangle]
pub extern "C" fn strchr(s: *const c_char, c: c_int) -> *const c_char {
    match strchr_sem(s as *const u8, c as u8) {
        Some(i) => unsafe { (s as *const u8).add(i) as *const c_char },
        None => core::ptr::null(),
    }
}

#[no_mangle]
pub extern "C" fn strrchr(s: *const c_char, c: c_int) -> *const c_char {
    match strrchr_sem(s as *const u8, c as u8) {
        Some(i) => unsafe { (s as *const u8).add(i) as *const c_char },
        None => core::ptr::null(),
    }
}

#[no_mangle]
pub extern "C" fn strncpy(dst: *mut c_char, src: *const c_char, n: size_t) -> *mut c_char {
    unsafe { berry_strmem::strncpy_sem(dst as *mut u8, src as *const u8, n) };
    dst
}

#[no_mangle]
pub extern "C" fn strdup(s: *const c_char) -> *mut c_char {
    unsafe {
        let n = strlen_sem(s as *const u8);
        let p = crate::alloc::malloc(n + 1);
        if p.is_null() {
            return core::ptr::null_mut();
        }
        memcpy_sem(p as *mut u8, s as *const u8, n + 1);
        p as *mut c_char
    }
}

#[no_mangle]
pub extern "C" fn atoi(s: *const c_char) -> c_int {
    atoi_sem(s as *const u8)
}
