//! # berry-strmem —— mem/str 族字节序语义单源（纯函数面）
//!
//! ## Overview (for crates.io users)
//!
//! `mem`/`str` family semantics for the [BerryOS](https://github.com/miuiadmin/BerryOS)
//! microkernel libc shim, extracted as pure functions: volatile-disciplined
//! `memcpy`/`memmove`/`memset` (with a both-8-aligned `u64` fast path),
//! byte-order `memcmp`/`bcmp`, `strlen`/`strnlen`, `strcmp`,
//! `strchr`/`strrchr` scanning, `strncpy` zero-padding law, and `atoi`
//! with overflow clamping. The consumer is berry-libc `strmem.rs` (thin
//! `#[no_mangle]` C-ABI shells over these); this crate holds no syscall
//! face. no_std core; builds and unit-tests on any host.
//!
//! Status: experimental (0.1.0) — the API moves with the BerryOS v1
//! userland ABI. Repo docs are mostly zh-CN.
//!
//! ## 为什么独立 crate（berry-tls/mmap/signal/winplan/argv/spawngeo 先例族）
//!
//! berry-libc 结构性不可 `cargo test`：crt0 `global_asm!` 的 ADRP/`:lo12:`
//! 是 ELF 目标写法，darwin 汇编器拒非 GOT 形式（lib.rs errno_probe 头注
//! 实测在册）。strmem 面因此零测试。外迁后语义单源在此，berry-libc 仅留
//! 薄壳——**对外 C 符号面零漂移**（`#[no_mangle]` 名、签名、返回值全在
//! 原处），「不长第二套」。
//!
//! ## volatile 纪律（法证教训一级防御）
//!
//! memcpy/memset/strlen 等的朴素 Rust 循环会被 LLVM「惯用形识别」升格回
//! 对同名 libc 的调用（自递归）；逐字节/逐 u64 的 `read_volatile`/
//! `write_volatile` 不可识别，代价是慢——TCG 下无所谓，换来零 musl 字节
//! + 零递归。u64 快路只在 src/dst 都 8 对齐时启用（未对齐走逐字节，无
//! UB 面）。
//!
//! ## 语义源（迁出时逐字等价，勿漂移）
//!
//! 本文件函数体 = berry-libc strmem.rs 的 `*_volatile` 内部函数逐字迁居
//! （2026-11-10 外迁）；消费端 berry-libc strmem.rs 改薄壳调用。改语义
//! 双侧同步：本 crate 改语义源，berry-libc 壳零改动自动跟随。

// berry-mmap/berry-argv 同款：非 test 构建无 std（workspace none-target
// 成员），host 测试构建放开 std（tests/ 独立 crate 本就带 std）。
#![cfg_attr(not(test), no_std)]

use core::ptr;

/// `memcpy` 语义体：dst/src 都 8 对齐时 u64 快路，尾段逐字节 volatile。
///
/// 调用方（berry-libc `#[no_mangle]` 壳）负责区间不重叠契约；重叠面走
/// [`memmove_sem`]。
pub fn memcpy_sem(dst: *mut u8, src: *const u8, n: usize) {
    let mut dst = dst;
    let mut src = src;
    let mut n = n;
    if src as usize & 7 == 0 && dst as usize & 7 == 0 {
        while n >= 8 {
            unsafe {
                ptr::write_volatile(dst as *mut u64, ptr::read_volatile(src as *const u64));
            }
            dst = unsafe { dst.add(8) };
            src = unsafe { src.add(8) };
            n -= 8;
        }
    }
    while n > 0 {
        unsafe {
            ptr::write_volatile(dst, ptr::read_volatile(src));
        }
        dst = unsafe { dst.add(1) };
        src = unsafe { src.add(1) };
        n -= 1;
    }
}

/// `memset` 语义体：字节模式扩成 u64 重复字（0x01 ×8），8 对齐快路 +
/// 尾段逐字节 volatile。
pub fn memset_sem(dst: *mut u8, val: u8, n: usize) {
    let mut dst = dst;
    let mut n = n;
    let w = (val as u64) * 0x0101_0101_0101_0101;
    if dst as usize & 7 == 0 {
        while n >= 8 {
            unsafe { ptr::write_volatile(dst as *mut u64, w) };
            dst = unsafe { dst.add(8) };
            n -= 8;
        }
    }
    while n > 0 {
        unsafe { ptr::write_volatile(dst, val) };
        dst = unsafe { dst.add(1) };
        n -= 1;
    }
}

/// `memmove` 语义体：不重叠或 dst < src 正向拷；重叠且 dst > src 从尾
/// 往前拷（前向拷会边写边读脏源）。
pub fn memmove_sem(dst: *mut u8, src: *const u8, n: usize) {
    if (dst as usize) < (src as usize) || dst as usize >= src as usize + n {
        memcpy_sem(dst, src, n);
    } else {
        // 重叠且 dst > src：从尾往前拷
        let mut d = unsafe { dst.add(n) };
        let mut s = unsafe { src.add(n) };
        let mut left = n;
        while left > 0 {
            d = unsafe { d.sub(1) };
            s = unsafe { s.sub(1) };
            unsafe { ptr::write_volatile(d, ptr::read_volatile(s)) };
            left -= 1;
        }
    }
}

/// 单字节 volatile 读（LLVM 惯用形识别防御的读侧底座）。
pub fn byte_at_sem(p: *const u8, i: usize) -> u8 {
    unsafe { ptr::read_volatile(p.add(i)) }
}

/// `memcmp` 语义体：首个不等字节的无符号差（`x as c_int - y as c_int`，
/// musl 同形符号律）；全等返 0。
pub fn memcmp_sem(a: *const u8, b: *const u8, n: usize) -> i32 {
    for i in 0..n {
        let (x, y) = (byte_at_sem(a, i), byte_at_sem(b, i));
        if x != y {
            return x as i32 - y as i32;
        }
    }
    0
}

/// `bcmp` 语义体：musl 返回值语义 = memcmp 同形（非零 = 不等，符号无
/// 承诺）。v1 直接转发 memcmp（同构实现无收益）。
pub fn bcmp_sem(a: *const u8, b: *const u8, n: usize) -> i32 {
    memcmp_sem(a, b, n)
}

/// `strlen` 语义体：NUL 前字节数（NUL 不计）。
pub fn strlen_sem(s: *const u8) -> usize {
    let mut i = 0usize;
    while byte_at_sem(s, i) != 0 {
        i += 1;
    }
    i
}

/// `strnlen` 语义体：`min(strlen, max)`——max 内无 NUL 即返 max。
pub fn strnlen_sem(s: *const u8, max: usize) -> usize {
    let mut i = 0usize;
    while i < max && byte_at_sem(s, i) != 0 {
        i += 1;
    }
    i
}

/// `strcmp` 语义体：逐字节比到首差或双 NUL——首差 = 无符号字节差；
/// 双 NUL 同字节 = 差恰 0（`0 - 0`）。
pub fn strcmp_sem(a: *const u8, b: *const u8) -> i32 {
    let mut i = 0usize;
    loop {
        let (x, y) = (byte_at_sem(a, i), byte_at_sem(b, i));
        if x != y || x == 0 {
            return x as i32 - y as i32;
        }
        i += 1;
    }
}

/// `strchr` 语义体：c 的低 8 位（`c_int` 截 u8——musl 同形）；命中返
/// 偏移（`Some(i)`），串尾 NUL 前无命中返 `None`。
pub fn strchr_sem(s: *const u8, c: u8) -> Option<usize> {
    let mut i = 0usize;
    loop {
        let b = byte_at_sem(s, i);
        if b == c {
            return Some(i);
        }
        if b == 0 {
            return None;
        }
        i += 1;
    }
}

/// `strrchr` 语义体：从 NUL 尾往头扫，首个命中即最右；无命中返 None。
/// 注意 `c == 0` 时恒命中尾 NUL（`strlen` 位点）——C 语义同形。
pub fn strrchr_sem(s: *const u8, c: u8) -> Option<usize> {
    let n = strlen_sem(s);
    let mut i = n;
    loop {
        if byte_at_sem(s, i) == c {
            return Some(i);
        }
        if i == 0 {
            return None;
        }
        i -= 1;
    }
}

/// `strncpy` 语义体：拷 `min(strnlen(src,n), n)` 字节后，**余量整段
/// 零填到恰 n 字节**（C 标准的 pad-to-n 律，不是 NUL 终结一条）。
/// 返回实际拷贝字节数（不含零填），供壳层与调用方对账。
pub fn strncpy_sem(dst: *mut u8, src: *const u8, n: usize) -> usize {
    let src_len = strnlen_sem(src, n);
    memcpy_sem(dst, src, src_len);
    memset_sem(unsafe { dst.add(src_len) }, 0, n - src_len);
    src_len
}

/// `atoi` 语义体：前导空格（仅 `' '`——musl 同形，不含 \t/\n 族）+
/// 一元符号 + 数字串；无数字返 0（含仅符号面）；溢出夹紧
/// `[i32::MIN, i32::MAX]`（i64 中转，饱和点 `i32::MAX+1` 单调夹）。
pub fn atoi_sem(s: *const u8) -> i32 {
    let mut i = 0usize;
    while byte_at_sem(s, i) == b' ' {
        i += 1;
    }
    let mut neg = false;
    match byte_at_sem(s, i) {
        b'-' => neg = true,
        b'+' => {}
        // 无符号面：退一格（下面 +1 回到原位）。原实现 `wrapping_sub` 后
        // 紧接裸 `+= 1`——i=0 时 MAX+1 在 debug 构建（overflow-checks 开）
        // 直接 panic，release 静默回绕 0（原意图）。berry-libc 结构性
        // 不可 host test 故从未暴露；外迁首测即咬。改 `wrapping_add`：
        // release 字节语义不变，debug 不再炸。
        _ => i = i.wrapping_sub(1),
    }
    i = i.wrapping_add(1);
    let mut v: i64 = 0;
    loop {
        let b = byte_at_sem(s, i);
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
    v.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}
