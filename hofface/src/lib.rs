//! berry-hofface —— BerryBoot→内核 handoff 交接协议纯函数面（W3-Q15）。
//!
//! **动机**：交接协议的几何与界检判定律（probe 的 x0 对齐/窗界/version
//! 门、init_segs 的同页自相对/段几何、gop_info 的四非零门与 fmt≤2）此前
//! 只活在 `kernel/src/handoff.rs` 的裸表达式里——host 测不可达（kernel
//! 依赖 HAL global_asm，只 aarch64-unknown-none 可编），ktest 只覆盖 boot
//! 相。交接几何的回归面（DTB 魔数不误吞、自相对契约按 version 分派、
//! 回卷拒、越窗拒）需要一张 host 可测的**钉板**：本 crate 把常量与判定
//! 按值域抽成纯函数 + 表驱动测试，`kernel` 侧消费同一函数（零第二实现
//! ——抽离即唯一事实源，非镜像）。
//!
//! 抽离面（与 kernel 侧逐字对齐，改这里的语义 = 改 kernel 行为）：
//! - [`HANDOFF_MAGIC`]/[`HANDOFF_VERSION`]/[`INIT_SEGS_MAX`]/
//!   [`HANDOFF_SIZE`]/[`INIT_SEG_SIZE`]：协议常量与布局尺寸单源
//! （双镜像纪律——bootloader/berryboot/app/src/lib.rs 同表逐行对齐，
//! 两侧编译期 const assert；本 crate 再钉字面值防漂移）。
//! - [`probe_gate`]:x0 分流门——对齐（8B）+ 176B 结构落 [RAM_BASE,
//!   RAM_TOP)（checked 防加法回卷）+ magic/version 认定。命中 →
//!   Some(())；kernel 侧再加 phys_to_virt 解引用（raw 机械留壳）。
//! - [`init_segs_gate`]:分段数组几何门——count ≤ 32、同页自相对契约
//!   （基址 = hpa + 176(v2)/128(v1)，按 version 分派）、每段 pa 4KiB
//!   对齐 + len>0 + 段区落窗（checked）。任一违约 → None（降级存活，
//!   boot 期无 panic 纪律）。
//! - [`gop_gate`]:GOP 事实四非零门 + fmt≤2（3 = BltOnly 无线性帧缓冲，
//!   装载器已按缺席递送，双保险）。
//!
//! 与冻结面的关系：boot 协议是**仓内版本化面**，非 abi.toml 冻结面
//! （handoff.rs 头注——不触保留门 C）。`probe`/`init_segs` 的高半直映
//! raw 解引用（SAFETY 注与生命周期）不在此 crate——本面只钉「几何怎么
//! 判」，不钉「地址怎么解」。
//!
//! 值域注：kernel 侧 x0/hpa 为 usize（boot 寄存器宽）；本面统一 u64
//! 值域（ARM64 全宽覆盖 usize，`as usize` 收窄在 kernel 壳内）。

#![cfg_attr(not(test), no_std)]

// ---- 协议常量（kernel/src/handoff.rs 单源迁此；字面值钉板见 tests）----

/// 魔数：LE u32 读出 = ASCII "BRYH"（BerryOS Handoff）。
pub const HANDOFF_MAGIC: u32 = 0x4252_5948;

/// 协议版本：v2 = 2（V10 度量/签名槽留位）；v1 = 1（A1-3b init 分段）；
/// v0 = 0（V-0b 刀 3 冻结）。probe 认 ≤ 当前版（双向兼容）。
pub const HANDOFF_VERSION: u32 = 2;

/// 分段描述符数上限（A1-3b §3.2.1）：32×16B + 176B 头 = 688B < 4KiB——
/// handoff 单页自含的执法点；越顶 = 递送面违约。
pub const INIT_SEGS_MAX: usize = 32;

/// 交接结构尺寸（v2 = 176B 头；kernel 侧 const assert 同值）。
pub const HANDOFF_SIZE: u64 = 176;

/// 分段描述符尺寸（16B：pa + len 双 u64）。
pub const INIT_SEG_SIZE: u64 = 16;

// ---- 布局常量（自相对契约按 version 分派用）----

/// v2 头长：描述符数组基址偏移（meas_root/sig_slot 尾追加后）。
pub const SEGS_BASE_OFF_V2: u64 = 176;
/// v1 头长：描述符数组基址偏移（旧装载器照跑——双向兼容的版本门控）。
pub const SEGS_BASE_OFF_V1: u64 = 128;

/// 段域提取（`init_segs_gate` 泛型面）：kernel 侧 `InitSeg`（repr(C)
/// pa/len 双 u64）与本 crate 测试元组 `(u64, u64)` 共用一门——kernel 在
/// `handoff.rs` 为 `InitSeg` 实现（孤儿规则放行：trait 本crate、类型
/// kernel crate），零拷贝零转换消费。
pub trait SegRaw {
    /// (pa, len) 值域投影。
    fn seg_pa_len(&self) -> (u64, u64);
}

impl SegRaw for (u64, u64) {
    #[inline]
    fn seg_pa_len(&self) -> (u64, u64) {
        *self
    }
}

// ---- 几何门（kernel probe/init_segs/gop_info 的判定律单源）----

/// probe 界检前置门（解引用**前**调用——raw 读须界检先行，旧实现判序）：
/// x0 8 对齐 + [x0, x0+176) 落 [ram_base, ram_top)（checked 防回卷）。
/// [`probe_gate`] 内含本门（全判单入口）；kernel 壳在 raw 解引用前单独
/// 调本函数（先界检后解引用的 SAFETY 前置），再走 gate 终判。
#[inline]
pub fn probe_bounds_ok(x0: u64, ram_base: u64, ram_top: u64) -> bool {
    if x0 & 7 != 0 {
        return false;
    }
    let end = match x0.checked_add(HANDOFF_SIZE) {
        Some(e) => e,
        None => return false,
    };
    x0 >= ram_base && end <= ram_top
}

/// probe x0 分流门（`kernel/src/handoff.rs::probe` 判定段的纯投影）：
/// - x0 8 对齐（结构 u64 字段自身对齐要求；x0=0 QEMU `-kernel` 实测
///   过此关走 magic 不中）；
/// - [x0, x0+176) 落 [ram_base, ram_top)（checked_add 防回卷——u64
///   全域无回卷，语义与 kernel 侧 usize checked 同形）；
/// - magic 命中 + version ≤ 当前版（>v2 同 None——fail-safe 降级既有
///   DTB/fw_cfg 路；DTB 魔数 0xD00D_FEED 与 BRYH 互不碰撞）。
///
/// kernel 壳职责：ram_base/ram_top 传 berry_hal_aarch64::{RAM_BASE,
/// RAM_TOP}；**解引用前先调 [`probe_bounds_ok`]**（界检先行，raw 读后
/// 置），命中后 phys_to_virt raw 解引用 + `&'static` 生命周期。
#[inline]
pub fn probe_gate(x0: u64, magic: u32, version: u32, ram_base: u64, ram_top: u64) -> bool {
    probe_bounds_ok(x0, ram_base, ram_top) && magic == HANDOFF_MAGIC && version <= HANDOFF_VERSION
}

/// init_segs 几何门（`kernel/src/handoff.rs::init_segs` 判定段的纯
/// 投影，**含段表内容**——数组切片由 kernel 壳解引用后传入）：
/// - count > 32 → None（递送面违约）；
/// - count == 0 → init_segs_pa 必须为 0（零值缺席语义，禁只看 count）
///   → Some(空)；
/// - 同页自相对契约：init_segs_pa == hpa + 176(v2)/128(v1)（按
///   handoff.version 分派——描述符数组随头迁移的双向兼容门）；
/// - 每段：pa 4KiB 对齐（AllocatePages 出品）+ pa≠0 + len≠0 + 段区
///   [pa, pa+len) 落 [ram_base, ram_top)（checked 防回卷）。
///
/// kernel 壳职责：base 传 phys_to_virt(init_segs_pa) 后 from_raw_parts
/// 出切片再入本门；Σlen 总量帽不在本面（消费侧 ROOTFS_TOTAL_MAX 口径）。
pub fn init_segs_gate<S: SegRaw>(
    version: u32,
    hpa: u64,
    init_segs_pa: u64,
    count: u32,
    segs: &[S],
    ram_base: u64,
    ram_top: u64,
) -> Option<()> {
    let n = count as usize;
    if n > INIT_SEGS_MAX {
        return None;
    }
    if n == 0 {
        // 空段（无 INITRD）：pa 也须为零（零值缺席语义）。
        return if init_segs_pa == 0 { Some(()) } else { None };
    }
    // 同页自相对契约：数组基址随头按 version 分派。
    let base_off: u64 = if version >= 2 {
        SEGS_BASE_OFF_V2
    } else {
        SEGS_BASE_OFF_V1
    };
    let expect = hpa.checked_add(base_off)?;
    if init_segs_pa != expect {
        return None;
    }
    // 段数与切片一致（kernel 壳 from_raw_parts(count) 恒成立；host 直测
    // 面显式拒不一致——防御错配调用）。
    if segs.len() != n {
        return None;
    }
    for s in segs {
        let (pa, len) = s.seg_pa_len();
        if pa == 0 || len == 0 || pa & 0xfff != 0 {
            return None;
        }
        let end = pa.checked_add(len)?;
        if pa < ram_base || end > ram_top {
            return None;
        }
    }
    Some(())
}

/// gop 事实门（`kernel/src/handoff.rs::gop_info` 判定段的纯投影）：
/// fb/宽/高/stride 四非零 + format ≤ 2（0=RGBx 1=BGRx 2=BitMask；
/// 3 = BltOnly 无线性帧缓冲——装载器按缺席递送，此处双保险）。
#[inline]
pub fn gop_gate(fb_pa: u64, width: u32, height: u32, stride: u32, format: u32) -> bool {
    fb_pa != 0 && width != 0 && height != 0 && stride != 0 && format <= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    // QEMU virt 装载窗（berry_hal_aarch64 RAM_BASE/RAM_TOP 同源；probe
    // 用 [RAM_BASE, RAM_TOP) 全窗 = 0x4000_0000..0x5000_0000）。
    const RB: u64 = 0x4000_0000;
    const RT: u64 = 0x5000_0000;

    // ---- 常量字面值钉板（双镜像纪律：kernel + berryboot 两侧 const
    // assert 钉布局，本 crate 钉协议常量不漂移——改值须三处同动）----

    #[test]
    fn test_protocol_constants_frozen() {
        assert_eq!(HANDOFF_MAGIC, 0x4252_5948); // LE 读出 ASCII "BRYH"
        assert_eq!(HANDOFF_VERSION, 2);
        assert_eq!(INIT_SEGS_MAX, 32);
        assert_eq!(HANDOFF_SIZE, 176);
        assert_eq!(INIT_SEG_SIZE, 16);
        // 单页自含执法点：32×16 + 176 = 688 < 4096。
        assert_eq!(
            INIT_SEGS_MAX as u64 * INIT_SEG_SIZE + HANDOFF_SIZE,
            688
        );
        assert!(INIT_SEGS_MAX as u64 * INIT_SEG_SIZE + HANDOFF_SIZE < 4096);
        // v1 布局同自含（128B 头 + 512 = 640 < 4KiB）。
        assert!(INIT_SEGS_MAX as u64 * INIT_SEG_SIZE + SEGS_BASE_OFF_V1 < 4096);
        // DTB 魔数（BE 0xD00D_FEED）与 HANDOFF_MAGIC 互不碰撞——按 BRYH
        // 认领不会误吞 DTB（probe 头注契约）。
        assert_ne!(HANDOFF_MAGIC, 0xD00D_FEED);
        assert_ne!(HANDOFF_MAGIC, 0xFEED_D00D);
        // 偏移分派表：v2 在 v1 之后（头只增不迁移回）。
        assert!(SEGS_BASE_OFF_V2 > SEGS_BASE_OFF_V1);
    }

    // ---- probe_gate：对齐/窗界/magic/version 表驱动 ----

    #[test]
    fn test_probe_gate_table() {
        // (x0, magic, version, expect)——合法基址取窗内 8 对齐点。
        let ok_pa: u64 = RB + 0x1000;
        let cases: &[(u64, u32, u32, bool)] = &[
            // 命中三形：v0/v1/v2 全认（probe 认 ≤ 当前版）。
            (ok_pa, HANDOFF_MAGIC, 0, true),
            (ok_pa, HANDOFF_MAGIC, 1, true),
            (ok_pa, HANDOFF_MAGIC, 2, true),
            // magic 不中：DTB 指针（0xD00D_FEED BE 读出 LE 形）/任意值。
            (ok_pa, 0xD00D_FEED, 2, false),
            (ok_pa, 0xFEED_D00D, 1, false),
            (ok_pa, 0, 0, false),
            // 版本不识别（>v2）→ None：fail-safe 降级既有路。
            (ok_pa, HANDOFF_MAGIC, 3, false),
            (ok_pa, HANDOFF_MAGIC, u32::MAX, false),
            // 对齐拒：x0 非 8 对齐（页对齐但差 4——最易混形）。
            (ok_pa + 4, HANDOFF_MAGIC, 2, false),
            (ok_pa + 1, HANDOFF_MAGIC, 2, false),
            // x0=0（QEMU -kernel 实测首参）：过对齐、窗外（< RB）→ None。
            (0, HANDOFF_MAGIC, 2, false),
            // 窗界：基址 = RB 恰入（边界含）；基址 < RB 拒。
            (RB, HANDOFF_MAGIC, 2, true),
            (RB - 8, HANDOFF_MAGIC, 2, false),
            // 顶界：end = x0+176 恰 ≤ RT 过；end > RT 拒（半开窗）。
            (RT - HANDOFF_SIZE, HANDOFF_MAGIC, 2, true),
            (RT - HANDOFF_SIZE + 8, HANDOFF_MAGIC, 2, false),
        ];
        for &(x0, m, v, want) in cases {
            assert_eq!(probe_gate(x0, m, v, RB, RT), want, "x0={:#x}", x0);
        }
    }

    #[test]
    fn test_probe_gate_wraparound_reject() {
        // 回卷拒：u64::MAX 附近 checked_add 溢出 → false（kernel 侧
        // usize checked_add 同形——地址环绕不得误判窗内）。
        assert!(!probe_gate(u64::MAX, HANDOFF_MAGIC, 2, RB, RT));
        assert!(!probe_gate(u64::MAX - 100, HANDOFF_MAGIC, 2, RB, RT));
        // 溢出点恰在界外也是拒（checked 语义 = 回卷不算窗内）。
        assert!(!probe_gate(u64::MAX - 175, HANDOFF_MAGIC, 2, 0, u64::MAX));
        // 对照：全域窗 [0, u64::MAX) 内 8 对齐低址 + 命中形过（checked
        // 只拒回卷，不拒低址）。
        assert!(probe_gate(0x1000, HANDOFF_MAGIC, 2, 0, u64::MAX));
        // 全域窗顶（对齐顶 top = ...FF0）：end = x0+176 恰 = top 过（半开
        // 含顶前）；再 +8 越顶拒。u64::MAX 本身非 8 对齐——对齐门先拒。
        let top = 0xFFFF_FFFF_FFFF_FFF0u64;
        assert!(!probe_gate(u64::MAX, HANDOFF_MAGIC, 2, 0, top));
        assert!(probe_gate(top - HANDOFF_SIZE, HANDOFF_MAGIC, 2, 0, top));
        assert!(!probe_gate(top - HANDOFF_SIZE + 8, HANDOFF_MAGIC, 2, 0, top));
    }

    #[test]
    fn test_probe_gate_dtb_pointer_shape() {
        // QEMU -kernel 路 x0 = DTB PA（窗内 8 对齐真形）——magic 不中
        // 走原路零扰动（probe 头注：未中 → None，DTB/fw_cfg 原路）。
        // DTB 常落窗内中部（QEMU virt 默认 high DTB 区），取窗内中点对齐。
        let dtb_pa = RB + 0x0800_0000;
        assert!(dtb_pa < RT && dtb_pa & 7 == 0);
        assert!(!probe_gate(dtb_pa, 0xFEED_D00D, 0, RB, RT));
        // x0=0 形（无 DTB）：窗外拒在前，magic 无从读——None。
        assert!(!probe_gate(0, 0, 0, RB, RT));
    }

    // ---- init_segs_gate：count/自相对/段几何 表驱动 ----

    #[test]
    fn test_init_segs_gate_count_zero_absence_semantics() {
        // count=0 双形：pa=0 → Some（无 INITRD 合法缺席）；pa≠0 → None
        // （零值缺席语义——禁只看 count，handoff.rs 头注裁决 B）。
        let empty: &[(u64, u64)] = &[];
        assert_eq!(init_segs_gate(2, RB, 0, 0, empty, RB, RT), Some(()));
        assert_eq!(init_segs_gate(1, RB, 0, 0, empty, RB, RT), Some(()));
        assert_eq!(init_segs_gate(2, RB, RB + 176, 0, empty, RB, RT), None);
        assert_eq!(init_segs_gate(2, RB, 0xDEAD_BEEF, 0, empty, RB, RT), None);
    }

    #[test]
    fn test_init_segs_gate_count_cap() {
        // 越顶拒：count > 32 = 递送面违约（单页自含执法点）。
        let hpa = RB;
        let segs32 = vec![(RB + 0x1000, 0x1000u64); 32];
        // 32 段全合法形：自相对 + 每段几何过 → Some。
        assert_eq!(
            init_segs_gate(2, hpa, hpa + 176, 32, &segs32, RB, RT),
            Some(())
        );
        // 33 段：count 越顶拒（切片内容无所谓——count 门在前）。
        let segs33 = vec![(RB + 0x1000, 0x1000u64); 33];
        assert_eq!(
            init_segs_gate(2, hpa, hpa + 176, 33, &segs33, RB, RT),
            None
        );
        // u32 极端值同样拒（截断/垃圾不进段循环）。
        assert_eq!(
            init_segs_gate(2, hpa, hpa + 176, u32::MAX, &segs33, RB, RT),
            None
        );
    }

    #[test]
    fn test_init_segs_gate_self_relative_dispatch() {
        // 同页自相对契约按 version 分派：v2 基址 @176 / v1 @128。
        let hpa = RB;
        let one = [(RB + 0x2000, 0x1000u64)];
        // v2：hpa+176 命中；hpa+128（v1 形）→ 拒（版本分派不互通）。
        assert_eq!(
            init_segs_gate(2, hpa, hpa + 176, 1, &one, RB, RT),
            Some(())
        );
        assert_eq!(init_segs_gate(2, hpa, hpa + 128, 1, &one, RB, RT), None);
        // v1：hpa+128 命中；hpa+176 → 拒（旧装载器头形互斥）。
        assert_eq!(
            init_segs_gate(1, hpa, hpa + 128, 1, &one, RB, RT),
            Some(())
        );
        assert_eq!(init_segs_gate(1, hpa, hpa + 176, 1, &one, RB, RT), None);
        // v0：version<1 走 init_blob_* 原路，分段门按 v1 基址判（kernel
        // 侧仅 version≥1 消费者调用——此处钉 v0 不落 v2 基址形）。
        assert_eq!(
            init_segs_gate(0, hpa, hpa + 128, 1, &one, RB, RT),
            Some(())
        );
        // 全零基址 + 非零 count → 拒（自相对门拒 0 基址段形）。
        assert_eq!(init_segs_gate(2, hpa, 0, 1, &one, RB, RT), None);
        // 回卷拒：hpa 近 u64 顶 + 176 溢出 → None（checked_add）。
        // （期望基址参数用 wrapping 形——测的就是 gate 内 checked 拒。）
        let wrap_hpa = u64::MAX - 100;
        assert_eq!(
            init_segs_gate(2, wrap_hpa, wrap_hpa.wrapping_add(176), 1, &one, RB, RT),
            None
        );
    }

    #[test]
    fn test_init_segs_gate_segment_geometry_table() {
        let hpa = RB;
        let base = hpa + 176;
        // (pa, len, expect)——单段形，其余参数恒合法。
        let cases: &[(u64, u64, bool)] = &[
            // 合法：窗内页对齐非零段（页倍/页内截断末段均可）。
            (RB + 0x1000, 0x1000, true),
            (RB + 0x1000, 1, true),        // len=1 末段页内截断
            (RB + 0x1000, 0xfff, true),    // 页内非页倍
            (RT - 0x1000, 0x1000, true),   // 顶界恰贴
            // pa 违法：零 / 非 4KiB 对齐（页对齐差 1/差 0x800）。
            (0, 0x1000, false),
            (RB + 0x1001, 0x1000, false),
            (RB + 0x1800, 0x1000, false),
            // len 违法：零。
            (RB + 0x1000, 0, false),
            // 窗界：pa < RB 拒（含回卷后落窗下的伪合法形）。
            (RB - 0x1000, 0x1000, false),
            (0x1000, 0x1000, false),
            // end 越顶拒：pa 窗内但 pa+len > RT。
            (RT - 0x1000, 0x1001, false),
            (RT - 0x800, 0x1000, false),
            (RB + 0x1000, u64::MAX, false), // len 极值回卷后越顶
        ];
        for &(pa, len, want) in cases {
            let segs = [(pa, len)];
            assert_eq!(
                init_segs_gate(2, hpa, base, 1, &segs, RB, RT),
                if want { Some(()) } else { None },
                "pa={:#x} len={:#x}",
                pa,
                len
            );
        }
    }

    #[test]
    fn test_init_segs_gate_slice_count_mismatch() {
        // 切片长度与 count 不一致 → None（kernel 壳 from_raw_parts(count)
        // 恒等长；host 显式钉错配调用防御）。
        let hpa = RB;
        let one = [(RB + 0x1000, 0x1000u64)];
        assert_eq!(init_segs_gate(2, hpa, hpa + 176, 2, &one, RB, RT), None);
        let two = [(RB + 0x1000, 0x1000u64), (RB + 0x2000, 0x800u64)];
        assert_eq!(init_segs_gate(2, hpa, hpa + 176, 1, &two, RB, RT), None);
    }

    #[test]
    fn test_init_segs_gate_multi_segment_mixed_reject() {
        // 多段混合形：任一段违约整体拒（一票否决——整源降级「无 rootfs」
        // 存活，boot 期无 panic 纪律）。合法 32 段中埋一段毒段。
        let hpa = RB;
        let mut segs: Vec<(u64, u64)> = (0..32)
            .map(|i| (RB + 0x1000 + (i as u64) * 0x1000, 0x1000))
            .collect();
        assert_eq!(
            init_segs_gate(2, hpa, hpa + 176, 32, &segs, RB, RT),
            Some(())
        );
        segs[17] = (0xBEEF, 0x1000); // 毒段：非对齐 + 窗外
        assert_eq!(
            init_segs_gate(2, hpa, hpa + 176, 32, &segs, RB, RT),
            None
        );
        // 段区重叠不在本面（BerryBoot 逐段 AllocatePages 出品互斥；
        // 消费侧按序拼装无重叠假设——handoff.rs 同口径，不越权加律）。
    }

    // ---- gop_gate：四非零 + fmt≤2 表驱动 ----

    #[test]
    fn test_probe_bounds_ok_mirror() {
        // 前置门与全门界检面逐形一致（probe_gate 的界检子集——kernel 壳
        // 解引用前单独调用的面）。
        let ok_pa: u64 = RB + 0x1000;
        assert!(probe_bounds_ok(ok_pa, RB, RT));
        assert!(probe_bounds_ok(RB, RB, RT));
        assert!(probe_bounds_ok(RT - HANDOFF_SIZE, RB, RT));
        assert!(!probe_bounds_ok(0, RB, RT));
        assert!(!probe_bounds_ok(RB - 8, RB, RT));
        assert!(!probe_bounds_ok(RT - HANDOFF_SIZE + 8, RB, RT));
        assert!(!probe_bounds_ok(ok_pa + 4, RB, RT));
        assert!(!probe_bounds_ok(u64::MAX, RB, RT));
    }

    #[test]
    fn test_gop_gate_table() {
        // (fb, w, h, stride, fmt, expect)。
        let cases: &[(u64, u32, u32, u32, u32, bool)] = &[
            // 合法三 fmt（0=RGBx 1=BGRx 2=BitMask）。
            (0x4000_0000, 800, 600, 3200, 0, true),
            (0x4000_0000, 800, 600, 3200, 1, true),
            (0x4000_0000, 800, 600, 3200, 2, true),
            // 全零缺席形（QEMU -kernel 路/无显示 EFI 路）：拒。
            (0, 0, 0, 0, 0, false),
            // 四非零门逐位：任一为零拒。
            (0, 800, 600, 3200, 0, false),
            (0x4000_0000, 0, 600, 3200, 0, false),
            (0x4000_0000, 800, 0, 3200, 0, false),
            (0x4000_0000, 800, 600, 0, 0, false),
            // fmt=3 BltOnly：无线性帧缓冲拒（装载器已按缺席递送，双保险）。
            (0x4000_0000, 800, 600, 3200, 3, false),
            // fmt 垃圾极值拒。
            (0x4000_0000, 800, 600, 3200, u32::MAX, false),
            // 几何极小合法形（1×1 最小线性缓冲）。
            (0x4000_0000, 1, 1, 4, 0, true),
        ];
        for &(fb, w, h, s, f, want) in cases {
            assert_eq!(gop_gate(fb, w, h, s, f), want, "fb={:#x} f={}", fb, f);
        }
    }
}
