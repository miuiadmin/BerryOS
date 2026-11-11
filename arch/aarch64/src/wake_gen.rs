//! ABI 常量层——**生成文件，勿手改**。
//! 单源 = `tools/abi.toml`；再生 = `just abi-gen`；
//! 守卫 = `just abi-check`（`tools/gen_abi.py --check`，挂 build/smoke 前置）。
//! 唤醒源枚举（`tools/abi.toml` [[wakesrc]]；add-only——新源只追加，
//! 不改值/不改义/不复用退役值，值域 u8 ≤ 0x7F，下一位空闲 0x08，占用以在册为准）。
//! 消费：`wake.rs`（per-CPU cell 初值）+ `trap.rs`（IRQ 臂 IAR→源
//! 映射）；内核 telemetry per-source 记账同名常量直引。语义协议详注 →
//! research/pm-notes/刀1-前置两问-裁决.md §5（PM-17 终审问①裁 A1）。

// ---- 唤醒源（哨兵/锚 = NONE 0x00 与 UNKNOWN 0x7F，add-only）----
/// 哨兵：未接线语境（仅 CPU1 首任务切入前窗口可持有）；到达 on_wake = 绊网 panic（写点 L0 执法，负延迟同族）。
pub const WAKE_SRC_NONE: u8 = 0x00;
/// boot 期非 IRQ 语境（rust_main 直跑窗的 spawn/唤醒）；首任务切入后冻结——G1.5 断言其不再增长（无主-3 空类证明）。
pub const WAKE_SRC_KERNEL_BOOT: u8 = 0x01;
/// CNTV PPI 27（tick 重编程与超时到期唤醒——expire_timeouts 路径）。
pub const WAKE_SRC_TIMER: u8 = 0x02;
/// PL011 SPI 33（控制台 RX 到达唤醒——rx_irq_notify 路径）。
pub const WAKE_SRC_UART_RX: u8 = 0x03;
/// SGI 1（跨核重调度投递）。
pub const WAKE_SRC_SGI_RESCHED: u8 = 0x04;
/// SGI 2（TLB shootdown 收端——无唤醒路径，只计到达）。
pub const WAKE_SRC_SGI_TLB: u8 = 0x05;
/// 任务语境基线：SVC/同步异常/内核任务（note_switch 切入即设）语境的一切 wake_matching 唤醒。
pub const WAKE_SRC_IPC: u8 = 0x06;
/// PL061 GPIO3 电源键（sim-S2 A4②：QEMU virt gpio-keys/poweroff，SPI 7=GIC INTID 39，QMP system_powerdown 脉冲）。v1 无主到达：只进 IRQ 级记账 + no_waiter 台账（unowned），恒无唤醒——永不进环（wake_src_valid 有意不含本源，到达-only 三号位与 NONE/UNKNOWN 同类；进环即 wake_check ⑤ 红）。
pub const WAKE_SRC_POWER_KEY: u8 = 0x07;
/// 未知 IRQ 号（GIC IAR 落未知臂）：只进 IRQ 级到达计数、永不进环；到达 on_wake = 写路径 bug → panic。
pub const WAKE_SRC_UNKNOWN: u8 = 0x7F;
