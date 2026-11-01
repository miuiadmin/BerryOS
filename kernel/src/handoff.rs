//! BerryHandoff——BerryBoot（引导装载器）→ 内核的交接协议结构（V-0b 刀 3
//! 冻结 v0）。x0 现认 DTB 指针/0；刀 4 起 BerryBoot 跳 entry 时 **x0 =
//! handoff 物理指针**，内核按 `magic` 判别（DTB 魔数 0xD00D_FEED 大端序，
//! 与 HANDOFF_MAGIC 互不碰撞）——QEMU `-kernel` 直加载路径 x0=0 照旧兼容。
//!
//! **双镜像纪律**（同 `docs/系统调用ABI设计.md` §8 abi_gen.rs kernel/berry-sys
//! 双面镜像精神）：本结构与 `bootloader/berryboot/app/src/lib.rs` 的
//! `BerryHandoff` 逐字段同步——**改任一侧必须同步另一侧**，两侧各带同一张
//! const assert 偏移表（编译期炸响，漂移无法静默上线）。
//!
//! v0 字段语义（裁决 A/B，2026-10-24）：
//! - 刀 3 填：magic/version/entry/dram_base/dram_top；段装载几何不在结构内
//!   （内核 ELF 自描述，span 由装载器按 PT_LOAD 认领）。
//! - 刀 4 填：memmap_*（GetMemoryMap 快照，EBS 前最后一份）。
//! - 留位零值（裁决 B）：`gop_*` V-6a 起由 BerryBoot 填（banner 期
//!   QueryMode 事实快照——`docs/VM里程碑设计草案.md` §4 V-6 fbcon 数据
//!   源，全零 = 无此物）+ `cmdline_*`（留位零值待 V-1d+）。
//!   零值 = 「无此物」：pa=0 且 len=0 双字段共同判定，禁只看 pa。
//!
//! v1 扩面（A1-3b，`docs/容量工程前置-A1设计草案.md` §3.2.1）：init 递送从
//! 单 blob（`init_blob_*`）升级**分段描述符数组**——BerryBoot 按目标段
//! （8MiB）逐段 AllocatePages 页钉，绕开「固件给不出大连续段」的地形墙；
//! 描述符数组随 handoff **同页 @128**（32×16B+128B=640B < 4KiB 单页自含，
//! 无第二块分配）。版本语义：version=1 时 `init_blob_*`（@80/@88）置零
//! 不读——分段数组是唯一 init 事实源；version=0 恒旧 112B 形。**双向兼容**：
//! 内核认 version≤1（v0 走 `init_blob_*` 原路——旧装载器照跑）；旧内核见
//! v1 → probe 拒 → ROOTFS absent 降级非炸（fail-safe 由版本门控天然保证）。
//! boot 协议是仓内版本化面，非 abi.toml 冻结面（不触保留门 C）。
#![allow(dead_code)] // V-1a 消费 magic/version；V-1b 消费 memmap_*；V-1c 消费 init_blob_*；V-6a 消费 gop_*；A1-3b 消费 init_segs_*；cmdline_* 留位零值待 V-1d+

use berry_hal_aarch64::{RAM_BASE, RAM_TOP};
use berry_mm::va::phys_to_virt;
use core::mem::{offset_of, size_of};

/// 魔数：LE u32 读出 = ASCII "BRYH"（BerryOS Handoff）。
pub const HANDOFF_MAGIC: u32 = 0x4252_5948;

/// 协议版本：v1 = 1（A1-3b 起 init 分段递送；v0 = 0 冻结于 V-0b 刀 3，
/// 扩字段 = bump version）。BerryBoot 写当前版；内核 [`probe`] 认 ≤ 当前版
/// （v0/v1 双向兼容——旧装载器 v0 照跑，内核消费按 version 分派）。
pub const HANDOFF_VERSION: u32 = 1;

/// 分段描述符数上限（A1-3b §3.2.1）：32×16B + 128B 头 = 640B < 4KiB——
/// handoff 单页自含的执法点；越顶 = 递送面违约。
pub const INIT_SEGS_MAX: usize = 32;

/// init 分段描述符（v1 描述符数组元素，随 handoff 同页 @128）。
/// pa = AllocatePages 出品（4KiB 对齐）；len = 段内实读字节数（≤ 段页容量，
/// 末段可页内截断）。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InitSeg {
    pub pa: u64,
    pub len: u64,
}

/// 交接结构（v1 = 128B 头，全 8B 字段 8 对齐；无指针——PA 直递，装载器/
/// 内核各自解址，杜绝跨边界引用生命周期假设——本仓 ABI 通行纪律）。
/// v0 读者按 version 分派只读前 112B；描述符数组 @128 起仅 v1 读者消费。
#[repr(C)]
pub struct BerryHandoff {
    pub magic: u32,               // @0   "BRYH"
    pub version: u32,             // @4   1
    pub entry: u64,               // @8   内核入口 PA
    pub dram_base: u64,           // @16  装载窗基（QEMU virt = 0x4000_0000）
    pub dram_top: u64,            // @24  装载窗顶（半开；-m 128M = 0x4800_0000）
    pub memmap_pa: u64,           // @32  内存图快照 PA（刀 4 填）
    pub memmap_size: u32,         // @40  字节长（刀 4 填）
    pub memmap_desc_size: u32,    // @44  描述符步长（刀 4 填）
    pub memmap_desc_version: u32, // @48  EFI_MEMORY_DESCRIPTOR 版本（刀 4 填）
    pub _r0: u32,                 // @52  保留零（对齐）
    pub gop_fb_pa: u64,           // @56  帧缓冲 PA（V-6a 起 BerryBoot 填；零=缺席）
    pub gop_width: u32,           // @64  像素宽（V-6a 起 BerryBoot 填）
    pub gop_height: u32,          // @68  像素高（V-6a 起 BerryBoot 填）
    pub gop_stride: u32,          // @72  行字节数（V-6a 起 BerryBoot 填）
    pub gop_format: u32,          // @76  EFI PixelFormat 原值（V-6a 起 BerryBoot 填）
    pub init_blob_pa: u64,        // @80  init blob PA（v0 事实源；v1 置零不读）
    pub init_blob_len: u64,       // @88  init blob 字节长（v0 事实源；v1 置零不读）
    pub cmdline_pa: u64,          // @96  命令行 PA（留位零——裁决 B）
    pub cmdline_len: u64,         // @104 命令行字节长（留位零——裁决 B）
    pub init_segs_pa: u64,        // @112 描述符数组 PA（v1；= handoff 页内 @128 自相对；0=无）
    pub init_segs_count: u32,     // @120 段数（v1；0=无 INITRD；1..=32 合法）
    pub _r1: u32,                 // @124 保留零（对齐）
}

/// 布局执法表（与 berryboot 侧逐行相同——两侧同表，任一侧漂移编译期炸响）。
const _: () = {
    assert!(size_of::<BerryHandoff>() == 128);
    assert!(size_of::<InitSeg>() == 16);
    assert!(offset_of!(InitSeg, pa) == 0);
    assert!(offset_of!(InitSeg, len) == 8);
    assert!(offset_of!(BerryHandoff, magic) == 0);
    assert!(offset_of!(BerryHandoff, version) == 4);
    assert!(offset_of!(BerryHandoff, entry) == 8);
    assert!(offset_of!(BerryHandoff, dram_base) == 16);
    assert!(offset_of!(BerryHandoff, dram_top) == 24);
    assert!(offset_of!(BerryHandoff, memmap_pa) == 32);
    assert!(offset_of!(BerryHandoff, memmap_size) == 40);
    assert!(offset_of!(BerryHandoff, memmap_desc_size) == 44);
    assert!(offset_of!(BerryHandoff, memmap_desc_version) == 48);
    assert!(offset_of!(BerryHandoff, _r0) == 52);
    assert!(offset_of!(BerryHandoff, gop_fb_pa) == 56);
    assert!(offset_of!(BerryHandoff, gop_width) == 64);
    assert!(offset_of!(BerryHandoff, gop_height) == 68);
    assert!(offset_of!(BerryHandoff, gop_stride) == 72);
    assert!(offset_of!(BerryHandoff, gop_format) == 76);
    assert!(offset_of!(BerryHandoff, init_blob_pa) == 80);
    assert!(offset_of!(BerryHandoff, init_blob_len) == 88);
    assert!(offset_of!(BerryHandoff, cmdline_pa) == 96);
    assert!(offset_of!(BerryHandoff, cmdline_len) == 104);
    assert!(offset_of!(BerryHandoff, init_segs_pa) == 112);
    assert!(offset_of!(BerryHandoff, init_segs_count) == 120);
    assert!(offset_of!(BerryHandoff, _r1) == 124);
};

/// V-1a：入口 x0 分流判别——EFI 路首参 = handoff PA（BerryBoot `br entry`），
/// QEMU `-kernel` 路首参 = DTB PA/0。界检 [RAM_BASE, RAM_TOP) + 高半直映读
/// magic/version；未中 → `None`（DTB/fw_cfg 原路零扰动）。DTB 魔数
/// 0xD00D_FEED（BE）与 HANDOFF_MAGIC 互不碰撞——按 BRYH 认领不会误吞 DTB；
/// 版本不识别（>v1）同样 `None`：handoff 不消费、fail-safe 降级既有路
/// （A1-3b 起 v1 在册；更旧内核见 v1 同样不认领——双向兼容的版本门控）。
///
/// 返回引用按 boot 期静态处理：handoff 页由 BerryBoot AllocatePages 认领
/// （EFI 图内非 Conventional——V-1b reserve 后内核不再分配它），boot 后只读。
pub fn probe(x0: usize) -> Option<&'static BerryHandoff> {
    // 对齐 + 界检：x0=0（QEMU -kernel 实测）与窗外 PA 直接判非；128B 全结构
    // 须落窗内（checked 防加法回卷）。8 对齐 = 结构自身对齐要求（u64 字段）。
    if x0 & 7 != 0 {
        return None;
    }
    let end = x0.checked_add(size_of::<BerryHandoff>())?;
    if x0 < RAM_BASE || end > RAM_TOP {
        return None;
    }
    // SAFETY: 界检过 + set_kernel_va_base 已注入（rust_main 首行先于本函数）
    // ——高半直映 L2 全覆盖 RAM 窗，128B 读必落已映射块；boot 单线程、此后
    // 只读（BerryBoot 写完才 br entry，无并发写者）。
    let h = unsafe { &*(phys_to_virt(x0) as *const BerryHandoff) };
    if h.magic == HANDOFF_MAGIC && h.version <= HANDOFF_VERSION {
        Some(h)
    } else {
        None
    }
}

/// v1 init 分段视图（A1-3b）：结构校验后返回描述符数组切片。校验件：
/// - `hpa` = handoff 页基（调用方入口 x0）——`init_segs_pa` 必须恰为
///   `hpa + 128`（描述符数组随 handoff 同页的自相对契约，§3.2.1）；
/// - count 0..=[`INIT_SEGS_MAX`]（0 = 无 INITRD，返回空切片——调用方按
///   源缺席回落；越顶 = 递送面违约）；
/// - 每段：pa 4KiB 对齐（AllocatePages 出品）+ len>0 + 段区落
///   [RAM_BASE, RAM_TOP)（checked 防回卷）。
/// 任一不过 → `None`（调用方整源降级为「无 rootfs」存活，boot 期无 panic
/// 纪律）。Σlen 总量帽不在本面（消费侧 [`crate::ROOTFS_TOTAL_MAX`] 口径）。
/// boot 期只读面——与 `probe` 同一生命周期（BerryBoot 写完才 br entry）。
pub fn init_segs<'a>(h: &'a BerryHandoff, hpa: usize) -> Option<&'a [InitSeg]> {
    let count = h.init_segs_count as usize;
    if count > INIT_SEGS_MAX {
        return None;
    }
    if count == 0 {
        // 空段（无 INITRD）：pa 也须为零（零值缺席语义，禁只看 count）。
        return if h.init_segs_pa == 0 {
            Some(&[])
        } else {
            None
        };
    }
    // 同页自相对契约：数组恰在 handoff 页 @128（640B < 4KiB 单页自含）。
    if h.init_segs_pa != (hpa as u64).checked_add(128)? {
        return None;
    }
    let base = phys_to_virt(h.init_segs_pa as usize) as *const InitSeg;
    let segs = unsafe { core::slice::from_raw_parts(base, count) };
    for s in segs {
        if s.pa == 0 || s.len == 0 || s.pa & 0xfff != 0 {
            return None;
        }
        let end = s.pa.checked_add(s.len)?;
        if s.pa < RAM_BASE as u64 || end > RAM_TOP as u64 {
            return None;
        }
    }
    Some(segs)
}

/// V-6a：GOP 携面消费视图——BerryBoot banner 期 QueryMode 事实
/// （handoff @56..@76，fbcon 数据源）。界检四件：fb/几何/stride 非零 +
/// fmt≤2（0=RGBx 1=BGRx 2=BitMask——3 BltOnly 无线性帧缓冲，装载器已
/// 按缺席递送，此处双保险）。`None` = 无可用线性帧缓冲（无显示设备
/// EFI 路 / QEMU `-kernel` 路恒缺席）——消费面零扰动。
pub struct GopInfo {
    pub fb_pa: u64,  // 线性帧缓冲物理基
    pub width: u32,  // HorizontalResolution
    pub height: u32, // VerticalResolution
    pub stride: u32, // 行字节数（PixelsPerScanLine × 4）
    pub format: u32, // EFI PixelFormat 原值
}

/// GOP 事实提取（含界检）。boot 期只读面——与 `probe` 同一生命周期
/// （handoff 页 BerryBoot 写完才 `br entry`，无并发写者）。
pub fn gop_info(h: &BerryHandoff) -> Option<GopInfo> {
    if h.gop_fb_pa == 0 || h.gop_width == 0 || h.gop_height == 0 || h.gop_stride == 0 {
        return None;
    }
    if h.gop_format > 2 {
        return None;
    }
    Some(GopInfo {
        fb_pa: h.gop_fb_pa,
        width: h.gop_width,
        height: h.gop_height,
        stride: h.gop_stride,
        format: h.gop_format,
    })
}
