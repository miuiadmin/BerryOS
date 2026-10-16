//! 最小 FDT（flattened device tree）walker——P4 余项 G-eco 入机侧第一刀。
//!
//! 用途单一：解析入口 x0 DTB 的 `/chosen` initrd 物理区间（`linux,initrd-start/end`）。
//! 这是**真机/Linux boot 协议**的递送形态——QEMU 非 Linux ELF `-kernel` 直加载
//! 实测不走此协议（x0=0、RAM 无 DTB、`-initrd` 不加载，2026-10 实证；QEMU 侧
//! 主源是 fw_cfg 具名文件，见 `架构相关/aarch64/src/fwcfg.rs`）。本模块把区间
//! 解析出来——后续步包成只读 VMO 形对象交 svcmgr（cpio/ELF 解析全在用户态；
//! 内核只补「物理段变对象」原语，微内核原则）。
//!
//! 纪律：
//! - **纯函数、零 panic**——DTB 虽来自 QEMU（可信），畸形输入（坏魔数/
//!   越界偏移/截断/未知 token）一律 `None`，boot 侧降级为「无 initrd」
//!   存活（ABI §8.0「永不 panic」同源精神）。
//! - FDT 大端字段全 `from_be`；结构块偏移由推进逻辑保证 4 对齐。
//! - 迭代 walk 不递归——恶意深嵌套只耗 cycle 不炸栈，自然终止
//!   （每 token 至少前进 4 字节，越界即 `None` 出口）。

use berry_hal_aarch64::{RAM_BASE, RAM_TOP};
use berry_mm::va::phys_to_virt;

const FDT_MAGIC: u32 = 0xD00D_FEED;
const FDT_BEGIN_NODE: u32 = 0x1;
const FDT_END_NODE: u32 = 0x2;
const FDT_PROP: u32 = 0x3;
const FDT_NOP: u32 = 0x4;
const FDT_END: u32 = 0x9;
/// FDT 头固定 10 个 u32（magic/totalsize/off_mem_rsvmap/off_dt_struct/
/// off_dt_strings/version/last_comp/boot_cpuid/size_strings/size_struct）。
const FDT_HEADER_LEN: usize = 40;

/// 界检大端 u32 读取（`off + 4` 越界即 `None`）。
fn rd32(b: &[u8], off: usize) -> Option<u32> {
    let end = off.checked_add(4)?;
    if end > b.len() { return None; }
    Some(u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]))
}

/// NUL 结尾字符串定位（返回字节串与**含 NUL** 的长度；越界无 NUL 即 `None`）。
fn cstr_at(b: &[u8], off: usize) -> Option<(&[u8], usize)> {
    if off >= b.len() { return None; }
    let rest = &b[off..];
    let nul = rest.iter().position(|&c| c == 0)?;
    Some((&rest[..nul], nul + 1))
}

/// 属性值按 cell 数转 u64（1 cell = u32 / 2 cell = u64，大端；其他长度畸形）。
fn prop_u64(dt: &[u8], off: usize, len: usize) -> Option<u64> {
    match len {
        4 => Some(rd32(dt, off)? as u64),
        8 => Some(((rd32(dt, off)? as u64) << 32) | rd32(dt, off.checked_add(4)?)? as u64),
        _ => None,
    }
}

/// 在合法 FDT 里找 `/chosen`（**根的直接子节点**）的 initrd 区间。
///
/// 返回 `(start, end)` 物理地址（`end > start`；start==0 视无效——QEMU 不会
/// 把 initrd 放 0，0 用作「无」哨兵）。
pub fn find_initrd(dtb: &[u8]) -> Option<(u64, u64)> {
    if rd32(dtb, 0)? != FDT_MAGIC { return None; }
    let totalsize = rd32(dtb, 4)? as usize;
    let off_struct = rd32(dtb, 12)? as usize;
    let off_strings = rd32(dtb, 16)? as usize;
    // 声明超实给 = 截断，视畸形（不悄悄缩水解析）。
    if totalsize < FDT_HEADER_LEN || totalsize > dtb.len() { return None; }
    let dt = &dtb[..totalsize];
    if off_struct >= dt.len() || off_strings >= dt.len() { return None; }
    let strings = &dt[off_strings..];

    let mut off = off_struct;
    let mut depth = 0usize; // 根 BEGIN 后为 1；根的直接子节点为 2
    let mut in_chosen = false;
    let mut start: Option<u64> = None;
    let mut end: Option<u64> = None;
    loop {
        match rd32(dt, off)? {
            FDT_BEGIN_NODE => {
                off = off.checked_add(4)?;
                let (name, len) = cstr_at(dt, off)?;
                // 名字段（含 NUL）向上 4 对齐。
                off = off.checked_add((len + 3) & !3)?;
                depth += 1;
                in_chosen = depth == 2 && name == b"chosen";
            }
            FDT_PROP => {
                let len = rd32(dt, off.checked_add(4)?)? as usize;
                let nameoff = rd32(dt, off.checked_add(8)?)? as usize;
                let data_off = off.checked_add(12)?;
                off = data_off.checked_add((len + 3) & !3)?;
                if in_chosen {
                    let (name, _) = cstr_at(strings, nameoff)?;
                    if name == b"linux,initrd-start" {
                        start = Some(prop_u64(dt, data_off, len)?);
                    } else if name == b"linux,initrd-end" {
                        end = Some(prop_u64(dt, data_off, len)?);
                    }
                }
            }
            FDT_END_NODE => {
                off = off.checked_add(4)?;
                depth = depth.checked_sub(1)?;
                if depth <= 1 { in_chosen = false; }
            }
            FDT_NOP => { off = off.checked_add(4)?; }
            FDT_END => break,
            _ => return None, // 未知 token = 畸形
        }
    }
    match (start, end) {
        (Some(s), Some(e)) if s != 0 && e > s => Some((s, e)),
        _ => None,
    }
}

/// boot 侧探测：入口 x0 的 DTB 物理地址 → 高半直映切片 → `/chosen` 区间。
///
/// `dtb_pa` 须落在 `[RAM_BASE, RAM_TOP)`（QEMU virt 把 DTB 放 RAM 内）；
/// `totalsize` 以「到 RAM 顶」为上限截（QEMU 可信，界检是纪律不是不信任）。
pub fn boot_probe(dtb_pa: usize) -> Option<(u64, u64)> {
    if dtb_pa < RAM_BASE || dtb_pa >= RAM_TOP { return None; }
    // 先只读头 8 字节拿 totalsize（u8 读无对齐要求）。
    // SAFETY: boot 单线程；pa 已界检在 [RAM_BASE, RAM_TOP)——高半直映 L2
    // 覆盖到 0x8000_0000，8 字节读必落在已映射块内。
    let hdr = unsafe { core::slice::from_raw_parts(phys_to_virt(dtb_pa) as *const u8, 8) };
    let total = rd32(hdr, 4)? as usize;
    if total < FDT_HEADER_LEN { return None; }
    let limit = (RAM_TOP - dtb_pa).min(total);
    // SAFETY: 同上；长度取 totalsize 与 RAM 顶界的最小值，界内必映。
    let dtb = unsafe { core::slice::from_raw_parts(phys_to_virt(dtb_pa) as *const u8, limit) };
    find_initrd(dtb)
}
