//! bos-hal-aarch64 —— ARM64 HAL（arch 层，unsafe 收敛地）。
//!
//! Phase 0：boot 入口（EL2→EL1 降级，见 `boot.rs`）+ PL011 UART 输出 + halt。
//! Phase 1 Step 4：stage-1 页表 + 开 MMU（见 [`mmu`]、[`paging`]）。
//! Phase 1 Step 6：异常向量表 + 同步异常/IRQ 处理（见 [`trap`]）。
//! Phase 1 Step 7：GICv2 中断控制器（见 [`gic`]）。
//! Phase 1 Step 8：CNTV 虚拟定时器（见 [`timer`]）。
//! Phase 1 Step 9：SVC syscall 派发（见 [`syscall`]）。
//! Phase 2a Step 1：任务上下文 + 上下文切换 asm（见 [`task`]）。
//! 仅 `#![no_std]`；`panic_handler` / `no_main` 由最终 bin crate（`内核/`）提供。
#![no_std]

mod boot;
pub mod gic;
mod mmio;
mod mmu;
mod paging;
pub mod syscall;
pub mod task;
pub mod timer;
pub mod trap;

// ---- 平台内存图（QEMU `virt`）----
// RAM 几何原在 bos-mm，现挪进 HAL（平台层；bos-mm 保持 arch 无关，由调用方传入）。

/// 物理内存起点（QEMU `virt` RAM）。
pub const RAM_BASE: usize = 0x4000_0000;
/// 物理内存终点（128MiB）。
pub const RAM_TOP: usize = 0x4800_0000;
/// QEMU `virt` PL011 UART0 基址。
pub const UART0_BASE: usize = 0x0900_0000;
/// GICv2 distributor 基址（QEMU `virt`）。
pub const GICD_BASE: usize = 0x0800_0000;
/// GICv2 CPU interface 基址（QEMU `virt`）。
pub const GICC_BASE: usize = 0x0801_0000;

/// 内核高半基址 = 39-bit VA 高半的底（T1SZ=25 → TTBR1 区 = `[此值, 2^64)`）。
/// `KERNEL_VA_BASE + pa` 是内核 direct map 的**唯一规则**（RAM 与 MMIO 统一，见
/// `内存管理设计.md` §5）；bits[38:0]=0 使 KVA 表索引 == PA 表索引。2b-1 起
/// 内核主体已链高半；`bos_mm::va` 的交换点由 `rust_main` 注入此值（x86_64 最小
/// 集移植时按目标另定，注入 0 = 恒等）。
pub const KERNEL_VA_BASE: usize = 0xFFFF_FF80_0000_0000;

/// 启动后期初始化：装异常向量表 + 初始化 GIC。
/// （MMU/页表已在 boot 期由 `.text.boot` 的 `build_boot_pt`/`enable_mmu` 就绪，
/// 2b-1 起 `rust_main` 进来时内核已在高半执行。）
///
/// 必须在 `rust_main`、单核、EL1、MMU 开时调用一次。
pub fn init() {
    // SAFETY: EL1、MMU 已开（向量表在直映的内核镜像内）；任何 DAIF 解蔽之前。
    unsafe { trap::init() };
    // SAFETY: MMIO（Device 映射区）；EL1、单核、DAIF 仍屏蔽。
    unsafe { gic::init() };
}

/// 高半别名自校验：RAM 魔数 + GIC PIDR2 双别名（低半身份 × 高半直映）。
/// 证两套翻译一致生效——2b-2 逐进程页表 / 低半退役的地基。
pub fn kva_ok() -> bool {
    mmu::kva_alias_probe()
}

// ---- IRQ 派发回调（HAL→上层，函数指针槽反转）----
//
// HAL 是叶子 crate，**不依赖** bos-sched（否则循环依赖：sched → hal → sched）。
// 但 timer/IRQ 抖动需要通知调度器（Step 5 打印 tick、Step 6 驱动抢占）——用**函数指针
// 槽**把调用反转：调度器不进 HAL 的依赖图，`内核` 在 boot 时注册回调（`set_irq_dispatch`），
// `exception_irq` 经槽调。HAL 永不知 scheduler 之名（plan 循环依赖规避 + R10）。

/// IRQ 派发回调类型（EOIR 之后、`eret` 之前调；C-ABI 约定——ADR #18 syscall/FFI 边界）。
pub type IrqDispatch = unsafe extern "C" fn(&mut trap::TrapFrame);

static mut IRQ_DISPATCH: Option<IrqDispatch> = None;

/// 注册 IRQ 派发回调。须在**任何 `enable_irq` 之前**调用一次（否则首 IRQ 落空槽——
/// 由下方 `if let Some` 平凡处理，但 boot 期就该注册好）。
pub fn set_irq_dispatch(f: IrqDispatch) {
    // SAFETY: 启动单线程、DAIF 屏蔽（enable_irq 前）；无并发 IRQ、无竞态。
    unsafe { IRQ_DISPATCH = Some(f) };
}

/// 取已注册的 IRQ 派发回调（`exception_irq` 用）。未注册时返回 `None`。
///
/// # Safety
/// 读 `static mut`。仅 IRQ 上下文（单核顺序）调用。
pub unsafe fn irq_dispatch() -> Option<IrqDispatch> {
    IRQ_DISPATCH
}

/// 输出一个字节到 UART0。
#[inline]
pub fn uart_putc(c: u8) {
    // unsafe 收敛在此：MMIO 写。
    unsafe { mmio::write(UART0_BASE, c as u32) };
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

/// 解蔽 IRQ（`daifclr #2`，清 DAIF.I）。解蔽序的最后一步（VBAR→GIC→timer→此）。
pub fn enable_irq() {
    // SAFETY: 写 DAIF 标志位；EL1。
    unsafe { core::arch::asm!("msr daifclr, #2", options(nostack, preserves_flags)) };
}

/// 屏蔽 IRQ（`daifset #2`，置 DAIF.I）。
#[allow(dead_code)]
pub fn disable_irq() {
    unsafe { core::arch::asm!("msr daifset, #2", options(nostack, preserves_flags)) };
}

/// 单次 `wfi`（等中断唤醒一次）。idle 循环用——区别于 [`halt`] 的死循环 wfi。
pub fn wfi() {
    // SAFETY: 特权指令；EL1。
    unsafe { core::arch::asm!("wfi", options(nostack, preserves_flags)) };
}

/// 永久挂起（`wfi` 循环）。DAIF 屏蔽时无中断唤醒 → 真正停；
/// 解蔽后 IRQ 唤醒 wfi、走完向量再回此循环（idle 主循环）。
pub fn halt() -> ! {
    loop {
        // unsafe 收敛在此：特权指令。
        unsafe { core::arch::asm!("wfi") };
    }
}
