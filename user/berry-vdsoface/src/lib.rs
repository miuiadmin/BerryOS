//! berry-vdsoface —— vDSO 数据页布局与换算纯函数面（W3-Q106，03 号审计机会三）。
//!
//! clock_read(0x61, CLOCK_MONOTONIC) 的 vDSO 化单源：内核把 CNTFRQ 快照进
//! 一个只读数据页（`Perm::UserRo` 映射进每个用户 AS 固定 VA），用户态
//! `mrs cntvct_el0`（EL0VCTEN 自 timer::init 已开）+ 页内参数换算，整号
//! 不陷 SVC。REALTIME（id=1）不走页——rtc 注入面语义仍归 syscall 回落。
//!
//! 页布局（v1，u64 域，小端 = 端序即宿主）：
//!
//! ```text
//! +0x00  magic    0x42525644_4f533130 ("BRVDSO10")
//! +0x08  freq     CNTFRQ_EL0 快照（Hz；0 = 页未就绪 → 回落 syscall）
//! +0x10  seq      版本序（v1 恒 0：boot 一次写定终生不变）
//! +0x18  ..       保留（0 填充；REALTIME epoch 随 v2 seqlock 扩展）
//! ```
//!
//! 纪律：布局常量与换算式是**冻结 ABI 面**——magic/version 域只可追加
//! （`docs/系统调用ABI设计-v1.0.md` add-only 同精神）；换算式与内核
//! `sys_clock_read` 逐位同式（先商后余防中间溢出——ticks×1e9 在 ~5 分钟
//! 处溢 u64，拆两项后 ~584 年），两处不得各自漂移（本 crate 即单源）。

#![cfg_attr(not(test), no_std)]

/// vDSO 数据页在用户 VA 的固定映射地址。
///
/// 选址论证（2026-11-19 全带扫描）：映像带 0x1000+（ELF 帽 16MiB → 顶
/// ≤0x1052_0000）/ mmap 窗 0x5000_0000 / pthread 栈 bump 0x6000_0000 /
/// MMIO 0x7000_0000、fb 0x7100_0000 / USER_TOP 2^39——0x4800_0000 落映像
/// 带顶与 mmap 窗之间的大空档，与所有带大幅隔离；页对齐。ktest
/// rogue_ptr 模板占用 0x4000_0000（读未映射 VA 杀进程判例），故避让。
pub const VDSO_VA: usize = 0x4800_0000;

/// 数据页 magic（"BRVDSO10" —— BerryOS vDSO 布局 v1.0）。
pub const VDSO_MAGIC: u64 = 0x4252_5644_4f53_3130;

/// 页内 `freq` 域偏移。
pub const VDSO_OFF_FREQ: usize = 0x08;

/// 页内 `seq` 域偏移（v1 恒 0；REALTIME epoch 扩展预留）。
pub const VDSO_OFF_SEQ: usize = 0x10;

/// 数据页大小（一页 = 4096B；映射粒度口径）。
pub const VDSO_PAGE: usize = 4096;

/// ticks → 纳秒（与内核 `sys_clock_read` 同式：先商后余防中间溢出）。
///
/// `freq == 0` → `None`（页未就绪/异常频率——调用方回落 syscall）；
/// 纯函数无副作用，host 对拍内核两除式逐位一致。
pub const fn ticks_to_ns(ticks: u64, freq: u64) -> Option<u64> {
    if freq == 0 {
        return None;
    }
    Some((ticks / freq) * 1_000_000_000 + ((ticks % freq) * 1_000_000_000) / freq)
}

/// 从数据页 16 字节头判页有效性（magic + freq 非 0）。
///
/// `head` = 页首 16B（magic 域 + freq 域小端读）。旧内核/未映射页由调用方
/// fault 兜底，本谓词只判「已映射页是否就绪」。
pub const fn page_ready(head: [u8; 16]) -> bool {
    u64::from_le_bytes([head[0], head[1], head[2], head[3], head[4], head[5], head[6], head[7]])
        == VDSO_MAGIC
        && u64::from_le_bytes([
            head[8], head[9], head[10], head[11], head[12], head[13], head[14], head[15],
        ]) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 内核两除式逐位对拍（同式自证 + 溢出安全域抽查）。
    #[test]
    fn ticks_to_ns_matches_kernel_formula() {
        // QEMU virt 实测频率 62.5MHz。
        let f = 62_500_000u64;
        for t in [0u64, 1, 62_500, 62_500_000, 62_500_000_000, u64::MAX / 64] {
            let got = ticks_to_ns(t, f).unwrap();
            let want = (t / f) * 1_000_000_000 + ((t % f) * 1_000_000_000) / f;
            assert_eq!(got, want, "ticks={t}");
        }
        // 整秒域：1s/10s/584 年界内抽查。
        assert_eq!(ticks_to_ns(f, f).unwrap(), 1_000_000_000);
        assert_eq!(ticks_to_ns(10 * f, f).unwrap(), 10_000_000_000);
    }

    /// freq=0 → None（回落语义的谓词面）。
    #[test]
    fn ticks_to_ns_zero_freq_none() {
        assert!(ticks_to_ns(123, 0).is_none());
    }

    /// 非二次幂频率（真机形态）精度抽查：误差 ≤ 1ns（双除式本身精确，
    /// 此处钉「商余拆项不引入额外误差」）。
    #[test]
    fn ticks_to_ns_non_pow2_exact() {
        let f = 19_200_000u64; // 常见板级频率
        for t in [1u64, f - 1, f, f + 1, 123_456_789u64] {
            let want = (t / f) * 1_000_000_000 + ((t % f) * 1_000_000_000) / f;
            assert_eq!(ticks_to_ns(t, f).unwrap(), want);
        }
    }

    /// 页头谓词：magic+freq 组合四态。
    #[test]
    fn page_ready_gate() {
        let mut head = [0u8; 16];
        assert!(!page_ready(head)); // 全零页
        head[..8].copy_from_slice(&VDSO_MAGIC.to_le_bytes());
        assert!(!page_ready(head)); // magic 对 freq=0（未就绪）
        head[8..].copy_from_slice(&62_500_000u64.to_le_bytes());
        assert!(page_ready(head)); // 就绪
        head[0] ^= 0xff;
        assert!(!page_ready(head)); // magic 坏
    }

    /// 布局常量钉板（冻结 ABI 面——漂移即编译期炸响）。
    #[test]
    fn layout_frozen() {
        assert_eq!(VDSO_VA % VDSO_PAGE, 0);
        assert_eq!(VDSO_OFF_FREQ, 8);
        assert_eq!(VDSO_OFF_SEQ, 16);
        assert_eq!(VDSO_MAGIC, 0x4252_5644_4f53_3130); // "BRVDSO10"
    }
}
