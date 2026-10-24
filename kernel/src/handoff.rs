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
//! - 留位零值（裁决 B）：gop_*（V-1 fbcon 数据源——`docs/VM里程碑设计草案.md`
//!   L114 携面规划）+ init_blob_*/cmdline_*（V-1+ rootfs 递送替代 fw_cfg）。
//!   零值 = 「无此物」：pa=0 且 len=0 双字段共同判定，禁只看 pa。
#![allow(dead_code)] // V-1a 消费 magic/version；V-1b 消费 memmap_*；V-1c 消费 init_blob_*；gop_*/cmdline_* 留位零值待 V-1d+

use berry_hal_aarch64::{RAM_BASE, RAM_TOP};
use berry_mm::va::phys_to_virt;
use core::mem::{offset_of, size_of};

/// 魔数：LE u32 读出 = ASCII "BRYH"（BerryOS Handoff）。
pub const HANDOFF_MAGIC: u32 = 0x4252_5948;

/// 协议版本：v0 = 0（字段集冻结于 V-0b 刀 3，扩字段 = bump version）。
pub const HANDOFF_VERSION: u32 = 0;

/// 交接结构（112B，全 8B 字段 8 对齐；无指针——PA 直递，装载器/内核各自
/// 解址，杜绝跨边界引用生命周期假设——本仓 ABI 通行纪律）。
#[repr(C)]
pub struct BerryHandoff {
    pub magic: u32,               // @0   "BRYH"
    pub version: u32,             // @4   0
    pub entry: u64,               // @8   内核入口 PA
    pub dram_base: u64,           // @16  装载窗基（QEMU virt = 0x4000_0000）
    pub dram_top: u64,            // @24  装载窗顶（半开；-m 128M = 0x4800_0000）
    pub memmap_pa: u64,           // @32  内存图快照 PA（刀 4 填）
    pub memmap_size: u32,         // @40  字节长（刀 4 填）
    pub memmap_desc_size: u32,    // @44  描述符步长（刀 4 填）
    pub memmap_desc_version: u32, // @48  EFI_MEMORY_DESCRIPTOR 版本（刀 4 填）
    pub _r0: u32,                 // @52  保留零（对齐）
    pub gop_fb_pa: u64,           // @56  帧缓冲 PA（留位零——V-1 fbcon）
    pub gop_width: u32,           // @64  像素宽（留位零）
    pub gop_height: u32,          // @68  像素高（留位零）
    pub gop_stride: u32,          // @72  行字节数（留位零）
    pub gop_format: u32,          // @76  EFI PixelFormat 原值（留位零）
    pub init_blob_pa: u64,        // @80  init blob PA（留位零——裁决 B）
    pub init_blob_len: u64,       // @88  init blob 字节长（留位零）
    pub cmdline_pa: u64,          // @96  命令行 PA（留位零——裁决 B）
    pub cmdline_len: u64,         // @104 命令行字节长（留位零）
}

/// 布局执法表（与 berryboot 侧逐行相同——两侧同表，任一侧漂移编译期炸响）。
const _: () = {
    assert!(size_of::<BerryHandoff>() == 112);
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
};

/// V-1a：入口 x0 分流判别——EFI 路首参 = handoff PA（BerryBoot `br entry`），
/// QEMU `-kernel` 路首参 = DTB PA/0。界检 [RAM_BASE, RAM_TOP) + 高半直映读
/// magic/version；未中 → `None`（DTB/fw_cfg 原路零扰动）。DTB 魔数
/// 0xD00D_FEED（BE）与 HANDOFF_MAGIC 互不碰撞——按 BRYH 认领不会误吞 DTB；
/// 版本不识别（>v0）同样 `None`：handoff 不消费、fail-safe 降级既有路。
///
/// 返回引用按 boot 期静态处理：handoff 页由 BerryBoot AllocatePages 认领
/// （EFI 图内非 Conventional——V-1b reserve 后内核不再分配它），boot 后只读。
pub fn probe(x0: usize) -> Option<&'static BerryHandoff> {
    // 对齐 + 界检：x0=0（QEMU -kernel 实测）与窗外 PA 直接判非；112B 全结构
    // 须落窗内（checked 防加法回卷）。8 对齐 = 结构自身对齐要求（u64 字段）。
    if x0 & 7 != 0 {
        return None;
    }
    let end = x0.checked_add(size_of::<BerryHandoff>())?;
    if x0 < RAM_BASE || end > RAM_TOP {
        return None;
    }
    // SAFETY: 界检过 + set_kernel_va_base 已注入（rust_main 首行先于本函数）
    // ——高半直映 L2 全覆盖 RAM 窗，112B 读必落已映射块；boot 单线程、此后
    // 只读（BerryBoot 写完才 br entry，无并发写者）。
    let h = unsafe { &*(phys_to_virt(x0) as *const BerryHandoff) };
    if h.magic == HANDOFF_MAGIC && h.version == HANDOFF_VERSION {
        Some(h)
    } else {
        None
    }
}
