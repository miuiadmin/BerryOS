//! 统一 Driver trait 框架（驱动子系统研究报告 P1 缺口框架件 + 波 2.2 先行刀）。
//!
//! 形状借 Linux probe 生命周期（报告 §2.4「借三件形状，不借住址」）：
//! [`Driver::probe`] 对 MMIO 窗自检认领（Linux `bus->match()` + `probe()`
//! 的极小化身），[`Driver::start`] 服役，[`Driver::stop`] 退役——三口可循环
//! （stop 后可再 start），不借 .ko 运行时装载与 sysfs 全量树（ADR #46 立法
//! 拒绝项）。失败面是值不炸：探测不认领返回 [`DriverError::UnsupportedDevice`]
//! 等错误值，registry 换下一家继续问——「单设备单主人」的认领仲裁归 devmgr
//! v2（波 1），本 crate 只钉生命周期契约。
//!
//! 形态 = berry-virtio 族例：`cfg_attr(not(test), no_std)` 库本体 + alloc
//! （registry 的 Vec 面）；test 构建放开 std 仅为默认测试挂具。接线示例见
//! [`virtio_example`]（berry-virtio 框架库怎么被新 trait 包装——真消费面
//! vblk/vnet 接线是后续刀）。
//!
//! 与 berry-hal 的关系（2026-11-20 债务清理刀定谳：**契约面正交组合**）：
//! 本 crate 钉**生命周期**契约（probe 认领 → start 服役 → stop 退役），
//! berry-hal 钉**硬件访问**契约（Mmio/IrqCtl/DmaAlloc/Clock 四窄
//! trait）——一枚完整驱动 = 两者的正交组合：Driver 实现侧在 probe/
//! start 里经 hal 的 trait 对象读写窗、注册中断。依赖方向单向可选：
//! 本 crate 的示例/实现侧可引 berry-hal 兑现 volatile 读写（vprobe
//! 同款接线形态），hal 永不反向依赖本 crate；trait 面不硬编 HAL
//! （probe 的 `mmio_base` 裸指针是总线中立最小面，volatile 读写方式
//! 归实现侧自选——mock 可零 HAL 独立测试）。缝合点（DeviceCap 三铸
//! 件：MMIO 窗 VMO + Interrupt + Bti）归波 2.2 后续刀，届时组合面
//! 升格为 bus 层铸件，本 crate 生命周期契约保持不被 HAL 契约绑架。

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod registry;
pub mod virtio_example;

pub use registry::{DriverRegistry, ProbeResult};

/// 统一驱动生命周期 trait。实现方持有设备状态机；probe 认领的窗与 IRQ 由
/// 实现方自记（trait 不硬编存字段——mock 与真驱动同契约）。
pub trait Driver {
    /// 驱动名（诊断面/registry 报告用，静态串）。
    fn name(&self) -> &'static str;

    /// 探测认领：对给定 MMIO 窗基址与 IRQ 号自检匹配性。认领返回
    /// [`ProbeInfo`]；不认领返回 Err——registry 探测失败不炸，继续问下家。
    fn probe(&mut self, mmio_base: *mut u8, irq: u32) -> Result<ProbeInfo, DriverError>;

    /// 服役：probe 成功后拉起设备（真驱动在此协商特性、使能队列/中断）。
    fn start(&mut self) -> Result<(), DriverError>;

    /// 退役：停设备、释放服役态；stop 后可再 start（生命周期循环）。
    fn stop(&mut self) -> Result<(), DriverError>;
}

/// probe 认领出的设备标识三件套（virtio-mmio 语义下 = DEVICE_ID/VENDOR_ID
/// 寄存器读数与传输层 VERSION——见 virtio_example 的填法；非 virtio 总线
/// 由实现方自定义语义，字段名保持总线中立）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeInfo {
    pub device_id: u32,
    pub vendor_id: u32,
    pub version: u32,
}

/// 驱动生命周期错误面（值不炸——探测/起停失败走错误值，不 panic）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverError {
    /// MMIO 窗不可用（空指针/破窗）——真机上对应越界或未授 VMO。
    MmioFault,
    /// 窗在但不是本驱动管的设备（device_id = 读到的原始设备 ID，诊断面
    /// 对回槽里挂的是什么）。
    UnsupportedDevice { device_id: u32 },
    /// 状态机未就绪（如未 probe 就 start、未 start 就 stop 依赖面）。
    NotReady,
    /// 等设备就绪超时（如状态位回读验不过）。
    Timeout,
}
