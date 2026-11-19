//! berry-trapface——AArch64 异常分类面纯函数（W3-Q37 host 钉板）。
//!
//! 单源面 = `arch/aarch64/src/trap.rs` 的**异常分类判定律**：
//! - [`ec_of`]（ESR.elt×26 位域提取——EC 占 bits[31:26]）；
//! - [`EcKind`] 十三类词表（SVC/I-abort 双档/D-abort 双档/SError/BRK/
//!   PC-align/SP-align/FP 双档/Unknown——trap.rs 两 handler `match ec`
//!   全分流面的纯结构化镜像）；
//! - [`far_relevant`]（FAR 归类判：仅取指/数据访问类 EC 的 FAR 有效——
//!   ARMv8 ARM DDI 0487 §11.5.2「FAR_validity」；其余 EC FAR 为保留值，
//!   查 FAR = 幻影吞面）；
//! - [`ec_name`]（定名表——dump 行 `EC=0x.. (name)` 的字节串单源）。
//!
//! 消费侧：`berry-hal-aarch64::trap` re-export（EC 常量与定名表单源）；
//! 异常向量 asm/探针分流/杀链物理臂留 HAL 壳——本 crate 零 unsafe、
//! 零原子、零裸指针：入参全是 `u64`（ESR 原值）/`u32`（EC）值拷贝。
//!
//! 分类合同（与 HAL 侧逐字等价——`exception_sync`/`exception_sync_el0`
//! 两 `match` 的分类面）：
//!
//! ```text
//! ec_of(esr)                = (esr >> 26) & 0x3f
//! classify(ec):
//!   0x15                    → EcKind::Svc            （两 EL 同号派发）
//!   0x3c                    → EcKind::Brk            （G0 可恢复：ELR+=4）
//!   0x25                    → EcKind::DAbootCurrent  （demand 探针→期望槽→canary→fatal）
//!   0x24                    → EcKind::DAbootLower    （canary→EL0 探针→杀链）
//!   0x20                    → EcKind::IAbootLower    （EL0 探针（G4-4 X-2）→杀链）
//!   0x21                    → EcKind::IAbootCurrent  （fatal）
//!   0x22                    → EcKind::PcAlign        （fatal/杀链）
//!   0x26                    → EcKind::SpAlign        （fatal/杀链）
//!   0x2f                    → EcKind::SError         （fatal/杀链）
//!   0x00                    → EcKind::Unknown        （fatal/杀链）
//!   0x07                    → EcKind::FpSimdTrap     （fatal/杀链）
//!   0x0d                    → EcKind::FpTrap         （fatal/杀链）
//!   其余                    → EcKind::Other          （dump 定名 "other"）
//! ```

#![cfg_attr(not(test), no_std)]

/// EC=0x15：SVC（AArch64 Supervisor Call——两 EL 同号，`syscall::dispatch`）。
pub const EC_SVC: u32 = 0x15;
/// EC=0x3C：BRK（软件断点——G0 判据可恢复路径：dump 后 ELR+=4）。
pub const EC_BRK: u32 = 0x3c;
/// EC=0x25：D-abort 自 current EL（EL1 内核态数据中止）。
pub const EC_DABORT_CURRENT: u32 = 0x25;
/// EC=0x24：D-abort 自 lower EL（EL0 用户态数据中止——野指针主面）。
pub const EC_DABORT_LOWER: u32 = 0x24;
/// EC=0x21：I-abort 自 current EL（内核态取指中止）。
pub const EC_IABORT_CURRENT: u32 = 0x21;
/// EC=0x20：I-abort 自 lower EL（EL0 取指中止——G4-4 X-2 exec 代码页用例）。
pub const EC_IABORT_LOWER: u32 = 0x20;
/// EC=0x22：PC 对齐错误（目标非 4 对齐的分支/异常返回）。
pub const EC_PC_ALIGN: u32 = 0x22;
/// EC=0x26：SP 对齐错误（SP 非 16 对齐的访问）。
pub const EC_SP_ALIGN: u32 = 0x26;
/// EC=0x2F：SError（系统错误——异步/RAS 面，fatal）。
pub const EC_SERROR: u32 = 0x2f;
/// EC=0x00：Unknown（EC 无法解码——ISS=_insn 位面，UNDEFINED 面）。
pub const EC_UNKNOWN: u32 = 0x00;
/// EC=0x07：FP/SIMD 访问陷阱（CPACR_EL1.FPEN 关门面）。
pub const EC_FP_SIMD_TRAP: u32 = 0x07;
/// EC=0x0D：FP 异常陷阱（AArch32 FP 运行时异常——v1 面外，定名在册）。
pub const EC_FP_TRAP: u32 = 0x0d;

/// ESR 中 EC 域右移位数（bits[31:26]）。
pub const EC_SHIFT: u32 = 26;
/// ESR 中 EC 域位掩码（6 位）。
pub const EC_MASK: u64 = 0x3f;

/// 异常分类十三类词表（trap.rs 两 `match` 分流面的纯结构化镜像）。
///
/// 「词表」而非全 64 EC 枚举：分类面只钉 **内核可分流** 的类——其余
/// 归 [`EcKind::Other`]（dump 定名 `other`、fatal/杀链路径），与 HAL
/// `match` 的 `_` 臂逐字对应。枚举位序无关（分类判等驱动，不落 ABI）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EcKind {
    /// EC=0x00——Unknown（UNDEFINED 指令面）。
    Unknown,
    /// EC=0x07——FP/SIMD 访问陷阱（CPACR 关门）。
    FpSimdTrap,
    /// EC=0x0D——FP 异常陷阱。
    FpTrap,
    /// EC=0x15——SVC（两 EL 同号派发 syscall）。
    Svc,
    /// EC=0x20——I-abort(lower EL)（EL0 取指中止——dpgp I 腿受验）。
    IAbootLower,
    /// EC=0x21——I-abort(current EL)。
    IAbootCurrent,
    /// EC=0x22——PC 对齐错。
    PcAlign,
    /// EC=0x24——D-abort(lower EL)（EL0 野指针主面）。
    DAbootLower,
    /// EC=0x25——D-abort(current EL)（demand 探针→期望槽→canary 链）。
    DAbootCurrent,
    /// EC=0x26——SP 对齐错。
    SpAlign,
    /// EC=0x2F——SError。
    SError,
    /// EC=0x3C——BRK（G0 可恢复）。
    Brk,
    /// 词表外 EC（dump 定名 `other`、fatal/杀链——HAL `_` 臂镜像）。
    Other,
}

/// ESR 原值提取 EC（bits[31:26]）。
///
/// HAL 侧 `((tf.esr >> 26) & 0x3f) as u32` 的单源替代——常量
/// [`EC_SHIFT`]/[`EC_MASK`] 亦自本函数消费（一处定义）。
#[inline]
pub const fn ec_of(esr: u64) -> u32 {
    ((esr >> EC_SHIFT) & EC_MASK) as u32
}

/// EC 域值 → 分类（十三类词表）。
///
/// 与 HAL 两 `match` 分流面逐字等价：词表内 EC 各归其类，词表外一律
/// [`EcKind::Other`]——分类面**不存在**「未知即 Unknown」的折叠
/// （`Unknown` 专指 EC=0x00 的 UNDEFINED 指令面；`Other` 才是词表外）。
#[inline]
pub const fn classify(ec: u32) -> EcKind {
    match ec {
        EC_UNKNOWN => EcKind::Unknown,
        EC_FP_SIMD_TRAP => EcKind::FpSimdTrap,
        EC_FP_TRAP => EcKind::FpTrap,
        EC_SVC => EcKind::Svc,
        EC_IABORT_LOWER => EcKind::IAbootLower,
        EC_IABORT_CURRENT => EcKind::IAbootCurrent,
        EC_PC_ALIGN => EcKind::PcAlign,
        EC_DABORT_LOWER => EcKind::DAbootLower,
        EC_DABORT_CURRENT => EcKind::DAbootCurrent,
        EC_SP_ALIGN => EcKind::SpAlign,
        EC_SERROR => EcKind::SError,
        EC_BRK => EcKind::Brk,
        _ => EcKind::Other,
    }
}

/// FAR 归类判：该 EC 的 FAR 是否架构有效（查 FAR 才有语义）。
///
/// 依据 ARMv8 ARM DDI 0487A §11.5.2（FAR validity）：FAR 只对**取指
/// 中止/数据中止/对齐错误**（与 PC/SP 对齐族）记载有效虚地址；SVC/
/// BRK/FP 陷阱/Unknown/SError 等 EC 的 FAR 为**保留值**——读到的任何
/// 数值都不是故障地址，按其分流 = 幻影吞面（对抗审查 R4 收紧同源：
/// `exception_sync_el0` 的 canary 臂只对 EC=0x24 查 FAR，注释原文
/// 「其余 EC FAR 为保留值，查 = 幻影吞面」）。
///
/// 有效面 = I-abort 双档（0x20/0x21）/ D-abort 双档（0x24/0x25）/
/// PC-align（0x22）/ SP-align（0x26）。
#[inline]
pub const fn far_relevant(ec: u32) -> bool {
    matches!(
        ec,
        EC_IABORT_LOWER
            | EC_IABORT_CURRENT
            | EC_PC_ALIGN
            | EC_DABORT_LOWER
            | EC_DABORT_CURRENT
            | EC_SP_ALIGN
    )
}

/// EC 定名表（dump 行 `EC=0x.. (name)` 的字节串单源）。
///
/// 与 HAL 侧原 `ec_name` 逐字节一致（迁移面钉板：`b"FP/SIMD-trap"` 族
/// 十二名 + `b"other"` 兜底；字节串非 NUL 结尾定长切片——UART 直写面，
/// 不引入 core::str）。
#[inline]
pub fn ec_name(ec: u32) -> &'static [u8] {
    match ec {
        EC_UNKNOWN => b"Unknown",
        EC_FP_SIMD_TRAP => b"FP/SIMD-trap",
        EC_FP_TRAP => b"FP/trap",
        EC_SVC => b"SVC",
        EC_IABORT_LOWER => b"I-abort(lower)",
        EC_IABORT_CURRENT => b"I-abort(current)",
        EC_PC_ALIGN => b"PC-align",
        EC_DABORT_LOWER => b"D-abort(lower)",
        EC_DABORT_CURRENT => b"D-abort(current)",
        EC_SP_ALIGN => b"SP-align",
        EC_SERROR => b"SError",
        EC_BRK => b"BRK",
        _ => b"other",
    }
}

impl EcKind {
    /// 本类代表 EC（词表内 = 唯一 EC；[`EcKind::Other`] = 0x2A 词表外
    /// 代表值——仅 Debug 展示用，分类判等不消费）。
    #[inline]
    pub const fn ec(self) -> u32 {
        match self {
            EcKind::Unknown => EC_UNKNOWN,
            EcKind::FpSimdTrap => EC_FP_SIMD_TRAP,
            EcKind::FpTrap => EC_FP_TRAP,
            EcKind::Svc => EC_SVC,
            EcKind::IAbootLower => EC_IABORT_LOWER,
            EcKind::IAbootCurrent => EC_IABORT_CURRENT,
            EcKind::PcAlign => EC_PC_ALIGN,
            EcKind::DAbootLower => EC_DABORT_LOWER,
            EcKind::DAbootCurrent => EC_DABORT_CURRENT,
            EcKind::SpAlign => EC_SP_ALIGN,
            EcKind::SError => EC_SERROR,
            EcKind::Brk => EC_BRK,
            EcKind::Other => 0x2a,
        }
    }

    /// 本类定名（[`ec_name`] 的分类视角——host 断言便利面）。
    #[inline]
    pub const fn name(self) -> &'static [u8] {
        match self {
            EcKind::Unknown => b"Unknown",
            EcKind::FpSimdTrap => b"FP/SIMD-trap",
            EcKind::FpTrap => b"FP/trap",
            EcKind::Svc => b"SVC",
            EcKind::IAbootLower => b"I-abort(lower)",
            EcKind::IAbootCurrent => b"I-abort(current)",
            EcKind::PcAlign => b"PC-align",
            EcKind::DAbootLower => b"D-abort(lower)",
            EcKind::DAbootCurrent => b"D-abort(current)",
            EcKind::SpAlign => b"SP-align",
            EcKind::SError => b"SError",
            EcKind::Brk => b"BRK",
            EcKind::Other => b"other",
        }
    }
}

#[cfg(test)]
mod tests;
