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
/// IAR 读到的「无中断」值。
pub const SPURIOUS: u32 = 1023;

// ---- GICD 寄存器偏移 ----
const GICD_CTLR: usize = 0x000;
const GICD_ISENABLER0: usize = 0x100;
const GICD_ICENABLER0: usize = 0x180;
const GICD_ICPENDR0: usize = 0x280;
const GICD_ICACTIVER0: usize = 0x380;
/// 每 IRQ 一字节。
const GICD_IPRIORITYR: usize = 0x400;
/// 每 IRQ 一字节（PPI/SGI 段只读、固定指向当前 CPU；**SPI 段可写且复位 0**）。
const GICD_ITARGETSR: usize = 0x800;
/// 2-bit 触发配置寄存器组基址（每 16 IRQ 一个寄存器：ICFGR0=SGI 0..15、
/// ICFGR1=PPI 16..31、ICFGR2=SPI 32..47…）。
const GICD_ICFGR: usize = 0xc00;

// ---- GICC 寄存器偏移 ----
const GICC_CTLR: usize = 0x000;
const GICC_PMR: usize = 0x004;
const GICC_IAR: usize = 0x00c;
const GICC_EOIR: usize = 0x010;

/// IRQ27 优先级（0=最高）。PMR=0xFF 全放行，故任意值通过；取 0xa0 避开 secure 预留口径。
const PRIO_CNTV: u8 = 0xa0;

/// 初始化 GICv2：distributor + CPU interface，启用 IRQ27。
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
