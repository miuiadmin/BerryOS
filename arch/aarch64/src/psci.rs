//! PSCI 探测与 SYSTEM_SUSPEND 深睡面（PM-16 §4.4 步3 / sim-S3）。
//!
//! HVC conduit 钉板同 [`crate::smp`]（S-0/S-1：默认机型 boot EL1、QEMU
//! `-kernel` 路）；EFI/handoff 路与 EL2 机型门住不发（conduit 归固件管 /
//! 分型未实测——`smp_bringup` 同款盒子）。探测 = PSCI_VERSION 正向对照
//!（conduit 活证诊断行）+ PSCI_FEATURES(SYSTEM_SUSPEND) 三档现实表：
//! TCG virt = FEATURES 返 -1 → `PSCI susp=0`（本刀实测）；KVM = 待 spike
//!（预期 susp=1）；真机 = 待硬件 v2（详 `docs/PM仿真S3断言实施设计-v1.0.md`）。

use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::smp::BOOT_EL;
use crate::{uart_put_dec, uart_putc, uart_puts};

// PSCI 1.0 fnid 钉板（32 位函数号走 HVC conduit；SMCCC 有符号返回语义）。
const PSCI_VERSION_FNID: u64 = 0x8400_0000;
const PSCI_FEATURES_FNID: u64 = 0x8400_000A;
const PSCI_SYSTEM_SUSPEND_FNID: u64 = 0x8400_000C;
/// PSCI_VERSION 最小有效值（PSCI 1.0 = 0x00010000；-1 = NOT_SUPPORTED 按符
/// 号语义天然排除——`ver >= 最小值` 即「有效」判）。
const PSCI_VERSION_MIN: u32 = 0x0001_0000;

/// 深睡能力探测结果缓存（probe 尾单写点，此后只读；ktest PWRDEG /
/// request_deep_sleep 降级门的消费面）。
static PSCI_SUSPEND_SUPPORTED: AtomicBool = AtomicBool::new(false);

/// 深睡能力读面（probe 缓存；未 probe / 门住路恒 false）。
pub fn suspend_supported() -> bool {
    PSCI_SUSPEND_SUPPORTED.load(Ordering::Relaxed)
}

/// 读 boot asm 钉存的入口 EL（`smp.rs` 同款私有 helper 复制：高半 `.bss`
/// 普通静态量，主核单写点后只读）。
fn boot_el() -> u64 {
    // SAFETY: 读 boot asm 单写点钉存的静态量（此后只读）。
    unsafe { (&raw const BOOT_EL).read_volatile() }
}

/// 模块本地 HVC 调用底座（SMCCC 形——`smp_bringup` CPU_ON 的 asm 钉板同款：
/// x0 inout 装 fnid，参数寄存器按调用各自定，x4-x17 + x18 全 clobber——规范
/// 允许固件不保留，不赌实现保留）。返回 i64（SMCCC 符号语义：-1 =
/// NOT_SUPPORTED）。
fn hvc_call(fnid: u64, x1: u64) -> i64 {
    let ret: u64;
    // SAFETY: 特权指令 HVC（EL1 → 固件 PSCI conduit）；调用窗 = boot DAIF
    // 屏蔽尾窗（probe）或 ktest/PM-03 直调窗，单核静默无并发；
    // 除返回值外全 clobber，无内存副作用。
    unsafe {
        asm!(
            "hvc #0",
            inout("x0") fnid => ret,
            inout("x1") x1 => _,
            out("x2") _, out("x3") _,
            out("x4") _, out("x5") _, out("x6") _, out("x7") _,
            out("x8") _, out("x9") _, out("x10") _, out("x11") _,
            out("x12") _, out("x13") _, out("x14") _, out("x15") _,
            out("x16") _, out("x17") _, out("x18") _,
            // Rust asm 默认允许破坏 NZCV（未传 preserves_flags），覆盖 SMCCC
            // “callee 可破坏条件标志”语义；AArch64 无独立 "flags" operand。
        );
    }
    ret as i64
}

/// 32 位值 8 位大写 hex（诊断行专用——[`crate::uart_put_hex64`] 为 16 位
/// 小写全宽，形态不合本行）。
fn put_hex32_upper(v: u32) {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    for i in (0..8).rev() {
        uart_putc(DIGITS[((v >> (i * 4)) & 0xf) as usize]);
    }
}

/// boot 期深睡能力探测（**必须在 ktest::run 之前**——PWRDEG 断言读缓存；
/// EFI/EL2/TCG/KVM 四路同点打印）。
///
/// - 门住路（`efi_boot`——conduit 归固件管；或入口非 EL1——HVC 分型未实测）
///   **不发 HVC**，一行降级 `PSCI susp=0`（带原因后缀，保持子串可 grep）。
/// - EL1 路：PSCI_VERSION 有效（≥0x00010000）→ 打诊断行 `PSCI version 0x…`
///   （非标记，hex 大写——conduit 活的正向对照）；无效 → `PSCI susp=0`。
///   有效时再 PSCI_FEATURES(SYSTEM_SUSPEND)：返 0 → `PSCI susp=1`；否则
///   （TCG：无 PSCI_FEATURES 实现，返 -1）→ `PSCI susp=0`。
///
/// 打印走与相邻 boot 行同款 UART 路径（`BerryOS boot` 同期窗口）。
pub fn probe(efi_boot: bool) {
    let mut supported = false;
    if efi_boot {
        uart_puts(b"PSCI susp=0 (EFI handoff)\n");
    } else if boot_el() != 1 {
        uart_puts(b"PSCI susp=0 (EL");
        uart_put_dec(boot_el());
        uart_puts(b")\n");
    } else {
        // PSCI_VERSION 是 32 位函数：先截到 u32 位型再判值，不赌 conduit
        // 对 -1 做符号扩展还是零扩展（0xFFFFFFFF 一律 NOT_SUPPORTED）。
        let ver = hvc_call(PSCI_VERSION_FNID, 0) as u32;
        if ver != u32::MAX && ver >= PSCI_VERSION_MIN {
            // 正向对照：conduit 活证（PSCI 1.x 在席）——版本号诊断行。
            uart_puts(b"PSCI version 0x");
            put_hex32_upper(ver);
            uart_puts(b"\n");
            // PSCI_FEATURES(SYSTEM_SUSPEND)：0 = 支持；-1/其它 = 不支持。
            if hvc_call(PSCI_FEATURES_FNID, PSCI_SYSTEM_SUSPEND_FNID) == 0 {
                supported = true;
                uart_puts(b"PSCI susp=1\n");
            } else {
                uart_puts(b"PSCI susp=0\n");
            }
        } else {
            uart_puts(b"PSCI susp=0\n");
        }
    }
    PSCI_SUSPEND_SUPPORTED.store(supported, Ordering::Relaxed);
}

/// SYSTEM_SUSPEND HVC（0x8400000C，x0 装 fnid；**x1=0 = NULL re-entry**：规范要求返 INVALID_PARAMETERS(-3)，实现行为不可假设。SMCCC
/// 语义：成功 = 下电不归，**返回（任何值）= 失败**。TCG 不可达面（probe 后
/// `suspend_supported()`=false 门住——真调用只发生在 KVM/真机 spike 路；
/// AArch64 态规范另有 SMC64 形 0xC400000C，TF-A/KVM 实作两形都收——spike
/// 时核验目标 hypervisor 接受度并同步 FEATURES 查询键）。
pub fn system_suspend_hvc() -> i64 {
    hvc_call(PSCI_SYSTEM_SUSPEND_FNID, 0)
}
