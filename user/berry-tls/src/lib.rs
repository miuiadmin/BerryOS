//! # berry-tls —— TLS 拷贝式放置纯函数面（T-1 §1）
//!
//! ## Overview (for crates.io users)
//!
//! Copy-style TLS (thread-local storage) placement math for the
//! [BerryOS](https://github.com/miuiadmin/BerryOS) microkernel: ELF
//! `PT_TLS` walk, placement arithmetic, and template globals — pure
//! functions, host-testable. Main thread (crt0) and pthread trampolines
//! share the same placement arithmetic; the consumer is berry-libc.
//! Zero arch-specific code: no_std core, builds and unit-tests on any
//! host.
//!
//! Status: experimental (0.1.0) — the API moves with the BerryOS v1
//! userland ABI. Repo docs are mostly zh-CN.
//!
//! 崩因背景：原位 TP 方案下 `.tbss` 零初始化契约被三方共违约（lld 标准布局
//! + berry-exec 零 PT_TLS 感知 + crt0 原位直用），TLS 胞元初值 = 活
//! `.data.rel.ro` 文件字节 → 对齐 D-abort——详见
//! `docs/法证-TLS原位方案崩因归因.md`。修法 = **拷贝式**：静态 arena 里
//! `tls_place` 放置 → 显式清零 + 拷模板 → TP = data−0x10。
//!
//! 本 crate 承载全部**纯函数面**（放置算术 + phdr 走查 + 模板四全局）：
//! - 消费方 = `berry-libc` crt0（主线程装配）+ 第二层 pthread（T-1 §3
//!   trampoline 对 malloc 块**复用同一算术**——主/子线程块形状恒同，
//!   裁决三「统一算术口径」）；
//! - host 单测挂本 crate（`just test-host` 显式行）——**不进 berry-libc**
//!   的原因：libc 同名符号在 host 测试二进制里对 libSystem write 发生
//!   符号插入，panic 钩子被劫自旋（首跑实测，见 Cargo.toml 注）。
//!
//! TP 偏移口径：LLD 的 AArch64 TLSLE 重定位把 `tprel` 解成「符号地址 −
//! TP」；惯例 tprel = st_value + 0x10（TP 上方留 16B TCB 洞，musl/glibc
//! aarch64 同形）。拷贝式下访问 = TP + tprel = data + st_value——与原位
//! 平移数值同形，全部既有 TLS 代码零改动。
//!
//! **python/rust 双份算术（义务注记）**：`TLS_ARENA_CAP` 与 `tls_place`
//! 的放置算术由 `tools/tlsscan.py` 逐行镜像（F1 判据）——改本侧常量/
//! 算术后必须同步 python 侧；两侧同向量钉板（4080 恰合/4081 溢/align32
//! 边界，见 `#[cfg(test)] mod tests` 与 tlsscan `--selftest`）。

#![cfg_attr(not(test), no_std)]

// —— TLS 模板四全局（T-1 裁决三：tls 案四全局接口）——
//
// 第二层 pthread（trampoline 装配子线程 TLS 窗）的唯一数据源：主线程
// arena 里已就位的模板 = `[BERRY_TLS_VADDR, +filesz)`（镜像原位，进程
// 生命周期内常驻可拷）。**写点唯一 = crt0 `berry_tls_setup`（先于
// main）**；读点 = `tls_template()` 访问器 + 第二层。不挂 `#[used]`
// ——fat LTO 下第一层无消费者被 DCE 无害，第二层引用后自动保留。

/// PT_TLS `p_vaddr`（.tdata 模板镜像原位基址）。
#[no_mangle]
pub static mut BERRY_TLS_VADDR: usize = 0;
/// PT_TLS `p_filesz`（.tdata 模板字节数）。
#[no_mangle]
pub static mut BERRY_TLS_FILESZ: usize = 0;
/// PT_TLS `p_memsz`（TLS 窗全长：tdata + tbss）。
#[no_mangle]
pub static mut BERRY_TLS_MEMSZ: usize = 0;
/// PT_TLS `p_align`。**0 ⟺ 无 PT_TLS**（pthread_create 守门数据源：
/// 无 PT_TLS 程序子线程块 = 纯 struct pthread 无 TLS 胞元）。
#[no_mangle]
pub static mut BERRY_TLS_ALIGN: usize = 0;

/// 四全局安全读取（vaddr, filesz, memsz, align）。ALIGN=0 ⟺ 无 PT_TLS。
pub fn tls_template() -> (usize, usize, usize, usize) {
    unsafe {
        (
            core::ptr::read(core::ptr::addr_of!(BERRY_TLS_VADDR)),
            core::ptr::read(core::ptr::addr_of!(BERRY_TLS_FILESZ)),
            core::ptr::read(core::ptr::addr_of!(BERRY_TLS_MEMSZ)),
            core::ptr::read(core::ptr::addr_of!(BERRY_TLS_ALIGN)),
        )
    }
}

/// TLS 拷贝式主线程 arena 容量（T-1 §1）：家族 memsz 峰值 769B
/// （bin/sort 0x301）的 5.3x 余量。
pub const TLS_ARENA_CAP: usize = 4096;

/// 纯放置函数：TLS 数据区在 `[base, base+cap)` 块内的落点。
///
/// - 拒非 2 幂 align（含 0——`0 & (0−1) == 0` 会骗过幂检，显式拦）；
/// - 拒 filesz > memsz；
/// - `data = (base+0x10+align−1) & !(align−1)`（TCB 洞 [TP, TP+0x10)
///   让位 + 对齐垫）；TP = data − 0x10；
/// - `data+memsz > base+cap` → None（溢出 = crt0 运行期 brk / lint 门 F1）。
///
/// 主线程（crt0 静态 arena）与子线程（第二层 malloc 块）**同一算术**
/// （裁决三「统一算术口径」）。host 单测 + `tools/tlsscan.py` python
/// 镜像同向量钉板（同源义务，见模块头注）。
pub fn tls_place(
    base: usize,
    cap: usize,
    filesz: usize,
    memsz: usize,
    align: usize,
) -> Option<usize> {
    if align == 0 || (align & (align.wrapping_sub(1))) != 0 {
        return None; // 非 2 幂（0 显式拒：幂检对 0 失明）
    }
    if filesz > memsz {
        return None;
    }
    let data = (base + 0x10 + align - 1) & !(align - 1);
    if data.wrapping_add(memsz) > base.wrapping_add(cap) {
        return None;
    }
    Some(data)
}

/// ELF64 phdr walk 抽纯函数：返回 PT_TLS 的 `(p_vaddr, p_filesz, p_memsz,
/// p_align)`。**含 p_align@phdr+0x30**（旧原位版漏读，裁决三第四全局的
/// 数据源；ELF64 Phdr 布局 p_align@0x30——T-1 草案 §1「@0x38」笔误，
/// 0x38 是 Phdr 全长，+0x38 会读到下一 phdr 的 p_type，实施按 0x30、与
/// berry-loader 测试同口径）。无 PT_TLS → None。host 测试喂合成
/// mini-ehdr。
pub fn find_ptls(ehdr: *const u8) -> Option<(usize, usize, usize, usize)> {
    unsafe {
        let phoff = core::ptr::read_unaligned(ehdr.add(0x20) as *const u64) as usize;
        let phentsize = core::ptr::read_unaligned(ehdr.add(0x36) as *const u16) as usize;
        let phnum = core::ptr::read_unaligned(ehdr.add(0x38) as *const u16) as usize;
        for i in 0..phnum {
            let ph = ehdr.add(phoff + i * phentsize);
            // ELF64 Phdr: p_type@0 p_flags@4 p_offset@8 p_vaddr@0x10
            //             p_paddr@0x18 p_filesz@0x20 p_memsz@0x28 p_align@0x30
            if core::ptr::read_unaligned(ph as *const u32) == 7 {
                let vaddr = core::ptr::read_unaligned(ph.add(0x10) as *const u64) as usize;
                let filesz = core::ptr::read_unaligned(ph.add(0x20) as *const u64) as usize;
                let memsz = core::ptr::read_unaligned(ph.add(0x28) as *const u64) as usize;
                let align = core::ptr::read_unaligned(ph.add(0x30) as *const u64) as usize;
                return Some((vaddr, filesz, memsz, align));
            }
        }
    }
    None
}

// —— host 测试面（T-1 §1：tls_place 实向量 + 边界 + find_ptls 合成
//    mini-ehdr；与 tools/tlsscan.py --selftest 同向量共识钉板——python/
//    rust 双份算术漂移任一侧改动后首跑即显形）——
#[cfg(test)]
mod tests {
    use super::*;

    // —— tls_place：实向量（家族形状 align=8 → data−base 恒 0x10）——

    #[test]
    fn place_family_shape_typical() {
        // 家族实测峰：bin/sort filesz=0x258 / memsz=0x301 / align=8。
        let base = 0x5000;
        assert_eq!(tls_place(base, TLS_ARENA_CAP, 0x258, 0x301, 8), Some(base + 0x10));
    }

    #[test]
    fn place_cap_exact_fit_4080() {
        // 4080 恰合：data−base=0x10，0x10+4080 == 4096 贴帽沿。
        assert_eq!(tls_place(0x5000, TLS_ARENA_CAP, 0, 4080, 8), Some(0x5010));
    }

    #[test]
    fn place_cap_overflow_4081() {
        // 4081 溢：0x10+4081 > 4096 → None（crt0 brk / lint 门 F1 面）。
        assert_eq!(tls_place(0x5000, TLS_ARENA_CAP, 0, 4081, 8), None);
    }

    #[test]
    fn place_align32_boundary() {
        // align=32：base 16 对齐 → data−base=0x20；4064 恰合 / 4065 溢。
        let base = 0x5000;
        assert_eq!(tls_place(base, TLS_ARENA_CAP, 0, 4064, 32), Some(base + 0x20));
        assert_eq!(tls_place(base, TLS_ARENA_CAP, 0, 4065, 32), None);
        // base=0（tlsscan python 镜像基准）同 worst case：data=0x20。
        assert_eq!(tls_place(0, TLS_ARENA_CAP, 0, 4064, 32), Some(0x20));
    }

    // —— tls_place：拒绝面 ——

    #[test]
    fn place_reject_non_pow2_and_zero_align() {
        assert_eq!(tls_place(0x5000, TLS_ARENA_CAP, 0, 8, 3), None);
        assert_eq!(tls_place(0x5000, TLS_ARENA_CAP, 0, 8, 24), None); // 24 非 2 幂
        assert_eq!(tls_place(0x5000, TLS_ARENA_CAP, 0, 8, 0), None); // 0 骗不过显式拦
    }

    #[test]
    fn place_reject_filesz_gt_memsz() {
        assert_eq!(tls_place(0x5000, TLS_ARENA_CAP, 100, 50, 8), None);
    }

    #[test]
    fn place_align_over_cap() {
        // align 8192 > cap 4096：垫越帽沿 → None（结构性放不下）。
        assert_eq!(tls_place(0x5000, TLS_ARENA_CAP, 0, 8, 8192), None);
    }

    #[test]
    fn place_sub16_align_still_offsets_0x10() {
        // align ≤ 16 且 base 16 对齐：data−base 恒 0x10（洞恒 16B）。
        for a in [1usize, 2, 4, 8, 16] {
            assert_eq!(tls_place(0x5000, TLS_ARENA_CAP, 0, 16, a), Some(0x5010));
        }
    }

    // —— find_ptls：合成 mini-ehdr（berry-loader 测试同款口径）——

    /// 56B ELF64 phdr（LE）：p_type p_flags p_offset p_vaddr p_paddr
    /// p_filesz p_memsz p_align。
    fn phdr(ptype: u32, vaddr: u64, filesz: u64, memsz: u64, align: u64) -> [u8; 56] {
        let mut p = [0u8; 56];
        p[0..4].copy_from_slice(&ptype.to_le_bytes());
        p[4..8].copy_from_slice(&5u32.to_le_bytes()); // p_flags
        p[8..16].copy_from_slice(&0u64.to_le_bytes()); // p_offset
        p[0x10..0x18].copy_from_slice(&vaddr.to_le_bytes());
        p[0x18..0x20].copy_from_slice(&0u64.to_le_bytes()); // p_paddr
        p[0x20..0x28].copy_from_slice(&filesz.to_le_bytes());
        p[0x28..0x30].copy_from_slice(&memsz.to_le_bytes());
        p[0x30..0x38].copy_from_slice(&align.to_le_bytes());
        p
    }

    /// 64B ehdr + phdr 串（e_phoff=64 / e_phentsize=entsize）。
    fn ehdr(phdrs: &[u8], entsize: u16) -> Vec<u8> {
        let mut e = vec![0u8; 64];
        e[..4].copy_from_slice(b"\x7fELF");
        e[4] = 2; // ELFCLASS64
        e[5] = 1; // ELFDATA2LSB
        e[0x20..0x28].copy_from_slice(&64u64.to_le_bytes()); // e_phoff
        e[0x36..0x38].copy_from_slice(&entsize.to_le_bytes());
        e[0x38..0x3a].copy_from_slice(
            &((phdrs.len() / entsize as usize) as u16).to_le_bytes(),
        );
        e.extend_from_slice(phdrs);
        e
    }

    #[test]
    fn find_ptls_hit_reads_all_four_fields() {
        // 含补读的 p_align@0x30（旧原位版漏读——裁决三第四全局数据源）。
        let ph = [
            phdr(1, 0x1000, 0x100, 0x100, 0x1000),
            phdr(7, 0x3000, 0x258, 0x301, 8),
        ];
        let buf = ehdr(&concat_bytes(&ph), 56);
        assert_eq!(find_ptls(buf.as_ptr()), Some((0x3000, 0x258, 0x301, 8)));
    }

    #[test]
    fn find_ptls_miss_when_no_tls_segment() {
        let ph = [phdr(1, 0x1000, 0x100, 0x100, 0x1000)];
        let buf = ehdr(&concat_bytes(&ph), 56);
        assert_eq!(find_ptls(buf.as_ptr()), None);
    }

    #[test]
    fn find_ptls_nonstandard_entsize_stride() {
        // e_phentsize=64（带垫）：walk 必须按 entsize 步进，PT_TLS 在第二槽。
        let mut ph = Vec::new();
        ph.extend_from_slice(&phdr(1, 0x1000, 0x100, 0x100, 0x1000));
        ph.extend_from_slice(&[0u8; 8]); // 垫到 64
        ph.extend_from_slice(&phdr(7, 0x4000, 0x10, 0x18, 8));
        ph.extend_from_slice(&[0u8; 8]);
        let buf = ehdr(&ph, 64);
        assert_eq!(find_ptls(buf.as_ptr()), Some((0x4000, 0x10, 0x18, 8)));
    }

    #[test]
    fn tls_template_defaults_zero_align_sentinel() {
        // ALIGN=0 ⟺ 无 PT_TLS 哨兵：纯 crate 无写点（唯一写点在 crt0
        // berry_tls_setup），四全局保持 0 初值。
        assert_eq!(tls_template(), (0, 0, 0, 0));
    }

    fn concat_bytes(phdrs: &[[u8; 56]]) -> Vec<u8> {
        let mut v = Vec::new();
        for p in phdrs {
            v.extend_from_slice(p);
        }
        v
    }
}
