//! 本仓 mem* 整数版覆写（GUI-0「内核 SIMD-free 合同」的 builtins 半边）。
//!
//! 为什么：compiler-builtins 预编译 rlib 的 mem* 家族形态随工具链版本漂移——
//! rustc 1.97.0 实测仅 `memset` 是 NEON（`dup v0.4h` 反汇编实证，2026-10-29），
//! memcpy/memmove/memcmp/bcmp/strlen 暂为 GPR WEAK 版；但仓无 rust-toolchain 钉版、
//! rustflags 又对预编译 rlib 无效（只有链接期强符号这道防线不随版本漂）——内核
//! 任何 copy/zero/比较路径链到 NEON 版都会踩 EL0 任务的 FPSIMD 态。本 crate 的
//! 四件 + `bcmp` 强定义先于 rlib 按需拉入，链接期即令 builtins 版出局（「哪些件
//! 今天恰好是 NEON」不进合同——全覆 = 对形态漂移钝感）；`bcmp` 必须一并定义：
//! LLVM 把 `memcmp(a,b,n)==0` 标准变换为 bcmp 调用，漏了它相等比较仍可能链到
//! builtins 版（contract-1 修复轮钉板）；`strlen` 无生产者（core 不下界调用、
//! 仓内无 C 串路径）不定义。配合 `.cargo/config.toml` 的
//! `[target.aarch64-unknown-none] rustflags = -C target-feature=-fp-armv8,-neon`
//! （本仓 crate 全 GPR codegen）构成「内核 SIMD-free」的两层机制，第三层 =
//! ktest FP canary 每次开机执法。core fmt 链（pad/do_count_chars 等）仍含 NEON，
//! 但仅 panic（= halt）路径可达——死后态无保存义务。
//!
//! host 测试面不链接本模块（`target_os = "none"` 门控——macOS 下与 libSystem
//! 符号冲突）；正确性由 ktest `test_fp` ③ 直验 + 每次 boot 的全量 copy/zero 路径
//! 事实覆盖（VMO/装载走查全是我们的实现）。
//!
//! 实现纪律：**正确性优先**——8 字节块（`read_unaligned`/`write_unaligned`，
//! 不赌对齐）+ 字节尾；`memmove` 按方向处理重叠；`memcmp` 命中差异后块内逐字节
//! 定位首异字节（LE 装载下低地址字节在低位，逐字节比较即得 memcmp 语义）。
//! TCG 下数量级足够（4KiB 零填 ≈ 512 块迭代）；性能回归走 B-2 icount 通道重定标。

#[cfg(target_os = "none")]
mod imp {
    /// `void *memset(void *s, int c, size_t n)`。
    #[no_mangle]
    pub extern "C" fn memset(dst: *mut u8, c: u8, n: usize) -> *mut u8 {
        let word = (c as u64) * 0x0101_0101_0101_0101;
        let mut p = dst;
        let mut rem = n;
        // SAFETY（调用方合同）：dst 指向 n 可写字节；块写经 unaligned 原语不赌对齐。
        unsafe {
            while rem >= 8 {
                (p as *mut u64).write_unaligned(word);
                p = p.add(8);
                rem -= 8;
            }
            while rem > 0 {
                *p = c;
                p = p.add(1);
                rem -= 1;
            }
        }
        dst
    }

    /// `void *memcpy(void *dst, const void *src, size_t n)`（不重叠——调用方合同）。
    #[no_mangle]
    pub extern "C" fn memcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
        let mut d = dst;
        let mut s = src;
        let mut rem = n;
        // SAFETY（调用方合同）：区间有效且不重叠。
        unsafe {
            while rem >= 8 {
                (d as *mut u64).write_unaligned((s as *const u64).read_unaligned());
                d = d.add(8);
                s = s.add(8);
                rem -= 8;
            }
            while rem > 0 {
                *d = *s;
                d = d.add(1);
                s = s.add(1);
                rem -= 1;
            }
        }
        dst
    }

    /// `void *memmove(void *dst, const void *src, size_t n)`（允许重叠）。
    #[no_mangle]
    pub extern "C" fn memmove(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
        // SAFETY：与 memcpy 同，重叠时按方向分治——前向（dst < src）正拷、
        // 后向（dst > src）从尾逆拷，逐块序保证不自我覆盖。
        unsafe {
            if (dst as usize) < (src as usize) {
                memcpy(dst, src, n)
            } else if (dst as usize) > (src as usize) {
                let mut d = dst.add(n);
                let mut s = src.add(n);
                let mut rem = n;
                while rem >= 8 {
                    d = d.sub(8);
                    s = s.sub(8);
                    (d as *mut u64).write_unaligned((s as *const u64).read_unaligned());
                    rem -= 8;
                }
                while rem > 0 {
                    d = d.sub(1);
                    s = s.sub(1);
                    *d = *s;
                    rem -= 1;
                }
                dst
            } else {
                dst
            }
        }
    }

    /// `int memcmp(const void *a, const void *b, size_t n)`——相等快路（u64 块）+
    /// 首异字节逐字节定位（返回 = 首异字节差的符号语义）。
    #[no_mangle]
    pub extern "C" fn memcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
        let mut pa = a;
        let mut pb = b;
        let mut rem = n;
        // SAFETY：区间 n 可读。
        unsafe {
            while rem >= 8 {
                let wa = (pa as *const u64).read_unaligned();
                let wb = (pb as *const u64).read_unaligned();
                if wa != wb {
                    // LE 装载：块首字节在最低位——逐字节升序比较即 memcmp 语义。
                    for i in 0..8 {
                        let xa = ((wa >> (i * 8)) & 0xFF) as u8;
                        let xb = ((wb >> (i * 8)) & 0xFF) as u8;
                        if xa != xb {
                            return xa as i32 - xb as i32;
                        }
                    }
                    // 两字 word 不等但 8 字节全等 = 不可能；兜底继续。
                }
                pa = pa.add(8);
                pb = pb.add(8);
                rem -= 8;
            }
            while rem > 0 {
                let xa = *pa;
                let xb = *pb;
                if xa != xb {
                    return xa as i32 - xb as i32;
                }
                pa = pa.add(1);
                pb = pb.add(1);
                rem -= 1;
            }
        }
        0
    }

    /// `int bcmp(const void *a, const void *b, size_t n)`——memcmp 的 0/非 0
    /// 伴生（相等面 0、差异面非 0，符号语义不作承诺）。必须一并覆写：LLVM 把
    /// `memcmp(a,b,n)==0` 标准变换为 bcmp 调用——漏定义它，内核的相等比较仍会
    /// 链到 builtins 版（contract-1 修复轮钉板）。复用 memcmp 实现。
    #[no_mangle]
    pub extern "C" fn bcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
        memcmp(a, b, n)
    }
}

#[cfg(not(target_os = "none"))]
pub mod _host_note {
    //! host 面（`cargo test --target <host>`）不覆写 mem*——与 libSystem 符号冲突；
    //! 正确性由 ktest `test_fp` ③（target 面）直验。
}
