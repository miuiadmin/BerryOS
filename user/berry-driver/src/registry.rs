//! 驱动注册表 + 全槽探测：注册的驱动族对一排 MMIO 槽逐一 probe，结果
//! （含失败）全量入 Vec 返回——探测失败不炸，失败值继续走（报告 P1：
//! vprobe 白名单外 did = trap 的炸面，在框架层改走错误值）。

use crate::{Driver, DriverError, ProbeInfo};
use alloc::vec::Vec;

/// 一次 probe 的结果记录：驱动 × 槽组合各一条，成功失败都记——消费面
/// （未来 devmgr）拿全量记录做认领仲裁（「单设备单主人」），本层不裁。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeResult {
    /// 探测方驱动名（`Driver::name`）。
    pub driver: &'static str,
    /// `mmio_slots` 下标（诊断面对回槽位；irq 不重复存，槽参数原样可查）。
    pub slot: usize,
    /// 认领出的设备信息或失败因。
    pub outcome: Result<ProbeInfo, DriverError>,
}

impl ProbeResult {
    /// 该次探测是否认领成功。
    pub fn is_ok(&self) -> bool {
        self.outcome.is_ok()
    }
}

/// 驱动注册表：持一组静态生命周期的驱动对象（真机形态 = 泄漏的全局驱动
/// 实例或 linker 段收集；host 测试经 `Box::leak` 造同形）。
pub struct DriverRegistry {
    drivers: Vec<&'static mut dyn Driver>,
}

impl DriverRegistry {
    pub fn new() -> Self {
        Self { drivers: Vec::new() }
    }

    /// 注册一个驱动（顺序即探测序——先注册先问）。
    pub fn register(&mut self, drv: &'static mut dyn Driver) {
        self.drivers.push(drv);
    }

    /// 注册驱动数。
    pub fn len(&self) -> usize {
        self.drivers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.drivers.is_empty()
    }

    /// 全组合探测：每个槽让每个注册驱动各 probe 一次，每组合一条
    /// [`ProbeResult`]（槽外层、驱动内层——同槽的竞争结果相邻，仲裁好读）。
    /// 任何失败（含 [`DriverError::MmioFault`]）只记不断——单槽单驱动的
    /// 失败不影响其余组合继续探测。
    pub fn probe_all(&mut self, mmio_slots: &[(*mut u8, u32)]) -> Vec<ProbeResult> {
        let mut out = Vec::new();
        for (slot, &(mmio_base, irq)) in mmio_slots.iter().enumerate() {
            for drv in self.drivers.iter_mut() {
                let driver = drv.name();
                let outcome = drv.probe(mmio_base, irq);
                out.push(ProbeResult { driver, slot, outcome });
            }
        }
        out
    }
}

impl Default for DriverRegistry {
    fn default() -> Self {
        Self::new()
    }
}
