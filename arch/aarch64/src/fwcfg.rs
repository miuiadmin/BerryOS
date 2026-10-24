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
