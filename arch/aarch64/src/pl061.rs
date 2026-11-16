//! PL061 GPIO 驱动（QEMU `virt` @`0x0903_0000`）——sim-S2 A4② 电源键通道
//! （PM-16 §4.2：无主 IRQ 正向对照，PM-17 三期线 200）。
//!
//! **接线事实**（QEMU `virt` 板级 + 自产 DTB）：PL061 → SPI 7 → GIC INTID 39
//! （[`crate::gic::IRQ_PL061`]，DTB `interrupts = <0 7 4>` = LEVEL_HIGH）；
//! GPIO3 = 电源键（gpio-keys/poweroff 节点）——QMP `system_powerdown` 注入
//! 脉冲（EFI/AAVMF 相 powerdown 走 ACPI GED——本通道 RX 相专用零重叠）。
//!
//! **去活铁序（GIC 电平触发）**：PL061 输出线挂 `MIS != 0`；边沿锁存
//! （istate）不随读清，**只有 ICR 写 1 清锁存**——handler 必须 ICR 在
//! **EOIR 之前**（CNTV rearm / PL011 drain 同一条电平铁序），否则 EOIR 后
//! 线仍高立即重投（同优先级活锁）。输入脚电平本身不挂线（边沿锁存型），
//! ICR 后线必降——重复注入安全。
//!
//! **双边沿（IBE）选型**：电源键脉冲极性（压低/抬高沿序）随 QEMU
//! gpio-power 模型实现细节——IBE bit3=1 双边沿锁存 = **极性无关**（单脉冲
//! 按下/抬起两沿各锁存一次，至多 2 IRQ/脉冲；smoke PM 微相 n 上界按此
//! 校准）。IEV（0x40C）随之失效不写；RIS（0x414）只读诊断面不消费。
//!
//! **v1 无消费面**：到达只进内核记账（`WAKE_SRC_POWER_KEY` 到达 +
//! no_waiter 台账——恒无唤醒、永不进环，`wake_src_valid` 有意不含本源）。
//!
//! **在席门在调用方**（UART1 同款纪律，见 `lib.rs PL061_BASE` 注）：PL061
//! 板级常在（非配置依赖），门 = fw_cfg `opt/berry/simdev` 注入族策略门
//!（main.rs boot 接线块）——门关不使能 SPI 39，裸跑/EFI 零扰动。

use crate::gic;
use crate::mmio;
use crate::{KERNEL_VA_BASE, PL061_BASE};

/// PL061 高半直映基址（与 UART0/1 同落 2MiB Device 块 `[0x0900_0000,
/// 0x0920_0000)`——`mmu.rs build_boot_pt` 已覆盖，零页表工作）。
const PL061: usize = KERNEL_VA_BASE + PL061_BASE;

// ---- PL061 寄存器偏移（PrimeCell PL061 TRM §3.2；另 IEV=0x40C/RIS=0x414
// 因 IBE 选型/无消费面不立常量，见模块头）----
/// 方向（0=输入、1=输出；复位全 0=输入）。电源键 = 输入。
const GPIO_DIR: usize = 0x400;
/// 中断检测型（0=边沿、1=电平；每脚 1 bit）。
const GPIO_IS: usize = 0x404;
/// 双边沿使能（1=双沿——覆盖 IEV 极性选择，极性无关选型）。
const GPIO_IBE: usize = 0x408;
/// 中断屏蔽（1=放行到 MIS/IRQ 线）。**最后开**——一开线即可能拉高。
const GPIO_IE: usize = 0x410;
/// 屏蔽后中断状态（IRQ 线电平源 = 本寄存器 != 0）。
const GPIO_MIS: usize = 0x418;
/// 写 1 清对应脚边沿锁存（**去活电平唯一手段**）。
const GPIO_ICR: usize = 0x41c;

/// 电源键脚号（QEMU `virt` GPIO3）。
const PIN: u32 = 3;
/// 脚位掩码（bit3）。
const PIN_MASK: u32 = 1 << PIN;

/// 「电源键无主到达」通知槽（HAL→内核反转，`set_rx_notify` 同模式）。IRQ
/// 上下文（PSTATE.I=1）pre-EOI 调；内核侧实现 = 到达 + no_waiter 双台账 +
/// `PWRKEY unowned n=` 标记行，**返回 false**（v1 恒无唤醒）。
static mut PL061_NOTIFY: Option<unsafe extern "C" fn() -> bool> = None;

/// 注册电源键通知（boot 期、`pl061_init` 之前——先注册后使能，槽空 = 首达
/// 静默无人记账）。
pub fn set_pl061_notify(f: unsafe extern "C" fn() -> bool) {
    // SAFETY: 启动单线程、DAIF 屏蔽。
    unsafe { *core::ptr::addr_of_mut!(PL061_NOTIFY) = Some(f) };
}

/// 电源键 IRQ handler（`exception_irq` INTID=39 分流、**EOIR 之前**调）：
/// MIS 确认 → **ICR 清锁存（去活电平铁序——不写则 EOIR 后线仍高立即重投）**
/// → 通知槽（内核记账面）。返回通知槽回报（v1 恒 false——无唤醒 ⇒ 无检查点）。
pub fn power_key_irq() -> bool {
    // SAFETY: IRQ 上下文（PSTATE.I=1）单核串行；MMIO 读写。
    unsafe {
        if mmio::read(PL061 + GPIO_MIS) & PIN_MASK == 0 {
            return false; // 非本脚（防御面：v1 板仅 GPIO3 接线，共享 IRQ 无他人）
        }
        mmio::write(PL061 + GPIO_ICR, PIN_MASK);
        if let Some(f) = *core::ptr::addr_of!(PL061_NOTIFY) {
            return f();
        }
    }
    false
}

/// 使能电源键中断（boot 期、DAIF 屏蔽窗内、simdev 门开后调一次）。次序四段
///（`rx_init` 同款纪律）：ICR 先清历史锁存（istate 锁存不随 IE 屏蔽——boot
/// 窗若有沿先擦掉，防使能即吃旧中断）→ 脚配置（DIR 输入 / IS 边沿 / IBE
/// 双边沿）→ GIC 使能 SPI 39（`SPI_ROUTES` 表路由 + 清历史 pending）→
/// **IE 最后开**（屏蔽一开线即可能拉高，此前 GIC/锁存已配干净）。
///
/// # Safety
/// MMIO 读写（Device 映射区）；boot 单线程、DAIF 屏蔽（任何 enable_irq 之前
/// ——通知槽已注册；**调用前提 = fw_cfg `opt/berry/simdev` 在席门已开**，
/// 门纪律见模块头）。
pub unsafe fn pl061_init() {
    mmio::write(PL061 + GPIO_ICR, PIN_MASK);
    let dir = mmio::read(PL061 + GPIO_DIR);
    mmio::write(PL061 + GPIO_DIR, dir & !PIN_MASK);
    let is = mmio::read(PL061 + GPIO_IS);
    mmio::write(PL061 + GPIO_IS, is & !PIN_MASK);
    let ibe = mmio::read(PL061 + GPIO_IBE);
    mmio::write(PL061 + GPIO_IBE, ibe | PIN_MASK);
    if !gic::enable_spi_routed(gic::IRQ_PL061) {
        crate::uart_puts(b"pl061: SPI39 missing from SPI_ROUTES\n");
    }
    let ie = mmio::read(PL061 + GPIO_IE);
    mmio::write(PL061 + GPIO_IE, ie | PIN_MASK);
}
