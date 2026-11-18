//! ARM64 stage-1 **boot 页表** + 开启 MMU（plan Step 4，命门）。
//!
//! 2b-1 高半翻转后（`内存管理设计.md` §5.1）：内核主体在高半执行；本模块的
//! `build_boot_pt`/`enable_mmu` 留 `.text.boot` 低半（VMA==LMA，boot asm 在开 MMU
//! 前后 `bl` 调用），4 张表留 `.bss.boot` 低半（VMA==PA：早期代码 ADRP 只有 ±4GB，
//! 引用不了高半静态量；也使 TTBR 写入的地址算式恒为 PA，无需 virt_to_phys）。
//!
//! 映射（低半 PA 与高半 `KERNEL_VA_BASE+pa` 两个别名同此规则）：
//!  - RAM `[RAM_BASE, RAM_TOP)`（含内核镜像 + 空闲 RAM）：**Normal WB**；
//!  - UART `0x0900_0000`、GIC `0x0800_0000` 各所在 2MiB 块：**Device-nGnRnE**。
//!  - TTBR0 = BOOT_L1 身份表（**临时**，2b-2 退役——届时低半访问必然 fault 是正向断言）；
//!    TTBR1 = KERNEL_L1 高半内核表（canonical，正式）。
//!
//! **最高风险点（任一漏=静默挂死无输出）**：开 MMU **前**设备 MMIO 必须先映进来——
//! 否则开启后下一次 `uart_putc` 写 VA 0x0900_0000 会同步外部中止、静默挂死。
//! cache 从 boot 起空（`boot.rs` 已 `sctlr_el1=0`，C=I=0），故建表后无需刷 cache，
//! 只需 `dsb ish` 保证表写入对页表遍历可见，再置 `sctlr_el1 = M|C|I`、`isb`。

use crate::paging::*;
use crate::{GICD_BASE, KERNEL_VA_BASE, RAM_BASE, RAM_TOP, UART0_BASE};
use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};

/// 免失效激活档（P0-3 `activate_ttbr0_keep_tlb`）可信度：`true` = 虚拟化层
/// 不环回 TTBR0 的 ASID 位域（QEMU HVF 实测形态——Hypervisor 自管 guest
/// ASID，寄存器读面丢弃位域），按号 TLB 语义不可依托。boot 单线程窗由
/// [`probe_asid_field_roundtrip`] 探测写入；调度切换临界区只读（Relaxed
/// 足够——程序序即真实序，A12 同款纪律）。
static ASID_TLB_UNTRUSTED: AtomicBool = AtomicBool::new(false);

const TWO_MB: usize = 1 << 21;
const L2_INDEX_MASK: usize = PTES_PER_TABLE - 1; // 9 位

/// L1 表（512 项，4KiB）。
#[repr(C, align(4096))]
struct L1Table([u64; PTES_PER_TABLE]);
/// L2 表（512 项，4KiB）。
#[repr(C, align(4096))]
struct L2Table([u64; PTES_PER_TABLE]);
/// L3 表（512 项，4KiB；W3-Q104 块拆分用——叶描述符表与 L2 同构）。
#[repr(C, align(4096))]
struct L3Table([u64; PTES_PER_TABLE]);

// boot.asm 先清 .bss.boot（__boot_bss_*）；静态零初始化 → NOLOAD 低半段。
// 2b-2 低半退役后，这些表经 phys_to_virt(pa==va) 访问，规则不变。
#[link_section = ".bss.boot"]
static mut BOOT_L1: L1Table = L1Table([0; PTES_PER_TABLE]);
/// L1[0]：低区（设备 MMIO，`[0, 1GiB)`）。
#[link_section = ".bss.boot"]
static mut BOOT_L2_LOW: L2Table = L2Table([0; PTES_PER_TABLE]);
/// L1[1]：RAM 区（`[0x4000_0000, 0x8000_0000)`，实际映 `[RAM_BASE, RAM_TOP)`）。
#[link_section = ".bss.boot"]
static mut BOOT_L2_RAM: L2Table = L2Table([0; PTES_PER_TABLE]);

/// TTBR1 内核表（2b-0 引入）。条目与 BOOT_L1 同构——指向**同一对 L2 表**：
/// 描述符输出的是 PA，与从哪张 L1 走查无关；`KERNEL_VA_BASE` bits[38:0]=0，
/// 故 KVA[38:30] == PA[38:30]，高半 L1 索引镜像身份 L1 索引。2b-2 低半退役后
/// 这张表成为内核唯一页表根。
#[link_section = ".bss.boot"]
static mut KERNEL_L1: L1Table = L1Table([0; PTES_PER_TABLE]);

/// 内核空表（2b-2 低半退役）：TTBR0 的「无 AS」落点——全零 → 任何低半访问必然
/// fault（身份表退役的正向断言即由此而来）。放普通 `.bss`（高半 VMA，boot 期已清零；
/// 仅供高半代码经直映访问）；TTBR0 写入时按 `VMA - KERNEL_VA_BASE` 折算 PA。
static NULL_L1: L1Table = L1Table([0; PTES_PER_TABLE]);

/// 2b-2 回退开关（§5.1「保留身份表分支」）：`false` 时 [`retire_low_half`] 不动
/// TTBR0（低半身份继续可用），供问题定位时快速回退。
const RETIRE_IDENTITY: bool = true;

/// TCR_EL1：4KiB granule、39-bit VA（T0SZ=T1SZ=25）、inner shareable、WB 页表走查、IPS=0。
///
/// 逐字段拼（cortex-a72 / ARMv8.0）。TG1 编码与 TG0 不同（4KiB: TG0=00、TG1=10）。
const TCR_EL1: u64 = (25 << 0) // T0SZ
    | (0b01 << 8)  // IRGN0（页表走查 inner WB）
    | (0b01 << 10) // ORGN0
    | (0b11 << 12) // SH0 = inner shareable
    | (0b00 << 14) // TG0 = 4KiB
    | (25 << 16)   // T1SZ
    | (0b01 << 24) // IRGN1
    | (0b01 << 26) // ORGN1
    | (0b11 << 28) // SH1
    | (0b10 << 30) // TG1 = 4KiB
    | (0b000 << 32); // IPS = 32-bit PA（UART/GIC/RAM 全 < 4GiB）

/// SCTLR_EL1 使能值：`M | C | I | SPAN`（开 MMU + 数据/统一 cache + 指令 cache +
/// 禁「异常入 EL1 自动置 PAN」）。A=0（不查对齐）。
///
/// SPAN（bit 23，ARMv8.1-PAN 扩展）= **必置 1**（2026-11-15 北极星审计修）：
/// PAN 实现上该位复位值 0 = 「取异常进 EL1 时硬件自动置 PSTATE.PAN=1」——
/// 本内核从不使用 PAN（无任何 `msr pan` 面；copy_from/to_user 靠 SW 走查
/// 逐页验 AP 位后直接访问，v1 契约 = EL1 可自由读写用户页），PAN 一开则
/// 一切 EL0 可访问页（AP=01）的 EL1 数据访问全变 level-3 权限错——首个
/// 踩雷点 = svcmgr 首个带用户出参的 syscall（`copy_to_user → memcpy` 写
/// `vmo_get_size` 出参），症状 = TAGTAB ok 后 D-abort(current) ISS=0x4f
/// （WnR=1 权限错 L3）fatal 停机。HVF `-cpu host`（Apple Silicon，
/// ARMv8.1+ PAN 在席）/ TCG `-cpu max` 必现；TCG `-cpu cortex-a72`
/// （ARMv8.0 无 PAN，SPAN RES 1）天然免疫——故冒烟主路（A72）全绿而
/// HVF 面 fatal 的「档间语义差」。写 1 对无 PAN 硬件 = 写 RES1 位，
/// 零漂移；对 PAN 硬件 = 显式回归 v1「内核自由访问用户页」契约。
/// 真正的 PAN 硬化（EL1 访用户面前 `msr pan` 显式开关）留 v2 安全批。
const SCTLR_ENABLE: u64 = (1 << 0) | (1 << 2) | (1 << 12) | (1 << 23);

/// 建表 + 编程系统寄存器 + 开 MMU。
///
/// 2b-1 起由 boot asm 在开 MMU 前后直接 `bl` 调用（顺序固定：先本函数后
/// [`enable_mmu`]）；不再经 HAL `init()`——rust_main 进来时 MMU 已开、在高半。
///
/// # Safety
/// 须在 EL1、MMU 关、启动单线程早期调用一次（由 boot asm 保证）；
/// 本函数与所调 paging 辅助全部落 `.text.boot` / 表落 `.bss.boot`（低半）。
#[no_mangle]
#[link_section = ".text.boot"]
pub unsafe extern "C" fn build_boot_pt() {
    // `&raw mut`：取静态裸指针，避开 `static_mut_refs` 借用告警（G0 单访问点）。
    let l1 = &raw mut BOOT_L1;
    let l2_low = &raw mut BOOT_L2_LOW;
    let l2_ram = &raw mut BOOT_L2_RAM;

    // L1[0] → 低区 L2；L1[1] → RAM L2。
    (&raw mut (*l1).0[0]).write_volatile(table_descriptor(l2_low as usize));
    (&raw mut (*l1).0[1]).write_volatile(table_descriptor(l2_ram as usize));

    // 低区 L2：映 UART、GIC 所在的 2MiB 块为 Device（identity）。
    let dev_blocks = [
        UART0_BASE & !(TWO_MB - 1),
        GICD_BASE & !(TWO_MB - 1),
    ];
    for pa in dev_blocks {
        let idx = (pa >> 21) & L2_INDEX_MASK;
        (&raw mut (*l2_low).0[idx]).write_volatile(block_descriptor(pa, ATTR_DEVICE, SH_NONE));
    }

    // RAM L2：[RAM_BASE, RAM_TOP) 逐 2MiB Normal 块（identity）。
    // BOOT_L2_RAM 由 L1[1] 指向，覆盖 [0x4000_0000, 0x8000_0000)；
    // pa>>21 & 0x1FF 对 [RAM_BASE, RAM_TOP) 恰落在 [0, 64)。
    //
    // K-2：本循环与上方 dev_blocks 循环**必须 volatile 标量写**——release（fat LTO
    // +O3）把全展开后的 64 个编译期常量描述符折叠成 .rodata.cst16 常量池（匿名
    // .LCP 符号，源码钉不住段位），以 adrp+ldr q 从高半 .rodata 装载 + stp q,q
    // 向量化直写 → .text.boot（低半）ADRP ±4GB 越界断链（debug opt=1 保留标量
    // 循环故不断）。volatile 逐写从语义上禁绝向量化/常量池化，值退回 movz/movk
    // 立即数（LLVM AArch64 标量整数不走 literal pool）——与 2b-1 gotcha 同源的
    // boot 路径纪律：.text.boot 不得产生对高半数据的引用。
    // K-3：上方 BOOT_L1[0]/[1] 与下方 KERNEL_L1[0]/[1] 四写同款 volatile 化——
    // stp 合并对写同样是值折叠→cst16 池化的合法 O3 入口形态（折叠为绝对常量
    // 对后可走 ldr q+str q 直写）；至此 build_boot_pt 内同类「编译期常量表填充」
    // 复发向量已全关（配套断言：size.py --assert-flip 钉 .text.boot 全部 ADRP
    // 目标必低半，debug/release 双面执法）。
    let mut pa = RAM_BASE;
    while pa < RAM_TOP {
        let idx = (pa >> 21) & L2_INDEX_MASK;
        (&raw mut (*l2_ram).0[idx]).write_volatile(block_descriptor(pa, ATTR_NORMAL, SH_INNER));
        pa += TWO_MB;
    }

    // 2b-0：TTBR1 内核表——高半 KVA 与低半 PA 走同一对 L2（见 KERNEL_L1 文档）。
    let k_l1 = &raw mut KERNEL_L1;
    (&raw mut (*k_l1).0[0]).write_volatile(table_descriptor(l2_low as usize));
    (&raw mut (*k_l1).0[1]).write_volatile(table_descriptor(l2_ram as usize));

    // 表写入对页表遍历可见。
    dsb_ish();
}

#[no_mangle]
#[link_section = ".text.boot"]
unsafe extern "C" fn enable_mmu() {
    write_mair_el1(MAIR_EL1);
    write_tcr_el1(TCR_EL1);
    // TTBR0 = 身份表（临时，2b-2 退役）；TTBR1 = 高半内核表（2b-0 起）。
    // 表在 .bss.boot 低半（VMA==PA），地址即 PA——2b-2 后新增表才需 virt_to_phys。
    write_ttbr0_el1((&raw const BOOT_L1) as u64);
    write_ttbr1_el1((&raw const KERNEL_L1) as u64);
    isb();
    // TTBR / 表写入全部落定后再开 MMU。
    dsb_ish();
    // 切换点：M|C|I 一齐开。本函数在 .text.boot（VA==PA，身份 TTBR0 已映），
    // 开启后下一条取指无缝；跳高半由 boot asm 随后完成（换 __stack_top 后 br）。
    write_sctlr_el1(SCTLR_ENABLE);
    isb();
}

/// 高半自校验（2b-0 引入；2b-2 起**只走高半**——低半身份已退役，PA 别名侧探针
/// 移交 ktest 的期望故障正向断言）：
/// - RAM 探针 = 静态魔数经高半直映读回并比对；
/// - 设备探针 = GICD_PIDR2（只读外设 ID 寄存器）经高半 Device 块读回，比对
///   GICv2 架构修订号——不只证「翻译不 fault」，还证读到的确是该外设。
/// 翻译坏了通常根本读不回（同步异常挂死）；值比对再兜一层「读回值正确」。
pub(crate) fn kva_alias_probe() -> bool {
    static PROBE_MAGIC: u64 = 0x0B05_2B0B_5A5C_0FF0;
    // 主体符号已链高半：本体 VMA 在高半直映区。
    let va = (&raw const PROBE_MAGIC) as usize;
    // SAFETY: 只读自身静态量（Normal 直映），读无副作用。
    let ram_ok = unsafe { (va as *const u64).read_volatile() } == PROBE_MAGIC;
    const GICD_PIDR2: usize = GICD_BASE + 0x0FE8;
    // SAFETY: 只读外设 ID 寄存器（高半 Device 块；翻译坏则异常挂死，即失败信号）。
    let pidr2 = unsafe { crate::mmio::read(KERNEL_VA_BASE + GICD_PIDR2) };
    // PIDR2 bits[7:4] = 架构修订号，GICv2 = 0b0010（ARM IHI 0048）。
    let gic_ok = (pidr2 >> 4) & 0xf == 0b0010;
    ram_ok && gic_ok
}

// ---- 2b-2：ASID + 每任务 TTBR0 激活 + 低半退役（内存管理设计 §5.1/§5.2）----

/// 探测 ASID 位宽：`ID_AA64MMFR0_EL1.ASIDBits`（bits[7:4]）——0 → 8 位、2 → 16 位；
/// 其余为架构保留值，保守取 8。QEMU `virt`/cortex-a72 实测值由 ktest 打进关卡记录。
pub(crate) fn asid_bits() -> u32 {
    let v: u64;
    // SAFETY: 只读系统寄存器。
    unsafe { asm!("mrs {0}, id_aa64mmfr0_el1", out(reg) v, options(nostack, preserves_flags)) };
    match (v >> 4) & 0xf {
        2 => 16,
        _ => 8, // 0 → 8 位；保留值保守取 8
    }
}

/// ASID 在 TTBR0 / `tlbi aside1` 操作数里的移位量：16 位 → [63:48]、8 位 → [63:56]
/// （TCR.A1=0：硬件 ASID 取自 TTBR0，与 TCR.AS=0 的 8 位取位一致）。
#[inline(always)]
fn asid_shift() -> u32 {
    if asid_bits() == 16 { 48 } else { 56 }
}

/// 按 ASID 失效（`tlbi aside1is`；IS 口径——单核下与非 IS 等价，多核语义先立对）。
/// Vmar 的 unmap/protect 改叶后调用（2b-3 起独立出口，激活路径复用）。
pub(crate) unsafe fn tlbi_aside1(asid: u16) {
    let shift = asid_shift();
    asm!("tlbi aside1is, {0}", in(reg) (asid as u64) << shift, options(nostack, preserves_flags));
    dsb_ish();
    isb();
}

/// TTBR0 ASID 位域环回探针（2026-11-15 北极星审计修第三刀；boot 高半单线程
/// 窗调一次，`rust_main` 早期）。
///
/// 干什么：保 TTBR0 表基不动、只把 ASID 位域换成探针值（1）→ isb → 读回。
/// 位域丢（≠ 探针值）= 虚拟化层自管 guest ASID、寄存器读面不回真值（QEMU
/// HVF `-accel hvf` 实测：写 `1<<56|base` 读回 `0|base`）→ 按号 TLB 语义
/// （`tlbi aside1` + P0-3 免失效激活档的「同号项只来自同号激活史」前提）
/// 不可依托，置 [`ASID_TLB_UNTRUSTED`]。
///
/// 为什么这是**安全可探测面**：表基不变 ⇒ 本探针不改变任何翻译结果，只动
/// TLB 标记（换号 → 旧号项天然失配、按需重填，无正确性面）；真机/TCG/KVM
/// 位域忠实环回（TCR.A1=0 下 TTBR0.ASID 就是架构 ASID 源）→ 探针零命中、
/// 免失效档照走，行为零漂移。
///
/// # Safety
/// EL1、MMU 已开、boot 单线程 DAIF 屏蔽窗（调用点 `rust_main` 早期，先于
/// 任何任务地址空间激活）；本函数自恢复原 TTBR0 值。
pub unsafe fn probe_asid_field_roundtrip() {
    let saved: u64;
    // SAFETY: 特权读；EL1，无副作用。
    unsafe { asm!("mrs {0}, ttbr0_el1", out(reg) saved, options(nostack, preserves_flags)) };
    let shift = asid_shift();
    let probe_ttbr = (saved & !(0xFFFF_u64 << shift)) | (1u64 << shift);
    // SAFETY: 特权写；表基不变，仅换 ASID 标记（见函数注）。
    unsafe { write_ttbr0_el1(probe_ttbr) };
    // SAFETY: 特权读；isb 后必见新值（或虚拟化层的丢弃形）。
    let back: u64;
    unsafe {
        isb();
        asm!("mrs {0}, ttbr0_el1", out(reg) back, options(nostack, preserves_flags));
    }
    // SAFETY: 自恢复；特权写。
    unsafe { write_ttbr0_el1(saved) };
    // SAFETY: 特权屏障；恢复后指令流必见原值。
    unsafe { isb() };
    if (back >> shift) & 0xFFFF != 1 {
        ASID_TLB_UNTRUSTED.store(true, Ordering::Relaxed);
    }
}

/// [`ASID_TLB_UNTRUSTED`] 只读透传（可观测性——关卡行/诊断面用）。
pub fn asid_tlb_untrusted() -> bool {
    ASID_TLB_UNTRUSTED.load(Ordering::Relaxed)
}

/// 激活任务地址空间：TTBR0 ← `ttbr0_pa` + ASID（`ttbr0_pa == 0` = 无 AS → 内核
/// 空表，低半必然 fault）；CONTEXTIDR.PROCID = asid（可观测性——硬件 ASID 恒取
/// TTBR0，A1=0）；`tlbi aside1is` 防上一轮同号映射经 TLB 残留（§5.1 坑「TLB」）。
///
/// 只做寄存器操作，**不负责页表内容屏障**——填表方须在调用前 `dsb ish`
/// （见内核 `aspace` 模块）。
pub(crate) unsafe fn activate_ttbr0(ttbr0_pa: usize, asid: u16) {
    activate_ttbr0_regs(ttbr0_pa, asid);
    // 同号旧映射失效。
    tlbi_aside1(asid);
}

/// 激活的**寄存器面**（P0-3 抽出件）：TTBR0 ← 表+ASID + CONTEXTIDR.PROCID，
/// **不做任何 TLB 失效**——与 [`activate_ttbr0`] 共用，另经
/// `activate_ttbr0_keep_tlb` 对调度器开放（激活代数消费的免失效档）。
unsafe fn activate_ttbr0_regs(ttbr0_pa: usize, asid: u16) {
    let table = if ttbr0_pa == 0 {
        (&raw const NULL_L1) as usize - KERNEL_VA_BASE
    } else {
        ttbr0_pa
    };
    let shift = asid_shift();
    let ttbr = (asid as u64) << shift | table as u64;
    write_ttbr0_el1(ttbr);
    isb();
    // CONTEXTIDR_EL1：PROCID = asid（调试/trace 关联用；A1=0 下非功能位）。
    asm!(
        "msr contextidr_el1, {0}",
        in(reg) (asid as u64) << 13,
        options(nostack, preserves_flags)
    );
}

/// 激活任务地址空间（**免失效档**）：寄存器面同 [`activate_ttbr0`]，但跳过
/// `tlbi aside1is`+`dsb ish`。调用方合同（P0-3 激活代数消费律）：本核上次
/// 激活该 ASID 以来**回卷代数未变**——该号在本核 TLB 里的项全为有效项
///（改叶路径自带按号失效 + 广播；同号复用只经回卷，回卷路径自带全量失效
/// 且使代数前进）。违约 = 同号旧映射可能残留（§5.1 坑「TLB」）。
///
/// **虚化降级**（2026-11-15 北极星审计修第三刀）：[`ASID_TLB_UNTRUSTED`]
/// 置位（TTBR0 ASID 位域读回不环回——QEMU HVF 形虚拟化）时**整档退化为
/// [`activate_ttbr0` 全量档**。免失效档的正确性前提 = 「按号失效 + 按号
/// 标记的 TLB 语义」成立（同号项只可能来自同号激活史）；HVF 把 guest 的
/// TTBR0.ASID 自管（读回丢弃位域——boot 探针实测），TLB 标记面不随 guest
/// ASID 走 ⇒ 跨表切换不失效 = 旧表项对新表别名（实测形态：TAGTAB 后 pager
/// 取指 0x10c64 / svcmgr `copy_to_user` 写栈页，均 L3 翻译错——SW 走查叶
/// 在、HW 走查叶无，HW/SW 走查面分裂）。全量档多付一次 `tlbi aside1is`，
/// 正确性优先；TCG/KVM/真机（位域环回）零漂移照走免失效档。
pub(crate) unsafe fn activate_ttbr0_keep_tlb(ttbr0_pa: usize, asid: u16) {
    if ASID_TLB_UNTRUSTED.load(Ordering::Relaxed) {
        activate_ttbr0(ttbr0_pa, asid);
        return;
    }
    activate_ttbr0_regs(ttbr0_pa, asid);
}

/// 读当前 TTBR0_EL1 的表基 PA（bits[47:12]，ASID 位剔除；无 AS 时 = 内核空表
/// PA）。3-2：syscall copy 路径逐页走查**调用者自身表**用——EL0 svc 时异常
/// 不切 AS，TTBR0 仍是调用者页表。
pub(crate) fn current_ttbr0_pa() -> usize {
    let ttbr: u64;
    // SAFETY: 特权读；EL1，无副作用。
    unsafe { asm!("mrs {0}, ttbr0_el1", out(reg) ttbr, options(nostack, preserves_flags)) };
    ttbr as usize & 0x0000_FFFF_FFFF_F000
}

/// 读当前 TTBR1_EL1 内核表基 PA（bits[47:12]；W3-Q104 freeze_syscalls_table
/// walk 用——高半 .text 不得静态引用 `.bss.boot` 低半表符号，见该函数内注释）。
pub(crate) fn current_ttbr1_pa() -> usize {
    let ttbr: u64;
    // SAFETY: 特权读；EL1，无副作用。
    unsafe { asm!("mrs {0}, ttbr1_el1", out(reg) ttbr, options(nostack, preserves_flags)) };
    ttbr as usize & 0x0000_FFFF_FFFF_F000
}

/// S-4 4-2：当前 ASID——syscall copy 链读者解析 per-AS PT 锁的键；demand
/// 探针（`dpg::probe`）asid 反查同源。与 [`current_ttbr0_pa`] 同一激活窗
/// （EL0 syscall 不切 AS，天然 = 调用者 ASID，同窗一致键）。
///
/// **读点 = CONTEXTIDR_EL1（软件镜像），不读 TTBR0 位域**（2026-11-15 北极
/// 星审计修第二刀）：TTBR0 读回取位在两档加速器上都不可靠——①TCG/HVF
/// 8 位 ASID 机（HVF `-cpu host` 实测 `ID_AA64MMFR0.ASIDBits=0`）上 ASID
/// 落 [63:56]，`>>48` 读出的是 `asid<<8`（错键）；②HVF 对 TTBR0_EL1 读回
/// **丢弃 ASID 位域**（实测写 `1<<56|table` 读回 `0|table`——Hypervisor 自管
/// shadow TLB 的 ASID，寄存器读面不回真值）。CONTEXTIDR 是激活面
/// [`activate_ttbr0_regs`] 每次同窗写入的软件镜像（`asid<<13` 编码，
/// 8 位号 ≤255 时 `<<13` 恒落 ASID 字段 [31:8] 内不溢出），两档均忠实回读
/// （HVF 实测 0x2000 = 1<<13 ✓）。per-CPU 寄存器 = SMP 各核各自回读各自
/// 激活值，无共享静态镜像的跨核串键面。
pub(crate) fn current_asid() -> u16 {
    let cid: u64;
    // SAFETY: 特权读；EL1，无副作用。
    unsafe { asm!("mrs {0}, contextidr_el1", out(reg) cid, options(nostack, preserves_flags)) };
    (cid >> 13) as u16
}

/// 低半身份表退役（2b-2）：TTBR0 ← 内核空表（ASID 0）+ **全量 TLB 失效**。
/// 此后低半任何访问必然 fault——ktest 以期望故障正向断言之。
/// 用 `vmalle1is`：身份时代的低半 TLB 项是 global（boot 块描述符 nG=0），按号
/// 失效够不着 global 项；全量清（高半内核项随后经 TTBR1 重走查，无害）。
/// 回退开关：[`RETIRE_IDENTITY`]。
pub(crate) unsafe fn retire_low_half() {
    if !RETIRE_IDENTITY {
        return;
    }
    activate_ttbr0(0, 0);
    flush_all_asid();
}

/// 全量失效（ASID 分配器回卷 / 低半退役用）。注意：正确的「全量」指令是
/// `vmalle1is`（含 global 项；`aside1` 家族只按号失效、够不着 global）。
/// 设计文档 §5.2 原文写的 `tlbi aside1all` 并非真实助记符，此处即更正。
pub(crate) unsafe fn flush_all_asid() {
    asm!("tlbi vmalle1is", options(nostack, preserves_flags));
    dsb_ish();
    isb();
}

/// S-4 收端本地全量失效（SGI #2 handler 专用，tlbshoot::irq_ack 调）：非 IS
/// `vmalle1`——发端 SGI 已按核定向（Others 排自核），收端只清自己的 TLB、
/// 不再广播。含 global 项（`aside1` 家族按号失效够不着 global）。`dsb ish`
/// 保证失效完成先于回执计数对发端可观（回执序的排序边）；随后 eret 回中断
/// 前无需 isb——TLB 已清 + dsb 收口，走查自然取新项。v1 收端粒度 = 全量
/// （无请求槽）；按号精确（请求槽 + 分代）= v2 优化。
pub(crate) unsafe fn flush_local_all() {
    asm!("tlbi vmalle1", options(nostack, preserves_flags));
    dsb_ish();
}

// ---- W3-Q104（03 号分派审计机会五 5b）：SYSCALLS 派发表固化 RO ----

/// 固化完成标记（ktest 自证用；boot 窗内置位后恒 `true`）。
static SYSCALLS_RO: AtomicBool = AtomicBool::new(false);

/// SYSCALLS 表是否已固化只读（ktest 断言门自证用）。
pub fn syscalls_table_ro() -> bool {
    SYSCALLS_RO.load(Ordering::Relaxed)
}

/// 把 `SYSCALLS` 派发表所在 2MiB 块拆成 4KiB 叶、仅该表页（含防跨界的
/// 邻伴页）置 **EL1 只读**，其余 510 页逐位保真原块 RW——函数指针表
/// W^X 固化（`research/内核深度进化/03-syscall分派审计.md` 机会五 5b：
/// boot 期 [`crate::syscall::install`] 填毕、终生只读却留可写页的分派面
/// 关死；此后任何写 = D-abort(current-EL) 权限错 fatal——ktest 期望
/// 故障探针正向断言之）。
///
/// 为什么必须拆块：高半内核映射是 L2 2MiB **块描述符**（`build_boot_pt`），
/// AP 位长在块上——直接改块权限 = 整个 2MiB（`.data`/`.bss` 同块全体静态
/// 量）一起失写，boot 必死。故插一张 L3 把该块劈成 512 张 4KiB 叶。
///
/// 时序合同：**只能在高半经 TTBR1 活跃时调**（`rust_main` 单线程 DAIF 屏
/// 蔽尾窗、smp_bringup 前）——本函数对该块内代码/数据的取指与数据访问
/// 走高半映射；低半身份已在 2b-2 退役（`retire_low_half` 在 boot 早期），
/// 不存在低半走查面。副核 `bl enable_mmu` 消费表基只读，walk 到的即新
/// 拓扑（表项写毕经 `dsb ish` 才切 L2 项），无共享窗。
///
/// # Safety
/// boot 单线程调用一次；只写页表项（RAM L2 目标槽 + `SPLIT_L3_SYSCALLS`
/// 专表，高半 .bss）与 `SYSCALLS_RO` 标记，不触碰任何运行期数据。
pub unsafe fn freeze_syscalls_table() {
    // 镜像布局：VMA - KERNEL_VA_BASE == LMA（linker 脚本）。
    let sva = crate::syscall::table_addr();
    debug_assert_eq!(sva & 0xFFF, 0, "SYSCALLS 须 4KiB 对齐");
    let spa = sva - KERNEL_VA_BASE;
    let l2_idx = (spa >> 21) & L2_INDEX_MASK;

    // RAM L2 表基：**运行期从 TTBR1_EL1 walk**（L1[1] → 下级表基 PA），不从
    // `BOOT_L2_RAM` 符号直取——该表落 `.bss.boot` 低半（VMA==PA），高半
    // .text 的静态引用必发 ADRP±4GiB 断链（R490 判例：rust-lld
    // `R_AARCH64_ADR_PREL_PG_HI21 out of range`，CI-light 不编目标机的双盲
    // 面）。KERNEL_L1[1] 与 BOOT_L1[1] 同指该表（build_boot_pt），TTBR1 写入
    // 值即其 PA——walk 零静态引用、零额外真值源。walk 读经高半直映别名。
    let l1_pa = current_ttbr1_pa();
    let l1 = (l1_pa + KERNEL_VA_BASE) as *const u64;
    let l2_desc = unsafe { (&raw const *l1.add(1)).read_volatile() };
    debug_assert_eq!(l2_desc & 0x3, 0b11, "L1[1] 须是有效表描述符");
    let l2 = (((l2_desc & 0x0000_FFFF_FFFF_F000) as usize) + KERNEL_VA_BASE) as *mut L2Table;

    // 分裂专表（高半普通 .bss——boot asm 开 MMU 前按 PA 清零主体 .bss，
    // 调用时已零；挂表时 VMA−KERNEL_VA_BASE 折算 PA。**不得**学四张 boot
    // 表落 .bss.boot 低半：高半 .text 的 ADRP 引用断链〔R490 链接红根因〕）。
    static mut SPLIT_L3_SYSCALLS: L3Table = L3Table([0; PTES_PER_TABLE]);
    let l3 = &raw mut SPLIT_L3_SYSCALLS;

    // 旧块 PA（清 bits[20:0] 属性位；MMU 开前建表后表项稳定，volatile 防折叠）。
    let old_block = unsafe { (&raw const (*l2).0[l2_idx]).read_volatile() };
    debug_assert_eq!(old_block & 0x3, 0b01, "拆分目标须是有效块描述符");
    let block_pa = (old_block & !0x1F_FFFF) as usize;

    // 512 叶：表页与前后邻页 RO（表 4KiB 恰容 256 槽×16B，但链接漂移可能
    // 使邻页承载表尾/表头——邻伴一并固化，代价 = 邻页静态量失写；邻页在
    // .bss/.data 尾带（SYSCALLS 是 align(4096) 大静态，邻居全是同类 boot
    // 期写毕即弃面，实测无运行期写者，ktest 冻结后全量 boot 绿即执法）。
    let tgt_pf = spa >> 12;
    for i in 0..PTES_PER_TABLE {
        let pf = (block_pa >> 12) + i;
        let ro = pf == tgt_pf || pf + 1 == tgt_pf || pf == tgt_pf + 1;
        let ap = if ro { AP_RO } else { AP_EL1_RW };
        unsafe {
            (&raw mut (*l3).0[i]).write_volatile(page_descriptor_kernel(pf << 12, ap));
        }
    }

    // 标记先置后翻（翻转前单线程窗零观察者）：若 SYSCALLS_RO 恰链进 RO
    // 三页窗，翻转后自写必 D-abort fatal——置位必须在权限翻转**前**完成。
    SYSCALLS_RO.store(true, Ordering::Release);

    // 先 L3 后 L2（walk 见表项时叶已就绪——无窗口），再屏障 + 全量 TLB
    // 失效（旧块项是 global，须 vmalle1is 家族）。
    unsafe { dsb_ish() };
    unsafe { (&raw mut (*l2).0[l2_idx]).write_volatile(table_descriptor(l3 as usize - KERNEL_VA_BASE)) };
    unsafe { dsb_ish() };
    unsafe { flush_all_asid() };
}

#[inline(always)]
unsafe fn write_mair_el1(v: u64) {
    asm!("msr mair_el1, {0}", in(reg) v, options(nostack, preserves_flags));
}
#[inline(always)]
unsafe fn write_tcr_el1(v: u64) {
    asm!("msr tcr_el1, {0}", in(reg) v, options(nostack, preserves_flags));
}
#[inline(always)]
unsafe fn write_ttbr0_el1(v: u64) {
    asm!("msr ttbr0_el1, {0}", in(reg) v, options(nostack, preserves_flags));
}
#[inline(always)]
unsafe fn write_ttbr1_el1(v: u64) {
    asm!("msr ttbr1_el1, {0}", in(reg) v, options(nostack, preserves_flags));
}
#[inline(always)]
unsafe fn write_sctlr_el1(v: u64) {
    asm!("msr sctlr_el1, {0}", in(reg) v, options(nostack));
}
#[inline(always)]
unsafe fn isb() {
    asm!("isb", options(nostack, preserves_flags));
}
#[inline(always)]
unsafe fn dsb_ish() {
    asm!("dsb ish", options(nostack, preserves_flags));
}
