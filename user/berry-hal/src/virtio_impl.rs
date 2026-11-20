//! [`Mmio`] trait 的 virtio-mmio 实现——把 volatile 读写包装为安全接口。
//!
//! berry-virtio 库只单源寄存器偏移表与鉴定值（「volatile 读写在驱动本体」
//! ——其 mmio 模块注释自述），本模块补上统一 HAL 侧的包装：①窗界检
//! （[`VirtioMmio::new`] 记窗长，越界/非对齐偏移读 0 写弃——值不炸，
//! unsafe volatile 面完全收敛在实现内部，调用方零 unsafe）；②virtio
//! 寄存器语义便捷面（[`magic`]/[`device_id`] 等——偏移全引
//! `berry_virtio::mmio` 单源不复制，berry-driver virtio_example 同款
//! 「引常量不复制」纪律）。
//!
//! LE 域说明：virtio-mmio 全域 LE（规范 §4.2.2），host 与目标 aarch64
//! 均 LE，`read_volatile` 直读即 LE 序（消费面 vprobe 与 berry-driver
//! virtio_example 全经本实现走读写，不再各持 volatile 面）。
//!
//! [`magic`]: VirtioMmio::magic
//! [`device_id`]: VirtioMmio::device_id

use crate::{HalError, Mmio};
use berry_virtio::mmio;

/// virtio-mmio 寄存器窗（真机形态 = 授窗 VMO 的基址与长度）。
///
/// 界检门在 [`VirtioMmio::new`]（null/零长窗拒）；读写偏移门 = 4 字节
/// 对齐且 `off + 4` 在窗内（越界读 0 写弃——[`Mmio`] 契约的值不炸面）。
pub struct VirtioMmio {
    base: *mut u8,
    len: usize,
}

impl VirtioMmio {
    /// 授窗：`base` 为窗基址、`len` 为窗长（字节）。null 或零长窗 =
    /// [`HalError::Invalid`]（真机语义 = 未授 VMO/破窗——真机上越界访问
    /// 会 D-abort，本层在授窗时就拒，读写期界检兜底）。
    pub fn new(base: *mut u8, len: usize) -> Result<Self, HalError> {
        if base.is_null() || len == 0 {
            return Err(HalError::Invalid);
        }
        Ok(Self { base, len })
    }

    /// 窗长（字节）。
    pub fn len(&self) -> usize {
        self.len
    }

    /// 窗是否为空（`new` 已拒零长窗，恒 false——门在构造期）。
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 偏移界检：4 对齐且 `off + 4` 在窗内（溢出面 = false）。
    fn in_bounds(&self, off: usize) -> bool {
        off % 4 == 0
            && off
                .checked_add(4)
                .map_or(false, |end| end <= self.len)
    }

    // ---- virtio 寄存器语义便捷面（偏移单源 = berry_virtio::mmio）----

    /// MAGIC 寄存器读数（鉴定值 = `mmio::MAGIC_VAL` "virt"）。
    pub fn magic(&self) -> u32 {
        self.read32(mmio::MAGIC)
    }

    /// 传输层版本（2 = modern，v1 legacy 已弃）。
    pub fn version(&self) -> u32 {
        self.read32(mmio::VERSION)
    }

    /// 设备 ID（DEVICE_ID_BLOCK/NETWORK/INPUT/GPU 族——鉴定值单源在
    /// `berry_virtio::mmio`）。
    pub fn device_id(&self) -> u32 {
        self.read32(mmio::DEVICE_ID)
    }

    /// 厂商 ID（QEMU virtio = 0x554d4551）。
    pub fn vendor_id(&self) -> u32 {
        self.read32(mmio::VENDOR_ID)
    }

    /// STATUS 寄存器读数（状态机位 = `berry_virtio::ST_*` 族）。
    pub fn status(&self) -> u32 {
        self.read32(mmio::STATUS)
    }

    /// STATUS 寄存器写（如状态机四步推进/写 0 复位）。
    pub fn set_status(&mut self, v: u32) {
        self.write32(mmio::STATUS, v)
    }

    /// 中断状态读（波 2.1 Interrupt 接线的轮询退面——EOI 前判源）。
    pub fn int_status(&self) -> u32 {
        self.read32(mmio::INT_STATUS)
    }

    /// 中断确认写（Split-EOI 的设备半边）。
    pub fn int_ack(&mut self, v: u32) {
        self.write32(mmio::INT_ACK, v)
    }

    /// 队列通知写（kick——virtqueue 发提交面的高频寄存器）。
    pub fn queue_notify(&mut self, idx: u32) {
        self.write32(mmio::Q_NOTIFY, idx)
    }

    /// 探测判据复合面：magic = "virt" 且 version = modern（vprobe 扫窗
    /// 同源判据序的前两级；did 认领归驱动业务层，本面只验传输层）。
    pub fn is_virtio_modern(&self) -> bool {
        self.magic() == mmio::MAGIC_VAL && self.version() == mmio::VERSION_MODERN
    }
}

impl Mmio for VirtioMmio {
    fn read32(&self, off: usize) -> u32 {
        // 越界/非对齐读 0（值不炸：窗外面板恒零——真机授窗 VMO 界外
        // 不可达，mock 零层面板同语义）。
        if !self.in_bounds(off) {
            return 0;
        }
        // 前置已证：base 非 null、off 对齐且界内——unsafe 收敛于此。
        unsafe { self.base.add(off).cast::<u32>().read_volatile() }
    }

    fn write32(&mut self, off: usize, v: u32) {
        // 越界/非对齐写弃（读 0 的对偶面：无错误通道的 void 写走丢弃）。
        if !self.in_bounds(off) {
            return;
        }
        unsafe { self.base.add(off).cast::<u32>().write_volatile(v) }
    }
}
