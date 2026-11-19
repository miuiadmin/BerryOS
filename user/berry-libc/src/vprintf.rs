//! snprintf/fprintf —— asm 变参蹦床 + % 子集解析器
//!
//! stable Rust 不能声明 C 变参函数；但 AArch64 变参与普通调用**传参同形**
//! （实参照常进 x0..x7，无 x86-64 的 al 计数约定）——蹦床把命名参之后的
//! 变参寄存器摊到栈上成 u64 数组，交 Rust 解析器按自家约定的「grva」数组
//! 消费。栈传参（第 6+ 个变参）不支持：解析器耗尽 grva 后按无参处理
//! （`%s` 打 `(null)`、数值打 0——可见降级不崩）。
//!
//! **FP 变参在 q0..q7**，蹦床不摊——`%f/%g/%e` 族打 `?`（纯 Rust 程序的
//! libc 格式面不该见到浮点；诚实缺口，v1 收口）。

use core::arch::global_asm;

use crate::{c_char, c_int, size_t, FILE};

global_asm!(
    ".globl snprintf",
    "snprintf:",                 // (buf=x0, size=x1, fmt=x2, ... 变参 x3..x7)
    "    stp x29, x30, [sp, #-112]!",
    "    stp x3, x4, [sp, #16]",
    "    stp x5, x6, [sp, #32]",
    "    str x7,      [sp, #48]",
    "    add x4, sp, #16",        // grva 基址（5 槽）
    "    bl  berry_snprintf_impl",
    "    ldp x29, x30, [sp], #112",
    "    ret",
    ".globl fprintf",
    "fprintf:",                   // (f=x0, fmt=x1, ... 变参 x2..x7)
    "    stp x29, x30, [sp, #-112]!",
    "    stp x2, x3, [sp, #16]",
    "    stp x4, x5, [sp, #32]",
    "    str x6,      [sp, #48]",
    "    str x7,      [sp, #56]",
    "    mov x2, x1",             // fmt
    "    add x3, sp, #16",        // grva 基址（6 槽）
    "    bl  berry_fprintf_impl",
    "    ldp x29, x30, [sp], #112",
    "    ret",
);

/// grva：变参整型槽视图（蹦床摊好的 x 寄存器数组）。
struct Grva {
    base: *const u64,
    left: usize,
}

impl Grva {
    fn take(&mut self) -> u64 {
        if self.left == 0 {
            return 0; // 耗尽 = 栈传参不支持，打缺省值
        }
        self.left -= 1;
        unsafe { core::ptr::read(self.base.add(self.left)) }
    }
}

/// 十进制无符号 → ASCII（`utoa_dec(v, buf)` 返回写入切片；__assert_fail 也用）。
pub fn utoa_dec(mut v: u64, buf: &mut [u8; 24]) -> &[u8] {
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    &buf[i..]
}

fn utoa_hex(mut v: u64, upper: bool, buf: &mut [u8; 24]) -> &[u8] {
    let digits = if upper { b"0123456789ABCDEF" } else { b"0123456789abcdef" };
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = digits[(v & 0xF) as usize];
        v >>= 4;
        if v == 0 {
            break;
        }
    }
    &buf[i..]
}

/// 长度修饰符 + 转换字符 → 取值宽度。返回**含 '%' 的整段消费字节数**
/// （入参 i0 = '%' 后一位；j 终于转换字符位）。
unsafe fn format_one(fmt: &[u8], i0: usize, g: &mut Grva, emit: &mut dyn FnMut(&[u8])) -> usize {
    let mut j = i0;
    // flags / width / precision（解析跳过，不实现补齐；* 宽度吃一个 grva）
    while j < fmt.len() && matches!(fmt[j], b'-' | b'+' | b' ' | b'#' | b'0') {
        j += 1;
    }
    if j < fmt.len() && fmt[j] == b'*' {
        g.take();
        j += 1;
    } else {
        while j < fmt.len() && fmt[j].is_ascii_digit() {
            j += 1;
        }
    }
    if j < fmt.len() && fmt[j] == b'.' {
        j += 1;
        if j < fmt.len() && fmt[j] == b'*' {
            g.take();
            j += 1;
        } else {
            while j < fmt.len() && fmt[j].is_ascii_digit() {
                j += 1;
            }
        }
    }
    // 长度修饰符
    let mut longs = 0usize;
    let mut halfs = 0usize;
    let mut zet = false;
    while j < fmt.len() {
        match fmt[j] {
            b'l' => longs += 1,
            b'h' => halfs += 1,
            b'z' | b'j' | b't' => {
                longs = 1;
                zet = true;
            }
            b'L' => {}
            _ => break,
        }
        j += 1;
    }
    let conv = if j < fmt.len() { fmt[j] } else { 0 };

    let take_int = |g: &mut Grva| -> u64 {
        let raw = g.take();
        // 窄宽度按 C 整型提升语义截断取符号
        if longs >= 2 || zet || longs >= 1 && core::mem::size_of::<usize>() == 8 {
            raw
        } else if halfs >= 2 {
            raw as i8 as i64 as u64
        } else if halfs == 1 {
            raw as i16 as i64 as u64
        } else {
            raw as i32 as i64 as u64
        }
    };

    let mut nb = [0u8; 24];
    match conv {
        b'%' => emit(b"%"),
        b'd' | b'i' => {
            let v = take_int(g) as i64;
            let s = if v < 0 {
                emit(b"-");
                utoa_dec(v.wrapping_neg() as u64, &mut nb)
            } else {
                utoa_dec(v as u64, &mut nb)
            };
            emit(s);
        }
        b'u' => {
            let s = utoa_dec(take_int(g), &mut nb);
            emit(s);
        }
        b'x' | b'X' => {
            let s = utoa_hex(take_int(g), conv == b'X', &mut nb);
            emit(s);
        }
        b'o' => {
            let v = take_int(g);
            let mut oct = [0u8; 24];
            let mut i2 = oct.len();
            let mut x = v;
            loop {
                i2 -= 1;
                oct[i2] = b'0' + (x & 7) as u8;
                x >>= 3;
                if x == 0 {
                    break;
                }
            }
            emit(&oct[i2..]);
        }
        b'c' => {
            let c = take_int(g) as u8;
            emit(&[c]);
        }
        b's' => {
            let p = g.take() as *const c_char;
            if p.is_null() {
                emit(b"(null)");
            } else {
                let n = crate::strmem::strlen(p);
                let s = unsafe { core::slice::from_raw_parts(p as *const u8, n) };
                emit(s);
            }
        }
        b'p' => {
            let v = g.take();
            emit(b"0x");
            let s = utoa_hex(v, false, &mut nb);
            emit(s);
        }
        b'f' | b'F' | b'g' | b'G' | b'e' | b'E' => {
            // FP 变参在 q 寄存器，蹦床不摊——诚实降级
            emit(b"?");
        }
        0 => return j - i0 + 2, // 串尾孤立 '%'：消费到底（外层 i 越界退出）
        _ => {
            emit(b"%");
            emit(&fmt[j..j + 1]);
        }
    }
    j - i0 + 2
}

unsafe fn vformat(fmt: *const c_char, grva: *const u64, ngr: usize, emit: &mut dyn FnMut(&[u8])) {
    unsafe {
        let n = crate::strmem::strlen(fmt);
        let fmt = core::slice::from_raw_parts(fmt as *const u8, n);
        let mut g = Grva { base: grva, left: ngr };
        let mut i = 0usize;
        while i < fmt.len() {
            if fmt[i] == b'%' {
                i += format_one(fmt, i + 1, &mut g, emit);
            } else {
                let start = i;
                while i < fmt.len() && fmt[i] != b'%' {
                    i += 1;
                }
                emit(&fmt[start..i]);
            }
        }
    }
}

#[no_mangle]
unsafe extern "C" fn berry_snprintf_impl(
    buf: *mut c_char,
    size: size_t,
    fmt: *const c_char,
    grva: *const u64,
) -> c_int {
    let mut total: usize = 0;
    let mut cap = size.saturating_sub(1); // size==0 → 0（不写含 NUL）
    let mut out = |bytes: &[u8]| {
        for &b in bytes {
            if cap > 0 {
                unsafe { *buf.add(total) = b as c_char };
                cap -= 1;
            }
            total += 1; // 返回「本应写满」的长度（C 语义）
        }
    };
    unsafe { vformat(fmt, grva, 5, &mut out) };
    if size > 0 {
        unsafe { *buf.add(total.min(size - 1)) = 0 };
    }
    total as c_int
}

#[no_mangle]
unsafe extern "C" fn berry_fprintf_impl(f: *mut FILE, fmt: *const c_char, grva: *const u64) -> c_int {
    let mut total: usize = 0;
    let mut out = |bytes: &[u8]| {
        crate::io::console_write(bytes);
        total += bytes.len();
    };
    let _ = f; // FILE* 不看：console 是唯一后端（fwrite 同理）
    unsafe { vformat(fmt, grva, 6, &mut out) };
    total as c_int
}
