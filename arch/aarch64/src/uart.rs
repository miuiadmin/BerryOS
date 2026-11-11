//! PL011 UART 驱动（QEMU `virt` UART0）——TX 面 + P5 A-1 起 RX 面/RX 中断。
//!
//! **TX**：`uart_putc` 直写 DR、不查 FR（QEMU PL011 对 TX 宽容——host 侧串口
//! 即时排空；真机不可）。P1 console 批量写起新增 [`uart_write_txff`]：批量
//! 写 DR 前查 FR.TXFF（真机硬化面；`uart_putc` 原样不动——boot/panic/ktest
//! 直写消费者零扰动）。历史上在 lib.rs，P5 A-1 起 UART 面收敛
//! 到本模块（lib.rs `pub use` 转发，调用方零改动）。
//!
//! **RX（P5 A-1 内核 RX 中断——GIC 第二个 IRQ 源，trap.rs 自此按 INTID 分流）**：
//! - 中断号 = GIC SPI 33（QEMU virt 接线：PL011 → SPI 1 + 16 SGI + 16 PPI；
//!   zCore 同板 `irq_enable(33)` + QEMU 自产 DTB `LEVEL_HIGH` 双活证，见
//!   `gic::IRQ_PL011_RX`）。bring-up 兜底：若接线有误，`exception_irq` 未知源
//! 分支首 4 个 loud 打 `irq? id=N`——接错号串口可见，不静默吞。
//! - **电平触发**：IRQ 线挂在「RX FIFO 非空」上——handler 必须 drain 到 FR.RXFE
//!   才能 EOIR（与 CNTV「rearm 在 EOIR 前」同一条电平铁序），否则 EOIR 后线仍
//!   高立即重投（同优先级活锁，timer 饿死）。
//! - **FEN=0（复位默认，v1 不写 LCR_H）**：FIFO 有效深度 1 字节。真机上突发
//!   第二字节在 IRQ 延迟窗内即 overrun 丢字（粘滞 OE 置位，粘滞清法见下）；
//!   **QEMU（11.0.3 源码实证）无此机制**——INT_OE 从不置位，chardev 流控
//!   （`can_receive` = 空位）把 host 输入押在设备外，不丢字；QEMU 上唯一
//!   丢字向量 = **内核环满**（`lost` 计数）。v1 消费面（console 行缓冲 +
//!   smoke 逐字节慢速注入——单字节-per-handler 的标记确定性，非 overrun
//!   规避）按打字速率设计；粘贴突发/`FEN=1`+RT 超时中断留 P5 A-2（bsh
//!   readline）再议——QEMU 对 RT 的建模未实测，别盲开。
//! - **零分配纪律**（IRQ 热路径，遥测环同款）：静态定长环形缓冲（深 256）+
//!   溢出丢字节计数；「数据到达」经函数指针槽反转通知内核（与 IRQ_DISPATCH
//!   同模式——HAL 不依赖 sched/内核 crate）。

use crate::gic;
use crate::mmio;
use crate::{KERNEL_VA_BASE, UART0_BASE, UART1_BASE};

/// UART0 高半直映基址（2b-2 低半退役：PA 直访必 fault；boot asm 早期打印
/// 例外——开 MMU 前直写 PA，不走这里）。
const UART: usize = KERNEL_VA_BASE + UART0_BASE;

// ---- PL011 寄存器偏移（TRMPrimecell UART PL011 §3.2；与 zCore
// `uart_pl011.rs` 对照一致——寄存器事实性数据，非代码拷贝）----
/// 数据寄存器（读 = RX FIFO 出队；写 = TX 入队，低 8 位有效）。
const UART_DR: usize = 0x000;
/// 标志寄存器（只读）。
const UART_FR: usize = 0x018;
/// 控制寄存器。
const UART_CR: usize = 0x030;
/// 中断屏蔽寄存器。
const UART_IMSC: usize = 0x038;
/// 写 1 清粘滞中断（含 overrun）。
const UART_ICR: usize = 0x044;

/// FR.RXFE：RX FIFO **空**（置位 = 无数据；=0 才可读 DR）。
const UART_FR_RXFE: u32 = 1 << 4;
/// FR.TXFF：TX FIFO **满**（FEN=0 = 保持寄存器忙）。置位时写 DR = 丢字节
///（UART0 批量写面 [`uart_write_txff`] 与 UART1 TX 面 `uart1_tx` 共用）。
const UART_FR_TXFF: u32 = 1 << 5;
/// CR 使能位（RXE=bit9 | TXE=bit8 | UARTEN=bit0）——QEMU `-kernel` 启动即此值，
/// rx_init 里 RMW 保险（万一 boot 流程动过 CR 也不失 TX）。
const UART_CR_EN: u32 = (1 << 9) | (1 << 8) | 1;
/// IMSC.RXIM：只开 RX 中断（TXIM 会在 TX FIFO 空时持续挂线——风暴，不开）。
const UART_IMSC_RXIM: u32 = 1 << 4;
/// ICR 写 1 清全部粘滞（0x7FF 全字段）。
const UART_ICR_ALL: u32 = 0x7ff;
/// 实际写的 ICR 值：**掩掉 RXIC（bit4）**。QEMU PL011（11.0.3 源码实证）的
/// INT_RX 只在入队时「严格 count==read_trigger(=1)」置位——drain 循环末次 FR
/// 读与 ICR 写之间到达的竞速字节若被 ICR 抹掉锁存，则字节滞留 FIFO、线永低、
/// 且 chardev 流控（can_receive=0）押住后续输入 → RX 面**永久静默挂起**。掩
/// RXIC 后：正常路径排空 FIFO 的 DR 读已清 INT_RX（线低不重投）；竞速路径
/// 锁存保持 → EOIR 时电平仍高 → GIC 电平重投 → 再进 handler 排空（无风暴）。
const UART_ICR_STICKY: u32 = UART_ICR_ALL & !(UART_IMSC_RXIM); // RXIC 与 RXIM 同 bit4

/// 输出一个字节到 UART0（2b-2 起走高半直映别名——低半身份已退役，PA 直写必 fault；
/// boot asm 早期打印不走这里，是 asm 里直写 PA）。
#[inline]
pub fn uart_putc(c: u8) {
    // unsafe 收敛在此：MMIO 写。
    unsafe { mmio::write(UART, c as u32) };
}

/// 输出一段字节串。
pub fn uart_puts(s: &[u8]) {
    for &b in s {
        uart_putc(b);
    }
}

/// 以十六进制输出 u64（固定 16 位，无 `0x` 前缀）。供 trap dump / panic 格式化复用。
pub fn uart_put_hex64(v: u64) {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut buf = [0u8; 16];
    for i in 0..16 {
        let shift = (15 - i) * 4;
        buf[i] = DIGITS[((v >> shift) & 0xf) as usize];
    }
    uart_puts(&buf);
}

/// 以十进制输出 u64（无符号，无前导空格）。供 tick 计数等复用。
pub fn uart_put_dec(v: u64) {
    if v == 0 {
        uart_putc(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut n = v;
    let mut i = 0usize;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        uart_putc(buf[i]);
    }
}

/// UART0 TX 丢弃计数（批量写 TXFF 有界自旋触界面——健康路径恒 0；诊断预留，
/// `UART1_TX_LOST` 同款）。真机/异常慢 sink wedge 面的观测位。
pub static UART0_TX_LOST: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(0);

/// 批量输出一段字节串到 UART0，写 DR 前逐字节查 FR.TXFF（P1 console 批量写
/// 路径专用新增面）。FR.TXFF 置位（FEN=0 = TX 保持寄存器忙）时写 DR = 丢
/// 字节——QEMU PL011 对 TX 宽容（宿主 sink 即时排空，实测不达界），真机不可。
/// 忙则自旋让位（`uart1_tx` 同款有界自旋——1M 次触界后整批余量记账丢弃、
/// 不逐字节再耗界，不 wedge；真机中断驱动 TX 归 v2）。逐字节发射仍走
/// [`uart_putc`]（发射面单源；`uart_putc`/`uart_puts` 本体不动——boot/
/// panic/ktest 直写消费者零扰动）。
pub fn uart_write_txff(s: &[u8]) {
    for (i, &b) in s.iter().enumerate() {
        // SAFETY: MMIO 读写。自旋窗 IRQ 仍开（任务态调用、调用方 PRINT_LOCK
        // preempt 计数包络「入而不切」）——不构成禁活锁面（uart1_tx 同口径）。
        unsafe {
            let mut spin: u32 = 0;
            while mmio::read(UART + UART_FR) & UART_FR_TXFF != 0 {
                spin += 1;
                if spin >= 1_000_000 {
                    UART0_TX_LOST
                        .fetch_add((s.len() - i) as u32, core::sync::atomic::Ordering::Relaxed);
                    return;
                }
            }
        }
        uart_putc(b);
    }
}

// ---- RX 面（P5 A-1）----

/// RX 静态环容量。console 侧一次 `rx_pop_all` 全搬出（消费缓冲 ≥ 此值则环必
/// 被搬空——这是「清 Event 位 ⟺ 环空」不变量的几何前提）。
pub const RX_RING_SIZE: usize = 256;

/// RX 环形缓冲（IRQ 生产者 / console 消费者；两下标均单调 mod 环长）。
struct RxRing {
    data: [u8; RX_RING_SIZE],
    /// 生产者写位（RX IRQ 上下文独占——SPI33 路由钉 CPU0，S-1 §3.4）。
    head: usize,
    /// 消费者读位（console 任务的 `disable_irq` 包络内独占——E26/S-2：该
    /// 包络是**同核**互斥，安全前提 = 生产者与消费者同钉 CPU0（RX 路由 +
    /// console 钉核，S-2 盒子）；消费者迁核 = 丢更新，S-3 per-CPU 化时重审）。
    tail: usize,
    /// 环满丢字节数（bring-up 可见性：console 标记行打印；不丢命只丢字节）。
    lost: u32,
}

static mut RX_RING: RxRing = RxRing { data: [0; RX_RING_SIZE], head: 0, tail: 0, lost: 0 };

/// 「RX 到数据」通知槽（HAL→内核反转，`set_irq_dispatch` 同模式）。IRQ 上下文
/// 调用（PSTATE.I=1）；内核侧实现 = 置 RX Event + `wake_matching`，**返回
/// 「是否唤醒」**（G3-2 修复批 F1：trap 臂据此在 EOIR 后决定是否检查点——
/// 检查点不得在 ack↔EOIR 之间，EOIR 饿死防线）。
static mut RX_NOTIFY: Option<unsafe extern "C" fn() -> bool> = None;

/// 注册 RX 到数据通知（boot 期、任何 `enable_irq` 之前——先注册后使能，否则
/// 首字节静默落环无人唤醒：槽空时 `rx_irq` 平凡跳过通知）。
pub fn set_rx_notify(f: unsafe extern "C" fn() -> bool) {
    // SAFETY: 启动单线程、DAIF 屏蔽。
    unsafe { *core::ptr::addr_of_mut!(RX_NOTIFY) = Some(f) };
}

/// RX FIFO 是否有数据可读（FR.RXFE=0）——bring-up 探针面（poll 消费/诊断用）。
pub fn rx_has_byte() -> bool {
    // SAFETY: MMIO 读。
    unsafe { mmio::read(UART + UART_FR) & UART_FR_RXFE == 0 }
}

/// 读一字节（调用前须 `rx_has_byte()`——FR 检查后 DR 读，bring-up 探针面）。
pub fn rx_read_byte() -> u8 {
    // SAFETY: MMIO 读。
    unsafe { mmio::read(UART + UART_DR) as u8 }
}

/// 环满丢字节数（console 标记行打印用）。
pub fn rx_lost() -> u32 {
    // SAFETY: 单核顺序读；addr_of 取裸指针再解引用（不借用 static mut）。
    unsafe { (*core::ptr::addr_of!(RX_RING)).lost }
}

/// RX IRQ handler（`exception_irq` 按 INTID=33 分流、**EOIR 之前**调）：drain
/// FIFO 到 RXFE（电平触发去活铁序——不排空则 EOIR 后线仍高立即重投）→ 清粘滞
/// （overrun 等）→ 经通知槽叫内核置 Event/唤醒等待者，**返回通知槽的「是否
/// 唤醒」回报**（G3-2 修复批 F1：trap 臂 EOIR 后用于检查点决策；槽空 = false
/// ——boot 早期平凡跳过）。全程零分配；FEN=0 下突发多字节 = 多次进 handler
/// （每次一字节，见模块头）。
pub fn rx_irq() -> bool {
    // SAFETY: IRQ 上下文（PSTATE.I=1）单生产者；MMIO 读写。
    unsafe {
        while mmio::read(UART + UART_FR) & UART_FR_RXFE == 0 {
            let c = mmio::read(UART + UART_DR) as u8;
            let r = &mut *core::ptr::addr_of_mut!(RX_RING);
            let next = (r.head + 1) % RX_RING_SIZE;
            if next == r.tail {
                r.lost += 1; // 环满：丢字节计数（消费侧欠账，不崩）
            } else {
                r.data[r.head] = c;
                r.head = next;
            }
        }
        // FIFO 已排空。清粘滞但**不清 RXIC**（UART_ICR_STICKY，见常量注释）：
        // 竞速字节的 INT_RX 锁存保住，EOIR 时电平仍高 → GIC 重投再 drain；
        // 正常路径的 INT_RX 已被排空 DR 读清（QEMU 源码：count==0 时随读清）。
        mmio::write(UART + UART_ICR, UART_ICR_STICKY);
        if let Some(f) = *core::ptr::addr_of!(RX_NOTIFY) {
            return f();
        }
    }
    false
}

/// 搬空 RX 环（console 侧消费）：环内字节依序写入 `buf`（至多 `buf.len()`），
/// 返回搬出条数。**必须在 `disable_irq` 包络内调**（与 IRQ 生产者串行——
/// `ai_probe` 扫 SLEEPQ 同款包络纪律；E26/S-2：disable_irq 只屏蔽**本核**，
/// 互斥成立的前提 = 生产者（SPI33 钉 CPU0）与消费者 console 同核）；
/// `buf` ≥ [`RX_RING_SIZE`] 则环必被搬空
/// ——这是随后清 Event 位成立的前提（「位 0 ⟺ 环空」不变量在包络内闭环）。
pub fn rx_pop_all(buf: &mut [u8]) -> usize {
    // SAFETY: 单消费者（console 任务）；调用方保证 IRQ 屏蔽包络。
    unsafe {
        let r = &mut *core::ptr::addr_of_mut!(RX_RING);
        let mut i = 0usize;
        while r.tail != r.head && i < buf.len() {
            buf[i] = r.data[r.tail];
            r.tail = (r.tail + 1) % RX_RING_SIZE;
            i += 1;
        }
        i
    }
}

/// 使能 RX 中断（boot 期、DAIF 屏蔽窗内调一次）。次序三段：UART 侧先清粘滞、
/// 确保收发使能；GIC 侧使能 SPI 33（含清历史 pending）；**IMSC.RXIM 最后开**
/// ——屏蔽位一开线即可能拉高，此前 GIC/pending 已配干净，首中断即真数据。
///
/// # Safety
/// MMIO 读写（Device 映射区）；boot 单线程、DAIF 屏蔽（任何 enable_irq 之前——
/// 使能后首中断须 handler/通知槽已就绪）。
pub unsafe fn rx_init() {
    // 同掩 RXIC（UART_ICR_STICKY）：boot 期已落 FIFO 的字节锁存保住，IMSC
    // 一开线即高、首中断即排空——ICR_ALL 会把滞留字节锁成永久死信。
    mmio::write(UART + UART_ICR, UART_ICR_STICKY);
    let cr = mmio::read(UART + UART_CR);
    mmio::write(UART + UART_CR, cr | UART_CR_EN);
    // S-1（§3.4）：路由经 gic::SPI_ROUTES 常量表（单核钉 CPU0）——表外源
    // 此处 loud（boot 无 panic 纪律；RX 失活另有 smoke RX 相断言兜底）。
    if !gic::enable_spi_routed(gic::IRQ_PL011_RX) {
        crate::uart_puts(b"uart: SPI33 missing from SPI_ROUTES\n");
    }
    mmio::write(UART + UART_IMSC, UART_IMSC_RXIM);
}

// ---- UART1 RX 面（sim-S1：第二 PL011，sim 设备注入通道——
// `docs/sim设备通道与契约设计-v1.1.md` §4）----
//
// 全套镜像 UART0 RX 面（结构同形、静态双份——避免泛型化搅动现役 UART0 面）。
// 关键差异两点：
// - **在席门在调用方**：UART1 设备随 QEMU 第二 `-serial` 配置存在（单串口 =
//   地址洞，见 lib.rs `UART1_BASE` 注）——`uart1_init` 只在 fw_cfg
//   `opt/berry/simdev` 在席门开后调（main.rs boot 接线块），本模块不自行探测。
// - **TX 面新增**（UART0 TX 常规面 `uart_putc` 不查 FR——批量面走
//   `uart_write_txff` 才查；UART1 的 ACK 回程是真
//   依赖）：`uart1_tx` 写 DR 前查 FR.TXFF（FEN=0 下 TX 保持寄存器忙即置位——
//   不查直接写 = 丢字节），忙则自旋让位（QEMU 宿主 sink 排水快，9B ACK 微秒
//   级；真机 hardening 归 v2——有界自旋后丢字节 + 计数，不 wedge）。

/// UART1 高半直映基址（与 UART0 同落 2MiB Device 块 `[0x0900_0000, 0x0920_0000)`
/// ——`mmu.rs build_boot_pt` dev_blocks 已覆盖，零页表工作）。
const UART1: usize = KERNEL_VA_BASE + UART1_BASE;

/// UART1 RX 静态环（`RX_RING_SIZE` 同容量——帧帽 98B，256 环 = 整帧 + 余量；
/// 消费者一次 `uart1_rx_pop_all` 全搬出，环空 ⟺ 可清位的几何前提同 UART0）。
static mut UART1_RING: RxRing = RxRing { data: [0; RX_RING_SIZE], head: 0, tail: 0, lost: 0 };

/// UART1「RX 到数据」通知槽（`RX_NOTIFY` 同模式；注册先于 `uart1_init`）。
static mut UART1_NOTIFY: Option<unsafe extern "C" fn() -> bool> = None;

/// 注册 UART1 RX 通知（boot 期、`uart1_init` 之前——先注册后使能）。
pub fn set_uart1_notify(f: unsafe extern "C" fn() -> bool) {
    // SAFETY: 启动单线程、DAIF 屏蔽。
    unsafe { *core::ptr::addr_of_mut!(UART1_NOTIFY) = Some(f) };
}

/// UART1 环满丢字节数（诊断面）。
pub fn uart1_rx_lost() -> u32 {
    // SAFETY: 单核顺序读；addr_of 取裸指针（不借用 static mut）。
    unsafe { (*core::ptr::addr_of!(UART1_RING)).lost }
}

/// UART1 RX IRQ handler（`exception_irq` INTID=40 分流、EOIR 之前调）——
/// `rx_irq` 逐句同形：drain FIFO 到 RXFE（电平去活铁序）→ 清粘滞不清 RXIC
/// （`UART_ICR_STICKY` 竞速字节锁存保住，EOIR 时线仍高 → GIC 重投再 drain）
/// → 通知槽（置 Event + wake），回报「是否唤醒」（EOIR 后检查点决策）。
pub fn uart1_rx() -> bool {
    // SAFETY: IRQ 上下文（PSTATE.I=1）单生产者（SPI40 路由钉 CPU0）；MMIO 读写。
    unsafe {
        while mmio::read(UART1 + UART_FR) & UART_FR_RXFE == 0 {
            let c = mmio::read(UART1 + UART_DR) as u8;
            let r = &mut *core::ptr::addr_of_mut!(UART1_RING);
            let next = (r.head + 1) % RX_RING_SIZE;
            if next == r.tail {
                r.lost += 1; // 环满：丢字节计数（消费侧欠账，不崩）
            } else {
                r.data[r.head] = c;
                r.head = next;
            }
        }
        mmio::write(UART1 + UART_ICR, UART_ICR_STICKY);
        if let Some(f) = *core::ptr::addr_of!(UART1_NOTIFY) {
            return f();
        }
    }
    false
}

/// 搬空 UART1 RX 环（task_simcon 消费）——`rx_pop_all` 同形：必须在
/// `disable_irq` 包络内调（与 IRQ 生产者同核串行——SPI40 钉 CPU0 + 消费任务
/// 钉核前提，console/S-2 盒子同款边界）。
pub fn uart1_rx_pop_all(buf: &mut [u8]) -> usize {
    // SAFETY: 单消费者（task_simcon）；调用方保证 IRQ 屏蔽包络。
    unsafe {
        let r = &mut *core::ptr::addr_of_mut!(UART1_RING);
        let mut i = 0usize;
        while r.tail != r.head && i < buf.len() {
            buf[i] = r.data[r.tail];
            r.tail = (r.tail + 1) % RX_RING_SIZE;
            i += 1;
        }
        i
    }
}

/// UART1 写一字节（ACK 回程；task_simcon 语境）。FR.TXFF 忙则自旋让位——
/// QEMU 下宿主 sink 排水微秒级、实测不达界；有界自旋（防真机/异常慢 sink
/// wedge 整核）后丢字节计数（静默——ACK 丢失由宿主 client 超时对拍暴露，
/// 断链诊断走 client 面）。FEN=0 下 TXFF=保持寄存器忙，逐字节写前必查。
pub fn uart1_tx(c: u8) {
    // SAFETY: MMIO 读写。自旋窗 IRQ 仍开（任务态调用）、抢占点照常——
    // 不构成禁活锁面。
    unsafe {
        let mut spin: u32 = 0;
        while mmio::read(UART1 + UART_FR) & UART_FR_TXFF != 0 {
            spin += 1;
            if spin >= 1_000_000 {
                UART1_TX_LOST.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                return;
            }
        }
        mmio::write(UART1, c as u32);
    }
}

/// UART1 TX 丢弃计数（TXFF 有界自旋触界面——健康路径恒 0；诊断预留）。
pub static UART1_TX_LOST: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(0);

/// 使能 UART1 RX 中断（boot 期、DAIF 屏蔽窗内、fw_cfg simdev 门开后调一次）
/// ——`rx_init` 三段序同形：清粘滞（掩 RXIC 保竞速锁存）→ CR 确保收发使能 →
/// GIC 使能 SPI 40（`SPI_ROUTES` 表路由）→ IMSC.RXIM 最后开。
///
/// # Safety
/// MMIO 读写（Device 映射区）；boot 单线程、DAIF 屏蔽（任何 enable_irq 之前
/// ——通知槽已注册；**调用前提 = fw_cfg `opt/berry/simdev` 在席**，否则单
/// 串口配置下本函数首读即 external abort，见 lib.rs `UART1_BASE` 注）。
pub unsafe fn uart1_init() {
    mmio::write(UART1 + UART_ICR, UART_ICR_STICKY);
    let cr = mmio::read(UART1 + UART_CR);
    mmio::write(UART1 + UART_CR, cr | UART_CR_EN);
    if !gic::enable_spi_routed(gic::IRQ_PL1_RX) {
        crate::uart_puts(b"uart1: SPI40 missing from SPI_ROUTES\n");
    }
    mmio::write(UART1 + UART_IMSC, UART_IMSC_RXIM);
}
