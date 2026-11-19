//! berry-virtio 框架库的 Driver trait 包装示例（示例用——真消费面 vblk/vnet
//! 接线是后续刀，本模块钉「包装法」的形状）：
//!
//! - probe = vprobe 扫窗判据同源：MAGIC 验 "virt" + VERSION 验 modern(2)
//!   + DEVICE_ID 对本驱动的认领位——三个鉴定值/偏移全引 `berry_virtio::mmio`
//!   单源，不复制常量；
//! - start = virtio 规范 §2.1 状态机四步（ACK|DRIVER → FEATURES_OK 回读验
//!   未被设备撤 → DRIVER_OK），回读验不过走 [`DriverError::Timeout`]；
//! - stop = STATUS 写 0 复位，退回 Probed 态可再 start（生命周期循环）。
//!
//! 不在本示例的面（真驱动刀补）：特性协商的 DEV_FEAT/DRV_FEAT 寄存器对
//! （示例恒 FEATURES_OK 直通）、virtqueue 内存与 DMA、IRQ 注册（irq 参数
//! 只记不使能——Interrupt 对象归波 2.1）。

use crate::{Driver, DriverError, ProbeInfo};
use berry_virtio::mmio;
use berry_virtio::{ST_ACKNOWLEDGE, ST_DRIVER, ST_DRIVER_OK, ST_FEATURES_OK};

/// 生命周期状态机：Unprobed → Probed ↔ Started（probe 一次性，start/stop
/// 可循环）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Unprobed,
    Probed,
    Started,
}

/// 认领指定设备 ID 的 virtio-mmio 驱动示例（did 用 `berry_virtio::mmio`
/// 的 DEVICE_ID_* 常量族：块设备 = `DEVICE_ID_BLOCK` 等）。
pub struct VirtioMmioDriver {
    /// 本驱动认领的 virtio 设备 ID。
    did: u32,
    state: State,
    /// probe 认领的窗与 IRQ（start/stop 复用；Unprobed 态恒 null）。
    mmio_base: *mut u8,
    irq: u32,
    info: ProbeInfo,
}

impl VirtioMmioDriver {
    /// 造一个认领 `did` 设备的驱动实例（未 probe 态）。
    pub fn new(did: u32) -> Self {
        Self {
            did,
            state: State::Unprobed,
            mmio_base: core::ptr::null_mut(),
            irq: 0,
            // 零值占位（Unprobed 态无意义，probe 成功即覆写）。
            info: ProbeInfo { device_id: 0, vendor_id: 0, version: 0 },
        }
    }

    /// 最近一次 probe 认领的信息（Unprobed 态为零值占位）。
    pub fn info(&self) -> ProbeInfo {
        self.info
    }
}

/// LE 32 位 volatile 寄存器读（virtio-mmio 全域 LE；偏移来自 `mmio` 模块
/// 冻结值表）。调用方保证 base 非 null 且窗内界检（probe 前置 null 检查，
/// 界检 = 授窗面 VMO 的职责，本层不重复执法）。
unsafe fn rd32(base: *mut u8, off: usize) -> u32 {
    base.add(off).cast::<u32>().read_volatile()
}

/// LE 32 位 volatile 寄存器写。
unsafe fn wr32(base: *mut u8, off: usize, v: u32) {
    base.add(off).cast::<u32>().write_volatile(v)
}

impl Driver for VirtioMmioDriver {
    fn name(&self) -> &'static str {
        "virtio-mmio"
    }

    fn probe(&mut self, mmio_base: *mut u8, irq: u32) -> Result<ProbeInfo, DriverError> {
        if mmio_base.is_null() {
            return Err(DriverError::MmioFault);
        }
        // 判据序 = vprobe 扫窗同源：magic → version → did（逐级拒绝，错误
        // 值带回读数供诊断面对账）。
        let magic = unsafe { rd32(mmio_base, mmio::MAGIC) };
        if magic != mmio::MAGIC_VAL {
            return Err(DriverError::UnsupportedDevice { device_id: magic });
        }
        let version = unsafe { rd32(mmio_base, mmio::VERSION) };
        if version != mmio::VERSION_MODERN {
            // legacy(v1)/非 modern 传输层不认领（vprobe 拒 legacy 同律）。
            return Err(DriverError::UnsupportedDevice { device_id: self.did });
        }
        let device_id = unsafe { rd32(mmio_base, mmio::DEVICE_ID) };
        if device_id != self.did {
            return Err(DriverError::UnsupportedDevice { device_id });
        }
        let vendor_id = unsafe { rd32(mmio_base, mmio::VENDOR_ID) };
        self.state = State::Probed;
        self.mmio_base = mmio_base;
        self.irq = irq;
        self.info = ProbeInfo { device_id, vendor_id, version };
        Ok(self.info)
    }

    fn start(&mut self) -> Result<(), DriverError> {
        match self.state {
            State::Unprobed => Err(DriverError::NotReady),
            State::Started => Ok(()), // 幂等（重复 start 不是错——restart 面零特判）
            State::Probed => {
                // 规范 §2.1 状态机：ACK → DRIVER → FEATURES_OK（回读验设备
                // 未撤）→ DRIVER_OK。特性协商寄存器对留真驱动刀（示例
                // FEATURES_OK 直通——host 测试窗读回即所写，恒过）。
                unsafe {
                    let base = self.mmio_base;
                    wr32(base, mmio::STATUS, ST_ACKNOWLEDGE | ST_DRIVER);
                    wr32(
                        base,
                        mmio::STATUS,
                        ST_ACKNOWLEDGE | ST_DRIVER | ST_FEATURES_OK,
                    );
                    let rb = rd32(base, mmio::STATUS);
                    if rb & ST_FEATURES_OK == 0 {
                        return Err(DriverError::Timeout); // 设备撤位——真机形态
                    }
                    wr32(
                        base,
                        mmio::STATUS,
                        ST_ACKNOWLEDGE
                            | ST_DRIVER
                            | ST_FEATURES_OK
                            | ST_DRIVER_OK,
                    );
                }
                self.state = State::Started;
                Ok(())
            }
        }
    }

    fn stop(&mut self) -> Result<(), DriverError> {
        match self.state {
            State::Unprobed => Err(DriverError::NotReady),
            State::Probed => Ok(()), // 幂等（未服役即停 = 无操作）
            State::Started => {
                // STATUS 写 0 = 规范复位（设备回初始态，重走 probe 状态机）。
                unsafe { wr32(self.mmio_base, mmio::STATUS, 0) };
                self.state = State::Probed;
                Ok(())
            }
        }
    }
}
