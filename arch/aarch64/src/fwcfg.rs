//! QEMU virt fw_cfg（firmware configuration）读面——G-eco 入机侧。
//!
//! 背景（2026-10 实测 QEMU 11，QEMU 源 hw/arm/boot.c 同源）：非 Linux
//! `-kernel` ELF 直加载**不走 Linux boot 协议**——入口 x0=0、RAM 内无 DTB、
//! `-initrd` 整体不加载（initrd/cmdline/dtb 机制全在 is_linux 分支里）。
//! rootfs（initrd 角色）的递送通道改走 **fw_cfg 具名文件**：
//! `-fw_cfg name=opt/berry/rootfs,file=rootfs.cpio` —— 目录自描述（名字+
//! 大小，内核无需 cpio 步进定界）、无魔法地址（boot 期装进内核缓冲）、
//! 可扩展多 blob。真机/Linux boot 协议路径留 DTB 侧（`kernel/src/dtb.rs`，
//! 入口 x0 ≠ 0 时自然生效）。
//!
//! 协议（legacy 无 DMA 面，boot 一次性拷贝无热路径顾虑）：
//! - selector 口（base+8）收**大端** u16 键 → data 口（base+0）按序弹出项字节；
//! - u32 读出的低字节 = 项内**首个**字节（签名 "QEMU" 校验已证此序）——
//!   `to_le_bytes` 回写即保项内字节序，BE 数值字段 `swap_bytes` 还原；
//! - 文件目录（selector 0x19）：u32 count + N×72B 项
//!   `{u32 size, u16 select, u16 reserved, char name[56]}`，size/select 大端。
//!
//! 纪律：MMIO 读失败不设错误面（QEMU 可信），但目录行走全界检、畸形一律
//! `None` 降级（ABI §8.0「永不 panic」同源精神）。

use crate::mmio;
use crate::KERNEL_VA_BASE;

const FW_CFG_BASE: usize = KERNEL_VA_BASE + 0x0902_0000;
const DATA: usize = FW_CFG_BASE;
const SELECTOR: usize = FW_CFG_BASE + 0x8;

const FW_CFG_SIGNATURE: u16 = 0x00;
const FW_CFG_FILE_DIR: u16 = 0x19;

/// rootfs（initrd 角色）的 fw_cfg 具名——justfile `-fw_cfg name=…` 与此处
/// 的单一事实源（两侧对读维护）。
pub const ROOTFS_NAME: &str = "opt/berry/rootfs";

/// 多卷上限（A1-3b）：`opt/berry/rootfs0..9` —— 单数字后缀即够（10 卷 ×
/// 单卷帽 32MiB ≫ 总量帽 128MiB，帽先绑定；mkcpio `--split` 产链对口）。
pub const ROOTFS_VOLS: usize = 10;

/// 按序号查多卷具名项（`opt/berry/rootfs{i}`，i=0..ROOTFS_VOLS——
/// [`ROOTFS_NAME`] 的多卷同位扩面）。越界序号 = 恒 `None`。
pub fn rootfs_volume(i: usize) -> Option<FileEntry> {
    if i >= ROOTFS_VOLS {
        return None;
    }
    // 栈上定格名（17B 定长——HAL 无 alloc，格式化 = 末字节拼数字）。
    let mut name = *b"opt/berry/rootfs0";
    name[16] = b'0' + i as u8;
    let s = core::str::from_utf8(&name).ok()?;
    find_file(s)
}

/// fw_cfg 在位校验：SIGNATURE 项 = ASCII "QEMU"（低字节 'Q' = 0x51）。
pub fn present() -> bool {
    // SAFETY: selector/data 是对齐 MMIO 口；只读路径无副作用。selector 收
    // BE u16——先 to_be 再整字写（MMIO 总线宽度敏感，不作 u32 拼接）。
    unsafe {
        mmio::write_u16(SELECTOR, FW_CFG_SIGNATURE.to_be());
        mmio::read(DATA) == 0x554D_4551
    }
}

/// 顺序读一个 u32（selector 已设好），按项内字节序解释。
unsafe fn stream_le32() -> u32 {
    mmio::read(DATA)
}

/// 顺序读一个 BE u32（目录的 count/size 等数值字段）。
unsafe fn stream_be32() -> u32 {
    stream_le32().swap_bytes()
}

/// fw_cfg 文件目录项。
#[derive(Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub select: u16,
    pub size: u32,
}

/// 在 fw_cfg 文件目录里按名查项；不在位/目录畸形/名字超 55 字节一律 `None`。
pub fn find_file(name: &str) -> Option<FileEntry> {
    if name.len() >= 56 {
        return None;
    }
    // SAFETY: 只读 MMIO 流；目录行走全界检（count 上限 1024 项防畸形大数）。
    unsafe {
        mmio::write_u16(SELECTOR, FW_CFG_FILE_DIR.to_be());
        let count = stream_be32();
        if count > 1024 {
            return None;
        }
        for _ in 0..count {
            let size = stream_be32();
            let sel_rsv = stream_be32(); // 高 16 位 select、低 16 位 reserved（BE）
            let select = (sel_rsv >> 16) as u16;
            // name[56]：14 × u32 顺序读，低字节在前 = 项内字节序原样恢复。
            let mut nbuf = [0u8; 56];
            for chunk in nbuf.chunks_mut(4) {
                chunk.copy_from_slice(&stream_le32().to_le_bytes()[..chunk.len()]);
            }
            let nul = nbuf.iter().position(|&c| c == 0)?;
            if &nbuf[..nul] == name.as_bytes() {
                return Some(FileEntry { select, size });
            }
        }
    }
    None
}

/// 把 `entry` 的文件内容整读进 `buf`（`buf.len()` 须恰为 `entry.size`——
/// 由调用方保证；尾字节不足 4 的余数按前缀截取）。
///
/// 返回 false = 项不在位/长度不符（零填充不写入）。
pub fn read_to(entry: &FileEntry, buf: &mut [u8]) -> bool {
    if buf.len() != entry.size as usize {
        return false;
    }
    // SAFETY: 只读 MMIO 流；恰读 size 字节后停（不越过项尾）。
    unsafe {
        mmio::write_u16(SELECTOR, entry.select.to_be());
        for chunk in buf.chunks_mut(4) {
            let w = stream_le32().to_le_bytes();
            chunk.copy_from_slice(&w[..chunk.len()]);
        }
    }
    true
}

/// 顺序续读会话（A1-3b）：大 blob 逐 chunk 拼装用——selector **一次选定**，
/// data 口 offset 随每次读自然推进（无 seek；discard 前进 O(n²) 禁）。
/// [`read_to`] 是恰全长整读（单卷小 blob 一次成），本面是它的分块版：
/// 语义约束 = **会话存续期内不得有其他 fw_cfg 访问**（selector 是全局单
/// 活动流——boot 单线程按序使用，无抢占窗）。
pub struct SeqRead {
    /// 剩余未读字节（项尾守卫：越项尾的 data 口行为不承诺，绝不越过）。
    remaining: u32,
}

impl SeqRead {
    /// 选定 `entry` 开始顺序读（selector 写一次）。空项（size==0）→ `None`。
    pub fn open(entry: &FileEntry) -> Option<SeqRead> {
        if entry.size == 0 {
            return None;
        }
        // SAFETY: 只写 selector 口（选定后 data 口从项首起流）。
        unsafe { mmio::write_u16(SELECTOR, entry.select.to_be()) };
        Some(SeqRead { remaining: entry.size })
    }

    /// 顺序读 ≤`buf.len()` 字节（项尾截断），返回实读数（0 = 本项读完）。
    ///
    /// 4 字节粒度约束：data 口按 u32 弹出、尾字节不足 4 的余数按**前缀截取**
    /// （与 [`read_to`] 同序——被截掉的尾 1-3 字节已从流中消费）。因此**非
    /// 末段读**（本步没读到项尾）的实读数向下取整到 4 的倍数——调用方 chunk
    /// 缓冲取 4 的倍数（A1-3b 内核侧 = 4MiB）即天然无截损。
    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        if self.remaining == 0 || buf.is_empty() {
            return 0;
        }
        let mut want = core::cmp::min(buf.len() as u32, self.remaining);
        // 非末段：4 字节粒度推进（余数只许发生在项尾——中途半 u32 = 跳字节）。
        if want < self.remaining {
            want &= !3;
            if want == 0 {
                return 0;
            }
        }
        // SAFETY: 只读 MMIO 流；remaining 守卫保证不越过项尾。
        unsafe {
            for chunk in buf[..want as usize].chunks_mut(4) {
                let w = stream_le32().to_le_bytes();
                chunk.copy_from_slice(&w[..chunk.len()]);
            }
        }
        self.remaining -= want;
        want as usize
    }
}

// ---- GUI P2：ramfb 模式设置（一次性 boot 提交；每帧呈现走共享 RAM 面） ----
//
// QEMU ramfb 的配置项 `etc/ramfb` 是**可写** fw_cfg 文件：28B 大端
// { addr u64, fourcc u32, flags u32, width u32, height u32, stride u32 }。
// 写入必须走 DMA 口（legacy PIO 只读）——这也是本文件从「只读」扩到
// 「一次性写」的全部原因。写成功后 QEMU 立刻替换 scanout surface，VNC
// 与宿主共享 RAM 两侧同时看到新几何；此后客机只画像素，不需要每帧
// commit（宿主 `berry shell` 直接读 `memory-backend-file` 的 RAM 镜）。
//
// 约束：DMA 源缓冲必须在客机物理 RAM 且 PA 正确。这里用 HAL `.bss` 的
// 64B 对齐静态块——内核镜像 VMA = KERNEL_VA_BASE + LMA，故 PA = VA -
// KERNEL_VA_BASE；不依赖分配器，可在 `mm::init` 前调用。

const DMA_REG: usize = FW_CFG_BASE + 0x10;
const DMA_CTRL_ERROR: u32 = 1;
const DMA_CTRL_SELECT: u32 = 1 << 3;
const DMA_CTRL_WRITE: u32 = 1 << 4;

/// 64B 对齐静态块：前 16B = FWCfgDmaAccess 描述符，后 28B = ramfb 配置。
/// 描述符在**客机 RAM**里（fw_cfg DMA 寄存器只收它的 PA），所以数据与
/// 描述符同一块，PA = VA - KERNEL_VA_BASE。
#[repr(align(64))]
struct Align64([u8; 64]);

static mut RAMFB_CFG: Align64 = Align64([0; 64]);

#[inline]
unsafe fn put_be32(p: *mut u8, v: u32) {
    core::ptr::copy_nonoverlapping(v.to_be_bytes().as_ptr(), p, 4);
}

#[inline]
unsafe fn put_be64(p: *mut u8, v: u64) {
    core::ptr::copy_nonoverlapping(v.to_be_bytes().as_ptr(), p, 8);
}

#[inline]
unsafe fn get_be32(p: *const u8) -> u32 {
    let mut b = [0u8; 4];
    core::ptr::copy_nonoverlapping(p, b.as_mut_ptr(), 4);
    u32::from_be_bytes(b)
}

/// 写 ramfb 配置项：descriptor {control,length,address} + 28B cfg 同块。
/// `data_pa` 指向 cfg 本体；descriptor 在静态块首。返回 false = DMA 拒/超时。
unsafe fn dma_write(select: u16, data_pa: u64, len: u32) -> bool {
    let base = core::ptr::addr_of_mut!(RAMFB_CFG.0) as *mut u8;
    let ctrl = ((select as u32) << 16) | DMA_CTRL_SELECT | DMA_CTRL_WRITE;
    put_be32(base, ctrl);
    put_be32(base.add(4), len);
    put_be64(base.add(8), data_pa);
    // 描述符先落到 RAM，再敲 8B DMA 门铃（门铃值 = descriptor PA）。
    core::arch::asm!("dsb ish", options(nostack, preserves_flags));
    let desc_pa = base as usize - KERNEL_VA_BASE;
    mmio::write_u64(DMA_REG, (desc_pa as u64).to_be());
    for _ in 0..1_000_000 {
        let v = get_be32(base);
        if v & DMA_CTRL_ERROR != 0 {
            return false;
        }
        if v == 0 {
            return true;
        }
    }
    false
}

/// 把 GOP 缓冲切成 `width × height`（stride = width×4）并通知 QEMU。
/// 返回 false = 无 `etc/ramfb` 项/项长非 28/DMA 拒——调用方保持原几何。
pub fn ramfb_set(fb_pa: u64, width: u32, height: u32, stride: u32) -> bool {
    let Some(entry) = find_file("etc/ramfb") else {
        crate::uart_puts(b"ramfb: item absent\n");
        return false;
    };
    crate::uart_puts(b"ramfb: select=");
    crate::uart_put_dec(entry.select as u64);
    crate::uart_puts(b" size=");
    crate::uart_put_dec(entry.size as u64);
    crate::uart_puts(b"\n");
    if entry.size != 28 || width == 0 || height == 0 || stride == 0 {
        return false;
    }
    // SAFETY: 静态块独占；boot 单线程、任何 EL0 读取之前。
    unsafe {
        let base = core::ptr::addr_of_mut!(RAMFB_CFG.0) as *mut u8;
        let cfg = base.add(16);
        put_be64(cfg, fb_pa);
        put_be32(cfg.add(8), 0x3432_5258); // 'XR24'
        put_be32(cfg.add(12), 0);
        put_be32(cfg.add(16), width);
        put_be32(cfg.add(20), height);
        put_be32(cfg.add(24), stride);
        let cfg_pa = cfg as usize - KERNEL_VA_BASE;
        let ok = dma_write(entry.select, cfg_pa as u64, 28);
        if !ok {
            let base = core::ptr::addr_of_mut!(RAMFB_CFG.0) as *mut u8;
            crate::uart_puts(b"ramfb: dma fail ctrl=");
            crate::uart_put_hex64(get_be32(base) as u64);
            crate::uart_puts(b" desc_pa=");
            crate::uart_put_hex64((base as usize - KERNEL_VA_BASE) as u64);
            crate::uart_puts(b" cfg_pa=");
            crate::uart_put_hex64(cfg_pa as u64);
            crate::uart_puts(b"\n");
        }
        ok
    }
}
