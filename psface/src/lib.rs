//! berry-psface——PSCI 版本门/FEATURES 侦察/能力判纯面（host 钉板）。
//!
//! W3-Q72：`arch/aarch64/src/psci.rs` 的判定段单源外迁：
//! - 探测路由门（EFI 门住 / 非 EL1 门住 / EL1 发探测）；
//! - PSCI_VERSION 编码域（major/minor 16+16 拆装）与最小有效值门
//!   （≥1.0 且非 0xFFFFFFFF——SMCCC -1 符号语义天然排除）；
//! - PSCI_FEATURES(SYSTEM_SUSPEND) 三档现实表（0 = 支持；-1/其它 = 不支持）；
//! - SYSTEM_OFF 能力判（conduit 活 = PSCI_VERSION 有效——0.2 起强制函数，
//!   不查 FEATURES）。
//!
//! 纯面纪律：无 HVC、无原子、无 unsafe——host 可测的路由/域/侦察判。
//! HVC 臂（`hvc_call` asm）、诊断行打印与原子缓存留壳在 HAL psci.rs。

#![cfg_attr(not(test), no_std)]

// ---- PSCI 1.0 fnid 钉板（32 位函数号走 HVC conduit）----
pub const PSCI_VERSION_FNID: u64 = 0x8400_0000;
pub const PSCI_FEATURES_FNID: u64 = 0x8400_000A;
pub const PSCI_SYSTEM_SUSPEND_FNID: u64 = 0x8400_000C;
/// PSCI SYSTEM_OFF（0.2 起强制函数——无 PSCI_FEATURES 查询面）。
pub const PSCI_SYSTEM_OFF_FNID: u64 = 0x8400_0008;

/// PSCI_VERSION 最小有效值（PSCI 1.0 = 0x00010000）。
pub const PSCI_VERSION_MIN: u32 = 0x0001_0000;

// ---- PSCI_VERSION 编码域（major/minor 16+16）----

/// 版本号 major 域（bits[31:16]）。
pub const fn version_major(ver: u32) -> u32 {
    ver >> 16
}

/// 版本号 minor 域（bits[15:0]）。
pub const fn version_minor(ver: u32) -> u32 {
    ver & 0xFFFF
}

/// major/minor 拼装（编码往返）。
pub const fn version_encode(major: u32, minor: u32) -> u32 {
    ((major & 0xFFFF) << 16) | (minor & 0xFFFF)
}

/// PSCI_VERSION 有效判（conduit 活证）：
/// - `0xFFFFFFFF`（= -1，NOT_SUPPORTED 的 u32 截断形）一律无效；
/// - 其余 ≥ `PSCI_VERSION_MIN`（1.0）即有效。
///
/// 输入是 HVC 返回 i64 **先截 u32 位型**再判值（HAL 侧截断律：不赌
/// conduit 对 -1 做符号扩展还是零扩展）。
pub const fn version_valid(ver: u32) -> bool {
    ver != u32::MAX && ver >= PSCI_VERSION_MIN
}

// ---- 探测路由门 ----

/// 探测路由三形（HAL `probe` 的判定面）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeRoute {
    /// EFI handoff 路——conduit 归固件管，**不发 HVC**，恒降级
    /// （诊断行 `PSCI susp=0 (EFI handoff)`）。
    EfiGated,
    /// 入口非 EL1 路——HVC 分型未实测，**不发 HVC**，恒降级
    /// （诊断行 `PSCI susp=0 (EL<n>)`）。
    BootElGated(u64),
    /// EL1 路——发 PSCI_VERSION 正向对照。
    Probe,
}

/// 路由判（门住两形先于探测形；`boot_el` 为 boot asm 钉存入口 EL）。
pub const fn probe_route(efi_boot: bool, boot_el: u64) -> ProbeRoute {
    if efi_boot {
        ProbeRoute::EfiGated
    } else if boot_el != 1 {
        ProbeRoute::BootElGated(boot_el)
    } else {
        ProbeRoute::Probe
    }
}

// ---- FEATURES 侦察 ----

/// PSCI_FEATURES(SYSTEM_SUSPEND) 侦察判：SMCCC 语义 0 = 支持；
/// -1（NOT_SUPPORTED）或任何非零 = 不支持（TCG 无 FEATURES 实现实测返 -1）。
pub const fn features_suspend_supported(ret: i64) -> bool {
    ret == 0
}

/// SMCCC 有符号错误码（PSCI 0.2+ 实装通用域）。
pub const NOT_SUPPORTED: i64 = -1;
/// SYSTEM_SUSPEND 的 NULL re-entry 参数错（规范要求返 -3；实现行为不可假设）。
pub const INVALID_PARAMETERS: i64 = -3;

/// SYSTEM_OFF 能力判：conduit 活（= PSCI_VERSION 有效）即支持——0.2 起
/// 强制函数不查 FEATURES（比 SYSTEM_SUSPEND 的三档现实表宽：TCG 的
/// susp=0 但 SYSTEM_OFF 在）。
pub const fn off_supported_from(version_ret: u32) -> bool {
    version_valid(version_ret)
}

#[cfg(test)]
mod tests;
