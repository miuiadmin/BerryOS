//! berry-virtio 框架库的 Driver trait 包装示例（示例用——真消费面 vblk/vnet
//! 接线是后续刀，本模块钉「包装法」的形状）：
//!
//! - MMIO 读写经 berry-hal [`VirtioMmio`]（vprobe 同款接线，2026-11-20
//!   消费面扩展）：volatile 与窗界检（4 对齐 + off+4 ≤ 窗长）收敛在 HAL
//!   实现内部，本文件零 unsafe——原先的本地 rd32/wr32 volatile 对退役；
//! - probe = vprobe 扫窗判据同源：MAGIC 验 "virt" + VERSION 验 modern(2)
//!   + DEVICE_ID 对本驱动的认领位——鉴定值全引 `berry_virtio::mmio` 单源
//!   不复制常量，寄存器读写走 HAL 语义便捷面（magic()/version() 等，
//!   偏移单源在 HAL 实现侧）；
//! - start = virtio 规范 §2.1 状态机四步（ACK|DRIVER → FEATURES_OK 回读验
//!   未被设备撤 → DRIVER_OK），回读验不过走 [`DriverError::Timeout`]；
//! - stop = STATUS 写 0 复位，退回 Probed 态可再 start（生命周期循环）。
//!
//! 不在本示例的面（真驱动刀补）：特性协商的 DEV_FEAT/DRV_FEAT 寄存器对
//! （示例恒 FEATURES_OK 直通）、virtqueue 内存与 DMA、IRQ 注册（irq 参数
//! 只记不使能——Interrupt 对象归波 2.1）。

use crate::{Driver, DriverError, ProbeInfo};
use berry_hal::VirtioMmio;
use berry_virtio::mmio;
use berry_virtio::{ST_ACKNOWLEDGE, ST_DRIVER, ST_DRIVER_OK, ST_FEATURES_OK};

/// 授窗窗长：virtio-mmio 槽距 0x200（QEMU virt 32 槽几何——vprobe 扫窗
/// 同值单源 berry-exec，本 crate 保持零 syscall 面不引它，本地镜像 =
/// devmgr 几何常量同款惯例；本模块所触寄存器〔ID 头 0x00-0x0F 与
/// STATUS@0x70〕全在界内）。
const WIN_LEN: usize = 0x200;

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
    /// probe 认领的寄存器窗（berry-hal [`VirtioMmio`]——volatile 收敛其内）
    /// 与 IRQ（start/stop 复用；Unprobed 态恒 None）。
    win: Option<VirtioMmio>,
    irq: u32,
    info: ProbeInfo,
}

impl VirtioMmioDriver {
    /// 造一个认领 `did` 设备的驱动实例（未 probe 态）。
    pub fn new(did: u32) -> Self {
        Self {
            did,
            state: State::Unprobed,
            win: None,
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

impl Driver for VirtioMmioDriver {
    fn name(&self) -> &'static str {
        "virtio-mmio"
    }

    fn probe(&mut self, mmio_base: *mut u8, irq: u32) -> Result<ProbeInfo, DriverError> {
        if mmio_base.is_null() {
            return Err(DriverError::MmioFault);
        }
        // 授窗经 berry-hal（vprobe 同款接线）：null 已上拒，此支 Invalid
        // 现几何下不可达——map_err 兜底防窗长常量未来漂移。
        let win = VirtioMmio::new(mmio_base, WIN_LEN).map_err(|_| DriverError::MmioFault)?;
        // 判据序 = vprobe 扫窗同源：magic → version → did（逐级拒绝，错误
        // 值带回读数供诊断面对账——寄存器读经 HAL 语义面，零 unsafe）。
        let magic = win.magic();
        if magic != mmio::MAGIC_VAL {
            return Err(DriverError::UnsupportedDevice { device_id: magic });
        }
        let version = win.version();
        if version != mmio::VERSION_MODERN {
            // legacy(v1)/非 modern 传输层不认领（vprobe 拒 legacy 同律）。
            return Err(DriverError::UnsupportedDevice { device_id: self.did });
        }
        let device_id = win.device_id();
        if device_id != self.did {
            return Err(DriverError::UnsupportedDevice { device_id });
        }
        let vendor_id = win.vendor_id();
        self.state = State::Probed;
        self.win = Some(win);
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
                // FEATURES_OK 直通——host 测试窗读回即所写，恒过）。STATUS
                // 读写经 HAL 语义面 set_status/status（零 unsafe）。
                let Some(win) = self.win.as_mut() else {
                    // Probed 态窗恒在（probe 成功即授窗）——状态机互证兜底。
                    return Err(DriverError::MmioFault);
                };
                win.set_status(ST_ACKNOWLEDGE | ST_DRIVER);
                win.set_status(ST_ACKNOWLEDGE | ST_DRIVER | ST_FEATURES_OK);
                if win.status() & ST_FEATURES_OK == 0 {
                    return Err(DriverError::Timeout); // 设备撤位——真机形态
                }
                win.set_status(ST_ACKNOWLEDGE | ST_DRIVER | ST_FEATURES_OK | ST_DRIVER_OK);
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
                if let Some(win) = self.win.as_mut() {
                    win.set_status(0);
                }
                self.state = State::Probed;
                Ok(())
            }
        }
    }
}
