//! mem/str 族 —— 真实现档（G-eco-d 附录 A）
//!
//! **volatile 纪律**：memcpy/memset/strlen 等的朴素 Rust 循环会被 LLVM
//! 「惯用形识别」升格回对同名 libc 的调用（自递归）；逐字节/逐 u64 的
//! `read_volatile`/`write_volatile` 不可识别，代价是慢——TCG 下无所谓，
//! 换来零 musl 字节 + 零递归。u64 快路只在 src/dst 都 8 对齐时启用。

use core::ptr;

use crate::{c_char, c_int, c_void, size_t};

pub(crate) unsafe fn memcpy_volatile(mut dst: *mut u8, mut src: *const u8, mut n: usize) {
    unsafe {
        if src as usize & 7 == 0 && dst as usize & 7 == 0 {
            while n >= 8 {
                ptr::write_volatile(dst as *mut u64, ptr::read_volatile(src as *const u64));
                dst = dst.add(8);
                src = src.add(8);
                n -= 8;
            }
        }
        while n > 0 {
            ptr::write_volatile(dst, ptr::read_volatile(src));
            dst = dst.add(1);
            src = src.add(1);
            n -= 1;
        }
    }
}

pub(crate) unsafe fn memset_volatile(mut dst: *mut u8, val: u8, mut n: usize) {
    unsafe {
        let w = (val as u64) * 0x0101_0101_0101_0101;
        if dst as usize & 7 == 0 {
            while n >= 8 {
                ptr::write_volatile(dst as *mut u64, w);
                dst = dst.add(8);
                n -= 8;
            }
        }
        while n > 0 {
            ptr::write_volatile(dst, val);
            dst = dst.add(1);
            n -= 1;
        }
    }
}

unsafe fn memmove_volatile(dst: *mut u8, src: *const u8, n: usize) {
    unsafe {
        if (dst as usize) < (src as usize) || dst as usize >= src as usize + n {
            memcpy_volatile(dst, src, n);
        } else {
            // 重叠且 dst > src：从尾往前拷
            let mut d = dst.add(n);
            let mut s = src.add(n);
            let mut left = n;
            while left > 0 {
                d = d.sub(1);
                s = s.sub(1);
                ptr::write_volatile(d, ptr::read_volatile(s));
                left -= 1;
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn memcpy(dst: *mut c_void, src: *const c_void, n: size_t) -> *mut c_void {
    unsafe { memcpy_volatile(dst as *mut u8, src as *const u8, n) };
    dst
}

#[no_mangle]
pub extern "C" fn memmove(dst: *mut c_void, src: *const c_void, n: size_t) -> *mut c_void {
    unsafe { memmove_volatile(dst as *mut u8, src as *const u8, n) };
    dst
}

#[no_mangle]
pub extern "C" fn memset(dst: *mut c_void, val: c_int, n: size_t) -> *mut c_void {
    unsafe { memset_volatile(dst as *mut u8, val as u8, n) };
    dst
}

unsafe fn byte_at(p: *const u8, i: usize) -> u8 {
    unsafe { ptr::read_volatile(p.add(i)) }
}

#[no_mangle]
pub extern "C" fn memcmp(a: *const c_void, b: *const c_void, n: size_t) -> c_int {
    unsafe {
        let (a, b) = (a as *const u8, b as *const u8);
        for i in 0..n {
            let (x, y) = (byte_at(a, i), byte_at(b, i));
            if x != y {
                return x as c_int - y as c_int;
            }
        }
        0
    }
}

/// musl 的 bcmp 返回值语义 = memcmp 同形（非零 = 不等，符号无承诺）。
#[no_mangle]
pub extern "C" fn bcmp(a: *const c_void, b: *const c_void, n: size_t) -> c_int {
    memcmp(a, b, n)
}

#[no_mangle]
pub extern "C" fn strlen(s: *const c_char) -> size_t {
    unsafe {
        let mut i = 0usize;
        while byte_at(s as *const u8, i) != 0 {
            i += 1;
        }
        i
    }
}

#[no_mangle]
pub extern "C" fn strnlen(s: *const c_char, max: size_t) -> size_t {
    unsafe {
        let mut i = 0usize;
        while i < max && byte_at(s as *const u8, i) != 0 {
            i += 1;
        }
        i
    }
}

#[no_mangle]
pub extern "C" fn strcmp(a: *const c_char, b: *const c_char) -> c_int {
    unsafe {
        let (a, b) = (a as *const u8, b as *const u8);
        let mut i = 0usize;
        loop {
            let (x, y) = (byte_at(a, i), byte_at(b, i));
            if x != y || x == 0 {
                return x as c_int - y as c_int;
            }
            i += 1;
        }
    }
}

#[no_mangle]
pub extern "C" fn strchr(s: *const c_char, c: c_int) -> *const c_char {
    unsafe {
        let c = c as u8;
        let s = s as *const u8;
        let mut i = 0usize;
        loop {
            let b = byte_at(s, i);
            if b == c {
                return s.add(i) as *const c_char;
            }
            if b == 0 {
                return ptr::null();
            }
            i += 1;
        }
    }
}

#[no_mangle]
pub extern "C" fn strrchr(s: *const c_char, c: c_int) -> *const c_char {
    unsafe {
        let n = strlen(s);
        let c = c as u8;
        let s = s as *const u8;
        let mut i = n;
        loop {
            if byte_at(s, i) == c {
                return s.add(i) as *const c_char;
            }
            if i == 0 {
                return ptr::null();
            }
            i -= 1;
        }
    }
}

#[no_mangle]
pub extern "C" fn strncpy(dst: *mut c_char, src: *const c_char, n: size_t) -> *mut c_char {
    unsafe {
        let src_len = strnlen(src, n);
        memcpy_volatile(dst as *mut u8, src as *const u8, src_len);
        memset_volatile(dst.add(src_len) as *mut u8, 0, n - src_len);
        dst
    }
}

#[no_mangle]
pub extern "C" fn strdup(s: *const c_char) -> *mut c_char {
    unsafe {
        let n = strlen(s);
        let p = crate::alloc::malloc(n + 1);
        if p.is_null() {
            return ptr::null_mut();
        }
        memcpy_volatile(p as *mut u8, s as *const u8, n + 1);
        p as *mut c_char
    }
}

#[no_mangle]
pub extern "C" fn atoi(s: *const c_char) -> c_int {
    unsafe {
        let s = s as *const u8;
        let mut i = 0usize;
        while byte_at(s, i) == b' ' {
            i += 1;
        }
        let mut neg = false;
        match byte_at(s, i) {
            b'-' => neg = true,
            b'+' => {}
            _ => i = i.wrapping_sub(1),
        }
        i += 1;
        let mut v: i64 = 0;
        loop {
            let b = byte_at(s, i);
            if !(b as char).is_ascii_digit() {
                break;
            }
            v = v * 10 + (b - b'0') as i64;
            if v > i32::MAX as i64 + 1 {
                v = i32::MAX as i64 + 1;
            }
            i += 1;
        }
        let v = if neg { -v } else { v };
        v.clamp(i32::MIN as i64, i32::MAX as i64) as c_int
    }
}
