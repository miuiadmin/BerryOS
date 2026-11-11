//! 最小 FDT（flattened device tree）walker——P4 余项 G-eco 入机侧第一刀。
//!
//! 用途单一：解析入口 x0 DTB 的 `/chosen` initrd 物理区间（`linux,initrd-start/end`）。
//! 这是**真机/Linux boot 协议**的递送形态——QEMU 非 Linux ELF `-kernel` 直加载
//! 实测不走此协议（x0=0、RAM 无 DTB、`-initrd` 不加载，2026-10 实证；QEMU 侧
//! 主源是 fw_cfg 具名文件，见 `arch/aarch64/src/fwcfg.rs`）。本模块把区间
//! 解析出来——后续步包成只读 VMO 形对象交 svcmgr（cpio/ELF 解析全在用户态；
//! 内核只补「物理段变对象」原语，微内核原则）。
//!
//! **判定律单源（W3-Q18）**：rd32/cstr_at/prop_u64 界检读 + find_initrd 迭代
//! walk 状态机（token 推进律/`/chosen` 深度判/4 对齐步进/畸形九态全 None 零
//! panic）已外迁 `berry-dtbface`（host 可测钉板，14 测）。本文件只保留
//! `boot_probe` 物理臂——`from_raw_parts` 高半直映切片 + RAM 界检属内核侧
//! 「地址怎么解」，「字节怎么判」在 dtbface。
//!
//! 纪律（自外迁前逐字继承）：
//! - **纯函数、零 panic**——DTB 虽来自 QEMU（可信），畸形输入（坏魔数/
//!   越界偏移/截断/未知 token）一律 `None`，boot 侧降级为「无 initrd」
//!   存活（ABI §8.0「永不 panic」同源精神）。
//! - FDT 大端字段全 `from_be`；结构块偏移由推进逻辑保证 4 对齐。
//! - 迭代 walk 不递归——恶意深嵌套只耗 cycle 不炸栈，自然终止
//!   （每 token 至少前进 4 字节，越界即 `None` 出口）。

pub use berry_dtbface::find_initrd; // 单源 re-export（ktest M 段消费）
use berry_dtbface::rd32;
use berry_hal_aarch64::RAM_BASE;
use berry_hal_aarch64::RAM_TOP;
use berry_mm::va::phys_to_virt;

/// FDT 头固定 10 个 u32（magic/totalsize/off_mem_rsvmap/off_dt_struct/
/// off_dt_strings/version/last_comp/boot_cpuid/size_strings/size_struct）。
const FDT_HEADER_LEN: usize = 40;

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
