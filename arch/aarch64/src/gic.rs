//! GICv2 中断控制器驱动（QEMU `virt` 默认 GICv2）。
//!
//! 内存图（QEMU `hw/arm/virt.c`）：GICD（distributor）@`0x0800_0000`、
//! GICC（CPU interface）@`0x0801_0000`。Phase 1 单核：仅配 **IRQ27 = CNTV 虚拟
//! 定时器**（PPI；SGI=0..15、PPI=16..31、SPI=32+）。
//!
//! **GICD 序**（plan Step 7）：`CTLR=0` → 禁/清 pending/去活 ID 0..31
//! （`ICENABLER0`/`ICPENDR0`/`ICACTIVER0`）→ 设 IRQ27 优先级/target/触发 →
//! `ISENABLER0 bit27` → `CTLR=1`。**GICC**：`PMR=0xFF` → `CTLR=1`。
//! DAIF 仍屏蔽——Step 8 才 `daifclr #2`。
//!
//! **IAR-before-EOIR 不变式**：handler 先读 [`acknowledge`]（取 IRQ ID、ack），
//! 处理完写 [`end`]（同 ID）。漏 EOIR → GIC 卡死不再投递（Step 8 在 timer handler 落地）。
//!
//! **触发方式**：ARM 架构定时器（generic timer）内部为电平触发，故 ICFGR 用
//! **level（0b01）**——非 plan 草记的 edge（0b10）。edge 下若重武装时电平仍高会漏周期
//! tick；level 配合「先重武装 timer、再 EOIR」最稳。QEMU ICFGR 对 PPI 可能强制 level，
//! 写 level 与读回一致。

use crate::mmio;
use crate::{GICD_BASE, GICC_BASE, KERNEL_VA_BASE};

/// MMIO 走高半直映别名（2b-2 低半退役：低半身份不再可用）。物理基址常量留在
/// `lib.rs` 供 boot 建表（表内容须 PA）。
const GICD: usize = KERNEL_VA_BASE + GICD_BASE;
const GICC: usize = KERNEL_VA_BASE + GICC_BASE;

/// IRQ 编号：CNTV 虚拟定时器（PPI 11，GICv2 ID 27）。
pub const IRQ_CNTV: usize = 27;
/// IRQ 编号：PL011 UART0 RX（SPI 1 + 16 SGI + 16 PPI，P5 A-1）。QEMU `virt`
/// 接线活证双源：zCore 同板 `irq_enable(33)` + QEMU 自产 DTB `LEVEL_HIGH`。
/// 若接线有误，`exception_irq` 未知源分支 loud 打 `irq? id=N`（bring-up 面）。
pub const IRQ_PL011_RX: usize = 33;
/// IRQ 编号：第二 PL011 UART1 RX（SPI 8 + 16 SGI + 16 PPI，sim-S1）。QEMU
/// `virt` 双 `-serial` 配置 DTB 实证（QEMU 11.0.3 dumpdtb：`interrupts =
/// <0x00 0x08 0x04>` = SPI 8/LEVEL_HIGH；单串口配置设备不存在——在席门见
/// [`crate::UART1_BASE`] 注，fw_cfg `opt/berry/simdev`）。
pub const IRQ_PL1_RX: usize = 40;
/// IRQ 编号：PL061 GPIO（SPI 7 + 16 SGI + 16 PPI，sim-S2 A4②）。QEMU `virt`
/// DTB 实证：`pl061@9030000` `interrupts = <0x00 0x07 0x04>` = SPI 7/LEVEL_HIGH
///（GPIO3 = 电源键——QMP `system_powerdown` 脉冲）。电平去活 = PL061 ICR
/// 写 1 清边沿锁存（handler pre-EOI 铁序，`pl061.rs` 模块头）。
pub const IRQ_PL061: usize = 39;
/// SGI 号分配（S-3 §2.2）：1 = `SGI_RESCHEDULE`（跨核唤醒戳）；2 = TLB
/// shootdown 回执（S-4 4-1 现役——收端 = trap.rs SGI_TLB 臂 + tlbshoot 模块）；
/// **0 保留不用**（复位默认值域，观测面干净性）。
pub const SGI_RESCHEDULE: u8 = 1;
pub const SGI_TLB_SHOOTDOWN: u8 = 2;
/// IAR 读到的「无中断」值。
pub const SPURIOUS: u32 = 1023;

// ---- GICD 寄存器偏移 ----
const GICD_CTLR: usize = 0x000;
const GICD_ISENABLER0: usize = 0x100;
const GICD_ICENABLER0: usize = 0x180;
const GICD_ISPENDR0: usize = 0x200;
const GICD_ICPENDR0: usize = 0x280;
const GICD_ICACTIVER0: usize = 0x380;
/// 每 IRQ 一字节。
const GICD_IPRIORITYR: usize = 0x400;
/// 每 IRQ 一字节（PPI/SGI 段只读、固定指向当前 CPU；**SPI 段可写且复位 0**）。
const GICD_ITARGETSR: usize = 0x800;
/// 2-bit 触发配置寄存器组基址（每 16 IRQ 一个寄存器：ICFGR0=SGI 0..15、
/// ICFGR1=PPI 16..31、ICFGR2=SPI 32..47…）。
const GICD_ICFGR: usize = 0xc00;
/// SGI 生成寄存器（S-3a）：**仅受理 32-bit 整字写**——QEMU 对子宽落
/// bad_reg 静默丢弃（宽度敏感铁律 A6 同族）。
const GICD_SGIR: usize = 0xf00;

// ---- GICC 寄存器偏移 ----
const GICC_CTLR: usize = 0x000;
const GICC_PMR: usize = 0x004;
const GICC_IAR: usize = 0x00c;
const GICC_EOIR: usize = 0x010;

/// IRQ27 优先级（0=最高）。PMR=0xFF 全放行，故任意值通过；取 0xa0 避开 secure 预留口径。
const PRIO_CNTV: u8 = 0xa0;

/// 初始化 GICv2：distributor + CPU interface，启用 IRQ27。
///
/// **主核（CPU0）boot 期一次、仅此一处**——S-1 起副核**绝不**调用本函数：
/// GICD 是全局单份，重跑的 `GICD_CTLR=0` 会清光 CPU0 已配的中断（结构性
/// 灾难）；副核 GICC 面由 secondary_entry asm 内联配置（S-1）、CNTV 使能
/// 面经 [`enable_cntv_this_core`]（S-2b）。
///
/// # Safety
/// MMIO 写（Device 映射区）；EL1、启动早期、单核。
pub unsafe fn init() {
    // ---- distributor：先关，再清状态 ----
    mmio::write(GICD + GICD_CTLR, 0);
    mmio::write(GICD + GICD_ICENABLER0, !0); // 禁用 ID 0..31
    mmio::write(GICD + GICD_ICPENDR0, !0); // 清 pending
    mmio::write(GICD + GICD_ICACTIVER0, !0); // 去活

    // IRQ27 配置。
    set_priority(IRQ_CNTV, PRIO_CNTV);
    // PPI target 段只读（固定指向当前 CPU=CPU0）；写为 no-op 但无害。
    set_target(IRQ_CNTV, 0x01);
    // 触发：level（见模块注释）。
    set_config(IRQ_CNTV, 0b01);

    // 启用 IRQ27 + 开 distributor。
    mmio::write(GICD + GICD_ISENABLER0, 1 << IRQ_CNTV);
    // SGI_RESCHEDULE(1) 本核（CPU0）banked 使能（S-3a）：上方 `ICENABLER0=!0`
    // 把 ID 0..31 全段禁了（含 SGI 0..15），收端必须逐核补——CPU0 此处、
    // 副核 secondary_main 起手（[`enable_sgi_this_core`]，banked 纪律同款）。
    // 单核相零漂移：只使能无发送者，面结构性不触发。
    mmio::write(GICD + GICD_ISENABLER0, 1 << SGI_RESCHEDULE);
    // S-4 4-1：SGI_TLB_SHOOTDOWN(2) 本核（CPU0）banked 使能——收端必须逐核
    // 补（副核 secondary_main 起手同款）；单核相零漂移同上：只使能无发送者，
    // 面结构性不触发（发送计数 tlbshoot::TX 单核恒 0 = 等价镜实证项）。
    mmio::write(GICD + GICD_ISENABLER0, 1 << SGI_TLB_SHOOTDOWN);
    mmio::write(GICD + GICD_CTLR, 1);

    // ---- CPU interface：全优先级放行 + 开 ----
    mmio::write(GICC + GICC_PMR, 0xff);
    mmio::write(GICC + GICC_CTLR, 1);
}

/// 读回 IRQ27 配置自检（Step 7 信号）。
pub fn verify() -> bool {
    unsafe {
        let en = mmio::read(GICD + GICD_ISENABLER0) & (1 << IRQ_CNTV) != 0;
        let dist = mmio::read(GICD + GICD_CTLR) & 1 != 0;
        let cpuif = mmio::read(GICC + GICC_CTLR) & 1 != 0;
        let pmr = mmio::read(GICC + GICC_PMR);
        let prio = mmio::read_u8(GICD + GICD_IPRIORITYR + IRQ_CNTV);
        en && dist && cpuif && pmr == 0xff && prio == PRIO_CNTV
    }
}

/// 读 IAR 应答并取 IRQ ID（1023=无/spurious）。
#[allow(dead_code)]
pub fn acknowledge() -> u32 {
    unsafe { mmio::read(GICC + GICC_IAR) & 0x3ff }
}

/// 写 EOIR 结束（必须传 [`acknowledge`] 读到的同一 ID）。
#[allow(dead_code)]
pub fn end(irq_id: u32) {
    unsafe { mmio::write(GICC + GICC_EOIR, irq_id & 0x3ff) };
}

// ---- 单 IRQ 配置原语 ----

unsafe fn set_priority(irq: usize, prio: u8) {
    mmio::write_u8(GICD + GICD_IPRIORITYR + irq, prio);
}

unsafe fn set_target(irq: usize, target: u8) {
    mmio::write_u8(GICD + GICD_ITARGETSR + irq, target);
}

/// `cfg`：0b01=level、0b10=edge（见模块注释，定时器/PL011 均电平）。ICFGR 组
/// 寻址按 `irq/16` 泛化（P5 A-1 前硬编码 PPI 段；SPI 33 → 33/16=2 落 ICFGR2、
/// 33%16=1 即 **bits[3:2]**（bits[1:0] 是 IRQ 32 的）。irq=27 时 idx=1/shift=22，
/// 与旧硬编码恒等）。
unsafe fn set_config(irq: usize, cfg: u32) {
    let shift = 2 * (irq % 16);
    let off = GICD + GICD_ICFGR + 4 * (irq / 16);
    let mut v = mmio::read(off);
    v &= !(0b11u32 << shift);
    v |= (cfg & 0b11) << shift;
    mmio::write(off, v);
}

/// 使能一个 SPI（≥32；P5 A-1：PL011 RX=33，GICv2 第二个 IRQ 源）。
/// distributor/CPU interface 已由 [`init`] 开启，本函数只配该线：优先级 /
/// target / 触发 + **清历史 pending**（GIC 对 disable 的线照样锁存 pending——
/// boot 前 UART 线若有粘滞先擦掉，防使能即吃旧中断）+ ISENABLER。
///
/// **SPI 段 ITARGETSR 可写且复位 0（= 不投任何核）**——与 PPI 段只读不同，
/// target 必须真写：漏写 = 中断永远不上 CPU 且无任何报错（静默丢）。
///
/// # Safety
/// MMIO 写；boot 单线程、DAIF 屏蔽窗内调用（使能后首中断须 handler 已就绪）。
pub unsafe fn enable_spi(irq: usize, prio: u8, target: u8, cfg: u32) {
    set_priority(irq, prio);
    set_target(irq, target);
    set_config(irq, cfg);
    mmio::write(GICD + GICD_ICPENDR0 + 4 * (irq / 32), 1 << (irq % 32));
    mmio::write(GICD + GICD_ISENABLER0 + 4 * (irq / 32), 1 << (irq % 32));
}

/// 副核使能本核 CNTV（PPI27，S-2b）：INTID<32 的 ISENABLER0/IPRIORITYR 是
/// **per-CPU banked**——写 GICD 偏移落的是**调用核自己的 bank**，与 CPU0
/// init 配置互不可见、互不干扰；ICFGR 不 banked（CPU0 已配 level，免配）；
/// GICD_CTLR/SPI 面一概不碰（S-1 纪律：副核绝不动全局面）。GICC CTLR/PMR
/// 已由 secondary_entry asm 配好（S-1 §3.2 ⑥）。本核 CVAL 由 secondary_main
/// 循环的 arm_slice 武装。
///
/// # Safety
/// MMIO 写；副核 DAIF 屏蔽窗内调用（secondary_main 起手，使能后首中断须
/// 探测环已就位）。
pub unsafe fn enable_cntv_this_core() {
    set_priority(IRQ_CNTV, PRIO_CNTV);
    mmio::write(GICD + GICD_ICPENDR0, 1 << IRQ_CNTV); // 擦历史 pending
    mmio::write(GICD + GICD_ISENABLER0, 1 << IRQ_CNTV);
}

// ---- S-3a：SGI IPI 面（`docs/SMP-S3实施设计草案.md` §2）----

/// SGI 目标形态（GICD_SGIR TargetListFilter[25:24] 直映）。
pub enum SgiTarget {
    /// 0b00：按核位图（affinity 0 簇内每核 1 bit，bit0=CPU0）。
    List(u8),
    /// 0b01：除发送核外全体。
    Others,
    /// 0b10：仅发送核自身。
    Self_,
}

/// 发 SGI（32-bit 整字写，见 [`GICD_SGIR`] 宽度注）。字段**实证钉板 =
/// QEMU `hw/intc/arm_gic.c` `gic_dist_writel`（2026-11-01 源码核对）**：
/// SGIINTID=`value & 0xF`、CPUTargetList=bits[23:16]、TargetListFilter=
/// bits[25:24]——不照规范想象写死（S-0 实证纪律）。收端 pending 在其
/// bank 锁存、**使能前不投递**——先发后使能不丢（S-3a probe 实证面）。
///
/// # Safety
/// MMIO 写；任意上下文可调（无锁、零分配、零打印——IRQ 上下文纪律合规；
/// 不动 distributor 全局面，仅本寄存器写即投递）。
pub unsafe fn send_sgi(id: u8, target: SgiTarget) {
    let (filter, list) = match target {
        SgiTarget::List(m) => (0u32, (m as u32) << 16),
        SgiTarget::Others => (1u32 << 24, 0),
        SgiTarget::Self_ => (2u32 << 24, 0),
    };
    mmio::write(GICD + GICD_SGIR, filter | list | (id as u32 & 0xf));
}

/// 本核使能一个 SGI（S-3a）：INTID<32 的 ISENABLER0/IPRIORITYR **per-CPU
/// banked**——写 GICD 偏移落的是调用核自己的 bank（[`enable_cntv_this_core`]
/// 同款纪律）。CPU0 的 `gic::init` 已用 `ICENABLER0=!0` 禁过 ID 0..31（每核
/// 各禁各的 bank；副核 bank 复位态不可假设），收端必须自己补使能 + 擦历史
/// pending。IPRIORITYR 复位 0（最高优先级）免配；GICD_CTLR=1 全局面已开、
/// SGI 路由无需再动。
///
/// # Safety
/// MMIO 写；本核 DAIF 屏蔽窗内调用（收端 handler 就位后再放中断）。
pub unsafe fn enable_sgi_this_core(id: u8) {
    mmio::write(GICD + GICD_ICPENDR0, 1 << (id & 0xf)); // 擦历史 pending
    mmio::write(GICD + GICD_ISENABLER0, 1 << (id & 0xf));
}

/// 读本核 bank 的 SGI/PPI pending 位（S-3a probe ③）：`GICD_ISPENDR0` 对
/// INTID<32 **per-CPU banked**——调用核只见自己 bank 的 pending 面。DAIF
/// 屏蔽窗内发送的 Self SGI 不投递、pending 锁存于此——直读即「锁存后投递」
/// 语义的**发端**自证（收端版 = probe ①「先发后使能不丢」）。
pub fn sgi_pending_this_core(id: usize) -> bool {
    // SAFETY: 只读 MMIO（GICD ISPENDR0 本核 bank），无写面无副作用。
    unsafe { mmio::read(GICD + GICD_ISPENDR0) & (1 << (id & 0xf)) != 0 }
}

// ---- S-1：per-源 SPI 路由表（`SMP设计草案.md` §3.4 裁决）----
// 原先 target 散在调用点硬编码（uart.rs 曾 0x01 三处硬编）——副核时代收敛
// 为常量表：单核钉 CPU0（0x01），S-3 起按核扩。新 SPI 源必须入表（表外源
// = 路由未入册，enable_spi_routed 拒配 loud）。

/// per-源 SPI 路由表：(irq, prio, target, cfg)。console/RX 钉 CPU0——
/// S-1 盒子边界（副核稳态零中断源、零 UART 写）。
pub const SPI_ROUTES: &[(usize, u8, u8, u32)] = &[
    (IRQ_PL011_RX, 0xa0, 0x01, 0b01), // PL011 RX：电平触发（DTB LEVEL_HIGH）
    (IRQ_PL1_RX, 0xa0, 0x01, 0b01),   // PL011#1 RX（sim-S1）：电平（DTB 实证同上）
    (IRQ_PL061, 0xa0, 0x01, 0b01),    // PL061 电源键（sim-S2 A4②）：电平（DTB 实证）
];

/// 经 [`SPI_ROUTES`] 使能一个 SPI：查表命中才动 GICD（表外源返 false——
/// 路由漂移在调用点 loud，不静默散落）。
pub fn enable_spi_routed(irq: usize) -> bool {
    for &(r_irq, r_prio, r_target, r_cfg) in SPI_ROUTES {
        if r_irq == irq {
            // SAFETY: boot 单线程、DAIF 屏蔽窗内调用（与 enable_spi 同契约）。
            unsafe { enable_spi(r_irq, r_prio, r_target, r_cfg) };
            return true;
        }
    }
    false
}

// 副核 GIC 面（§3.2 ⑥）：**只动本核 banked 的 GICC**（PMR=0xff + CTLR=1）
// ——GICD 全局零触碰（重跑 `gic::init` 的 GICD_CTLR=0 会清光 CPU0 中断）。
// 实现在 `smp.rs` 副核 asm（PA 直写，开 MMU 前后读面进报告）；此注为
// 口径锚点：副核对 GIC 的全部写面收敛在 GICC 两寄存器。
